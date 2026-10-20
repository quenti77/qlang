//! qlang for the browser. The module exports the JSON protocol of `qlang-core` (the same
//! request and response as `qlang run --json`) through a tiny C interface, so that it needs
//! no tooling beyond `cargo build --target wasm32-unknown-unknown`:
//!
//! - `qlang_alloc(len)` reserves `len` bytes in the module memory and returns their address;
//! - `qlang_handle(ptr, len)` answers the JSON request found there. The answer is a buffer
//!   made of a 4-byte little-endian length followed by the JSON text;
//! - `qlang_version()` returns the version the same way;
//! - `qlang_free(ptr, len)` gives a buffer back (`len` as reserved or, for an answer, 4 plus
//!   the length it announces).
//!
//! `js/qlang.mjs` wraps all this. The module has no I/O: imported files come from the
//! request, `read()` lines too, and printed text comes back in the response.

use qlang_core::protocol::{Ceiling, handle};

fn leak(bytes: Box<[u8]>) -> *mut u8 {
    Box::into_raw(bytes) as *mut u8
}

fn framed(text: &str) -> *mut u8 {
    let mut out = Vec::with_capacity(4 + text.len());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    leak(out.into_boxed_slice())
}

#[unsafe(no_mangle)]
pub extern "C" fn qlang_alloc(len: usize) -> *mut u8 {
    leak(vec![0u8; len].into_boxed_slice())
}

/// # Safety
/// `ptr` and `len` must come from `qlang_alloc`, `qlang_handle` or `qlang_version`
/// (see the module documentation), and be freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qlang_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        // SAFETY: the caller promises that this buffer was leaked by this module with this length
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
    }
}

/// # Safety
/// `ptr` and `len` must designate a buffer returned by `qlang_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qlang_handle(ptr: *const u8, len: usize) -> *mut u8 {
    // SAFETY: see above
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let answer = match std::str::from_utf8(bytes) {
        Ok(request) => handle(request, &Ceiling::default()),
        Err(_) => r#"{"ok":false,"phase":"request","output":"","diagnostics":[{"severity":"error","code":"J001","message":"the request is not valid UTF-8","location":null,"notes":[]}],"steps":0}"#.to_string(),
    };
    framed(&answer)
}

#[unsafe(no_mangle)]
pub extern "C" fn qlang_version() -> *mut u8 {
    framed(env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(ptr: *mut u8) -> String {
        let n = u32::from_le_bytes(
            unsafe { std::slice::from_raw_parts(ptr, 4) }
                .try_into()
                .unwrap(),
        ) as usize;
        let text = String::from_utf8(unsafe { std::slice::from_raw_parts(ptr.add(4), n) }.to_vec())
            .unwrap();
        unsafe { qlang_free(ptr, 4 + n) };
        text
    }

    #[test]
    fn answers_a_request_through_the_c_interface() {
        let request = br#"{"files":{"main.q":"print(6 * 7)"}}"#;
        let ptr = qlang_alloc(request.len());
        unsafe { std::ptr::copy_nonoverlapping(request.as_ptr(), ptr, request.len()) };
        let answer = read(unsafe { qlang_handle(ptr, request.len()) });
        unsafe { qlang_free(ptr, request.len()) };
        assert!(answer.contains(r#""output":"42\n""#), "{answer}");
        assert_eq!(read(qlang_version()), env!("CARGO_PKG_VERSION"));
    }
}
