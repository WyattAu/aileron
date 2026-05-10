/// Custom X11 error handler that swallows benign X11 errors.
///
/// wry's build_as_child creates X11 child windows that winit doesn't own.
/// When winit's IME tries to unfocus these windows, it triggers GLXBadWindow
/// (error code 170) which winit's event processor .expect()s on, crashing the app.
/// This handler intercepts known benign errors and returns 0 (ignored).
/// All other errors are logged but still swallowed to prevent Xlib abort().
///
/// # Safety
///
/// FFI callback registered via XSetErrorHandler. Signature matches the
/// XErrorHandler typedef. Must only be registered with XSetErrorHandler.
#[cfg(target_os = "linux")]
pub unsafe extern "C" fn x11_error_handler(
    _display: *mut x11_dl::xlib::Display,
    event: *mut x11_dl::xlib::XErrorEvent,
) -> std::os::raw::c_int {
    if !event.is_null() {
        // SAFETY: Pointer validity guaranteed by null check. X11 provides valid XErrorEvent.
        let error = unsafe { &*event };
        match error.error_code {
            170 => {
                // GLXBadWindow -- wry's child windows, benign
            }
            169 => {
                // GLXBadDrawable -- also from wry child windows
            }
            3 => {
                // BadWindow -- stale window reference, benign
            }
            _ => {
                tracing::warn!(
                    target: "x11",
                    "Unhandled X11 error (code {}): request={} minor={}",
                    error.error_code,
                    error.request_code,
                    error.minor_code,
                );
            }
        }
    }
    0 // Always return 0 -- never let Xlib's default handler (which calls abort)
}
