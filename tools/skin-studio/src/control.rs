//! Authenticated loopback control, with per-request cancellation before dispatch. MCP is a
//! stdio client of this one workspace server, rather than two independent editor servers.
use super::*;
use photocraft_automation::{budgets, security};
use serde_json::{Value, json};
use std::{
    io::{BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
use vectorcraft_ui_egui::control::ControlRequest;

pub(super) struct Incoming {
    pub request: ControlRequest,
    pub expires: Instant,
}
pub(super) struct Server {
    pub receiver: mpsc::Receiver<Incoming>,
    stop: Arc<AtomicBool>,
    descriptor: PathBuf,
    address: String,
}

impl Server {
    pub fn start(home: &Path, ctx: egui::Context) -> Result<Self, String> {
        std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let token_file = home.join("control.token");
        let token = security::server_token(None, Some(&token_file)).map_err(|e| e.to_string())?;
        let descriptor = home.join("control.json");
        workspace::atomic(
            &descriptor,
            &serde_json::to_vec(
                &json!({"app":"SkinCraft","address":address,"tokenFile":token_file}),
            )
            .map_err(|e| e.to_string())?,
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&descriptor, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        let (tx, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let quitting = stop.clone();
        let token = Arc::new(token);
        std::thread::spawn(move || {
            let limiter = security::ConnectionLimiter::new(security::MAX_CONNECTIONS);
            while !quitting.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if let Some(permit) = limiter.try_acquire() {
                            let tx = tx.clone();
                            let ctx = ctx.clone();
                            let token = token.clone();
                            std::thread::spawn(move || {
                                let _permit = permit;
                                serve(stream, &token, tx, ctx);
                            });
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            receiver,
            stop,
            descriptor,
            address,
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(bytes) = std::fs::read(&self.descriptor)
            && let Ok(v) = serde_json::from_slice::<Value>(&bytes)
            && v["address"].as_str() == Some(&self.address)
        {
            let _ = std::fs::remove_file(&self.descriptor);
        }
    }
}

fn serve(mut stream: TcpStream, token: &str, sender: mpsc::Sender<Incoming>, ctx: egui::Context) {
    let run = || -> Result<(), String> {
        // Accepted sockets inherit O_NONBLOCK from the listener on macOS.
        stream.set_nonblocking(false).map_err(|e| e.to_string())?;
        security::configure_stream(&stream).map_err(|e| e.to_string())?;
        let reader = stream.try_clone().map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(security::AuthDeadlineStream::new(
            reader,
            security::AUTH_DEADLINE,
        ));
        let mut line = String::new();
        let mut authenticated = false;
        while matches!(
            security::read_bounded_line(&mut reader, &mut line),
            Ok(security::LineRead::Line)
        ) {
            if line.trim().is_empty() {
                continue;
            }
            let reply = if !authenticated {
                let (reply, valid) = security::authentication_reply(&line, token);
                if !valid {
                    budgets::write_reply(&mut stream, &reply).map_err(|e| e.to_string())?;
                    return Ok(());
                }
                authenticated = true;
                reader
                    .get_mut()
                    .authenticated()
                    .map_err(|e| e.to_string())?;
                reply
            } else if let Some(reply) = security::repeat_auth_reply(&line) {
                reply
            } else {
                let message: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
                let id = message.get("id").cloned().unwrap_or(Value::Null);
                let (request, reply) = ControlRequest::new(
                    message["method"].as_str().unwrap_or(""),
                    message.get("params").cloned().unwrap_or(json!({})),
                );
                let ticket = request.ticket();
                let expires = Instant::now() + Duration::from_secs(50);
                sender
                    .send(Incoming { request, expires })
                    .map_err(|e| e.to_string())?;
                ctx.request_repaint();
                let mut result = reply.recv_timeout(expires.saturating_duration_since(Instant::now())).unwrap_or_else(|_| {
                    let cancelled = ticket.give_up();
                    json!({"ok":false,"error":if cancelled {"Request expired before dispatch; nothing ran"} else {"Request timed out after dispatch; inspect state before retrying"}})
                });
                result["id"] = id;
                result
            };
            budgets::write_reply(&mut stream, &reply).map_err(|e| e.to_string())?;
            stream.flush().map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    let mut run = run;
    let _ = run();
}

pub(super) struct Capture {
    pub reply: mpsc::Sender<Value>,
    pub expires: Instant,
    pub token: u64,
    pub path: Option<PathBuf>,
}

impl host::SkinCraft {
    pub(super) fn automation_roots(
        &self,
    ) -> Result<Option<Arc<vectorcraft_engine::file_access::AutomationRoots>>, String> {
        let read = std::env::var_os("SKINCRAFT_READ_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| ddi_desktop::images::assets().join("noteskins"));
        let write = std::env::var_os("SKINCRAFT_WRITE_ROOT")
            .map(PathBuf::from)
            .or_else(|| self.panel.home.clone())
            .ok_or("No project home")?;
        vectorcraft_engine::file_access::AutomationRoots::new(Some(&read), Some(&write))
    }

    pub(super) fn control_tick(&mut self, ctx: &egui::Context) {
        self.captures.retain(|p| p.expires > Instant::now());
        for _ in 0..8 {
            let request = self
                .control
                .as_ref()
                .and_then(|s| s.receiver.try_recv().ok());
            let Some(Incoming { request, expires }) = request else {
                break;
            };
            if expires <= Instant::now() || !request.claim() {
                continue;
            }
            let result = self.request(&request.method, request.params.clone(), ctx);
            match result {
                Ok(result) if request.method == "screenshot" && result["pending"] == true => {
                    let token = self.capture_id;
                    let path = request.params["path"].as_str().map(PathBuf::from);
                    self.captures.push(Capture {
                        reply: request.reply,
                        expires,
                        token,
                        path,
                    });
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                        ("skincraft".to_string(), token),
                    )));
                }
                result => {
                    let _ = request.reply.send(match result {
                        Ok(v) => json!({"ok":true,"result":v}),
                        Err(e) => json!({"ok":false,"error":e}),
                    });
                }
            }
        }
    }

    /// All MCP tools and the loopback control transport share this dispatch path.
    pub(super) fn request(
        &mut self,
        method: &str,
        params: Value,
        ctx: &egui::Context,
    ) -> Result<Value, String> {
        let path = || {
            params["path"]
                .as_str()
                .map(PathBuf::from)
                .ok_or("Missing path".to_string())
        };
        match method {
            "inspect" => Ok(json!({"workspace":self.inspect(), "editor":if self.panel.vector_active(){"vector"}else{"raster"}, "vector":self.panel.vector.as_ref().map(|v| vectorcraft_ui_egui::control::inspect(&v.app,ctx)), "raster":photocraft_ui_egui::control::inspect(&self.photo,ctx)})),
            "list_files" => Ok(json!(self.panel.files.iter().map(|f| json!({"path":f.path,"name":f.name,"owner":f.owner,"inherited":f.inherited,"image":f.image,"text":f.text})).collect::<Vec<_>>())),
            "open_file" => { let p=path()?; self.check_read(&p)?; self.open_any(&p)?; self.panel.vector_tick(); self.panel.update_artwork(&mut self.photo)?; Ok(self.inspect()) }
            "load_skin" => { let p=path()?; self.check_read(&p)?; self.load_skin(&p)?; Ok(self.inspect()) }
            "read_text" | "write_text" => {
                let p=path()?.canonicalize().map_err(|e| e.to_string())?;
                if !self.panel.files.iter().any(|f| f.path==p && f.text) { return Err("Text file is not in the current project".into()); }
                if method=="write_text" {
                    if self.panel.vector.as_ref().is_some_and(|v| v.retained.contains_key(&p)) { return Err("SVG is managed by the vector editor; use release_svg_source before editing its text".into()); }
                    if self.panel.drawings.contains_key(&p) { return Err("SVG has a raster override; release it in the file browser first".into()); }
                    let text=params["text"].as_str().ok_or("Missing text")?.to_string();
                    self.panel.editor.open(&p)?; self.panel.editor.set(&p,text)?;
                    self.panel.text_reload=Some(Instant::now()-Duration::from_secs(1)); self.panel.checkpoint_dirty=true;
                    self.panel.workspace_tick(&mut self.photo,ctx);
                }
                Ok(json!({"path":p,"text":self.panel.editor.edits.get(&p).cloned().map(Ok).unwrap_or_else(|| files::read_text(&p))?,"error":self.panel.failure}))
            }
            "release_svg_source" => {
                let p=path()?.canonicalize().map_err(|e| e.to_string())?;
                if !self.panel.files.iter().any(|f| f.path==p && host::is_svg(&p)) { return Err("SVG is not in this project".into()); }
                if let Some(v)=&mut self.panel.vector {v.release(&p);}
                self.panel.editor.open(&p)?; self.panel.checkpoint_dirty=true; Ok(self.inspect())
            }
            "save_project" => { self.panel.update_artwork(&mut self.photo)?; self.panel.save_project(&self.photo)?; Ok(self.inspect()) }
            "export_skin" => { let p=path()?; self.check_write(&p)?; self.export_skin(&p)?; Ok(json!({"path":p})) }
            "save_file" => { let p=path()?; self.check_write(&p)?; self.save_copy(&p)?; Ok(json!({"path":p})) }
            "preview" => {
                if let Some(id)=params["ruleset"].as_str(){
                    self.panel.options.rules=super::preview_rules(id).ok_or("Unknown preview ruleset")?;
                }
                if let Some(auto)=params["autoplay_flashes"].as_bool(){self.panel.options.autoplay_flashes=auto;}
                if let Some(run)=params["running"].as_bool(){self.panel.running=run;}
                if let Some(time)=params["time"].as_f64(){if !time.is_finite() || time<0.0{return Err("Invalid time".into());} self.panel.time=time.min(self.panel.duration);}
                Ok(self.inspect())
            }
            "screenshot" => {
                if self.captures.len()>=4 {return Err("Too many pending screenshots".into());}
                if params["path"].is_string(){self.check_write(&path()?)?;}
                self.capture_id=self.capture_id.wrapping_add(1); ctx.request_repaint(); Ok(json!({"pending":true}))
            }
            "editor_request" => self.editor_request(&params,ctx),
            _ => Err(format!("Unknown SkinCraft method: {method}")),
        }
    }

    fn check_read(&self, path: &Path) -> Result<(), String> {
        if let Ok(p) = path.canonicalize()
            && (self.panel.files.iter().any(|f| f.path == p)
                || self.panel.home.as_ref().is_some_and(|h| p.starts_with(h))
                || self.panel.project.as_ref().is_some_and(|s| p == s.dir))
        {
            return Ok(());
        }
        let roots = self.automation_roots()?;
        vectorcraft_engine::file_access::confine(roots.as_ref(), || {
            vectorcraft_engine::file_access::check_read(&path.to_string_lossy())
        })
    }
    pub(super) fn check_write(&self, path: &Path) -> Result<(), String> {
        let roots = self.automation_roots()?;
        vectorcraft_engine::file_access::confine(roots.as_ref(), || {
            vectorcraft_engine::file_access::check_write(&path.to_string_lossy())
        })?;
        let p = native::resolved(path)?;
        if self.panel.project.as_ref().is_some_and(|pjt| {
            pjt.chain
                .iter()
                .filter_map(|i| pjt.skins.get(*i))
                .any(|s| p.starts_with(&s.dir))
        }) {
            return Err("Output is inside a source noteskin; choose a working folder".into());
        }
        Ok(())
    }
    fn editor_request(&mut self, params: &Value, ctx: &egui::Context) -> Result<Value, String> {
        let editor = params["editor"].as_str().unwrap_or("active");
        let vector = match editor {
            "active" => self.panel.vector_active(),
            "vector" => true,
            "raster" => false,
            _ => return Err("Editor must be active, vector or raster".into()),
        };
        let method = params["method"].as_str().ok_or("Missing editor method")?;
        if !matches!(
            method,
            "engine.execute"
                | "engine.commands"
                | "document.inspect"
                | "ui.inspect"
                | "ui.set"
                | "ui.pointer"
                | "ui.tool.select"
                | "ui.tool.list"
                | "ui.menu.list"
                | "ui.render"
        ) {
            return Err("Use SkinCraft file/project tools for file operations; this editor method is unavailable".into());
        }
        let args = params.get("params").cloned().unwrap_or(json!({}));
        if method == "engine.execute"
            && args["command"].as_str().is_some_and(|s| {
                s.starts_with("file.")
                    || s.starts_with("app.")
                    || s == "document.export"
                    || s == "document.open"
                    || s.starts_with("ui.")
            })
        {
            return Err("Use SkinCraft file/project tools for file operations".into());
        }
        if editor != "active"
            && let Some(v) = &mut self.panel.vector
        {
            v.active = vector;
        }
        if !vector && method == "ui.render" {
            use base64::Engine as _;
            let doc = &self.photo.session.active().ok_or("No raster document")?.doc;
            let bytes =
                photocraft_automation::files::render_png(doc, 1024).map_err(|e| e.to_string())?;
            return Ok(
                json!({"pngBase64":base64::engine::general_purpose::STANDARD.encode(bytes)}),
            );
        }
        let roots = self.automation_roots()?;
        let outcome = if vector {
            let v = self.panel.vector.as_mut().ok_or("No vector workspace")?;
            let (req, _) = ControlRequest::new(method, args);
            match vectorcraft_engine::file_access::confine(roots.as_ref(), || {
                vectorcraft_ui_egui::control::handle(&mut v.app, ctx, &req)
            }) {
                vectorcraft_ui_egui::control::Outcome::Done(v) => v,
                _ => return Err("Use SkinCraft screenshot for the shared window".into()),
            }
        } else {
            let (req, _) = photocraft_ui_egui::control::ControlRequest::new(method, args);
            match photocraft_ui_egui::control::handle(&mut self.photo, ctx, &req) {
                photocraft_ui_egui::control::Outcome::Done(v) => v,
                photocraft_ui_egui::control::Outcome::AfterJob(job) => {
                    json!({"ok":true,"result":{"pending":true,"job":job}})
                }
                _ => {
                    return Err(
                        "Synthetic input is pending; inspect editor state before continuing".into(),
                    );
                }
            }
        };
        self.panel.vector_tick();
        self.panel.update_artwork(&mut self.photo)?;
        if outcome["ok"] == false {
            return Err(outcome["error"].as_str().unwrap_or("Editor error").into());
        }
        ctx.request_repaint();
        Ok(outcome["result"].clone())
    }

    pub(super) fn capture_events(&mut self, raw: &mut egui::RawInput) {
        use base64::Engine as _;
        raw.events.retain(|event|{
            let egui::Event::Screenshot{user_data,image,..}=event else{return true};
            let Some((tag,token))=user_data.data.as_ref().and_then(|d|d.downcast_ref::<(String,u64)>()) else{return true};
            if tag!="skincraft"{return true;}
            if let Some(i)=self.captures.iter().position(|p|p.token==*token){
                let pending=self.captures.remove(i);
                let result=(||{
                    let pixels=Rgba{size:[image.size[0] as u32,image.size[1] as u32],data:image.pixels.iter().flat_map(|p|p.to_array()).collect()};
                    let bytes=project::png_bytes(&pixels)?;
                    if let Some(path)=&pending.path{self.check_write(path)?;workspace::atomic(path,&bytes)?;}
                    Ok::<_,String>(json!({"width":image.size[0],"height":image.size[1],"pngBase64":base64::engine::general_purpose::STANDARD.encode(bytes)}))
                })();
                let _=pending.reply.send(match result{Ok(v)=>json!({"ok":true,"result":v}),Err(e)=>json!({"ok":false,"error":e})});
            }
            false
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    #[test]
    fn control_authenticates_and_drops_abandoned_requests_before_dispatch() {
        let root = crate::project::tests::fixture("control-security");
        let server = Server::start(&root, egui::Context::default()).unwrap();
        let descriptor: Value =
            serde_json::from_slice(&std::fs::read(root.join("control.json")).unwrap()).unwrap();
        let address = descriptor["address"].as_str().unwrap();
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        writeln!(stream, "{}", json!({"id":1,"method":"save_project"})).unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["ok"], false);
        assert!(
            server.receiver.try_recv().is_err(),
            "no unauthenticated request reaches the UI"
        );
        let token = security::client_token(None, Some(&root.join("control.token"))).unwrap();
        let bridge = BridgeClientForTest::new(address, token);
        let caller = std::thread::spawn(move || bridge.call());
        let incoming = server
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(incoming.request.method, "inspect");
        assert!(incoming.request.claim());
        incoming
            .request
            .reply
            .send(json!({"ok":true,"result":{"workspace":"SkinCraft"}}))
            .unwrap();
        assert_eq!(caller.join().unwrap()["workspace"], "SkinCraft");
        let (request, _) = ControlRequest::new("save_project", json!({}));
        assert!(request.ticket().give_up());
        assert!(!request.claim());
        drop(server);
        std::fs::remove_dir_all(root).unwrap();
    }
    struct BridgeClientForTest {
        address: String,
        token: String,
    }
    impl BridgeClientForTest {
        fn new(address: &str, token: String) -> Self {
            Self {
                address: address.into(),
                token,
            }
        }
        fn call(self) -> Value {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                photocraft_automation::BridgeClient::new(self.address, self.token)
                    .unwrap()
                    .call("inspect", json!({}))
                    .await
                    .unwrap()
            })
        }
    }
}
