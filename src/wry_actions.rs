//! WryAction dispatch — processes rendering actions against wry panes.
//!
//! Extracted from main.rs to reduce the size of the event loop.

use tracing::info;

/// Process a single WryAction against the wry pane manager.
///
/// Returns Ok(()) on success, Err(message) on failure.
pub fn process_wry_action(
    action: crate::app::WryAction,
    active_id: uuid::Uuid,
    wry_panes: &mut crate::servo::WryPaneManager,
    offscreen_panes: &mut crate::offscreen_webview::OffscreenWebViewManager,
    app_state: &mut Option<crate::app::AppState>,
    content_scripts: &crate::scripts::ContentScriptManager,
) -> Result<(), String> {
    match action {
        crate::app::WryAction::Navigate(url) => {
            if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                wry_pane.navigate(&url);
            } else if let Some(pane) = offscreen_panes.get_mut(&active_id) {
                pane.navigate(&url);
            } else {
                return Err(format!(
                    "No pane for navigation: {}",
                    &active_id.to_string()[..8]
                ));
            }
        }
        crate::app::WryAction::Back => {
            if wry_panes.get(&active_id).is_some() {
                if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                    wry_pane.execute_js(crate::servo::SCROLL_SAVE_JS);
                }
                wry_panes.back(&active_id);
            } else {
                offscreen_panes.back(&active_id);
            }
        }
        crate::app::WryAction::Forward => {
            if wry_panes.get(&active_id).is_some() {
                if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                    wry_pane.execute_js(crate::servo::SCROLL_SAVE_JS);
                }
                wry_panes.forward(&active_id);
            } else {
                offscreen_panes.forward(&active_id);
            }
        }
        crate::app::WryAction::Reload => {
            if wry_panes.get(&active_id).is_some() {
                wry_panes.reload(&active_id);
            } else {
                if let Some(pane) = offscreen_panes.get(&active_id) {
                    pane.reload();
                }
            }
        }
        crate::app::WryAction::ToggleBookmark => {
            let url_str;
            let title_str;
            if let Some(wry_pane) = wry_panes.get(&active_id) {
                url_str = wry_pane.url().to_string();
                title_str = wry_pane.title().to_string();
            } else if let Some(pane) = offscreen_panes.get(&active_id) {
                url_str = pane.url().to_string();
                title_str = pane.title().to_string();
            } else {
                return Ok(());
            }
            let display_title = if title_str.is_empty() { &url_str } else { &title_str };
            if let Some(app_state) = app_state
                && let Some(ref conn) = app_state.db
            {
                if crate::db::bookmarks::is_bookmarked(conn, &url_str) {
                    let _ = crate::db::bookmarks::remove_bookmark(conn, &url_str);
                    app_state.status_message =
                        format!("Bookmark removed: {}", display_title);
                } else {
                    let _ = crate::db::bookmarks::add_bookmark(conn, &url_str, display_title);
                    app_state.status_message = format!("Bookmarked: {}", display_title);
                }
            }
        }
        crate::app::WryAction::Autofill { js } => {
            if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                info!("Auto-filling credentials into active pane");
                wry_pane.execute_js(&js);
            } else if let Some(pane) = offscreen_panes.get(&active_id) {
                info!("Auto-filling credentials into offscreen pane");
                pane.execute_js(&js);
            }
        }
        crate::app::WryAction::ToggleDevTools => {
            #[cfg(target_os = "linux")]
            {
                if wry_panes.get(&active_id).is_some() {
                    wry_panes.open_devtools(&active_id);
                }
            }
        }
        crate::app::WryAction::SmoothScroll { x, y } => {
            if let Some(pane) = offscreen_panes.get_mut(&active_id) {
                pane.execute_js(&format!(
                    "window.scrollBy({{top: {}, left: {}, behavior: 'smooth'}})", y, x
                ));
            }
            if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                let js = format!(
                    "window.scrollBy({{top: {}, left: {}, behavior: 'smooth'}})", y, x
                );
                wry_pane.execute_js(&js);
            }
        }
        crate::app::WryAction::ScrollBy { x, y } => {
            if let Some(wry_pane) = wry_panes.get(&active_id) {
                let js = format!("window.scrollBy({}, {})", x, y);
                wry_pane.execute_js(&js);
            } else if let Some(pane) = offscreen_panes.get_mut(&active_id) {
                pane.scroll_by(x, y);
            }
        }
        crate::app::WryAction::ScrollTo { fraction } => {
            if let Some(wry_pane) = wry_panes.get(&active_id) {
                let js = format!(
                    "window.scrollTo(0, document.documentElement.scrollHeight * {})",
                    fraction
                );
                wry_pane.execute_js(&js);
            } else if let Some(pane) = offscreen_panes.get(&active_id) {
                let js = format!(
                    "window.scrollTo(0, document.documentElement.scrollHeight * {})",
                    fraction
                );
                pane.execute_js(&js);
            }
        }
        crate::app::WryAction::CaptureScrollFraction => {
            // Execute JS that computes the scroll fraction and sends it back via IPC.
            // The IPC handler in frame_tasks.rs stores it in AppState.marks.
            let js = r#"(function(){
                var h = document.documentElement.scrollHeight || document.body.scrollHeight || 1;
                var y = window.scrollY || window.pageYOffset || 0;
                window.ipc.postMessage(JSON.stringify({t:'scroll-fraction', frac: y/h}));
            })()"#;
            if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                wry_pane.execute_js(js);
            } else if let Some(pane) = offscreen_panes.get(&active_id) {
                pane.execute_js(js);
            }
        }
        crate::app::WryAction::RunJs(js) => {
            if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                wry_pane.execute_js(&js);
            } else if let Some(pane) = offscreen_panes.get(&active_id) {
                pane.execute_js(&js);
            }
        }
        crate::app::WryAction::EnterReaderMode => {
            if let Some(pane) = wry_panes.get_mut(&active_id) {
                let reader_js = r#"
(function() {
    var article = document.querySelector('article') ||
                  document.querySelector('[role="main"]') ||
                  document.querySelector('main') ||
                  document.querySelector('.post-content') ||
                  document.querySelector('.article-content') ||
                  document.querySelector('.content') ||
                  document.querySelector('#content') ||
                  document.querySelector('.entry-content') ||
                  document.body;
    
    if (!article) return;
    
    var title = document.title || '';
    var metaDesc = document.querySelector('meta[name="description"]');
    var desc = metaDesc ? metaDesc.getAttribute('content') : '';
    
    var text = '';
    var blocks = article.querySelectorAll('p, h1, h2, h3, h4, h5, h6, li, pre, blockquote, td');
    if (blocks.length > 3) {
        blocks.forEach(function(block) {
            var tag = block.tagName.toLowerCase();
            if (tag === 'p' || tag === 'li' || tag === 'td') {
                text += block.textContent.trim() + '\n\n';
            } else if (tag.match(/^h[1-6]$/)) {
                text += '\n' + '#'.repeat(parseInt(tag[1])) + ' ' + block.textContent.trim() + '\n\n';
            } else if (tag === 'pre') {
                text += '\n```\n' + block.textContent.trim() + '\n```\n\n';
            } else if (tag === 'blockquote') {
                text += '> ' + block.textContent.trim().replace(/\n/g, '\n> ') + '\n\n';
            }
        });
    } else {
        text = article.textContent.trim();
    }
    
    if (!window._aileron_original_html) {
        window._aileron_original_html = document.documentElement.innerHTML;
        window._aileron_original_title = document.title;
    }
    
    var html = '<!DOCTYPE html><html><head><meta charset="utf-8"><title>' + title + ' (Reader)</title>' +
        '<style>' +
        'body { background: #1a1a1a; color: #d4d4d4; font-family: serif; max-width: 680px; margin: 0 auto; padding: 40px 20px; line-height: 1.7; }' +
        'h1, h2, h3 { color: #e0e0e0; margin-top: 1.5em; }' +
        'a { color: #4db4ff; }' +
        'pre { background: #2a2a2a; padding: 12px; border-radius: 4px; overflow-x: auto; font-family: monospace; font-size: 0.9em; }' +
        'blockquote { border-left: 3px solid #4db4ff; padding-left: 16px; color: #aaa; }' +
        '.meta { color: #666; margin-bottom: 2em; font-size: 0.9em; }' +
        '</style></head><body>' +
        '<h1>' + title + '</h1>' +
        (desc ? '<p class="meta">' + desc + '</p>' : '') +
        '<div style="white-space: pre-wrap;">' + text + '</div>' +
        '</body></html>';
    
    document.open();
    document.write(html);
    document.close();
})()
"#.to_string();
                pane.execute_js(&reader_js);
            } else if let Some(pane) = offscreen_panes.get_mut(&active_id) {
                let reader_js = r#"
(function() {
    var article = document.querySelector('article') ||
                  document.querySelector('[role="main"]') ||
                  document.querySelector('main') ||
                  document.querySelector('.post-content') ||
                  document.querySelector('.article-content') ||
                  document.querySelector('.content') ||
                  document.querySelector('#content') ||
                  document.querySelector('.entry-content') ||
                  document.body;
    if (!article) return;
    var title = document.title || '';
    if (!window._aileron_original_html) {
        window._aileron_original_html = document.documentElement.innerHTML;
        window._aileron_original_title = document.title;
    }
    var text = article.textContent.trim();
    var html = '<!DOCTYPE html><html><head><meta charset="utf-8"><title>' + title + ' (Reader)</title>' +
        '<style>body{background:#1a1a1a;color:#d4d4d4;font-family:serif;max-width:680px;margin:0 auto;padding:40px 20px;line-height:1.7;}</style></head><body>' +
        '<h1>' + title + '</h1><div style="white-space:pre-wrap;">' + text + '</div></body></html>';
    document.open();
    document.write(html);
    document.close();
})()
"#.to_string();
                pane.execute_js(&reader_js);
            }
        }
        crate::app::WryAction::ExitReaderMode => {
            if let Some(pane) = wry_panes.get_mut(&active_id) {
                let restore_js = r#"
(function() {
    if (window._aileron_original_html) {
        document.open();
        document.write(window._aileron_original_html);
        document.close();
        document.title = window._aileron_original_title || '';
        window._aileron_original_html = null;
        window._aileron_original_title = null;
    } else {
        location.reload();
    }
})()
"#
                .to_string();
                pane.execute_js(&restore_js);
            } else if let Some(pane) = offscreen_panes.get(&active_id) {
                let restore_js = r#"
(function() {
    if (window._aileron_original_html) {
        document.open();
        document.write(window._aileron_original_html);
        document.close();
        document.title = window._aileron_original_title || '';
        window._aileron_original_html = null;
        window._aileron_original_title = null;
    } else {
        location.reload();
    }
})()
"#
                .to_string();
                pane.execute_js(&restore_js);
            }
        }
        crate::app::WryAction::EnterMinimalMode => {
            if let Some(pane) = wry_panes.get_mut(&active_id) {
                let current_url = pane.url().clone();
                if !current_url.as_str().starts_with("aileron://") {
                    let minimal_js = format!(
                        r#"
(function() {{
    if (!window._aileron_original_url) {{
        window._aileron_original_url = '{url}';
    }}
    var style = document.createElement('style');
    style.id = 'aileron-minimal-mode';
    style.textContent = 'img, video, audio, iframe, svg, canvas, [style*="background-image"], .ad, .banner, .popup, .overlay {{ display: none !important; }}';
    document.head.appendChild(style);
    var scripts = document.querySelectorAll('script');
    scripts.forEach(function(s) {{ s.remove(); }});
    document.body.setAttribute('onload', '');
}})()
"#,
                        url = current_url.as_str().replace('\'', "\\'")
                    );
                    pane.execute_js(&minimal_js);
                }
            } else if let Some(pane) = offscreen_panes.get(&active_id) {
                let current_url = pane.url().clone();
                if !current_url.as_str().starts_with("aileron://") {
                    let minimal_js = format!(
                        r#"
(function() {{
    if (!window._aileron_original_url) {{
        window._aileron_original_url = '{url}';
    }}
    var style = document.createElement('style');
    style.id = 'aileron-minimal-mode';
    style.textContent = 'img, video, audio, iframe, svg, canvas, [style*="background-image"], .ad, .banner, .popup, .overlay {{ display: none !important; }}';
    document.head.appendChild(style);
}})()
"#,
                        url = current_url.as_str().replace('\'', "\\'")
                    );
                    pane.execute_js(&minimal_js);
                }
            }
        }
        crate::app::WryAction::ExitMinimalMode => {
            if let Some(pane) = wry_panes.get_mut(&active_id) {
                let restore_js = r#"
(function() {
    var style = document.getElementById('aileron-minimal-mode');
    if (style) style.remove();
    if (window._aileron_original_url) {
        location.href = window._aileron_original_url;
        window._aileron_original_url = null;
    }
})()
"#
                .to_string();
                pane.execute_js(&restore_js);
            } else if let Some(pane) = offscreen_panes.get(&active_id) {
                let restore_js = r#"
(function() {
    var style = document.getElementById('aileron-minimal-mode');
    if (style) style.remove();
    if (window._aileron_original_url) {
        location.href = window._aileron_original_url;
        window._aileron_original_url = null;
    }
})()
"#
                .to_string();
                pane.execute_js(&restore_js);
            }
        }
        crate::app::WryAction::SaveWorkspace { name, .. } => {
            let mut pane_urls: std::collections::HashMap<uuid::Uuid, String> = wry_panes
                .pane_ids()
                .into_iter()
                .filter_map(|id| wry_panes.url_for(&id).map(|url| (id, url.to_string())))
                .collect();
            for (id, pane) in offscreen_panes.iter() {
                pane_urls.insert(*id, pane.url().to_string());
            }
            if let Some(app_state) = app_state {
                match app_state.save_workspace_with_urls(&name, &pane_urls) {
                    Ok(()) => {
                        app_state.status_message = format!("Workspace saved: {}", name);
                        info!("Workspace saved: {} ({} panes)", name, pane_urls.len());
                    }
                    Err(e) => {
                        return Err(format!("Save failed: {}", e));
                    }
                }
            }
        }
        crate::app::WryAction::ShowPaneError { message } => {
            if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                let encoded = urlencoding::encode(&message);
                let error_url = url::Url::parse(&format!("aileron://error?msg={}", encoded))
                    .map_err(|e| format!("Invalid error URL: {}", e))?;
                wry_pane.navigate(&error_url);
            } else if let Some(pane) = offscreen_panes.get_mut(&active_id) {
                let encoded = urlencoding::encode(&message);
                let error_url = url::Url::parse(&format!("aileron://error?msg={}", encoded))
                    .map_err(|e| format!("Invalid error URL: {}", e))?;
                pane.navigate(&error_url);
            } else {
                return Err(format!(
                    "No pane to show error: {}",
                    &active_id.to_string()[..8]
                ));
            }
        }
        crate::app::WryAction::ListContentScripts => {
            let count = content_scripts.all_scripts().len();
            let enabled: usize = content_scripts
                .all_scripts()
                .iter()
                .filter(|s| s.enabled)
                .count();
            let names: Vec<&str> = content_scripts
                .all_scripts()
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            let msg = format!(
                "Scripts: {} ({} enabled): {}",
                count,
                enabled,
                if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                }
            );
            if let Some(app_state) = app_state {
                app_state.status_message = msg;
            }
        }
        crate::app::WryAction::GetNetworkLog => {
            if let Some(pane) = wry_panes.get_mut(&active_id) {
                let (tx, rx) = std::sync::mpsc::channel();
                pane.execute_js_with_callback(crate::servo::NETWORK_LOG_JS, move |json| {
                    let _ = tx.send(json);
                });
                if let Ok(json) = rx.try_recv() {
                    if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                        let count = entries.len();
                        let lines: Vec<String> = entries
                            .iter()
                            .take(20)
                            .map(|e| {
                                let method =
                                    e.get("method").and_then(|v| v.as_str()).unwrap_or("?");
                                let url = e.get("url").and_then(|v| v.as_str()).unwrap_or("?");
                                let status = e
                                    .get("status")
                                    .map(|v| {
                                        if v.is_null() {
                                            "...".to_string()
                                        } else {
                                            v.as_i64()
                                                .map(|n| n.to_string())
                                                .or_else(|| v.as_str().map(String::from))
                                                .unwrap_or_else(|| "?".into())
                                        }
                                    })
                                    .unwrap_or_else(|| "?".to_string());
                                let short_url = if url.len() > 60 {
                                    format!("{}...", &url[..57])
                                } else {
                                    url.to_string()
                                };
                                format!("{} {} [{}]", method, short_url, status)
                            })
                            .collect();
                        if let Some(app_state) = app_state {
                            app_state.status_message = format!(
                                "Network ({}): {}",
                                count,
                                if lines.is_empty() {
                                    "empty".into()
                                } else {
                                    lines.join(" \u{2502} ")
                                }
                            );
                        }
                    }
                } else if let Some(app_state) = app_state {
                    app_state.status_message = "Network log: collecting...".into();
                }
            } else if let Some(app_state) = app_state {
                app_state.status_message = "Network log: not available in offscreen mode".into();
            }
        }
        crate::app::WryAction::ClearNetworkLog => {
            if let Some(pane) = wry_panes.get_mut(&active_id) {
                pane.execute_js(crate::servo::NETWORK_CLEAR_JS);
            }
            if let Some(app_state) = app_state {
                app_state.status_message = "Network log cleared".into();
            }
        }
        crate::app::WryAction::GetConsoleLog => {
            if let Some(pane) = wry_panes.get_mut(&active_id) {
                let (tx, rx) = std::sync::mpsc::channel();
                pane.execute_js_with_callback(crate::servo::CONSOLE_LOG_JS, move |json| {
                    let _ = tx.send(json);
                });
                if let Ok(json) = rx.try_recv() {
                    if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                        let count = entries.len();
                        let lines: Vec<String> = entries
                            .iter()
                            .take(20)
                            .map(|e| {
                                let level = e.get("level").and_then(|v| v.as_str()).unwrap_or("?");
                                let msg = e.get("msg").and_then(|v| v.as_str()).unwrap_or("?");
                                let short_msg = if msg.len() > 50 {
                                    format!("{}...", &msg[..47])
                                } else {
                                    msg.to_string()
                                };
                                format!("[{}] {}", level, short_msg)
                            })
                            .collect();
                        if let Some(app_state) = app_state {
                            app_state.status_message = format!(
                                "Console ({}): {}",
                                count,
                                if lines.is_empty() {
                                    "empty".into()
                                } else {
                                    lines.join(" \u{2502} ")
                                }
                            );
                        }
                    }
                } else if let Some(app_state) = app_state {
                    app_state.status_message = "Console log: collecting...".into();
                }
            } else if let Some(app_state) = app_state {
                app_state.status_message = "Console log: not available in offscreen mode".into();
            }
        }
        crate::app::WryAction::ClearConsoleLog => {
            if let Some(pane) = wry_panes.get_mut(&active_id) {
                pane.execute_js(crate::servo::CONSOLE_CLEAR_JS);
            }
            if let Some(app_state) = app_state {
                app_state.status_message = "Console log cleared".into();
            }
        }
        crate::app::WryAction::SaveConfig => {
            let Some(app_state) = app_state.as_mut() else {
                return Err("No app state for SaveConfig".into());
            };
            match crate::config::Config::save(&app_state.config) {
                Ok(()) => {
                    app_state.status_message = "Config saved".into();
                }
                Err(e) => {
                    app_state.status_message = format!("Save failed: {}", e);
                }
            }
        }
        crate::app::WryAction::Print => {
            if let Some(pane) = offscreen_panes.get_mut(&active_id) {
                pane.print();
            } else if wry_panes.get(&active_id).is_some() {
                info!("Print requested for native pane (not supported)");
                if let Some(app_state) = app_state {
                    app_state.status_message = "Print: use :print in offscreen mode".into();
                }
            } else if let Some(app_state) = app_state {
                app_state.status_message = "No active pane to print".into();
            }
        }
        crate::app::WryAction::ToggleMute => {
            let js = if let Some(app_state) = app_state {
                let active_id = app_state.wm.active_pane_id();
                if app_state.muted_pane_ids.contains(&active_id) {
                    app_state.muted_pane_ids.remove(&active_id);
                    "document.querySelectorAll('video, audio').forEach(function(el) { el.muted = false; });"
                } else {
                    app_state.muted_pane_ids.insert(active_id);
                    "document.querySelectorAll('video, audio').forEach(function(el) { el.muted = true; el.pause(); });"
                }
            } else {
                "document.querySelectorAll('video, audio').forEach(function(el) { el.muted = true; });"
            };
            if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                wry_pane.execute_js(js);
            } else if let Some(pane) = offscreen_panes.get_mut(&active_id) {
                pane.execute_js(js);
            }
        }
    }
    Ok(())
}
