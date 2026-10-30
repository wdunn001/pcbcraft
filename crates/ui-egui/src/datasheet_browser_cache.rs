//! Persist datasheet cache records and PDF bytes in browser storage (wasm only).

use pcbcraft_datasheet::record::CacheRecord;

const INDEX_KEY: &str = "pcbcraft-datasheet-index";

#[cfg(target_arch = "wasm32")]
pub(crate) fn load_index() -> Vec<CacheRecord> {
    let Some(window) = web_sys::window() else {
        return Vec::new();
    };
    let Ok(Some(storage)) = window.local_storage() else {
        return Vec::new();
    };
    let Ok(Some(json)) = storage.get_item(INDEX_KEY) else {
        return Vec::new();
    };
    serde_json::from_str(&json).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load_index() -> Vec<CacheRecord> {
    Vec::new()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn save_index(index: &[CacheRecord]) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(Some(storage)) = window.local_storage() else {
        return;
    };
    if let Ok(json) = serde_json::to_string(index) {
        let _ = storage.set_item(INDEX_KEY, &json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_index(_index: &[CacheRecord]) {}

#[cfg(target_arch = "wasm32")]
pub(crate) fn store_bytes(sha256: &str, bytes: &[u8]) {
    let sha = sha256.to_owned();
    let bytes = bytes.to_vec();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = idb_put(&sha, &bytes).await;
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn store_bytes(_sha256: &str, _bytes: &[u8]) {}

#[cfg(target_arch = "wasm32")]
pub(crate) fn request_bytes(sha256: String, slot: egui::Id, ctx: &egui::Context) {
    let ctx = ctx.clone();
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(bytes) = idb_get(&sha256).await {
            ctx.data_mut(|d| d.insert_temp(slot, bytes));
            ctx.request_repaint();
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn request_bytes(_sha256: String, _slot: egui::Id, _ctx: &egui::Context) {}

#[cfg(target_arch = "wasm32")]
const DB: &str = "pcbcraft-datasheet";
#[cfg(target_arch = "wasm32")]
const STORE: &str = "pdf";

#[cfg(target_arch = "wasm32")]
async fn open_db() -> Result<web_sys::IdbDatabase, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let factory = window
        .indexed_db()
        .map_err(|_| "idb".to_string())?
        .ok_or_else(|| "idb off".to_string())?;
    let request = factory
        .open_with_u32(DB, 1)
        .map_err(|_| "idb open".to_string())?;
    let on_up = wasm_bindgen::closure::Closure::wrap(Box::new(move |evt: web_sys::Event| {
        if let Some(target) = evt.target() {
            if let Ok(db) = target.dyn_into::<web_sys::IdbDatabase>() {
                if db.object_store_names().length() == 0 {
                    let _ = db.create_object_store(STORE);
                }
            }
        }
    }) as Box<dyn FnMut(_)>);
    request.set_onupgradeneeded(Some(on_up.as_ref().unchecked_ref()));
    on_up.forget();
    let idb = JsFuture::from(request)
        .await
        .map_err(|_| "idb wait".to_string())?;
    idb.dyn_into().map_err(|_| "idb db".to_string())
}

#[cfg(target_arch = "wasm32")]
async fn idb_put(key: &str, bytes: &[u8]) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let db = open_db().await?;
    let tx = db
        .transaction_with_str_and_mode(STORE, web_sys::IdbTransactionMode::Readwrite)
        .map_err(|_| "tx".to_string())?;
    let store = tx.object_store(STORE).map_err(|_| "store".to_string())?;
    let arr = js_sys::Uint8Array::from(bytes);
    let req = store.put_with_key(&arr, &wasm_bindgen::JsValue::from_str(key))
        .map_err(|_| "put".to_string())?;
    JsFuture::from(req).await.map_err(|_| "put wait".to_string())?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn idb_get(key: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let db = open_db().await?;
    let tx = db
        .transaction_with_str(STORE)
        .map_err(|_| "tx".to_string())?;
    let store = tx.object_store(STORE).map_err(|_| "store".to_string())?;
    let req = store
        .get(&wasm_bindgen::JsValue::from_str(key))
        .map_err(|_| "get".to_string())?;
    let value = JsFuture::from(req).await.map_err(|_| "get wait".to_string())?;
    if value.is_undefined() {
        return Err("missing".into());
    }
    let u8arr = js_sys::Uint8Array::new(&value);
    let mut bytes = vec![0u8; u8arr.length() as usize];
    u8arr.copy_to(&mut bytes);
    Ok(bytes)
}
