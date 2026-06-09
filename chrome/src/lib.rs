//! Aileron Chrome -- Leptos WASM UI for the browser chrome.
//!
//! This crate compiles to `wasm32-unknown-unknown` and runs inside a
//! transparent wry child webview that overlays the native content area.
//! The Rust backend communicates state updates via `evaluate_script()`
//! and receives requests via wry IPC (`window.ipc.postMessage`).

use aileron_shared::{ChromeState, PaletteItem, SearchCategory};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Global state bridge
// ---------------------------------------------------------------------------

/// Holds the reactive chrome state signal. Provided as context to all children.
#[derive(Clone)]
struct ChromeStore {
    state: RwSignal<ChromeState>,
}

// ---------------------------------------------------------------------------
// WASM entry point
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Root application component.
#[component]
fn App() -> impl IntoView {
    let state = RwSignal::new(ChromeState::default());

    // Register JS callback so Rust backend can push state updates.
    register_bridge(state);

    provide_context(ChromeStore { state });

    view! {
        <div class="chrome-root" role="application" aria-label="Aileron browser chrome">
            <StatusBar />
            <UrlBar />
            <TabSidebar />
            <FindBar />
            <CommandPalette />
        </div>
    }
}

/// Status bar showing current mode, URL, and status message.
#[component]
fn StatusBar() -> impl IntoView {
    let store = expect_context::<ChromeStore>();

    let mode = move || store.state.read().mode.as_str();
    let mode_class = move || store.state.read().mode_color.clone();
    let status_msg = move || store.state.read().status_message.clone();
    let url = move || store.state.read().url.clone();
    let find_sub = move || {
        if store.state.read().find_bar_open {
            " FIND"
        } else {
            ""
        }
    };

    view! {
        <div class="status-bar" role="status" aria-label="Browser status bar">
            <span class=mode_class aria-label=move || format!("Mode: {}", mode())>{mode}{find_sub}</span>
            <span class="status-url" aria-label="Current URL">{url}</span>
            <span class="status-msg" aria-live="polite">{status_msg}</span>
        </div>
    }
}

/// URL bar for the active pane.
#[component]
fn UrlBar() -> impl IntoView {
    let store = expect_context::<ChromeStore>();

    let url = move || store.state.read().url.clone();
    let focused = move || store.state.read().url_bar_focused;

    view! {
        <div class="url-bar" role="navigation" aria-label="URL bar">
            <input
                type="text"
                class="url-input"
                prop:value=url
                prop:disabled=move || !focused()
                aria-label="URL input"
                placeholder="Enter URL or search..."
            />
        </div>
    }
}

/// Tab sidebar showing all panes in the BSP tree.
#[component]
fn TabSidebar() -> impl IntoView {
    let store = expect_context::<ChromeStore>();

    let panes = move || store.state.read().panes.clone();
    let sidebar_right = move || store.state.read().tab_sidebar_right;

    let on_drag_start = move |ev: web_sys::DragEvent, pane_id: String| {
        if let Some(data_transfer) = ev.data_transfer() {
            let _ = data_transfer.set_data("text/plain", &pane_id);
            let _ = data_transfer.set_effect_allowed("move");
        }
    };

    let on_drag_over = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        if let Some(data_transfer) = ev.data_transfer() {
            let _ = data_transfer.set_drop_effect("move");
        }
    };

    let on_drop = move |ev: web_sys::DragEvent, target_id: String| {
        ev.prevent_default();
        if let Some(data_transfer) = ev.data_transfer()
            && let Ok(from_id) = data_transfer.get_data("text/plain")
            && !from_id.is_empty()
            && from_id != target_id
        {
            let json = format!(
                r#"{{"kind":"reorder","payload":{{"from_id":"{from_id}","to_id":"{target_id}"}}}}"#
            );
            send_ipc_message(json);
        }
    };

    view! {
        <div class="tab-sidebar" class:sidebar-right=sidebar_right role="tablist" aria-label="Open tabs">
            <For
                each=panes
                key=|p| p.id.clone()
                let(pane)
            >
                {let pane_id_for_drag = pane.id.clone();
                let pane_id_for_drop = pane.id.clone();
                let title_for_aria = pane.title.clone();
                view! {
                    <div
                        class="tab-item"
                        class:tab-active=pane.active
                        role="tab"
                        aria-selected=move || pane.active
                        aria-label=move || title_for_aria.clone()
                        draggable="true"
                        on:dragstart=move |ev| on_drag_start(ev, pane_id_for_drag.clone())
                        on:dragover=on_drag_over
                        on:drop=move |ev| on_drop(ev, pane_id_for_drop.clone())
                    >
                        <span class="tab-title">{pane.title}</span>
                        <span class="tab-close" role="button" aria-label="Close tab">&times;</span>
                    </div>
                }}
            </For>
            <div class="tab-new" role="button" aria-label="New tab">+</div>
        </div>
    }
}

