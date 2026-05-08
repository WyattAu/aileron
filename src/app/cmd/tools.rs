use super::super::AppState;

pub fn cmd_tools(state: &mut AppState, query: &str) -> Option<()> {
    if let Some(pattern) = query.strip_prefix("grep ") {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            state.status_message = "Usage: :grep <pattern> [path]".into();
            return Some(());
        }
        let (pattern, search_path) = if pattern.contains(' ') {
            let mut parts = pattern.splitn(2, ' ');
            (parts.next().unwrap_or(""), parts.next().unwrap_or("."))
        } else {
            (pattern, ".")
        };

        let output = if std::path::PathBuf::from("/usr/bin/rg").exists()
            || std::path::PathBuf::from("/usr/local/bin/rg").exists()
        {
            std::process::Command::new("rg")
                .args(["--no-heading", "-n", "-i", pattern, search_path])
                .output()
        } else {
            std::process::Command::new("grep")
                .args(["-rn", "-i", pattern, search_path])
                .output()
        };

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = stdout.lines().take(15).collect();
                let total = stdout.lines().count();
                if lines.is_empty() {
                    state.status_message = "No matches found".into();
                } else {
                    let results: Vec<String> = lines
                        .iter()
                        .map(|l| {
                            if l.len() > 80 {
                                format!("{}...", &l[..77])
                            } else {
                                l.to_string()
                            }
                        })
                        .collect();
                    let suffix = if total > 15 {
                        format!(" (+{} more)", total - 15)
                    } else {
                        String::new()
                    };
                    state.status_message = format!("{}{}", results.join(" │ "), suffix);
                }
            }
            Ok(output) => {
                state.status_message = format!("grep: {}", String::from_utf8_lossy(&output.stderr));
            }
            Err(e) => {
                state.status_message = format!("grep failed: {e}");
            }
        }
        return Some(());
    }

    if query == "git-status" || query == "gs" {
        if let Some(root) =
            crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path())
        {
            match std::process::Command::new("git")
                .args(["-C", &root.to_string_lossy(), "status", "--short"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<&str> = stdout.lines().take(10).collect();
                    if lines.is_empty() {
                        state.status_message = "Working tree clean".into();
                    } else {
                        let total = stdout.lines().count();
                        let suffix = if total > 10 {
                            format!(" (+{} more)", total - 10)
                        } else {
                            String::new()
                        };
                        state.status_message = format!("{}{}", lines.join(" │ "), suffix);
                    }
                }
                Ok(output) => {
                    state.status_message =
                        format!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
                }
                Err(e) => state.status_message = format!("git failed: {e}"),
            }
        } else {
            state.status_message = "Not in a git repository".into();
        }
        return Some(());
    }

    if query == "git-log" || query == "gl" {
        if let Some(root) =
            crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path())
        {
            match std::process::Command::new("git")
                .args(["-C", &root.to_string_lossy(), "log", "--oneline", "-10"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    state.status_message = if stdout.is_empty() {
                        "No commits".into()
                    } else {
                        format!("Log: {}", stdout.replace('\n', " │ "))
                    };
                }
                Ok(output) => {
                    state.status_message =
                        format!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
                }
                Err(e) => state.status_message = format!("git failed: {e}"),
            }
        } else {
            state.status_message = "Not in a git repository".into();
        }
        return Some(());
    }

    if query == "git-diff" || query == "gd" {
        if let Some(root) =
            crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path())
        {
            match std::process::Command::new("git")
                .args(["-C", &root.to_string_lossy(), "diff", "--stat"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    state.status_message = if stdout.is_empty() {
                        "No changes".into()
                    } else {
                        format!("Diff: {}", stdout.replace('\n', " │ "))
                    };
                }
                Ok(output) => {
                    state.status_message =
                        format!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
                }
                Err(e) => state.status_message = format!("git failed: {e}"),
            }
        } else {
            state.status_message = "Not in a git repository".into();
        }
        return Some(());
    }

    if query == "terminal-clear" || query == "cls" {
        let active_id = state.wm.active_pane_id();
        if state.terminal_pane_ids.contains(&active_id) {
            state
                .pending_wry_actions
                .push_back(crate::app::WryAction::RunJs(
                r#"if (window._terminal && window._terminal.clear) { window._terminal.clear(); }"#
                    .into(),
            ));
            state.status_message = "Terminal cleared".into();
        } else {
            state.status_message = "Not a terminal pane".into();
        }
        return Some(());
    }

    if let Some(pattern) = query.strip_prefix("terminal-search ") {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            state.status_message = "Usage: :terminal-search <pattern>".into();
            return Some(());
        }
        let active_id = state.wm.active_pane_id();
        if state.terminal_pane_ids.contains(&active_id) {
            let escaped = pattern.replace('\\', "\\\\").replace('\'', "\\'");
            state
                .pending_wry_actions
                .push_back(crate::app::WryAction::RunJs(format!(
                    r#"
if (window._terminal && window._terminal.buffer) {{
    var buffer = window._terminal.buffer;
    var lines = buffer.active.bufferBase.getLines();
    var matches = [];
    for (var i = 0; i < lines.length; i++) {{
        if (lines[i].includes('{escaped}')) {{
            matches.push((i, lines[i].trim()));
        }}
    }}
    if (matches.length > 0) {{
        var firstMatch = matches[0];
        window._terminal.scrollToLine(firstMatch[0]);
    }}
    matches.length + ' match(es) in scrollback';
}}
"#
                )));
        } else {
            state.status_message = "Not a terminal pane".into();
        }
        return Some(());
    }

    if query == "print" {
        state
            .pending_wry_actions
            .push_back(crate::app::WryAction::Print);
        state.status_message = "Printing...".into();
        return Some(());
    }

    None
}
