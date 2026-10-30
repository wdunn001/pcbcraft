//! wasm32 entry points for the spike (no wasm-bindgen: plain exported functions).
//!
//! Context: lets headless Chromium load the module, open a PDF the page copied into linear memory,
//! render a page and read back the bitmap and text layer. The caller owns all buffers through
//! `spike_alloc` and `spike_free`.
//!
//! Do not: use this as the app interface; the app will use its own trait.

use crate::{open, render_page, search, text_layer};
use hayro::RenderCache;
use hayro::hayro_interpret::InterpreterCache;
use hayro::hayro_syntax::Pdf;
use std::cell::RefCell;

thread_local! {
    static DOC: RefCell<Option<Pdf>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Allocate `len` bytes in module memory and return the pointer.
#[unsafe(no_mangle)]
pub extern "C" fn spike_alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Free a buffer from `spike_alloc`.
///
/// # Safety
/// `ptr` and `len` must come from one `spike_alloc` call that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spike_free(ptr: *mut u8, len: usize) {
    drop(unsafe { Vec::from_raw_parts(ptr, len, len) });
}

/// Open the PDF at `ptr..ptr+len` (copied). Returns the page count, or -1 on failure.
///
/// # Safety
/// The range must be valid memory in this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spike_open(ptr: *const u8, len: usize) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    match open(bytes) {
        Ok(p) => {
            let n = p.pages().len() as i32;
            DOC.with(|d| *d.borrow_mut() = Some(p));
            n
        }
        Err(_) => -1,
    }
}

/// Render page `index` at `dpi` into the output buffer. Returns width in the high 16 bits and height
/// in the low 16 bits packed as `w << 16 | h`, or 0 on failure. Read bytes with `spike_out_ptr`.
#[unsafe(no_mangle)]
pub extern "C" fn spike_render(index: u32, dpi: f32) -> u32 {
    DOC.with(|d| {
        let d = d.borrow();
        let Some(pdf) = d.as_ref() else { return 0 };
        let cache = RenderCache::new();
        match render_page(pdf, index as usize, dpi, &cache) {
            Ok(b) => {
                let packed = (b.width << 16) | (b.height & 0xffff);
                OUT.with(|o| *o.borrow_mut() = b.rgba);
                packed
            }
            Err(_) => 0,
        }
    })
}

/// Search page `index` for the string at `ptr..ptr+len`; returns the hit count, or -1 on failure.
///
/// # Safety
/// The range must be valid UTF-8 memory in this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spike_search(index: u32, ptr: *const u8, len: usize) -> i32 {
    let needle =
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).to_string();
    DOC.with(|d| {
        let d = d.borrow();
        let Some(pdf) = d.as_ref() else { return -1 };
        let ic = InterpreterCache::new();
        match text_layer(pdf, index as usize, &ic) {
            Ok(l) => search::find(&l, &needle).len() as i32,
            Err(_) => -1,
        }
    })
}

/// Pointer to the last render's RGBA bytes.
#[unsafe(no_mangle)]
pub extern "C" fn spike_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

/// Count characters pdf_oxide extracts from page `index` of the PDF at `ptr..ptr+len`, or -1 on
/// failure. Only present with the `oxide` feature; exists to prove pdf_oxide builds and runs on wasm.
///
/// # Safety
/// The range must be valid memory in this module.
#[cfg(feature = "oxide")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spike_oxide_chars(index: u32, ptr: *const u8, len: usize) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    match pdf_oxide::PdfDocument::from_bytes(bytes).and_then(|d| d.extract_chars(index as usize)) {
        Ok(c) => c.len() as i32,
        Err(_) => -1,
    }
}
