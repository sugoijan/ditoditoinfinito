//! Song sources on desktop (`docs/plans/source-plugins.md`): where
//! plugins and what they keep live, `--source-check` (stage 1), and since
//! stage 2 the sources the game holds ([`Sources`]): the host
//! (`ddi_source_host`) with the plugins switched on and "This computer"
//! ([`this_computer`], behind its own switch), the songs they fetch
//! played without importing ([`fetched`]) or imported ([`import`]), and
//! the Find songs page's model ([`find`]). A player with the switch off
//! and no plugin switched on gets no host at all: nothing is started,
//! nothing shows, and the start pays reading the switches' file.
//!
//! - the plugins: the data folder's `plugins/<id>/`;
//! - which are switched on: the configuration folder's `plugins.json`
//!   (`ddi_source_host::switches`);
//! - each plugin's own folders, handed to it: the cache folder's
//!   `plugins/<id>/` and the data folder's `plugin-data/<id>/`;
//! - songs fetched without importing, where memory will not do: the
//!   cache folder's `sources/`, emptied at each start of the window's run
//!   ([`clear_store`]) and when the store is dropped;
//! - songs imported: the data folder's `sources/library/` ([`import`]);
//! - This computer's switch and folders: the desktop's own setting
//!   [`SETTING_KEY`] (never the settings the web shares).

pub mod fetched;
pub mod find;
pub mod import;
pub mod this_computer;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ddi_platform::storage::{self, Area};
use serde::{Deserialize, Serialize};

use ddi_source_host::installed::Found;
use ddi_source_host::store::{Store, StoreLimits};
use ddi_source_host::switches::Switches;
use ddi_source_host::{Event, Host, HostConfig};

/// This computer's switch and folders, the desktop's own setting.
pub const SETTING_KEY: &str = "ddi.desktop.sources.v1";

/// "This computer": on (off by default, `--sources on|off`) and the
/// folders it searches (chosen on the Find songs page).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    #[serde(default)]
    pub this_computer: bool,
    #[serde(default)]
    pub folders: Vec<PathBuf>,
}

impl Setting {
    pub fn load() -> Setting {
        storage::load(Area::Local, SETTING_KEY).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = storage::save(Area::Local, SETTING_KEY, self);
    }
}

/// `--sources on|off`: This computer's switch, written at once.
pub fn save_switch(on: bool) {
    let mut s = Setting::load();
    if s.this_computer != on {
        s.this_computer = on;
        s.save();
        let _ = storage::flush();
    }
}

/// The song sources the game holds: the host with This computer (where
/// it is on) and the plugins switched on.
pub struct Sources {
    pub host: Host,
    pub this_computer: Option<Arc<this_computer::ThisComputer>>,
}

impl Sources {
    /// The sources at the window's start: none where This computer is
    /// off and no plugin is switched on (nothing else is read then).
    /// `wake` wakes the event loop with their news.
    pub fn start(wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Option<Sources> {
        let setting = Setting::load();
        let on = switches_path()
            .map(|p| Switches::read(&p).on)
            .unwrap_or_default();
        if !setting.this_computer && on.is_empty() {
            return None;
        }
        let installed: Vec<_> = plugins_dir()
            .map(|d| ddi_source_host::discover(&d))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|f| match f {
                Found::Plugin(p) => Some(p),
                Found::Refused { dir, problem } => {
                    eprintln!("sources: {} not taken: {problem:?}", dir.display());
                    None
                }
            })
            .collect();
        let plugin_on = installed.iter().any(|p| on.contains(p.id()));
        if !setting.this_computer && !plugin_on {
            return None;
        }
        let store = Store::open(store_dir()?, StoreLimits::default()).shared();
        Some(Sources::with(
            host_config()?,
            installed,
            &on,
            store,
            wake,
            setting.this_computer.then_some(setting.folders),
        ))
    }

    /// The sources of `installed` (those in `on` switched on), This
    /// computer on `folders` where given.
    pub fn with(
        config: HostConfig,
        installed: Vec<ddi_source_host::Installed>,
        on: &BTreeSet<String>,
        store: ddi_source_host::SharedStore,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
        folders: Option<Vec<PathBuf>>,
    ) -> Sources {
        fetched::set_store(Some(store.clone()));
        let mut host = Host::new(config, installed, on, store, wake);
        let this_computer = folders.map(|f| Arc::new(this_computer::ThisComputer::new(f)));
        if let Some(t) = &this_computer {
            host.add_builtin(t.clone());
        }
        Sources {
            host,
            this_computer,
        }
    }

    /// The sources a player may ask now, in the page's order: This
    /// computer first, then the plugins switched on (by their names).
    pub fn offered(&self) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        if self.this_computer.is_some() {
            out.push((this_computer::ID.to_string(), None));
        }
        for p in self.host.plugins() {
            if self.host.is_on(p.id()) {
                let language = ddi_l10n::current().tag;
                out.push((
                    p.id().to_string(),
                    Some(p.manifest.name.get(language).to_string()),
                ));
            }
        }
        out
    }
}

/// The plugins' folder, in the data folder.
pub fn plugins_dir() -> Option<PathBuf> {
    crate::paths::data_dir().map(|d| d.join("plugins"))
}

