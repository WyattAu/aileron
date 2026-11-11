/// Shared WebKitGTK spell-checking FFI helper.
/// Used by both offscreen webview and Wayland-fallback GTK window paths.
#[cfg(target_os = "linux")]
pub fn configure_webkit_spellcheck() {
    let enabled = std::env::var("AILERON_SPELLCHECK")
        .map(|v| v != "0" && v != "false")
        .unwrap_or(true);
    if !enabled {
        return;
    }
    let context = webkit2gtk::WebContext::default();
    // SAFETY: FFI calls with valid pointer from WebContext::default().
    // gtk::glib::translate::ToGlibPtr provides to_glib_none().
    // CString pointers are null-terminated and outlive the call.
    unsafe {
        let ctx_ptr = gtk::glib::translate::ToGlibPtr::to_glib_none(&context).0;
        webkit2gtk::ffi::webkit_web_context_set_spell_checking_enabled(ctx_ptr, 1);
        // Null-terminated array required by WebKitGTK FFI.
        let c_en_us = std::ffi::CString::new("en_US").expect("en_US contains no NUL bytes");
        let c_en_gb = std::ffi::CString::new("en_GB").expect("en_GB contains no NUL bytes");
        let lang_ptrs: Vec<*const i8> = vec![c_en_us.as_ptr(), c_en_gb.as_ptr(), std::ptr::null()];
        webkit2gtk::ffi::webkit_web_context_set_spell_checking_languages(
            ctx_ptr,
            lang_ptrs.as_ptr(),
        );
    }
}
