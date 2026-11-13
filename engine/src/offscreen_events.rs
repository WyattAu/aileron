//! Offscreen webview event pump (S4 stage 2d-3: moved from frame_tasks).

use std::sync::Arc;

use crate::offscreen_webview::OffscreenWebViewManager;
use aileron_core::extensions::web_request::WebRequestInterceptorRegistry;
use aileron_core::mcp::bridge::McpBridge;
use aileron_core::scripts::ContentScriptManager;

/// Update the active tab's URL/title in the BSP tree pane's TabList
/// to match the webview's current state. This keeps tab data in sync
/// so that tab switching navigates to the correct URL.
pub fn sync_tab_url_title(
    core: &mut aileron_core::state::CoreState,
    pane_id: uuid::Uuid,
    url: &str,
    title: &str,
) {
    if let Some(pane) = core
        .wm
        .root_mut()
        .and_then(|root| aileron_core::wm::BspTree::find_pane_mut(root, pane_id))
    {
        let active_tab = pane.tabs.active_mut();
        if let Ok(parsed) = url::Url::parse(url) {
            active_tab.url = parsed;
        }
        active_tab.title = title.to_string();
    }
}

fn process_offscreen_events_inner(
    core: &mut aileron_core::state::CoreState,
    offscreen_panes: &mut OffscreenWebViewManager,
    content_scripts: &ContentScriptManager,
    adblocker: &aileron_core::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    let events = offscreen_panes.drain_all_events_flat();
    let ipc_messages = crate::ipc::process_events(
        core,
        offscreen_panes,
        content_scripts,
        adblocker,
        interceptor_registry,
        events,
    );
    for (pane_id, message) in ipc_messages {
        crate::ipc::handle_ipc_message_offscreen(core, offscreen_panes, pane_id, &message);
    }
}

pub fn process_offscreen_events(
    core: &mut aileron_core::state::CoreState,
    offscreen_panes: &mut OffscreenWebViewManager,
    content_scripts: &ContentScriptManager,
    mcp_bridge: &McpBridge,
    adblocker: &aileron_core::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    process_offscreen_events_inner(
        core,
        offscreen_panes,
        content_scripts,
        adblocker,
        interceptor_registry,
    );
    // Mirror the wry-pane path (process_wry_events): push the active pane's
    // url/title into the shared MCP state so read_active_pane and friends see
    // offscreen panes. The headed runtime masked this via its wry panes; the
    // agent runtime (offscreen-only) has no wry panes to mask it.
    let active_id = core.wm.active_pane_id();
    if let Some(pane) = offscreen_panes.get(&active_id) {
        mcp_bridge.update_state(pane.url().as_str(), pane.title());
        mcp_bridge.update_active_meta(
            core.mcp_settled_epoch.get(&active_id).copied().unwrap_or(0),
            pane.is_loading(),
        );
    }
}
