//! SkinCraft's stdio MCP facade. All tools address the same live workspace, including the
//! native editor command catalogs, in-progress strokes, source text and whole-window captures.
use photocraft_automation::BridgeClient;
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Clone)]
pub struct SkinCraftMcp {
    home: PathBuf,
    router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathParams {
    /// Absolute file or folder path. Project files are readable; other paths must be beneath SKINCRAFT_READ_ROOT or SKINCRAFT_WRITE_ROOT as appropriate.
    pub path: String,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WriteParams {
    pub path: String,
    pub text: String,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditorParams {
    /// active (default), vector, or raster. Discover commands with method engine.commands.
    pub editor: Option<String>,
    /// engine.commands, engine.execute, document.inspect, ui.inspect, ui.set, ui.pointer, ui.tool.select, ui.tool.list, ui.menu.list or ui.render. Use engine.execute with params {command, params}. Pointer coordinates are document coordinates by default.
    pub method: String,
    pub params: Option<Value>,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewParams {
    pub running: Option<bool>,
    pub time: Option<f64>,
    /// Compare autoplay/manual hit effects while keeping the demo's timing examples.
    pub autoplay_flashes: Option<bool>,
    /// itg, danoni, sm5 or ddr-a.
    pub ruleset: Option<String>,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotParams {
    /// Optional PNG copy beneath the write root. An inline image is always returned.
    pub path: Option<String>,
}

impl SkinCraftMcp {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            router: Self::router(),
        }
    }
    async fn call(&self, method: &str, params: Value) -> CallToolResult {
        let result=async {
            let bytes=tokio::fs::read(self.home.join("control.json")).await.map_err(|_|"SkinCraft is not running for this project home. Start tools/skin-studio/run.sh.".to_string())?;
            let descriptor:Value=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
            if descriptor["app"]!="SkinCraft" {return Err("Unexpected control server".into());}
            let token_path=descriptor["tokenFile"].as_str().ok_or("Missing token file")?;
            let token=photocraft_automation::security::client_token(None,Some(std::path::Path::new(token_path))).map_err(|e|e.to_string())?;
            let bridge=BridgeClient::new(descriptor["address"].as_str().ok_or("Missing server address")?,token).map_err(|e|e.to_string())?;
            bridge.call(method,params).await.map_err(|e|e.to_string())
        }.await;
        match result {
            Ok(mut value) => {
                let image = value
                    .as_object_mut()
                    .and_then(|v| v.remove("pngBase64"))
                    .and_then(|v| v.as_str().map(str::to_owned));
                let mut content = Vec::new();
                if let Some(png) = image {
                    content.push(ContentBlock::image(png, "image/png"));
                }
                content.push(ContentBlock::text(value.to_string()));
                CallToolResult::success(content)
            }
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error)]),
        }
    }
    pub async fn serve_stdio(self) -> Result<(), String> {
        let service = self
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| e.to_string())?;
        service.waiting().await.map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[tool_router(router = router)]
impl SkinCraftMcp {
    #[tool(
        description = "Inspect SkinCraft's project, preview, errors, source bindings and both native editor states.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn inspect_workspace(&self) -> CallToolResult {
        self.call("inspect", json!({})).await
    }
    #[tool(
        description = "List every image, script, metrics file and inherited resource in the current noteskin.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn list_files(&self) -> CallToolResult {
        self.call("list_files", json!({})).await
    }
    #[tool(
        description = "Open a project resource in its editor: SVG in VectorCraft, bitmap in PhotoCraft, script in text. Existing tabs and edits are reused. External artwork opens as an independent reference."
    )]
    async fn open_file(&self, Parameters(p): Parameters<PathParams>) -> CallToolResult {
        self.call("open_file", json!({"path":p.path})).await
    }
    #[tool(
        description = "Save the current project and load a noteskin folder beneath the configured read root."
    )]
    async fn load_skin(&self, Parameters(p): Parameters<PathParams>) -> CallToolResult {
        self.call("load_skin", json!({"path":p.path})).await
    }
    #[tool(
        description = "Read the current draft of a project text file, including unsaved edits.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn read_text(&self, Parameters(p): Parameters<PathParams>) -> CallToolResult {
        self.call("read_text", json!({"path":p.path})).await
    }
    #[tool(
        description = "Edit a project script, metrics or SVG text draft and reload the game preview. Invalid drafts remain editable and the last valid preview is retained."
    )]
    async fn write_text(&self, Parameters(p): Parameters<WriteParams>) -> CallToolResult {
        self.call("write_text", json!({"path":p.path,"text":p.text}))
            .await
    }
    #[tool(
        description = "Switch a project SVG from native vector editing to editable source text, preserving its last accepted SVG. Releases its session vector document/history."
    )]
    async fn release_svg_source(&self, Parameters(p): Parameters<PathParams>) -> CallToolResult {
        self.call("release_svg_source", json!({"path":p.path}))
            .await
    }
    #[tool(
        description = "Discover and run native editor commands, inspect objects/layers, select tools, or draw through ui.pointer. Use engine.commands first for exact command ids and parameters. File operations use SkinCraft tools."
    )]
    async fn editor_request(&self, Parameters(p): Parameters<EditorParams>) -> CallToolResult {
        self.call("editor_request",json!({"editor":p.editor.unwrap_or("active".into()),"method":p.method,"params":p.params.unwrap_or(json!({}))})).await
    }
    #[tool(
        description = "Save the whole SkinCraft project with editable vector/raster sources and script drafts."
    )]
    async fn save_project(&self) -> CallToolResult {
        self.call("save_project", json!({})).await
    }
    #[tool(
        description = "Save an independent copy of the active artwork. Vector copies use .svg or .vectorcraft; raster copies use .pcraft, PSD or image formats. Never overwrites original noteskin resources."
    )]
    async fn save_file(&self, Parameters(p): Parameters<PathParams>) -> CallToolResult {
        self.call("save_file", json!({"path":p.path})).await
    }
    #[tool(
        description = "Export a game-loadable noteskin into a new destination below the write root. Refuses existing destinations or invalid/pending source changes. SVGs remain SVG."
    )]
    async fn export_skin(&self, Parameters(p): Parameters<PathParams>) -> CallToolResult {
        self.call("export_skin", json!({"path":p.path})).await
    }
    #[tool(
        description = "Pause/resume or seek the shared game preview, for repeatable artwork comparisons."
    )]
    async fn set_preview(&self, Parameters(p): Parameters<PreviewParams>) -> CallToolResult {
        self.call(
            "preview",
            json!({"running":p.running,"time":p.time,"autoplay_flashes":p.autoplay_flashes,"ruleset":p.ruleset}),
        )
        .await
    }
    #[tool(
        description = "Capture the whole SkinCraft window, including active editor, files and actual game preview. Returns an inline PNG. The window must be visible."
    )]
    async fn screenshot(&self, Parameters(p): Parameters<ScreenshotParams>) -> CallToolResult {
        self.call("screenshot", json!({"path":p.path})).await
    }
}

#[tool_handler(router = self.router)]
impl ServerHandler for SkinCraftMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("skincraft",env!("CARGO_PKG_VERSION")))
            .with_instructions("SkinCraft noteskin workspace. Start with inspect_workspace and list_files. Open project images to edit SVGs in VectorCraft or bitmaps in PhotoCraft; editor_request engine.commands discovers exact native command ids. Scripts and metrics use read_text/write_text. Save project preserves editable sources; export_skin writes a new runtime tree. Default file reads are limited to bundled noteskins and the current project's files; writes are limited to the project home. SKINCRAFT_READ_ROOT/WRITE_ROOT configure additional working roots when starting the app. Never retry a timed-out edit blindly; inspect state first.")
    }
}

pub fn project_home() -> Result<PathBuf, String> {
    if let Some(home) = std::env::var_os("DDI_STUDIO_HOME") {
        return Ok(PathBuf::from(home));
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/SkinCraft"))
            .ok_or("No project home".into())
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(|h| PathBuf::from(h).join("SkinCraft"))
            .ok_or("No project home".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|h| h.join("skincraft"))
            .ok_or("No project home".into())
    }
}
