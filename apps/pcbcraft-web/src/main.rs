#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("Build the web app with trunk in apps/pcbcraft-web.");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_bindgen_futures::spawn_local(async {
        if let Err(error) = start().await {
            web_sys::console::error_1(&error);
        }
    });
}
#[cfg(target_arch = "wasm32")]
async fn start() -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("browser document unavailable"))?;
    let canvas = document
        .get_element_by_id("pcbcraft")
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("missing pcbcraft canvas"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let session = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                Ok(Box::new(WithFonts::new(pcbcraft_ui_egui::App::new(
                    cc, session, None, None, false,
                ))))
            }),
        )
        .await
}

/// Wraps the app so selecting Japanese or Chinese fetches that language's
/// font file over HTTP, exactly once, and installs it as a fallback. The
/// font lives beside the site as `fonts/<file>` (brotli-compressed by the
/// build), never inside the wasm. A failed fetch keeps the default fonts
/// and says so in the status line.
#[cfg(target_arch = "wasm32")]
struct WithFonts {
    inner: pcbcraft_ui_egui::App,
    /// File already fetched or being fetched, so one download per file.
    fetching: Option<String>,
    /// File whose fetch failed; the fetch task parks it here and the next
    /// frame turns it into a status-line sentence.
    failed: std::rc::Rc<std::cell::Cell<Option<String>>>,
}

#[cfg(target_arch = "wasm32")]
impl WithFonts {
    fn new(inner: pcbcraft_ui_egui::App) -> Self {
        Self {
            inner,
            fetching: None,
            failed: std::rc::Rc::new(std::cell::Cell::new(None)),
        }
    }

    /// Start the fetch for the current language's font file, if any.
    fn start_fetch(&mut self, ctx: &eframe::egui::Context) {
        let tag = self.inner.language();
        let Some(file) = pcbcraft_ui_egui::fonts::font_file_for(&tag) else {
            return;
        };
        if self.fetching.as_deref() == Some(file) {
            return;
        }
        self.fetching = Some(file.to_string());
        let ctx = ctx.clone();
        let failed = self.failed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match load_font_file(file).await {
                Ok(bytes) => {
                    pcbcraft_ui_egui::fonts::add_fallback(
                        &ctx,
                        pcbcraft_ui_egui::fonts::font_stem(file),
                        bytes,
                    );
                    ctx.request_repaint();
                }
                Err(_) => failed.set(Some(file.to_string())),
            }
        });
    }

    /// Turn a parked fetch failure into the status-line sentence. The
    /// app keeps working with the default fonts.
    fn report_failures(&mut self) {
        if let Some(file) = self.failed.take() {
            self.inner.report_missing_font(&file);
        }
    }
}

/// Fetch `fonts/<file>` and return its bytes.
#[cfg(target_arch = "wasm32")]
async fn load_font_file(file: &str) -> Result<Vec<u8>, wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let window = web_sys::window().ok_or_else(|| wasm_bindgen::JsValue::from_str("no window"))?;
    let response = JsFuture::from(window.fetch_with_str(&format!("fonts/{file}"))).await?;
    let response: web_sys::Response = response.dyn_into()?;
    if !response.ok() {
        return Err(wasm_bindgen::JsValue::from_str("font file not found"));
    }
    let body = response.array_buffer()?;
    let buffer = JsFuture::from(body).await?;
    let buffer: js_sys::ArrayBuffer = buffer.dyn_into()?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

#[cfg(target_arch = "wasm32")]
impl eframe::App for WithFonts {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
        self.start_fetch(ui.ctx());
        self.report_failures();
        self.inner.ui(ui, frame);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.inner.save(storage);
    }
}
