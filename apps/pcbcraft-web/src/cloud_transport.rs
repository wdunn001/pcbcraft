//! Async transport for CollabCraft cloud operations using cookie auth.
//!
//! Requests are made to the same origin with `credentials: 'include'`
//! so the session cookie is sent automatically.
//!
//! # Dependencies
//! - crates/mcp/src/project_host.rs
//!
//! # Do not
//! - Use token files or device flow.
//! - Make requests to a different origin.

use wasm_bindgen::JsValue;
use web_sys::{Request, RequestCredentials, RequestInit, RequestMode};

/// Base URL for the CollabCraft API on the same origin.
fn api_base() -> String {
    let window = web_sys::window().expect("browser window unavailable");
    let location = window.location();
    let protocol = location.protocol().expect("location protocol unavailable");
    let host = location.host().expect("location host unavailable");
    format!("{protocol}//{host}/api")
}

/// Fetch a project from the cloud by ID.
///
/// Returns the project bytes, or an error string if the request failed.
pub async fn fetch_project(project_id: &str) -> Result<Vec<u8>, String> {
    let url = format!("{}/projects/{}", api_base(), project_id);
    let request = Request::new(&url).map_err(|e| format!("request creation failed: {:?}", e))?;
    request.credentials().set(RequestCredentials::Include);
    request.mode().set(RequestMode::SameOrigin);

    let response = web_sys::window()
        .and_then(|w| w.fetch_with_request(&request))
        .and_then(|p| wasm_bindgen_futures::JsFuture::from(p).await.ok())
        .ok_or("fetch failed")?;

    if !response.ok() {
        return Err(format!("fetch failed with status {}", response.status()));
    }

    let array_buffer = response
        .array_buffer()
        .and_then(|p| wasm_bindgen_futures::JsFuture::from(p).await.ok())
        .ok_or("failed to read response body")?;

    let bytes = js_sys::Uint8Array::new(&array_buffer).to_vec();
    Ok(bytes)
}

/// Upload a project to the cloud.
///
/// Returns the project ID, or an error string if the upload failed.
pub async fn upload_project(name: &str, bytes: &[u8]) -> Result<String, String> {
    let url = format!("{}/projects", api_base());

    // Build multipart form data
    let form =
        web_sys::FormData::new().map_err(|e| format!("form data creation failed: {:?}", e))?;
    form.set("name", name);

    // Convert bytes to blob and add to form
    let array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    array.copy_from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array.buffer());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|e| format!("blob creation failed: {:?}", e))?;
    form.set_with_blob("file", &blob, "upload.pcbcraft")
        .map_err(|e| format!("form field set failed: {:?}", e))?;

    let init = RequestInit::new();
    init.method("POST");
    init.body(Some(form.as_ref()));

    let request = Request::new_with_init(&url, &init)
        .map_err(|e| format!("request creation failed: {:?}", e))?;
    request.credentials().set(RequestCredentials::Include);
    request.mode().set(RequestMode::SameOrigin);

    let response = web_sys::window()
        .and_then(|w| w.fetch_with_request(&request))
        .and_then(|p| wasm_bindgen_futures::JsFuture::from(p).await.ok())
        .ok_or("fetch failed")?;

    if !response.ok() {
        return Err(format!("upload failed with status {}", response.status()));
    }

    let text = response
        .text()
        .and_then(|p| wasm_bindgen_futures::JsFuture::from(p).await.ok())
        .ok_or("failed to read response body")?;

    // Parse JSON response to get project ID
    let value: JsValue = serde_wasm_bindgen::from_str(&text)
        .map_err(|e| format!("failed to parse response JSON: {:?}. Response: {}", e, text))?;

    let project_id = value
        .get("id")
        .and_then(|v| v.as_string())
        .ok_or("response missing 'id' field")?;

    Ok(project_id)
}

/// List projects for the signed-in user.
///
/// Returns a list of project IDs, or an error string if the request failed.
pub async fn list_projects() -> Result<Vec<String>, String> {
    let url = format!("{}/projects", api_base());
    let request = Request::new(&url).map_err(|e| format!("request creation failed: {:?}", e))?;
    request.credentials().set(RequestCredentials::Include);
    request.mode().set(RequestMode::SameOrigin);

    let response = web_sys::window()
        .and_then(|w| w.fetch_with_request(&request))
        .and_then(|p| wasm_bindgen_futures::JsFuture::from(p).await.ok())
        .ok_or("fetch failed")?;

    if !response.ok() {
        return Err(format!("list failed with status {}", response.status()));
    }

    let text = response
        .text()
        .and_then(|p| wasm_bindgen_futures::JsFuture::from(p).await.ok())
        .ok_or("failed to read response body")?;

    let value: JsValue = serde_wasm_bindgen::from_str(&text)
        .map_err(|e| format!("failed to parse response JSON: {:?}. Response: {}", e, text))?;

    let projects = value
        .get("projects")
        .and_then(|v| v.as_array())
        .ok_or("response missing 'projects' array")?;

    let mut ids = Vec::new();
    for project in projects {
        let id = project
            .get("id")
            .and_then(|v| v.as_string())
            .ok_or("project missing 'id' field")?;
        ids.push(id);
    }

    Ok(ids)
}
