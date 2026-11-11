//! GTK main-loop pumping (S4 stage 3: moved from frame_tasks).
//!
//! WebKitGTK delivers load events and JS callbacks on the GTK loop;
//! the MCP dispatch settle logic pumps it while polling. The real
//! implementation (with the WebKitGTK log-handler trap) lives in
//! `servo::wry_engine::pump_gtk` — this is the engine-wide entry point.

/// Pump pending GTK events once.
pub fn pump_gtk_loop() {
    crate::servo::wry_engine::pump_gtk();
}
