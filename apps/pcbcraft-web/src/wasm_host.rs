//! Browser file picker and blob download bridge for the web app shell.
use pcbcraft_ui_egui::library::ProjectHost;
use std::cell::{RefCell, RefMut};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, HtmlInputElement, Url};

const OPEN_ACCEPT: &str = ".pcbcraft,.zip,.pcbcraft-lib,.kicad_sym,.kicad_mod";

thread_local! {
    static PENDING_OPEN: RefCell<Option<(String, Vec<u8>)>> = RefCell::new(None);
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
            let Some(file_list) = input_for_closure.files() else {
                return;
            };
            if file_list.length() == 0 {
                return;
            }
            let Some(file) = file_list.get(0) else {
                return;
            };
            let name = file.name();
            let reader = match web_sys::FileReader::new() {
                Ok(value) => Rc::new(RefCell::new(value)),
                Err(_) => return,
            };
            let name_copy = name.clone();
            let reader_for_load = Rc::clone(&reader);
            let onload = Closure::wrap(Box::new(move |_event: web_sys::ProgressEvent| {
                let reader_ref: RefMut<'_, web_sys::FileReader> = reader_for_load.borrow_mut();
                match reader_ref.result() {
                    Ok(result) => {
                        let bytes = js_sys::Uint8Array::new(&result).to_vec();
                        PENDING_OPEN.with(|slot| {
                            *slot.borrow_mut() = Some((name_copy.clone(), bytes));
                        });
                    }
                    Err(_) => {}
                }
            }) as Box<dyn FnMut(_)>);
            {
                let reader_ref = reader.borrow();
                reader_ref.set_onload(Some(onload.as_ref().unchecked_ref()));
            }
            onload.forget();
            let _ = reader.borrow().read_as_array_buffer(&file);
            let _ = input_for_closure.set_value("");
        }) as Box<dyn FnMut(_)>);
        input.set_onchange(Some(closure.as_ref().unchecked_ref()));
        closure.forget();

        Ok(Self { input })
    }

    pub fn trigger_open(&self) {
        let _ = self.input.click();
    }
}

impl ProjectHost for WasmHost {
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
        bag.type_("application/octet-stream");
        let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &bag)
            .map_err(|_| "could not build download blob")?;
        let url = Url::create_object_url_with_blob(&blob)
            .map_err(|_| "could not create object URL")?;
        let anchor = document
            .create_element("a")
            .map_err(|_| "could not create download anchor")?
            .dyn_into::<HtmlAnchorElement>()
            .map_err(|_| "download anchor has wrong type")?;
        anchor.set_href(&url);
        anchor.set_download(filename);
        let _ = anchor.set_style("display", "none");
        let body = document.body().ok_or("document has no body")?;
        body.append_child(&anchor)
            .map_err(|_| "could not attach download anchor")?;
        anchor.click();
        let _ = body.remove_child(&anchor);
        Url::revoke_object_url(&url).map_err(|_| "could not revoke object URL")?;
        Ok(())
    }
}
