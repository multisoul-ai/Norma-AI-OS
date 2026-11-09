//! Minimal ABI for the real libdispatch functions used by the production module.
use std::ffi::c_void;

pub type Source = *mut c_void;
#[repr(C)]
#[derive(Clone, Copy)]
pub union Object {
    pub _ds: Source,
}

unsafe extern "C" {
    pub static _dispatch_main_q: c_void;
    pub static _dispatch_source_type_data_add: c_void;
    pub fn dispatch_source_create(
        kind: *const c_void,
        handle: usize,
        mask: usize,
        queue: Source,
    ) -> Source;
    pub fn dispatch_set_context(source: Object, context: *mut c_void);
    pub fn dispatch_source_set_event_handler_f(
        source: Source,
        handler: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    pub fn dispatch_source_merge_data(source: Source, value: usize);
    pub fn dispatch_source_cancel(source: Source);
    pub fn dispatch_resume(source: Object);
    pub fn dispatch_suspend(source: Object);
    pub fn dispatch_release(source: Object);
}