/// Find-in-page bar. Shown when `find_bar_open` is true in the pushed state.
/// Sends IPC messages back to Rust for find operations.
#[component]
fn FindBar() -> impl IntoView {
    let store = expect_context::<ChromeStore>();

    let visible = move || store.state.read().find_bar_open;
    let query = move || store.state.read().find_query.clone();

    let (local_query, set_local_query) = signal(String::new());

    // Sync the local query with the pushed state.
    // This keeps the input in sync when the Rust backend opens the bar
    // (find_query is cleared on open) or when the user presses Escape.
    Effect::new(move |_| {
        let pushed = query();
        set_local_query.set(pushed);
    });

    let on_keydown = move |ev: web_sys::KeyboardEvent| {
        let key = ev.key();
        match key.as_str() {
            "Escape" => {
                send_find_ipc("close", "");
            }
            "Enter" => {
                let q = local_query.get();
                send_find_ipc("submit", &q);
            }
            _ => {}
        }
    };

    let on_input = move |ev: web_sys::Event| {
        if let Some(target) = ev.target()
            && let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>()
        {
            set_local_query.set(input.value());
        }
    };

    let on_find_next = move |_| {
        send_find_ipc("next", "");
    };

    let on_find_prev = move |_| {
        send_find_ipc("prev", "");
    };

    let on_find_close = move |_| {
        send_find_ipc("close", "");
    };

    view! {
        <Show when=visible fallback=|| ()>
            <div class="find-bar" role="search" aria-label="Find in page">
                <span class="find-label">Find:</span>
                <input
                    type="text"
                    class="find-input"
                    prop:value=move || local_query.get()
                    on:keydown=on_keydown
                    on:input=on_input
                    placeholder="Search in page..."
                    aria-label="Find in page"
                />
                <button class="find-btn" on:click=on_find_next title="Find next" aria-label="Find next">
                    "\u{2193}"
                </button>
                <button class="find-btn" on:click=on_find_prev title="Find previous" aria-label="Find previous">
                    "\u{2191}"
                </button>
                <button class="find-btn find-close" on:click=on_find_close title="Close" aria-label="Close find bar">
                    "\u{2715}"
                </button>
            </div>
        </Show>
    }
}

/// Command palette overlay. Shown when `command_palette_open` is true.
/// Sends IPC messages back to Rust for palette operations.
#[component]
fn CommandPalette() -> impl IntoView {
    let store = expect_context::<ChromeStore>();

    let visible = move || store.state.read().command_palette_open;
    let results = move || store.state.read().palette_results.clone();
    let selected = move || store.state.read().palette_selected;

    let (local_query, set_local_query) = signal(String::new());

    // Sync local query with pushed state (clears on open/close).
    Effect::new(move |_| {
        let open = store.state.read().command_palette_open;
        // Always clear local query when palette closes.
        if !open {
            set_local_query.set(String::new());
        }
    });

    let on_keydown = move |ev: web_sys::KeyboardEvent| {
        let key = ev.key();
        match key.as_str() {
            "Escape" => {
                send_palette_ipc("close", "");
            }
            "Enter" => {
                send_palette_ipc("select", "");
            }
            "ArrowDown" => {
                // Send the current query so Rust updates selection
                let q = local_query.get();
                send_palette_ipc("input", &q);
            }
            "ArrowUp" => {
                let q = local_query.get();
                send_palette_ipc("input", &q);
            }
            "Backspace" => {
                // Let the browser handle the actual deletion; then sync.
                // The on:input handler fires after the key is processed.
            }
            _ => {}
        }
    };

    let on_input = move |ev: web_sys::Event| {
        if let Some(target) = ev.target()
            && let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>()
        {
            let val = input.value();
            set_local_query.set(val.clone());
            // Send to Rust for filtering and result computation.
            send_palette_ipc("input", &val);
        }
    };

    let on_item_click = move |item: PaletteItem| {
        // Navigate or execute the selected item.
        // For URL items (History, Bookmark, OpenTab), navigate.
        // For commands, send as action.
        match item.category {
            SearchCategory::History | SearchCategory::Bookmark | SearchCategory::OpenTab => {
                send_navigate_ipc(&item.id);
            }
            _ => {
                send_action_ipc(&item.id);
            }
        }
    };

    view! {
        <Show when=visible fallback=|| ()>
            <div class="palette-backdrop" role="dialog" aria-label="Command palette" aria-modal="true">
                <div class="palette-container">
                    <div class="palette-input-row">
                        <span class="palette-prompt">": "</span>
                        <input
                            type="text"
                            class="palette-input"
                            placeholder="Search commands, history, bookmarks..."
                            autofocus=true
                            on:keydown=on_keydown
                            on:input=on_input
                            aria-label="Command input"
                            aria-autocomplete="list"
                            aria-expanded="true"
                        />
                    </div>
                    <div class="palette-results" role="listbox" aria-label="Results">
                        <For
                            each=results
                            key=|item| item.id.clone()
                            let:item
                        >
                            {move || {
                                let sel = selected();
                                let item_clone = item.clone();
                                let is_selected = item_clone.id == {
                                    results().get(sel).map(|r| r.id.clone()).unwrap_or_default()
                                };
                                view! {
                                    <div
                                        class="palette-item"
                                        class:palette-selected=is_selected
                                        role="option"
                                        aria-selected=is_selected
                                        on:click=move |_| on_item_click(item_clone.clone())
                                    >
                                        <span class="palette-cat">
                                            {category_label(&item.category)}
                                        </span>
                                        <span class="palette-label">
                                            {item.label.clone()}
                                        </span>
                                        <span class="palette-desc">
                                            {item.description.clone()}
                                        </span>
                                    </div>
                                }
                            }}
                        </For>
                    </div>
                </div>
            </div>
        </Show>
    }
}

