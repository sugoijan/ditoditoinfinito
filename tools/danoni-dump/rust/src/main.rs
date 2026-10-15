//! JSON CLI over the same API that a desktop host can call.
use ddi_danoni_export::{CompileRequest, Error, compile_chart};
use std::io::{self, Read, Write};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [command] if command == "compile-chart" => run().and_then(|result| {
            serde_json::to_value(result).map_err(|e| Error {
                code: "output-json",
                message: e.to_string(),
            })
        }),
        #[cfg(feature = "native")]
        [command, root]
            if [
                "create-store",
                "inspect-store",
                "verify-store",
                "recover-store",
            ]
            .contains(&command.as_str()) =>
        {
            store_command(command, std::path::Path::new(root))
        }
        #[cfg(feature = "native")]
        [command, ready, root] if command == "install-import" => {
            install_import(ready, root, "reject")
        }
        #[cfg(feature = "native")]
        [command, ready, root, policy] if command == "install-import" => {
            install_import(ready, root, policy)
        }
        #[cfg(feature = "native")]
        [command, root] if command == "inspect-import" => {
            ddi_danoni_export::prepared_import::load_ready(std::path::Path::new(root), &mut |_| {
                Ok(())
            })
            .and_then(|v| {
                serde_json::to_value(v.summary()).map_err(|e| Error {
                    code: "output-json",
                    message: e.to_string(),
                })
            })
        }
        #[cfg(feature = "native")]
        [command, bundle, job, output] if command == "prepare-url-native" => {
            ddi_danoni_export::url_prepare::execute_job(
                std::path::Path::new(bundle),
                std::path::Path::new(job),
                std::path::Path::new(output),
            )
            .and_then(|out| {
                if out.ready {
                    Ok(out.manifest)
                } else {
                    Err(out.failure.unwrap())
                }
            })
        }
        #[cfg(feature = "native")]
        [command, bundle, job, output] if command == "extract-url-native" => {
            ddi_danoni_export::url_native::execute_job(
                std::path::Path::new(bundle),
                std::path::Path::new(job),
                std::path::Path::new(output),
            )
            .and_then(|out| {
                if out.ready {
                    Ok(out.manifest)
                } else {
                    Err(out.failure.unwrap())
                }
            })
        }
        #[cfg(feature = "native")]
        [command, job] if command == "fetch-url" => {
            ddi_danoni_export::network::fetch_job(std::path::Path::new(job))
        }
        #[cfg(feature = "native")]
        [command, job] if command == "replay-cache" => {
            ddi_danoni_export::network::replay_job(std::path::Path::new(job))
        }
        #[cfg(feature = "native")]
        [command, job] if command == "inspect-cached-page" => {
            ddi_danoni_export::acquisition::inspect_cached_page_job(std::path::Path::new(job))
                .and_then(|v| {
                    serde_json::to_value(v).map_err(|e| Error {
                        code: "output-json",
                        message: e.to_string(),
                    })
                })
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "prepare-work" => prepare(job, output),
        #[cfg(feature = "native")]
        [command, registry, exports, output] if command == "rebuild-accepted" => {
            ddi_danoni_export::native::rebuild_accepted(
                std::path::Path::new(registry),
                std::path::Path::new(exports),
                std::path::Path::new(output),
                &mut |_| Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "artwork" => {
            ddi_danoni_export::native::generate_artwork(
                std::path::Path::new(job),
                std::path::Path::new(output),
            )
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "package-generation" => {
            ddi_danoni_export::package::package_job(
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut |_| Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, input] if command == "verify-generation" => {
            ddi_danoni_export::package::verify_generation(std::path::Path::new(input), &mut |_| {
                Ok(())
            })
        }
        #[cfg(feature = "native")]
        [command, stage, dest, history] if command == "promote" => {
            ddi_danoni_export::transaction::promote(
                std::path::Path::new(stage),
                std::path::Path::new(dest),
                std::path::Path::new(history),
                &mut |_| Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, journal] if command == "recover" => {
            ddi_danoni_export::transaction::recover(std::path::Path::new(journal), &mut |_| Ok(()))
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "prepare-audio" => {
            ddi_danoni_export::native::prepare_audio(
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut || Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "estimate-tempo" => {
            ddi_danoni_export::native::estimate_tempo(
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut || Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, bundle, job, output] if command == "profile-native-dependencies" => {
            ddi_danoni_export::profiles::execute_dependencies_job(
                std::path::Path::new(bundle),
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut |_| Ok(()),
            )
            .and_then(|out| {
                if let Some(e) = out.failure {
                    Err(e)
                } else {
                    Ok(out.manifest)
                }
            })
        }
        #[cfg(feature = "native")]
        [command, bundle, job, output] if command == "profile-native" => {
            ddi_danoni_export::profiles::execute_job(
                std::path::Path::new(bundle),
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut |_| Ok(()),
            )
            .and_then(|out| {
                if let Some(e) = out.failure {
                    Err(e)
                } else {
                    Ok(out.manifest)
                }
            })
        }
        #[cfg(feature = "native")]
        [command, bundle, job, output] if command == "extract-native-dependencies" => {
            ddi_danoni_export::dependencies::execute_job(
                std::path::Path::new(bundle),
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut |_| Ok(()),
            )
            .and_then(|outcome| {
                if let Some(error) = outcome.failure {
                    Err(error)
                } else {
                    Ok(outcome.manifest)
                }
            })
        }
        #[cfg(feature = "native")]
        [command, bundle, job, output] if command == "extract-native" => {
            ddi_danoni_export::orchestration::execute_job(
                std::path::Path::new(bundle),
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut |_| Ok(()),
            )
            .and_then(|outcome| {
                if let Some(error) = outcome.failure {
                    Err(error)
                } else {
                    Ok(outcome.manifest)
                }
            })
        }
        #[cfg(feature = "native")]
        [command, source, output] if command == "prepare-worker" => {
            ddi_danoni_export::worker_build::prepare_sources(
                std::path::Path::new(source),
                std::path::Path::new(output),
                &mut || Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "package-worker" => {
            ddi_danoni_export::worker_build::package_job(
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut || Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, bundle] if command == "verify-worker" => {
            ddi_danoni_export::worker_build::load_bundle(std::path::Path::new(bundle), &mut || {
                Ok(())
            })
            .and_then(|w| {
                serde_json::to_value(w).map_err(|e| Error {
                    code: "output-json",
                    message: e.to_string(),
                })
            })
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "capture-native" => {
            ddi_danoni_export::supervisor::capture_job(
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut || Ok(()),
            )
            .and_then(|outcome| {
                if let Some(error) = outcome.failure {
                    Err(error)
                } else {
                    Ok(serde_json::json!({
                        "schema":"ddi-danoni-native-capture-written/1",
                        "ready":outcome.ready,
                        "output":outcome.output,
                        "run":outcome.run
                    }))
                }
            })
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "project-native" => {
            ddi_danoni_export::native::project_native(
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut || Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "demux-native" => {
            ddi_danoni_export::native::demux_native(
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut || Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "normalize-native" => {
            ddi_danoni_export::native::normalize_native(
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut || Ok(()),
            )
        }
        _ => {
            eprintln!(
                "Usage: ddi-danoni-export compile-chart < request.json\n       ddi-danoni-export inspect-cached-page JOB.json\n       ddi-danoni-export replay-cache JOB.json\n       ddi-danoni-export fetch-url JOB.json\n       ddi-danoni-export inspect-import READY_DIRECTORY\n       ddi-danoni-export create-store NEW_DIRECTORY\n       ddi-danoni-export inspect-store DIRECTORY\n       ddi-danoni-export verify-store DIRECTORY\n       ddi-danoni-export recover-store DIRECTORY\n       ddi-danoni-export install-import READY_DIRECTORY STORE [reject|keep-existing|replace]\n       ddi-danoni-export prepare-url-native BUNDLE JOB.json NEW_OUTPUT\n       ddi-danoni-export extract-url-native BUNDLE JOB.json NEW_OUTPUT\n       ddi-danoni-export profile-native-dependencies BUNDLE JOB.json NEW_OUTPUT\n       ddi-danoni-export profile-native BUNDLE JOB.json NEW_OUTPUT\n       ddi-danoni-export extract-native-dependencies BUNDLE JOB.json NEW_OUTPUT\n       ddi-danoni-export prepare-work JOB.json NEW_OUTPUT\n       ddi-danoni-export rebuild-accepted REGISTRY.json EXPORTS NEW_OUTPUT\n       ddi-danoni-export artwork JOB.json NEW_OUTPUT\n       ddi-danoni-export package-generation JOB.json NEW_OUTPUT\n       ddi-danoni-export verify-generation GENERATION\n       ddi-danoni-export promote STAGE DESTINATION HISTORY\n       ddi-danoni-export recover JOURNAL.json\n       ddi-danoni-export prepare-audio JOB.json NEW_OUTPUT\n       ddi-danoni-export estimate-tempo JOB.json NEW_OUTPUT\n       ddi-danoni-export extract-native BUNDLE JOB.json NEW_OUTPUT\n       ddi-danoni-export prepare-worker UPSTREAM_GIT NEW_SOURCES\n       ddi-danoni-export package-worker JOB.json NEW_BUNDLE\n       ddi-danoni-export verify-worker BUNDLE\n       ddi-danoni-export capture-native JOB.json NEW_OUTPUT\n       ddi-danoni-export project-native JOB.json NEW_OUTPUT\n       ddi-danoni-export demux-native JOB.json NEW_OUTPUT\n       ddi-danoni-export normalize-native JOB.json NEW_OUTPUT"
            );
            std::process::exit(2);
        }
    };
    match result {
        Ok(result) => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            if serde_json::to_writer_pretty(&mut out, &result).is_err() || writeln!(out).is_err() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{}", serde_json::to_string(&error).unwrap());
            std::process::exit(1);
        }
    }
}
fn run() -> Result<ddi_danoni_export::PreparedChart, Error> {
    const MAX_JSON: u64 = 16 * 1024 * 1024;
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(MAX_JSON + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error {
            code: "input-read",
            message: e.to_string(),
        })?;
    if bytes.len() as u64 > MAX_JSON {
        return Err(Error {
            code: "input-limit",
            message: "JSON exceeds 16 MiB".into(),
        });
    }
    let request: CompileRequest = serde_json::from_slice(&bytes).map_err(|e| Error {
        code: "invalid-json",
        message: e.to_string(),
    })?;
    if request.schema != "ddi-danoni-compile-request/1" {
        return Err(Error {
            code: "unsupported-schema",
            message: request.schema,
        });
    }
    compile_chart(&request.chart, &request.metadata, &request.options)
}

#[cfg(feature = "native")]
fn prepare(job: &str, output: &str) -> Result<serde_json::Value, Error> {
    use ddi_danoni_export::{native, work};
    let (job, source) = native::load_job(std::path::Path::new(job))?;
    let prepared = work::prepare_work(&job.work, &source, &mut |_| Ok(()))?;
    native::write_prepared(&prepared, std::path::Path::new(output), &mut |_| Ok(()))?;
    Ok(
        serde_json::json!({"schema":prepared.manifest["schema"],"songs":prepared.manifest["songs"].as_array().unwrap().len(),"output":output}),
    )
}

#[cfg(feature = "native")]
fn store_command(
    command: &str,
    root: &std::path::Path,
) -> ddi_danoni_export::Result<serde_json::Value> {
    use ddi_danoni_export::managed_store::Store;
    let mut store = if command == "create-store" {
        Store::create(root)?
    } else {
        Store::open(root)?
    };
    let value = match command {
        "recover-store" => serde_json::to_value(store.recover()?),
        "verify-store" => serde_json::to_value(store.verify()?),
        _ => serde_json::to_value(store.catalog()?),
    };
    value.map_err(|e| Error {
        code: "output-json",
        message: e.to_string(),
    })
}
#[cfg(feature = "native")]
fn install_import(
    ready: &str,
    root: &str,
    policy: &str,
) -> ddi_danoni_export::Result<serde_json::Value> {
    use ddi_danoni_export::{import_host as host, managed_store::Store, prepared_import};
    let policy = match policy {
        "reject" => host::DuplicatePolicy::Reject,
        "keep-existing" => host::DuplicatePolicy::KeepExisting,
        "replace" => host::DuplicatePolicy::Replace,
        _ => {
            return Err(Error {
                code: "invalid-duplicate-policy",
                message: policy.into(),
            });
        }
    };
    let import = prepared_import::load_ready(std::path::Path::new(ready), &mut |_| Ok(()))?;
    let mut store = Store::open(std::path::Path::new(root))?;
    let plan = host::plan(&import, &store.catalog()?.snapshot(), policy)?;
    let result = host::install(&import, &plan, &mut store, &mut |_| Ok(()))?;
    serde_json::to_value(result).map_err(|e| Error {
        code: "output-json",
        message: e.to_string(),
    })
}
