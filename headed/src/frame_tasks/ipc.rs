use uuid::Uuid;

use crate::app::AppState;
use crate::servo::WryPaneManager;

pub(crate) fn handle_ipc_message(
    app_state: &mut AppState,
    wry_panes: &mut WryPaneManager,
    pane_id: Uuid,
    message: &str,
) {
    aileron_engine::ipc::handle_ipc_message_generic(
        &mut app_state.core,
        wry_panes,
        pane_id,
        message,
    );
}
