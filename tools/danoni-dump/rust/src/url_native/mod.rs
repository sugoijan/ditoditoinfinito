//! Blocking job-thread adapter from reviewed public URLs to normalized native works.
//! Scripts run only in the existing confined worker; HTTP remains host-owned.
mod evidence;
use crate::{
    Error, Result,
    acquisition::{self, Movie, SourceAddress},
    assets::*,
    dependencies::{self, DependencyLimits, DependencyResponse, ResponseProvenance},
    model::Metadata,
    native,
    network::{
        AcquiredResource, FetchOptions, Fetcher, NetworkCancel, ReplayRequest, RequestContext,
    },
    orchestration,
    profiles::{NativeContext, ProfileLimits, ProfileRequest},
    supervisor::{Limits, WorkerSpec},
    swf,
};
use evidence::Evidence;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-url-native", message)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UrlRequest {
    pub url: String,
    /// Exact logical SWF URL when a page contains more than one candidate.
    pub movie_url: Option<String>,
    pub encoding_override: Option<String>,
    pub selectors: Option<Vec<usize>>,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub fetch: FetchOptions,
    #[serde(default)]
    pub dependencies: DependencyLimits,
    #[serde(default)]
    pub profile: ProfileLimits,
    #[serde(default)]
    pub capture: Limits,
}
impl UrlRequest {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            movie_url: None,
            encoding_override: None,
            selectors: None,
            metadata: Metadata::new(),
            fetch: FetchOptions::default(),
            dependencies: DependencyLimits::default(),
            profile: ProfileLimits::default(),
            capture: Limits::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: &'static str,
    pub url: Option<String>,
    pub dependency: Option<dependencies::Progress>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub ready: bool,
    pub output: PathBuf,
    pub manifest: Value,
    pub failure: Option<Error>,
}
fn check(cancel: &NetworkCancel, started: Instant, wall_ms: u64) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(Error::new("request-cancelled", "URL operation cancelled"));
    }
    if started.elapsed().as_millis() >= wall_ms as u128 {
        return Err(Error::new(
            "url-native-limit",
            "Whole URL operation deadline exceeded",
        ));
    }
    Ok(())
}
fn output_limit(root: &Path, maximum: u64, last: &mut Instant, force: bool) -> Result<()> {
    if force || last.elapsed() >= Duration::from_millis(50) {
        if orchestration::tree_size(root)? > maximum {
            return Err(Error::new(
                "url-native-limit",
                "Whole URL output byte limit exceeded",
            ));
        }
        *last = Instant::now();
    }
    Ok(())
}
fn acquire(
    runtime: &tokio::runtime::Runtime,
    fetcher: &mut Fetcher,
    address: &SourceAddress,
    maximum: usize,
    cancel: &NetworkCancel,
    site: &mut impl FnMut(&str) -> Result<String>,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<AcquiredResource> {
    checkpoint()?;
    runtime.block_on(async {
        let request = ReplayRequest {
            url: address.retrieval_url.clone(),
            context: RequestContext::default(),
            maximum_bytes: maximum,
        };
        let mut guard = |url: &str| {
            let actual = SourceAddress::parse(url)?;
            if address.capture.is_some() && actual.capture.is_none() {
                return Err(Error::new(
                    "archive-live-fallback",
                    "Archive redirect cannot silently switch to live retrieval",
                ));
            }
            Ok(())
        };
        let mut future = Box::pin(fetcher.fetch_checked(&request, cancel, site, &mut guard));
        let mut interval = tokio::time::interval(Duration::from_millis(50));
        loop {
            tokio::select! { biased;
                _ = interval.tick() => {
                    if let Err(e) = checkpoint() {
                        cancel.cancel();
                        // Drain cancellation so a started request retains its failure record.
                        let _ = future.await;
                        return Err(e);
                    }
                }
                result = &mut future => {
                    let value = result?;
                    checkpoint()?;
                    return Ok(value);
                }
            }
        }
    })
}
fn successful(resource: &AcquiredResource) -> Result<()> {
    if resource.response().status != 200 {
        return Err(Error::new(
            "url-native-unavailable",
            format!(
                "HTTP {} for {}",
                resource.response().status,
                resource.requested_url
            ),
        ));
    }
    Ok(())
}
fn choose(discovery: &acquisition::PageDiscovery, selected: Option<&str>) -> Result<Movie> {
    let selected = selected.map(SourceAddress::parse).transpose()?;
    let mut seen = BTreeSet::new();
    let movies: Vec<_> = discovery
        .movies
        .iter()
        .filter(|m| {
            selected.as_ref().is_none_or(|s| {
                s.logical_url == m.address.logical_url
                    && s.capture
                        .as_ref()
                        .is_none_or(|_| s.retrieval_url == m.address.retrieval_url)
            }) && seen.insert((
                m.address.retrieval_url.clone(),
                serde_json::to_string(&m.flash_vars).unwrap(),
            ))
        })
        .collect();
    // A common legacy layout has a game and a smaller Flash side panel.
    // This only proposes a candidate; profile and runtime proofs still gate it.
    if selected.is_none()
        && movies.len() > 1
        && movies
            .iter()
            .map(|m| &m.address.logical_url)
            .collect::<BTreeSet<_>>()
            .len()
            == movies.len()
        && movies.iter().all(|m| m.viewport.is_some())
    {
        let area = |m: &&Movie| m.viewport.map(|[w, h]| w as u64 * h as u64).unwrap();
        let largest = movies.iter().map(area).max().unwrap();
        let primary: Vec<_> = movies.iter().filter(|m| area(m) == largest).collect();
        if let [movie] = primary.as_slice() {
            let [w, h] = movie.viewport.unwrap();
            if w >= h
                && movies.iter().filter(|m| area(m) != largest).all(|m| {
                    let [side_w, side_h] = m.viewport.unwrap();
                    side_h == h && side_w < side_h
                })
            {
                return Ok((***movie).clone());
            }
        }
    }
    match movies.as_slice() {
        [m] => Ok((*m).clone()),
        [] => Err(Error::new(
            "url-native-no-movie",
            "No selected static SWF; scripts and navigation are not executed",
        )),
        _ => Err(Error::new(
            "url-native-ambiguous",
            "Several SWFs or FlashVars contexts; select an exact logical movie URL",
        )),
    }
}
/// Runs on a blocking job thread outside an async runtime. The host owns the
/// worker installation, site-policy decisions, cancellation and external roots.
/// Success means normalized source proofs, not musical approval or installation.
pub fn execute(
    request: &UrlRequest,
    cache: &Path,
    worker: &WorkerSpec,
    output: &Path,
    cancel: &NetworkCancel,
    site: &mut impl FnMut(&str) -> Result<String>,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<Outcome> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(invalid("Call URL preparation on a blocking job thread"));
    }
    let initial = SourceAddress::parse(&request.url)?;
    if let Some(url) = &request.movie_url {
        SourceAddress::parse(url)?;
    }
    if request.fetch.wall_ms == 0 || request.fetch.wall_ms > 300000 {
        return Err(invalid("URL operation wall bound"));
    }
    request.dependencies.validate()?;
    request.profile.validate()?;
    request.capture.validate()?;
    if request.selectors.as_ref().is_some_and(|v| {
        v.is_empty()
            || v.len() > 16
            || v.iter().any(|i| *i >= 16)
            || v.windows(2).any(|w| w[0] >= w[1])
    }) {
        return Err(invalid("Invalid native selector indices"));
    }
    if request
        .encoding_override
        .as_ref()
        .is_some_and(|e| encoding_rs::Encoding::for_label(e.as_bytes()).is_none())
    {
        return Err(invalid("Unknown explicit page encoding"));
    }
    let started = Instant::now();
    check(cancel, started, request.fetch.wall_ms)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(native::io)?;
    let mut fetcher = Fetcher::open(cache, request.fetch.clone())?;
    orchestration::worker_bytes(worker)?;
    let mut owned = orchestration::new_root(output)?;
    std::fs::create_dir(owned.root.join("acquisition")).map_err(native::io)?;
    let root = owned.root.join("acquisition");
    let evidence = Evidence::default();
    let mut acquisitions = Vec::new();
    let mut active = Progress {
        stage: "acquisition",
        url: Some(initial.retrieval_url.clone()),
        dependency: None,
    };
    let mut completed = Value::Null;
    let mut metadata = request.metadata.clone();
    let mut last_output_check = Instant::now();
    let result = (|| -> Result<()> {
        let mut checkpoint = || {
            check(cancel, started, request.fetch.wall_ms)?;
            output_limit(
                &owned.root,
                request.dependencies.output_bytes,
                &mut last_output_check,
                false,
            )?;
            progress(&active)?;
            check(cancel, started, request.fetch.wall_ms)
        };
        let explicit_swf = url::Url::parse(&initial.logical_url)
            .map_err(|e| invalid(e.to_string()))?
            .path()
            .to_ascii_lowercase()
            .ends_with(".swf");
        let first = acquire(
            &runtime,
            &mut fetcher,
            &initial,
            if explicit_swf {
                swf::MOVIE_LIMIT
            } else {
                acquisition::PAGE_LIMIT
            },
            cancel,
            site,
            &mut checkpoint,
        )?;
        acquisitions.push(evidence.archive(&root, &fetcher, &first)?);
        successful(&first)?;
        let (movie, page_url, captured) = if first.response().body.starts_with(b"FWS")
            || first.response().body.starts_with(b"CWS")
        {
            if request.movie_url.as_ref().is_some_and(|u| {
                SourceAddress::parse(u)
                    .ok()
                    .is_none_or(|s| s.logical_url != initial.logical_url)
            }) {
                return Err(invalid("Selected movie differs from direct SWF input"));
            }
            (
                Movie {
                    kind: "direct".into(),
                    address: initial.clone(),
                    flash_vars: BTreeMap::new(),
                    viewport: None,
                },
                None,
                first,
            )
        } else {
            if explicit_swf {
                return Err(invalid("SWF URL did not return an admitted SWF signature"));
            }
            if first.response().selected_response["mimeType"] != "text/html" {
                return Err(invalid("Need an HTTP HTML page or direct FWS/CWS movie"));
            }
            let page = acquisition::inspect_page(
                &first.response().response_url,
                &first.response().body,
                request
                    .encoding_override
                    .as_deref()
                    .or_else(|| first.response().selected_response["charset"].as_str()),
            )?;
            let discovery = evidence.json(&root, "discovery.json".into(), &page)?;
            if !metadata.contains_key("title")
                && !metadata.contains_key("musicTitle")
                && let Some(title) = &page.title
            {
                metadata.insert("title".into(), json!(title));
                metadata.insert("titleEvidence".into(), json!(format!("passive-html-title/1; url:discovery.json sha256:{}; page body sha256:{}; may include site/work-list wording",discovery.sha256,page.body_sha256)));
            }
            let selected = choose(&page, request.movie_url.as_deref())?;
            let second = acquire(
                &runtime,
                &mut fetcher,
                &selected.address,
                swf::MOVIE_LIMIT,
                cancel,
                site,
                &mut checkpoint,
            )?;
            acquisitions.push(evidence.archive(&root, &fetcher, &second)?);
            successful(&second)?;
            (selected, Some(page.source.logical_url), second)
        };
        checkpoint()?;
        let final_address = SourceAddress::parse(&captured.response().response_url)?;
        let dependency_base = if final_address.capture.is_some() {
            final_address
        } else {
            movie.address.clone()
        };
        let movie_binding = evidence.body(&captured.response().body_binding);
        let context = NativeContext {
            movie: movie_binding.path.clone(),
            source_url: movie.address.logical_url.clone(),
            page_url,
            flash_vars: movie.flash_vars.clone(),
            resources: vec![],
        };
        let context_binding = evidence.json(&root, "context.json".into(), &context)?;
        let source_binding = evidence.json(
            &root,
            "source-manifest.json".into(),
            &json!({
                "schema":"ddi-danoni-url-source/1", "sourceSwfSha256":movie_binding.sha256,
                "source": initial, "pageUrl":context.page_url, "movie":movie, "dependencyRetrievalBase":dependency_base, "acquisitions":acquisitions,
            "selectionPolicy":"Explicit URL, unique context, or unique largest declared viewport with narrow equal-height siblings; native profile and runtime proofs mandatory",
                "roots":{"url":".","cache":"cache"}
            }),
        )?;
        let profile = ProfileRequest {
            movie: movie_binding,
            source_manifest: source_binding,
            context: context_binding,
            optional_color: None,
            selectors: request.selectors.clone(),
            metadata,
            limits: request.profile.clone(),
            capture_limits: request.capture.clone(),
        };
        let source = evidence.clone();
        // Resolver and progress callbacks share no mutable closure state.
        let mut resolver = |missing: &dependencies::MissingDependency,
                            checkpoint: &mut dyn FnMut() -> Result<()>| {
            let target = if dependency_base.capture.is_some() {
                dependency_base.resolve(&missing.url)?
            } else {
                SourceAddress::parse(&missing.url)?
            };
            let acquired = acquire(
                &runtime,
                &mut fetcher,
                &target,
                missing.maximum_bytes as usize,
                cancel,
                site,
                checkpoint,
            )?;
            let receipt = evidence.archive(&root, &fetcher, &acquired)?;
            acquisitions.push(receipt.clone());
            successful(&acquired)?;
            let r = acquired.response();
            let body = evidence.body(&r.body_binding);
            let p = ResponseProvenance {
                schema: "ddi-danoni-dependency-response/1".into(),
                logical_url: missing.url.clone(),
                retrieval_url: r.response_url.clone(),
                retrieved_at: r.selected_response["retrievedAt"]
                    .as_str()
                    .ok_or_else(|| invalid("Missing response retrieval date"))?
                    .into(),
                status: r.status,
                sha256: body.sha256.clone(),
                policy_decision: format!("url:{}#sha256={}", receipt.path, receipt.sha256),
            };
            let provenance = evidence.json(
                &root,
                format!("dependencies/{}.json", sha256(missing.url.as_bytes())),
                &p,
            )?;
            Ok(DependencyResponse {
                body,
                provenance,
                http_charset: r.selected_response["charset"].as_str().map(str::to_owned),
            })
        };
        active.stage = "native";
        active.url = Some(movie.address.logical_url.clone());
        let outcome = dependencies::execute_profile(
            &profile,
            worker,
            &source,
            &owned.root.join("native"),
            &request.dependencies,
            &mut resolver,
            &mut |p| {
                active.dependency = Some(p.clone());
                check(cancel, started, request.fetch.wall_ms)?;
                output_limit(
                    &owned.root,
                    request.dependencies.output_bytes,
                    &mut last_output_check,
                    matches!(p.stage, "ready" | "attempt-ready"),
                )?;
                progress(&active)?;
                check(cancel, started, request.fetch.wall_ms)
            },
        )?;
        if !outcome.ready {
            return Err(outcome
                .failure
                .unwrap_or_else(|| invalid("Native dependency run did not complete")));
        }
        let mut normalized = outcome.manifest["completed"]["normalized"].clone();
        for key in ["manifest", "normalization"] {
            let path = normalized[key]
                .as_str()
                .ok_or_else(|| invalid("Missing normalized result path"))?;
            relative_path(path)?;
            normalized[key] = json!(format!("native/{path}"));
        }
        completed = json!({"receipt":{"path":"native/manifest.json","sha256":sha256(&std::fs::read(owned.root.join("native/manifest.json")).map_err(native::io)?)},"normalized":normalized});
        Ok(())
    })();
    let inputs = evidence.bindings()?;
    let mut failure = result.err();
    if failure.is_some() {
        completed = Value::Null;
    }
    let mut manifest = json!({"schema":"ddi-danoni-url-native-run/1","ready":failure.is_none(),"request":request,"roots":{"url":"acquisition","cache":"acquisition/cache"},"acquisitions":acquisitions,"inputs":inputs,"completed":completed,"transfers":fetcher.counts(),"failure":failure,"scope":"Normalized source proofs; no musical approval, HTML5 execution, index crawl or installation"});
    if failure.is_none() {
        // Do all potentially large serialization/I/O before the commit checkpoint.
        let body = serde_json::to_vec_pretty(&manifest).map_err(|e| invalid(e.to_string()))?;
        orchestration::write(&owned.root, ".manifest.json.tmp", &body)?;
        active.stage = "ready";
        active.dependency = None;
        failure = output_limit(
            &owned.root,
            request.dependencies.output_bytes,
            &mut last_output_check,
            true,
        )
        .and_then(|_| check(cancel, started, request.fetch.wall_ms))
        .and_then(|_| progress(&active))
        .and_then(|_| check(cancel, started, request.fetch.wall_ms))
        .err();
        if failure.is_none() {
            std::fs::rename(
                owned.root.join(".manifest.json.tmp"),
                owned.root.join("manifest.json"),
            )
            .map_err(native::io)?;
        } else {
            std::fs::remove_file(owned.root.join(".manifest.json.tmp")).map_err(native::io)?;
            manifest["ready"] = json!(false);
            manifest["completed"] = Value::Null;
            manifest["failure"] = json!(failure);
        }
    }
    if failure.is_some() {
        orchestration::atomic(&owned.root, "failure.json", &manifest)?;
    }
    owned.retain = true;
    Ok(Outcome {
        ready: failure.is_none(),
        output: owned.root.clone(),
        manifest,
        failure,
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema: String,
    cache: PathBuf,
    request: UrlRequest,
    origins: BTreeMap<String, String>,
}
pub fn execute_job(bundle: &Path, job: &Path, output: &Path) -> Result<Outcome> {
    let path = job.canonicalize().map_err(native::io)?;
    native::external(&path)?;
    let dir = path.parent().unwrap();
    let src = native::DirectorySource::new(BTreeMap::from([("job".into(), dir.to_owned())]))?;
    let job: Job = json(
        &src.read(
            "job",
            path.file_name()
                .unwrap()
                .to_str()
                .ok_or_else(|| invalid("Job filename"))?,
            JSON_LIMIT,
        )?,
    )?;
    if job.schema != "ddi-danoni-url-native-job/1" {
        return Err(invalid("Unsupported URL job schema"));
    }
    let worker = crate::worker_build::load_bundle(bundle, &mut || Ok(()))?;
    execute(
        &job.request,
        &if job.cache.is_absolute() {
            job.cache
        } else {
            dir.join(job.cache)
        },
        &worker,
        output,
        &NetworkCancel::default(),
        &mut |url| {
            let origin = url::Url::parse(url)
                .map_err(|e| invalid(e.to_string()))?
                .origin()
                .ascii_serialization();
            job.origins.get(&origin).cloned().ok_or_else(|| {
                Error::new(
                    "site-terms-denied",
                    "Origin has no host-reviewed public-access decision",
                )
            })
        },
        &mut |_| Ok(()),
    )
}

#[cfg(test)]
mod tests;
