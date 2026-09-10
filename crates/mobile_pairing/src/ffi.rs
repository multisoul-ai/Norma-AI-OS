//! Minimal C bridge for the native Swift package. No global state or retained credentials.
use super::{Client, MAX_OFFER_BYTES, MAX_SEALED_BYTES, MESSAGE_BYTES};
use std::{ffi::c_void, ptr, slice};

/// # Safety
/// code points to six readable ASCII bytes; message points to 33 writable bytes.
/// Return value is owned by the caller and must be freed once with norma_pairing_free.
#[no_mangle]
pub unsafe extern "C" fn norma_pairing_start(code: *const u8, message: *mut u8) -> *mut c_void {
    if code.is_null() || message.is_null() {
        return ptr::null_mut();
    }
    let bytes = unsafe { slice::from_raw_parts(code, 6) };
    let Ok(code) = std::str::from_utf8(bytes) else {
        return ptr::null_mut();
    };
    let Ok((client, outbound)) = Client::start(code) else {
        return ptr::null_mut();
    };
    unsafe {
        ptr::copy_nonoverlapping(outbound.as_ptr(), message, MESSAGE_BYTES);
    }
    Box::into_raw(Box::new(Some(client))).cast()
}

/// # Safety
/// handle is a live start result, used by only one thread. message is 33 readable bytes;
/// sealed is sealed_len readable bytes; output is 4096 writable bytes. Buffers cannot overlap.
/// Consumes the exchange inside handle, but caller must still free the handle, including on error.
#[no_mangle]
pub unsafe extern "C" fn norma_pairing_finish(
    handle: *mut c_void,
    message: *const u8,
    sealed: *const u8,
    sealed_len: usize,
    output: *mut u8,
) -> i32 {
    if handle.is_null()
        || message.is_null()
        || sealed.is_null()
        || output.is_null()
        || !(28..=MAX_SEALED_BYTES).contains(&sealed_len)
    {
        return -1;
    }
    let state = unsafe { &mut *handle.cast::<Option<Client>>() };
    let Some(client) = state.take() else {
        return -1;
    };
    let result = client.finish(
        unsafe { slice::from_raw_parts(message, MESSAGE_BYTES) },
        unsafe { slice::from_raw_parts(sealed, sealed_len) },
    );
    match result {
        Ok(plaintext) if plaintext.len() <= MAX_OFFER_BYTES => {
            unsafe {
                ptr::copy_nonoverlapping(plaintext.as_ptr(), output, plaintext.len());
            }
            plaintext.len() as i32
        }
        _ => -1,
    }
}

/// # Safety
/// handle is null or a live start result and is not accessed again after this call.
#[no_mangle]
pub unsafe extern "C" fn norma_pairing_free(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<Option<Client>>()) });
    }
}
