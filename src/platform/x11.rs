#[cfg(target_os = "linux")]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn x11_error_handler(
    _display: *mut x11_dl::xlib::Display,
    event: *mut x11_dl::xlib::XErrorEvent,
) -> std::os::raw::c_int {
    if !event.is_null() {
        let error = unsafe { &*event };
        match error.error_code {
            170 => {}
            169 => {}
            3 => {}
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
    0
}
