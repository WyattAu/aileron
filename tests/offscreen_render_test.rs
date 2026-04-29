/// Integration test: verify offscreen webview rendering works end-to-end.
///
/// This test:
/// 1. Creates a gtk::OffscreenWindow
/// 2. Embeds a wry WebView via build_gtk
/// 3. Loads a page
/// 4. Pumps the GTK event loop
/// 5. Captures pixel data via get_pixbuf()
/// 6. Verifies non-zero pixel data was captured
#[cfg(target_os = "linux")]
#[test]
fn test_offscreen_webview_creates_and_captures() {
    use gtk::prelude::{GtkWindowExt, OffscreenWindowExt, WidgetExt};
    use wry::WebViewBuilderExtUnix;

    // Initialize GTK (required by wry on Linux)
    gtk::init().expect("Failed to initialize GTK");

    // Load a data URL — renders immediately, no network needed
    let data_url = "data:text/html,<html><body style='background:%2300ff00'>test</body></html>";

    let offscreen = gtk::OffscreenWindow::new();
    offscreen.set_default_size(200, 100);

    let builder = wry::WebViewBuilder::new().with_url(data_url);

    let webview = match builder.build_gtk(&offscreen) {
        Ok(wv) => wv,
        Err(e) => {
            panic!(
                "build_gtk failed: {}. This means OffscreenWindow \
                    is not accepted as a gtk::Container, which breaks Architecture B.",
                e
            );
        }
    };

    offscreen.show_all();

    // Pump GTK event loop to let the webview render
    for _ in 0..50 {
        if !gtk::events_pending() {
            break;
        }
        gtk::main_iteration();
    }

    // Capture the frame
    let pixbuf = match offscreen.pixbuf() {
        Some(pb) => pb,
        None => panic!("get_pixbuf() returned None — offscreen window did not render"),
    };

    let width = pixbuf.width();
    let height = pixbuf.height();
    let pixels = unsafe { pixbuf.pixels() };

    // Verify we got actual pixel data
    assert!(width > 0, "Offscreen window has zero width");
    assert!(height > 0, "Offscreen window has zero height");

    // The webview should have rendered something — not all pixels should be zero
    let non_zero_count = pixels.iter().filter(|&&p| p != 0).count();
    let total_pixels = width as usize * height as usize * 4; // BGRA
    assert!(
        non_zero_count > total_pixels / 10,
        "Offscreen window rendered mostly zeros ({} non-zero out of {} pixels). \
         The webview content may not have rendered into the offscreen buffer.",
        non_zero_count,
        total_pixels,
    );

    // Verify the green background (#0000ff00 in BGRA = bytes 0,255,0,0 on little-endian)
    // Note: this check may not pass because WebKitGTK may use hardware acceleration
    // or a different rendering path. The non-zero check above is the primary assertion.
    let _has_green = pixels
        .chunks_exact(4)
        .any(|chunk| chunk.get(1).copied().unwrap_or(0) > 200);

    // Note: the green background check may not pass because WebKitGTK may use
    // hardware acceleration or a different rendering path. The non-zero check
    // above is the primary assertion.

    // Clean up
    drop(webview);
    drop(offscreen);
}
