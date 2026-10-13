//! Deterministic collection ZIPs and checked offline generations.
use crate::{
    Error, Result,
    assets::{Artifact, relative_path},
};
use std::{
    collections::BTreeSet,
    io::{Seek, Write},
};

/// A fixed timestamp and sorted names make repeated archives reproducible.
/// The host owns the sink; this function never opens files or runs work code.
pub fn write_zip<W: Write + Seek>(sink: W, files: &[Artifact]) -> Result<W> {
    let mut ordered = files.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.path.cmp(&b.path));
    let mut seen = BTreeSet::new();
    let mut zip = zip::ZipWriter::new(sink);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    for file in ordered {
        relative_path(&file.path)?;
        if !seen.insert(&file.path) {
            return Err(Error::new("duplicate-zip-path", &file.path));
        }
        zip.start_file(&file.path, options)
            .map_err(|e| Error::new("zip", e.to_string()))?;
        zip.write_all(&file.bytes)
            .map_err(|e| Error::new("zip", e.to_string()))?;
    }
    zip.finish().map_err(|e| Error::new("zip", e.to_string()))
}

#[cfg(feature = "native")]
mod filesystem {
    use super::*;
    use crate::{
        artwork::{Role, TitleRenderer},
        assets::*,
        native::{DirectorySource, bounded_file, external, io},
        work::*,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs,
        io::{Read, Write},
        path::{Path, PathBuf},
    };
    use unicode_normalization::UnicodeNormalization;
    const FILE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    pub struct Inventory {
        pub path: String,
        pub sha256: String,
        pub bytes: u64,
    }
    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct Collection {
        pub id: String,
        pub title: String,
        pub artist: String,
    }
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct PackageJob {
        pub schema: String,
        pub generation: PathBuf,
        pub roots: BTreeMap<String, PathBuf>,
        pub font: Binding,
        pub collections: Vec<Collection>,
    }
    fn invalid(s: impl Into<String>) -> Error {
        Error::new("invalid-generation", s)
    }
    pub(crate) fn root(path: &Path) -> Result<PathBuf> {
        let p = path.canonicalize().map_err(io)?;
        external(&p)?;
        if !p.is_dir() {
            return Err(invalid("Need directory"));
        }
        Ok(p)
    }
    fn walk(root: &Path, at: &Path, depth: usize, out: &mut Vec<String>) -> Result<()> {
        if depth > 64 {
            return Err(invalid("Tree depth exceeds 64"));
        }
        for entry in fs::read_dir(at).map_err(io)? {
            let e = entry.map_err(io)?;
            let ty = e.file_type().map_err(io)?;
            if ty.is_symlink() {
                return Err(invalid("Symlinks are not accepted"));
            }
            if ty.is_dir() {
                walk(root, &e.path(), depth + 1, out)?;
            } else if ty.is_file() {
                let p = e
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .ok_or_else(|| invalid("Non-UTF-8 path"))?
                    .to_owned();
                relative_path(&p)?;
                out.push(p);
                if out.len() > 100_000 {
                    return Err(invalid("Too many files"));
                }
            } else {
                return Err(invalid("Special files are not accepted"));
            }
        }
        Ok(())
    }
    pub(crate) fn files(root: &Path) -> Result<Vec<String>> {
        let mut v = Vec::new();
        walk(root, root, 0, &mut v)?;
        v.sort();
        Ok(v)
    }
    pub(crate) fn digest(
        path: &Path,
        control: &mut impl FnMut(Progress) -> Result<()>,
    ) -> Result<(String, u64)> {
        let mut f = fs::File::open(path).map_err(io)?;
        let mut h = Sha256::new();
        let mut bytes = 0;
        let mut buf = [0u8; 65536];
        loop {
            let n = f.read(&mut buf).map_err(io)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            if bytes > FILE_LIMIT {
                return Err(invalid("File exceeds 2 GiB"));
            }
            h.update(&buf[..n]);
            control(Progress {
                stage: Stage::ValidateSource,
                completed: bytes as usize,
                total: 0,
            })?;
        }
        Ok((
            h.finalize().iter().map(|b| format!("{b:02x}")).collect(),
            bytes,
        ))
    }
    pub(crate) fn inventory(
        root: &Path,
        control: &mut impl FnMut(Progress) -> Result<()>,
    ) -> Result<Vec<Inventory>> {
        files(root)?
            .into_iter()
            .filter(|p| p != "manifest.json")
            .map(|p| {
                let (sha256, bytes) = digest(&root.join(&p), control)?;
                Ok(Inventory {
                    path: p,
                    sha256,
                    bytes,
                })
            })
            .collect()
    }
    pub(crate) fn document(path: &Path) -> Result<Value> {
        json(&bounded_file(path, JSON_LIMIT)?)
    }
    pub(crate) fn save(path: &Path, value: &impl Serialize) -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(io)?;
        serde_json::to_writer_pretty(&mut f, value).map_err(|e| invalid(e.to_string()))?;
        f.write_all(b"\n").map_err(io)?;
        f.sync_all().map_err(io)?;
        Ok(())
    }
    struct Archived<'a> {
        source: &'a DirectorySource,
        manifest: &'a Value,
    }
    impl AssetSource for Archived<'_> {
        fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
            let key = format!("{ns}:{path}");
            let p = self.manifest["inputArtifacts"][&key]
                .as_str()
                .ok_or_else(|| invalid(format!("Unbound archived input: {key}")))?;
            self.source.read("work", p, max)
        }
    }
    /// Rebuild every song from its bundled source/review/media evidence, compare
    /// every artifact, then use the game's parser on the emitted full song.
    pub(crate) fn validate_works(
        root: &Path,
        generation: &Value,
        control: &mut impl FnMut(Progress) -> Result<()>,
    ) -> Result<(usize, usize)> {
        let works = generation["works"]
            .as_array()
            .ok_or_else(|| invalid("Missing works"))?;
        if works.is_empty() || works.len() > 2048 {
            return Err(invalid("Need bounded work list"));
        }
        let mut ids = BTreeSet::new();
        let (mut songs, mut charts) = (0, 0);
        for row in works {
            let id = text(row, "id")?;
            component(id)?;
            if !ids.insert(id) {
                return Err(invalid("Repeated work id"));
            }
            let path = text(row, "manifest")?;
            if path != format!("works/{id}/manifest.json") {
                return Err(invalid("Wrong work manifest path"));
            }
            let dir = root.join("works").join(id);
            let source = DirectorySource::new(BTreeMap::from([("work".into(), dir.clone())]))?;
            let manifest: Value = json(&source.read("work", "manifest.json", JSON_LIMIT)?)?;
            if manifest["schema"] != "ddi-danoni-prepared-work/1"
                || manifest["sourceManifestSha256"] != row["sourceManifestSha256"]
            {
                return Err(invalid("Wrong work identity"));
            }
            let request: WorkRequest = serde_json::from_value(manifest["request"].clone())
                .map_err(|e| invalid(e.to_string()))?;
            let prepared = prepare_work(
                &request,
                &Archived {
                    source: &source,
                    manifest: &manifest,
                },
                control,
            )?;
            if prepared.manifest != manifest {
                return Err(invalid(format!("Work metadata changed: {id}")));
            }
            let mut expected = BTreeSet::from(["manifest.json".to_owned()]);
            for file in &prepared.files {
                expected.insert(file.path.clone());
                let bytes = source.read("work", &file.path, file.bytes.len())?;
                if bytes.as_slice() != file.bytes.as_slice() {
                    return Err(invalid(format!("Artifact changed: {id}/{}", file.path)));
                }
            }
            if expected != files(&dir)?.into_iter().collect() {
                return Err(invalid("Missing or extra work files"));
            }
            for song in manifest["songs"]
                .as_array()
                .ok_or_else(|| invalid("Missing songs"))?
            {
                let body = source.read("work", text(song, "simfile")?, JSON_LIMIT)?;
                let parsed = ddi_chart::formats::sm::parse_ssc(
                    std::str::from_utf8(&body).map_err(|_| invalid("Non-UTF-8 SSC"))?,
                )
                .map_err(|e| invalid(e.to_string()))?;
                let reports = song["charts"]
                    .as_array()
                    .ok_or_else(|| invalid("Missing chart reports"))?;
                if parsed.charts.len() != reports.len()
                    || parsed.title != song["title"]
                    || parsed.artist != song["artist"]
                {
                    return Err(invalid("SSC headers/charts differ"));
                }
                for (chart, report) in parsed.charts.iter().zip(reports) {
                    if chart.layout != format!("danoni-{}", text(report, "mode")?) {
                        return Err(invalid("Wrong SSC layout"));
                    }
                    let padding = report["paddingTicks"]
                        .as_i64()
                        .ok_or_else(|| invalid("Missing pre-roll"))?;
                    let mut actual = BTreeMap::new();
                    let timing = chart.timing(&parsed);
                    for n in &chart.notes {
                        let kind = match n.kind {
                            ddi_chart::NoteKind::Tap => "tap",
                            ddi_chart::NoteKind::HoldHead { end } => {
                                actual.insert(
                                    (n.lane as usize, end.0, "hold-tail"),
                                    timing.seconds_at(end),
                                );
                                "hold-head"
                            }
                            _ => return Err(invalid("Unexpected note kind")),
                        };
                        actual.insert((n.lane as usize, n.tick.0, kind), timing.seconds_at(n.tick));
                    }
                    let endpoints = report["endpoints"]
                        .as_array()
                        .ok_or_else(|| invalid("Missing endpoints"))?;
                    if actual.len() != endpoints.len() {
                        return Err(invalid("Note count differs"));
                    }
                    for e in endpoints {
                        let lane = e["lane"]
                            .as_u64()
                            .ok_or_else(|| invalid("Invalid endpoint lane"))?
                            as usize;
                        let tick = e["tick"]
                            .as_i64()
                            .ok_or_else(|| invalid("Invalid endpoint tick"))?
                            .checked_add(padding)
                            .ok_or_else(|| invalid("Tick overflow"))?;
                        let t = actual
                            .remove(&(lane, tick, text(e, "kind")?))
                            .ok_or_else(|| invalid("Missing SSC endpoint"))?;
                        let source = e["sourceSeconds"]
                            .as_f64()
                            .ok_or_else(|| invalid("Invalid source seconds"))?;
                        let export = e["exportSeconds"]
                            .as_f64()
                            .ok_or_else(|| invalid("Invalid exported seconds"))?;
                        if !t.is_finite()
                            || (t - export).abs() > 1e-9
                            || (t - source).abs() > 0.009 + 1e-9
                        {
                            return Err(invalid("SSC endpoint timing differs"));
                        }
                    }
                }
                songs += 1;
                charts += reports.len();
            }
        }
        if generation["songs"] != songs || generation["charts"] != charts {
            return Err(invalid("Generation counts differ"));
        }
        Ok((songs, charts))
    }
    fn component(s: &str) -> Result<()> {
        relative_path(s)?;
        if s.contains('/') || s.len() > 180 {
            return Err(invalid("Invalid file component"));
        }
        Ok(())
    }
    struct Partial {
        path: PathBuf,
        armed: bool,
    }
    impl Drop for Partial {
        fn drop(&mut self) {
            if self.armed {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }
    /// Complete and validate a fresh self-contained collection generation.
    pub fn package_generation(
        job: &PackageJob,
        output: &Path,
        control: &mut impl FnMut(Progress) -> Result<()>,
    ) -> Result<Value> {
        if job.schema != "ddi-danoni-package-job/1" {
            return Err(invalid("Unsupported package job"));
        }
        let source = root(&job.generation)?;
        let generation = document(&source.join("manifest.json"))?;
        if generation["schema"] != "ddi-danoni-rust-offline-generation/1" {
            return Err(invalid("Need an offline generation"));
        }
        validate_works(&source, &generation, control)?;
        let fonts = DirectorySource::new(job.roots.clone())?;
        let mut inputs = Inputs::new(&fonts);
        let font = inputs.checked(&job.font, 32 * 1024 * 1024)?;
        let renderer = TitleRenderer::new(&font)?;
        let mut collections = BTreeMap::new();
        let mut titles = BTreeSet::new();
        for c in &job.collections {
            component(&c.id)?;
            component(&c.title)?;
            if ["works", "manifest.json", "source-generation.json"].contains(&c.id.as_str())
                || !titles.insert(c.title.nfkc().collect::<String>().to_lowercase())
                || collections.insert(c.id.clone(), c).is_some()
            {
                return Err(invalid("Duplicate collection"));
            }
        }
        if collections.is_empty() || collections.len() > 64 {
            return Err(invalid("Need 1..64 collections"));
        }
        let parent = output
            .parent()
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map_err(io)?;
        external(&parent)?;
        let name = output
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("Missing output name"))?;
        component(name)?;
        let output = parent.join(name);
        if output.starts_with(&source) || source.starts_with(&output) {
            return Err(invalid("Overlapping generation paths"));
        }
        fs::create_dir(&output).map_err(io)?;
        let mut guard = Partial {
            path: output.clone(),
            armed: true,
        };
        let before = inventory(&source, control)?;
        for row in &before {
            if row.path.starts_with("works/") {
                let dest = output.join(&row.path);
                fs::create_dir_all(dest.parent().unwrap()).map_err(io)?;
                fs::copy(source.join(&row.path), dest).map_err(io)?;
            }
        }
        save(&output.join("source-generation.json"), &generation)?;
        let mut counts = BTreeMap::<String, (usize, usize)>::new();
        let mut song_folders = BTreeSet::new();
        for row in generation["works"].as_array().unwrap() {
            let c = collections
                .get(text(row, "collection")?)
                .ok_or_else(|| invalid("Missing collection configuration"))?;
            let id = text(row, "id")?;
            let work = document(&source.join(text(row, "manifest")?))?;
            for song in work["songs"].as_array().unwrap() {
                let sid = text(song, "id")?;
                component(sid)?;
                let folder = output
                    .join(&c.id)
                    .join("files")
                    .join(&c.title)
                    .join(format!("{id}--{sid}"));
                if !song_folders.insert(
                    folder
                        .to_string_lossy()
                        .nfkc()
                        .collect::<String>()
                        .to_lowercase(),
                ) {
                    return Err(invalid("Ambiguous collection song directory"));
                }
                fs::create_dir_all(&folder).map_err(io)?;
                for key in ["simfile", "music", "banner"] {
                    if song[key].is_null() {
                        continue;
                    }
                    let path = text(song, key)?;
                    relative_path(path)?;
                    let name = Path::new(path).file_name().unwrap();
                    fs::copy(output.join("works").join(id).join(path), folder.join(name))
                        .map_err(io)?;
                }
                let n = counts.entry(c.id.clone()).or_default();
                n.0 += 1;
                n.1 += song["charts"].as_array().unwrap().len();
            }
        }
        let mut results = Vec::new();
        for (id, c) in &collections {
            let (ns, nc) = counts
                .get(id)
                .copied()
                .ok_or_else(|| invalid("Empty collection"))?;
            let dir = output.join(id);
            let pack = dir.join("files").join(&c.title);
            let mut proofs = Vec::new();
            for role in [Role::Pack, Role::Cover] {
                let (svg, png, proof) = renderer.card(&c.title, &c.artist, role)?;
                let name = if matches!(role, Role::Pack) {
                    "banner"
                } else {
                    "cover"
                };
                fs::write(pack.join(format!("{name}.png")), png).map_err(io)?;
                fs::write(dir.join(format!("{name}.svg")), svg).map_err(io)?;
                proofs.push(proof);
            }
            save(
                &dir.join("artwork.json"),
                &json!({"schema":"ddi-danoni-collection-artwork/1","template":"plain-title-rust/1","fontSha256":renderer.font_sha256,"fontDistributed":false,"records":proofs}),
            )?;
            let paths = files(&dir.join("files"))?;
            let f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(format!("{}.zip", c.title)))
                .map_err(io)?;
            let mut z = zip::ZipWriter::new(f);
            let opt = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(zip::DateTime::default())
                .unix_permissions(0o644);
            for path in paths {
                control(Progress {
                    stage: Stage::Assemble,
                    completed: 0,
                    total: 0,
                })?;
                z.start_file(&path, opt)
                    .map_err(|e| invalid(e.to_string()))?;
                let mut from = fs::File::open(dir.join("files").join(&path)).map_err(io)?;
                std::io::copy(&mut from, &mut z).map_err(io)?;
            }
            z.finish()
                .map_err(|e| invalid(e.to_string()))?
                .sync_all()
                .map_err(io)?;
            results.push(json!({"id":id,"title":c.title,"songs":ns,"charts":nc,"files":format!("{id}/files"),"zip":format!("{id}/{}.zip",c.title)}));
        }
        if inventory(&source, control)? != before
            || document(&source.join("manifest.json"))? != generation
        {
            return Err(invalid("Source generation changed during packaging"));
        }
        let mut result = generation.clone();
        result["schema"] = json!("ddi-danoni-rust-packaged-generation/1");
        result["collections"] = json!(results);
        result["artifacts"] = serde_json::to_value(inventory(&output, control)?)
            .map_err(|e| invalid(e.to_string()))?;
        result["validation"] = json!({"preparedSourcesRebuilt":true,"gameParserEndpointsChecked":true,"maximumTimingErrorMs":9,"suppliedMediaPolicyUnchanged":true});
        save(&output.join("manifest.json"), &result)?;
        verify_generation(&output, control)?;
        guard.armed = false;
        Ok(result)
    }
    pub fn verify_generation(
        path: &Path,
        control: &mut impl FnMut(Progress) -> Result<()>,
    ) -> Result<Value> {
        let root = root(path)?;
        let doc = document(&root.join("manifest.json"))?;
        if doc["schema"] != "ddi-danoni-rust-packaged-generation/1" {
            return Err(invalid("Unsupported sealed generation"));
        }
        let wanted: Vec<Inventory> =
            serde_json::from_value(doc["artifacts"].clone()).map_err(|e| invalid(e.to_string()))?;
        if wanted != inventory(&root, control)? {
            return Err(invalid("Generation inventory changed"));
        }
        validate_works(&root, &doc, control)?;
        // ZIP entries must exactly match the adjacent playable files, including
        // names, count and uncompressed bytes; the inventory alone is insufficient.
        let mut used = BTreeSet::new();
        let (mut songs, mut charts) = (0, 0);
        for c in doc["collections"]
            .as_array()
            .ok_or_else(|| invalid("Missing collections"))?
        {
            let id = text(c, "id")?;
            component(id)?;
            let title = text(c, "title")?;
            component(title)?;
            if !used.insert(id) {
                return Err(invalid("Duplicate collection"));
            }
            let relative = format!("{id}/files");
            if c["files"] != relative || c["zip"] != format!("{id}/{title}.zip") {
                return Err(invalid("Invalid collection path"));
            }
            let art = document(&root.join(id).join("artwork.json"))?;
            if art["schema"] != "ddi-danoni-collection-artwork/1"
                || art["template"] != "plain-title-rust/1"
                || art["fontDistributed"] != false
            {
                return Err(invalid("Unexpected collection artwork proof"));
            }
            let proofs = art["records"]
                .as_array()
                .ok_or_else(|| invalid("Missing artwork records"))?;
            if proofs.len() != 2 {
                return Err(invalid("Incomplete collection art"));
            }
            for (role, name) in [(Role::Pack, "banner"), (Role::Cover, "cover")] {
                let proof = proofs
                    .iter()
                    .find(|p| p["role"] == role.name())
                    .ok_or_else(|| invalid("Missing artwork role"))?;
                let png = bounded_file(
                    &root
                        .join(id)
                        .join("files")
                        .join(title)
                        .join(format!("{name}.png")),
                    EVIDENCE_LIMIT,
                )?;
                let svg = bounded_file(&root.join(id).join(format!("{name}.svg")), EVIDENCE_LIMIT)?;
                let [w, h] = role.dimensions();
                if png.len() < 33
                    || &png[..16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"
                    || u32::from_be_bytes(png[16..20].try_into().unwrap()) != w
                    || u32::from_be_bytes(png[20..24].try_into().unwrap()) != h
                    || proof["pngSha256"] != sha256(&png)
                    || proof["svgSha256"] != sha256(&svg)
                    || proof["dimensions"] != json!([w, h])
                    || proof["title"] != title
                {
                    return Err(invalid("Collection artwork differs from its proof"));
                }
            }
            let file_root = root.join(relative);
            let paths = files(&file_root)?;
            let f = fs::File::open(root.join(text(c, "zip")?)).map_err(io)?;
            let mut z = zip::ZipArchive::new(f).map_err(|e| invalid(e.to_string()))?;
            if z.len() != paths.len() {
                return Err(invalid("ZIP coverage differs"));
            }
            for (i, p) in paths.iter().enumerate() {
                let mut entry = z.by_index(i).map_err(|e| invalid(e.to_string()))?;
                if entry.name().map_err(|e| invalid(e.to_string()))?.as_ref() != p
                    || entry.size() > FILE_LIMIT
                {
                    return Err(invalid("ZIP entry differs"));
                }
                let mut local = fs::File::open(file_root.join(p)).map_err(io)?;
                let (mut a, mut b) = ([0; 65536], [0; 65536]);
                loop {
                    control(Progress {
                        stage: Stage::ValidateSource,
                        completed: i,
                        total: paths.len(),
                    })?;
                    let n = entry.read(&mut a).map_err(io)?;
                    local.read_exact(&mut b[..n]).map_err(io)?;
                    if a[..n] != b[..n] {
                        return Err(invalid("ZIP bytes differ"));
                    }
                    if n == 0 {
                        if local.read(&mut b).map_err(io)? != 0 {
                            return Err(invalid("ZIP truncated"));
                        }
                        break;
                    }
                }
            }
            let mut expected =
                BTreeSet::from([format!("{title}/banner.png"), format!("{title}/cover.png")]);
            let (mut ns, mut nc) = (0, 0);
            for row in doc["works"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|w| w["collection"] == id)
            {
                let wid = text(row, "id")?;
                let work = document(&root.join(text(row, "manifest")?))?;
                for s in work["songs"].as_array().unwrap() {
                    ns += 1;
                    nc += s["charts"].as_array().unwrap().len();
                    let sid = text(s, "id")?;
                    for key in ["simfile", "music", "banner"] {
                        if s[key].is_null() {
                            continue;
                        }
                        let p = text(s, key)?;
                        let name = Path::new(p).file_name().unwrap().to_str().unwrap();
                        let target = format!("{title}/{wid}--{sid}/{name}");
                        expected.insert(target.clone());
                        if digest(&file_root.join(&target), control)?
                            != digest(&root.join("works").join(wid).join(p), control)?
                        {
                            return Err(invalid("Collection song differs from verified work"));
                        }
                    }
                }
            }
            if paths.into_iter().collect::<BTreeSet<_>>() != expected
                || c["songs"] != ns
                || c["charts"] != nc
            {
                return Err(invalid("Collection coverage differs"));
            }
            songs += ns;
            charts += nc;
        }
        if doc["songs"] != songs || doc["charts"] != charts {
            return Err(invalid("Collection totals differ"));
        }
        Ok(doc)
    }
    pub fn package_job(
        path: &Path,
        output: &Path,
        control: &mut impl FnMut(Progress) -> Result<()>,
    ) -> Result<Value> {
        let path = path.canonicalize().map_err(io)?;
        external(&path)?;
        let mut job: PackageJob = json(&bounded_file(&path, JSON_LIMIT)?)?;
        if !job.generation.is_absolute() {
            job.generation = path.parent().unwrap().join(&job.generation);
        }
        for p in job.roots.values_mut() {
            if !p.is_absolute() {
                *p = path.parent().unwrap().join(&*p);
            }
        }
        package_generation(&job, output, control)
    }
}
#[cfg(feature = "native")]
pub use filesystem::*;