// ---------------------------------------------------------------------------
// IPC helper functions
// ---------------------------------------------------------------------------

/// Send a find bar IPC message to the Rust backend.
fn send_find_ipc(sub: &str, query: &str) {
    let json = format!(r#"{{"kind":"find","payload":{{"sub":"{sub}","query":"{query}"}}}}"#);
    send_ipc_message(json);
}

/// Send a command palette IPC message to the Rust backend.
fn send_palette_ipc(sub: &str, query: &str) {
    let json = format!(r#"{{"kind":"palette","payload":{{"sub":"{sub}","query":"{query}"}}}}"#);
    send_ipc_message(json);
}

/// Send a navigate IPC message to the Rust backend.
fn send_navigate_ipc(url: &str) {
    let json = format!(r#"{{"kind":"navigate","payload":{{"url":"{url}"}}}}"#);
    send_ipc_message(json);
}

/// Send an action IPC message to the Rust backend.
fn send_action_ipc(action: &str) {
    let json = format!(r#"{{"kind":"action","payload":{{"action":"{action}"}}}}"#);
    send_ipc_message(json);
}

/// Display label for a search category.
fn category_label(cat: &SearchCategory) -> &'static str {
    match cat {
        SearchCategory::History => "[H]",
        SearchCategory::Bookmark => "[B]",
        SearchCategory::Command => "[>]",
        SearchCategory::OpenTab => "[T]",
        SearchCategory::Setting => "[S]",
        SearchCategory::Credential => "[key]",
        SearchCategory::Custom => "[L]",
    }
}

// ---------------------------------------------------------------------------
// JS bridge: Rust -> WASM communication
// ---------------------------------------------------------------------------

/// Register global JavaScript functions that the Rust backend calls via
/// `evaluate_script()` to push state updates.
fn register_bridge(state: RwSignal<ChromeState>) {
    let closure = Closure::<dyn Fn(String)>::new(move |json_str: String| {
        if let Ok(new_state) = serde_json::from_str::<ChromeState>(&json_str) {
            state.set(new_state);
        }
    });

    // Attach to window so Rust can call:
    //   webview.evaluate_script("window.updateChromeState('{...}')")
    js_sys::Reflect::set(
        &js_sys::global(),
        &JsValue::from_str("updateChromeState"),
        closure.as_ref().unchecked_ref(),
    )
    .expect("failed to register updateChromeState");

    closure.forget();
}

/// Send a request from chrome to the Rust backend via wry IPC.
///
/// Call from JS: `window.ipc.postMessage(JSON.stringify({kind, payload}))`
#[wasm_bindgen]
pub fn send_ipc_message(json_str: String) {
    let ipc = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("ipc")).ok();
    let ipc = ipc.filter(|v| !v.is_undefined() && !v.is_null());
    if let Some(ipc) = ipc
        && let Some(post) = js_sys::Reflect::get(&ipc, &JsValue::from_str("postMessage")).ok()
        && let Ok(post_fn) = post.dyn_into::<js_sys::Function>()
    {
        let _ = post_fn
            .call1(&ipc, &JsValue::from_str(&json_str))
            .map_err(|e| {
                web_sys::console::error_1(&format!("Failed to post IPC message: {e:?}").into());
            });
    }
}
