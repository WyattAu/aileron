//! Shared event processing logic for both Wry and Offscreen webview managers.
//!
//! This module eliminates the ~660 lines of duplication between
//! `process_wry_events_inner` and `process_offscreen_events_inner` by
//! abstracting over the pane manager type via traits.
