//! Cookie sign-in client for the CollabCraft web build.
//!淀
//! # Flow
//! - User clicks "Sign in" and the browser navigates to the auth endpoint
//!   on the same origin. CollabCraft sets a session cookie and redirects
//!   back to the app.
//! - All subsequent requests to the same origin automatically include
//!   the cookie via `credentials: 'include'`.
//! - No token file or device flow is needed.
//!
//! # Dependencies
//! - crates/mcp/src/project_host.rs
//!
//! # Do not
//! - Use token files or device flow.
//! - Make requests to a different origin.

/// Redirect the browser to the CollabCraft auth endpoint on the same origin.
///
/// The endpoint sets a session cookie and redirects back to the app.
pub fn begin_sign_in() {
    let window = web_sys::window().expect("browser window unavailable");
    let location = window.location();
    let protocol = location.protocol().expect("location protocol unavailable");
    let host = location.host().expect("location host unavailable");
    let auth_url = format!("{protocol}//{host}/auth/signin");
    let _ = location.set_href(&auth_url);
}

/// Check if the user is signed in by making a credentials-included request
/// to the CollabCraft auth status endpoint.
///
/// Returns true if the user is authenticated, false otherwise.
pub async fn is_signed_in() -> bool {
    let window = web_sys::window().expect("browser window unavailable");
    let location = window.location();
    let protocol = location.protocol().expect("location protocol unavailable");
    let host = location.host().expect("location host unavailable");
    let status_url = format!("{protocol}//{host}/auth/status");

    let request = web_sys::Request::new(&status_url).expect("request creation failed");
    request
        .credentials()
        .set(web_sys::RequestCredentials::Include);

    let response = match web_sys::window()
        .and_then(|w| w.fetch_with_request(&request))
        .and_then(|p| wasm_bindgen_futures::JsFuture::from(p).await.ok())
    {
        Some(r) => r,
        None => return false,
    };

    let status = response.status();
    status == 200
}
