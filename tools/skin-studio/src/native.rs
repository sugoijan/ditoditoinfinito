//! Native services shared by the two embedded editors. Dialog futures are created on the UI
//! thread and awaited off it, keeping AppKit's modal loop outside winit's event handler.
use super::*;
use photocraft_ui_egui::{FileDialogAnswer as Answer, FileDialogRequest as Request};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, mpsc};
use vectorcraft_ui_egui::picks::PickRequest;

type PathsFuture = Pin<Box<dyn Future<Output = Vec<String>> + Send>>;
type Picker = Box<dyn FnMut(PickRequest) -> PathsFuture>;

#[derive(Clone, Copy)]
pub(super) enum Action {
    Open,
    Skin,
    Export,
    TextCopy,
}
pub(super) struct Runtime {
    pub pending: bool,
    tx: mpsc::Sender<(Action, Vec<String>)>,
    rx: mpsc::Receiver<(Action, Vec<String>)>,
    pub protected: Arc<Mutex<Vec<PathBuf>>>,
    picker: Picker,
    vector_services: Box<dyn Fn() -> vectorcraft_ui_egui::Services>,
}

pub(super) fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Wake(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(v) => return v,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}

fn picks(dialog: rfd::AsyncFileDialog, request: PickRequest) -> PathsFuture {
    use vectorcraft_ui_egui::FilePick;
    let configure = |mut d: rfd::AsyncFileDialog, p: FilePick| {
        if let Some(folder) = p.folder {
            d = d.set_directory(folder);
        }
        if !p.name.is_empty() {
            let path = Path::new(&p.name);
            if let Some(name) = path.file_name() {
                d = d.set_file_name(name.to_string_lossy());
            }
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                d = d.set_directory(parent);
            }
        }
        for (name, exts) in p.filters {
            d = d.add_filter(name, exts);
        }
        d
    };
    let path = |f: rfd::FileHandle| f.path().to_string_lossy().into_owned();
    match request {
        PickRequest::Open(p) => {
            let f = configure(dialog, p).pick_file();
            Box::pin(async move { f.await.into_iter().map(path).collect() })
        }
        PickRequest::OpenMany => {
            let f = dialog.pick_files();
            Box::pin(async move { f.await.unwrap_or_default().into_iter().map(path).collect() })
        }
        PickRequest::Save(p) => {
            let f = configure(dialog, p).save_file();
            Box::pin(async move { f.await.into_iter().map(path).collect() })
        }
        PickRequest::Folder => {
            let f = dialog.pick_folder();
            Box::pin(async move { f.await.into_iter().map(path).collect() })
        }
    }
}

/// Resolve the nearest existing ancestor before comparing, including writes to new files.
pub(super) fn resolved(path: &Path) -> Result<PathBuf, String> {
    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
    if path.exists() {
        return path.canonicalize().map_err(|e| e.to_string());
    }
    let name = path.file_name().ok_or("Invalid output path")?;
    if name == "." || name == ".." || path.is_symlink() {
        return Err("Invalid output path".into());
    }
    Ok(resolved(path.parent().ok_or("Output needs a parent")?)?.join(name))
}

pub(super) fn write(
    path: &str,
    bytes: &[u8],
    protected: &Mutex<Vec<PathBuf>>,
) -> Result<(), String> {
    let path = resolved(Path::new(path))?;
    if protected
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|root| path.starts_with(root))
    {
        return Err("Choose a working folder outside the source noteskin. Save project preserves edits; Export skin writes a new runtime copy.".into());
    }
    vectorcraft_engine::file_access::check_write(&path.to_string_lossy())?;
    photocraft_format::atomic_write(&path, bytes).map_err(|e| e.to_string())
}

