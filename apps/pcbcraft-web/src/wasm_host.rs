//! Browser file picker and blob download bridge for the web app shell.
use pcbcraft_mcp::ProjectHost;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, HtmlInputElement, Url};

const OPEN_ACCEPT: &str = ".pcbcraft,.zip,.pcbcraft-lib,.kicad_sym,.kicad_mod";

thread_local! {
    static PENDING_OPEN: RefCell<Option<(String, Vec<u8>)>> = RefCell::new(None);
}

/// Read one picked or dropped file and park its bytes for `pick_open`.
fn read_into_pending(file: &web_sys::File) {
    let name = file.name();
    let Ok(reader) = web_sys::FileReader::new() else {
        return;
    };
    let reader = Rc::new(reader);
    let reader_for_load = Rc::clone(&reader);
    let onload = Closure::wrap(Box::new(move |_event: web_sys::ProgressEvent| {
        if let Ok(result) = reader_for_load.result() {
            let bytes = js_sys::Uint8Array::new(&result).to_vec();
            PENDING_OPEN.with(|slot| *slot.borrow_mut() = Some((name.clone(), bytes)));
        }
    }) as Box<dyn FnMut(_)>);
    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
    onload.forget();
    let _ = reader.read_as_array_buffer(file);
}

/// Hidden file input plus object-URL downloads for the wasm build.
pub struct WasmHost {
    input: HtmlInputElement,
}

impl WasmHost {
    pub fn mount(document: &web_sys::Document) -> Result<Self, JsValue> {
        let input = document
            .create_element("input")?
            .dyn_into::<HtmlInputElement>()?;
        input.set_type("file");
        input.set_accept(OPEN_ACCEPT);
        input.style().set_property("display", "none")?;
        let body = document
            .body()
            .ok_or_else(|| JsValue::from_str("document has no body"))?;
        body.append_child(&input)?;

        let input_for_closure = input.clone();
        let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            if let Some(file) = input_for_closure.files().and_then(|list| list.get(0)) {
                read_into_pending(&file);
            }
            input_for_closure.set_value("");
        }) as Box<dyn FnMut(_)>);
        input.set_onchange(Some(closure.as_ref().unchecked_ref()));
        closure.forget();

        // Dropping a file anywhere on the page opens it like the picker does.
        let over = Closure::wrap(Box::new(|event: web_sys::Event| {
            event.prevent_default();
        }) as Box<dyn FnMut(_)>);
        let drop = Closure::wrap(Box::new(|event: web_sys::DragEvent| {
            event.prevent_default();
            if let Some(file) = event
                .data_transfer()
                .and_then(|transfer| transfer.files())
                .and_then(|list| list.get(0))
            {
                read_into_pending(&file);
            }
        }) as Box<dyn FnMut(_)>);
        body.add_event_listener_with_callback("dragover", over.as_ref().unchecked_ref())?;
        body.add_event_listener_with_callback("drop", drop.as_ref().unchecked_ref())?;
        over.forget();
        drop.forget();

        Ok(Self { input })
    }

    pub fn trigger_open(&self) {
        let _ = self.input.click();
    }
}

/// Contract: crates/mcp/src/project_host.rs
impl ProjectHost for WasmHost {
    fn takes_bytes(&self) -> bool {
        true
    }

    fn request_open(&mut self) {
        self.trigger_open();
    }

    fn pick_open(&mut self) -> Option<(String, Vec<u8>)> {
        PENDING_OPEN.with(|slot| slot.borrow_mut().take())
    }

    fn save_download(&mut self, filename: &str, bytes: &[u8]) -> Result<(), String> {
        let window = web_sys::window().ok_or("browser window unavailable")?;
        let document = window.document().ok_or("browser document unavailable")?;
        let array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
        array.copy_from(bytes);
        let parts = js_sys::Array::new();
        parts.push(&array.buffer());
        let mut bag = BlobPropertyBag::new();
        bag.set_type("application/octet-stream");
        let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &bag)
            .map_err(|_| "could not build download blob")?;
        let url =
            Url::create_object_url_with_blob(&blob).map_err(|_| "could not create object URL")?;
        let anchor = document
            .create_element("a")
            .map_err(|_| "could not create download anchor")?
            .dyn_into::<HtmlAnchorElement>()
            .map_err(|_| "download anchor has wrong type")?;
        anchor.set_href(&url);
        anchor.set_download(filename);
        anchor
            .style()
            .set_property("display", "none")
            .map_err(|_| "could not hide download anchor")?;
        let body = document.body().ok_or("document has no body")?;
        body.append_child(&anchor)
            .map_err(|_| "could not attach download anchor")?;
        anchor.click();
        let _ = body.remove_child(&anchor);
        Url::revoke_object_url(&url).map_err(|_| "could not revoke object URL")?;
        Ok(())
    }
}