/// The switches' file, in the configuration folder.
pub fn switches_path() -> Option<PathBuf> {
    crate::paths::config_dir().map(|c| c.join(ddi_source_host::switches::FILE))
}

/// Fetched songs' folder, in the cache folder.
pub fn store_dir() -> Option<PathBuf> {
    crate::paths::cache_dir().map(|c| c.join("sources"))
}

/// The host's configuration for this app's folders, or `None` where it
/// has none.
pub fn host_config() -> Option<HostConfig> {
    let mut config = HostConfig::new(
        crate::paths::cache_dir()?.join("plugins"),
        crate::paths::data_dir()?.join("plugin-data"),
    );
    config.app_version = env!("CARGO_PKG_VERSION").into();
    config.language = ddi_l10n::current().tag.to_string();
    config.legacy = crate::library::legacy_text;
    Some(config)
}

/// Fetched songs a run left (one that ended without its exit): deleted
/// by the folder's one app at its start, as rooms' songs are.
pub fn clear_store() {
    let Some(dir) = store_dir() else {
        return;
    };
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => eprintln!("sources: songs fetched before deleted ({})", dir.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!("sources: deleting songs fetched before ({}): {e}", dir.display()),
    }
}

/// `--source-check`: lists the installed plugins (their manifests, their
/// executables' SHA-256, whether they are switched on), then starts each,
/// switched on or not (the person asked), and says what it declared, as
/// one JSON document on stdout. No window, no sound, nothing fetched. The
/// exit status: 0 when every plugin found started, 1 otherwise.
pub fn check() -> i32 {
    let (Some(plugins), Some(mut config)) = (plugins_dir(), host_config()) else {
        eprintln!("source check: no data folder");
        return 1;
    };
    let on = switches_path()
        .map(|p| Switches::read(&p).on)
        .unwrap_or_default();
    let found = ddi_source_host::discover(&plugins);
    let mut listed = Vec::new();
    let mut installed = Vec::new();
    let mut ok = true;
    for f in found {
        match f {
            Found::Plugin(p) => {
                let m = &p.manifest;
                listed.push(serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "publisher": m.publisher,
                    "version": m.version,
                    "homepage": m.homepage,
                    "protocol": m.protocol,
                    "capabilities": m.capabilities,
                    "inputs": m.inputs,
                    "rooms": m.rooms,
                    "attribution": m.attribution,
                    "dir": p.dir,
                    "executable": p.executable,
                    "sha256": p.sha256,
                    "on": on.contains(&m.id),
                }));
                installed.push(p);
            }
            Found::Refused { dir, problem } => {
                ok = false;
                listed.push(serde_json::json!({
                    "dir": dir,
                    "refused": format!("{problem:?}"),
                }));
            }
        }
    }
    // A store of its own: the check runs beside an open app, whose
    // `sources/` it must not empty.
    let Some(scratch) = crate::paths::cache_dir()
        .map(|c| c.join(format!("sources-check-{}", std::process::id())))
    else {
        return 1;
    };
    config.timeouts.idle = Duration::from_secs(5);
    let ids: BTreeSet<String> = installed.iter().map(|p| p.id().to_string()).collect();
    let store = Store::open(scratch, StoreLimits::default()).shared();
    let mut host = Host::new(config.clone(), installed, &ids, store, None);
    let mut started = BTreeSet::new();
    for id in &ids {
        if host.start(id) {
            started.insert(id.clone());
        }
    }
    let mut reports = std::collections::BTreeMap::new();
    let deadline = Instant::now() + config.timeouts.hello + Duration::from_secs(5);
    while reports.len() < started.len() && Instant::now() < deadline {
        match host.wait_event(Duration::from_millis(100)) {
            Some(Event::Ready { plugin, accepted }) => {
                let filters: Vec<&str> = accepted.filters.iter().map(|f| f.id.as_str()).collect();
                reports.insert(
                    plugin,
                    serde_json::json!({
                        "ready": true,
                        "protocol": accepted.protocol,
                        "capabilities": accepted.capabilities,
                        "inputs": accepted.inputs,
                        "filters": filters,
                    }),
                );
            }
            Some(Event::Stopped {
                plugin,
                why,
                diagnostics,
                ..
            }) => {
                reports.entry(plugin).or_insert_with(|| {
                    serde_json::json!({
                        "ready": false,
                        "stopped": format!("{why:?}"),
                        "diagnostics": diagnostics,
                    })
                });
            }
            _ => {}
        }
    }
    host.shutdown();
    for entry in &mut listed {
        let Some(id) = entry.get("id").and_then(|i| i.as_str()).map(str::to_string) else {
            continue;
        };
        let report = reports.remove(&id).unwrap_or_else(|| {
            serde_json::json!({"ready": false, "stopped": "no answer in time"})
        });
        ok &= report["ready"] == true;
        entry["started"] = report;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "folder": plugins,
            "platform": ddi_source_host::installed::platform(),
            "protocols": ddi_source_host::protocol::VERSIONS,
            "plugins": listed,
        }))
        .unwrap_or_default()
    );
    i32::from(!ok)
}