impl Runtime {
    pub fn install(
        app: &mut host::SkinCraft,
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let protected = Arc::new(Mutex::new(Vec::new()));
        let parent = cc.winit_window().cloned();
        let picker: Picker = Box::new(move |request| {
            let mut dialog = rfd::AsyncFileDialog::new().set_title("SkinCraft");
            if let Some(parent) = &parent {
                dialog = dialog.set_parent(parent.as_ref());
            }
            picks(dialog, request)
        });
        let parent = cc.winit_window().cloned();
        let ctx = cc.egui_ctx.clone();
        let guard = protected.clone();
        let routes = tx.clone();
        let vector_services = Box::new(move || {
            let parent = parent.clone();
            let ctx = ctx.clone();
            let guard1 = guard.clone();
            let guard2 = guard.clone();
            let routes = routes.clone();
            vectorcraft_ui_egui::Services {
                // VectorCraft checks these capability slots before invoking start_pick.
                // Interactive requests use the asynchronous service below; an out-of-entry
                // fallback cancels safely rather than running a modal loop inside winit.
                pick_open: Some(Box::new(|_| None)),
                pick_save: Some(Box::new(|_| None)),
                pick_open_multi: Some(Box::new(Vec::new)),
                pick_folder: Some(Box::new(|| None)),
                start_pick: Some(Box::new(move |request| {
                    let mut dialog = rfd::AsyncFileDialog::new().set_title("SkinCraft");
                    if let Some(parent) = &parent {
                        dialog = dialog.set_parent(parent.as_ref());
                    }
                    let route = matches!(&request, PickRequest::Open(p) if p.filters == vectorcraft_engine::cmd::fileio::open_filters().collect::<Vec<_>>());
                    let future = picks(dialog, request);
                    let routes = routes.clone();
                    let (tx, rx) = mpsc::channel();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let paths = block_on(future);
                        let answer = if route {
                            let _ = routes.send((Action::Open, paths));
                            Vec::new()
                        } else {
                            paths
                        };
                        let _ = tx.send(answer);
                        ctx.request_repaint();
                    });
                    Some(rx)
                })),
                dialogs_are_sheets: cfg!(target_os = "macos"),
                read: Some(Box::new(|p| {
                    vectorcraft_engine::file_access::check_read(p)?;
                    std::fs::read(p).map_err(|e| e.to_string())
                })),
                write: Some(Box::new(move |p, bytes| write(p, bytes, &guard1))),
                write_shared: Some(Arc::new(move |p, bytes| write(p, bytes, &guard2))),
                exists: Some(Box::new(|p| Path::new(p).exists())),
                ..Default::default()
            }
        });
        let guard = protected.clone();
        let routes = tx.clone();
        let mut services = photocraft_ui_egui::Services {
            import: Some(Box::new(|name, bytes, depth| {
                photocraft_io::import_with_svg_group_depth(name, bytes, depth)
                    .map(|r| (r.document, r.warnings))
                    .map_err(|e| e.to_string())
            })),
            export: Some(Box::new(|doc, path, settings| {
                photocraft_automation::files::save_bytes(doc, path, None, &settings.io_options())
                    .map_err(|e| e.to_string())
            })),
            write: Some(Box::new(move |p, bytes| write(p, bytes, &guard))),
            file_stamp: Some(Box::new(|p| {
                std::fs::metadata(p)
                    .ok()
                    .map(|m| photocraft_ui_egui::FileStamp {
                        len: m.len(),
                        modified: m.modified().ok(),
                    })
            })),
            file_dialog: Some(Box::new(move |request, parent, reply| {
                let route = matches!(
                    &request,
                    Request::Open {
                        multiple: true,
                        extensions: None,
                        ..
                    }
                );
                let routes = routes.clone();
                let mut dialog = rfd::AsyncFileDialog::new().set_title("SkinCraft");
                if let Some(parent) = parent {
                    dialog = dialog.set_parent(parent);
                }
                let (pick, kind) = match request {
                    Request::Open {
                        multiple,
                        initial_dir,
                        extensions,
                    } => {
                        if let Some(dir) = initial_dir {
                            dialog = dialog.set_directory(dir);
                        }
                        if let Some(exts) = extensions {
                            dialog = dialog.add_filter("Supported files", &exts);
                        }
                        (
                            if multiple {
                                PickRequest::OpenMany
                            } else {
                                PickRequest::Open(Default::default())
                            },
                            0,
                        )
                    }
                    Request::Folder { initial_dir } => {
                        if let Some(dir) = initial_dir {
                            dialog = dialog.set_directory(dir);
                        }
                        (PickRequest::Folder, 1)
                    }
                    Request::Save { suggested } => (
                        PickRequest::Save(vectorcraft_ui_egui::FilePick::named(&suggested)),
                        2,
                    ),
                };
                let future = picks(dialog, pick);
                std::thread::spawn(move || {
                    let paths = block_on(future);
                    if route {
                        let _ = routes.send((Action::Open, paths));
                        reply.send(None);
                        return;
                    }
                    let answer = match (kind, paths.first()) {
                        (_, None) => None,
                        (1, Some(p)) => Some(Answer::FolderTo(p.clone())),
                        (2, Some(p)) => Some(Answer::SaveTo(p.clone())),
                        _ => Some(Answer::Paths(paths)),
                    };
                    reply.send(answer);
                });
            })),
            automation_command: Some(Box::new(|id, params| {
                photocraft_automation::workspace::authorize_desktop_engine_command(id, params)
                    .map_err(|e| e.to_string())
            })),
            automation_authorize: Some(
                photocraft_automation::workspace::authorize_desktop_engine_step,
            ),
            ..Default::default()
        };
        if let Some(home) = &app.panel.home {
            let prefs = home.join("photocraft-prefs.json");
            let out = prefs.clone();
            services.load_prefs = Some(Box::new(move || std::fs::read_to_string(&prefs).ok()));
            services.save_prefs = Some(Box::new(move |text| {
                workspace::atomic(&out, text.as_bytes())
            }));
        }
        let session = std::mem::replace(&mut app.photo.session, photocraft_engine::Session::new());
        app.photo = PhotocraftApp::new(session, services);
        app.photo.set_wgpu(app.panel.rs.clone());
        app.photo.ui.panels.menu_bar = false;
        let mut runtime = Self {
            pending: false,
            tx,
            rx,
            protected,
            picker,
            vector_services,
        };
        runtime.sync(app);
        Ok(runtime)
    }

    pub fn sync(&mut self, app: &mut host::SkinCraft) {
        if let Ok(mut roots) = self.protected.lock() {
            *roots = app
                .panel
                .project
                .as_ref()
                .map(|p| {
                    p.chain
                        .iter()
                        .filter_map(|i| p.skins.get(*i))
                        .map(|s| s.dir.clone())
                        .collect()
                })
                .unwrap_or_default();
        }
        if let Some(vector) = &mut app.panel.vector
            && vector.app.services.read.is_none()
        {
            vector.app.services = (self.vector_services)();
            if let Some(home) = &app.panel.home
                && let Ok(bytes) = std::fs::read(home.join("vectorcraft-prefs.json"))
                && bytes.len() < 4 << 20
                && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
            {
                if let Ok(prefs) = serde_json::from_value(value["prefs"].clone()) {
                    vector.app.session.prefs = prefs;
                }
                if let Ok(ui) = serde_json::from_value(value["ui"].clone()) {
                    vector.app.ui = ui;
                }
            }
        }
    }

    pub fn ask(&mut self, action: Action, ctx: &egui::Context, source: Option<&Path>) {
        if self.pending {
            return;
        }
        let request = match action {
            Action::Open => PickRequest::OpenMany,
            Action::Skin | Action::Export => PickRequest::Folder,
            Action::TextCopy => PickRequest::Save(vectorcraft_ui_egui::FilePick::named(
                source
                    .and_then(Path::file_name)
                    .and_then(|n| n.to_str())
                    .unwrap_or("source.txt"),
            )),
        };
        let future = (self.picker)(request);
        self.pending = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let paths = block_on(future);
            let _ = tx.send((action, paths));
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self) -> Option<(Action, Vec<String>)> {
        let result = self.rx.try_recv().ok()?;
        self.pending = false;
        Some(result)
    }
}
