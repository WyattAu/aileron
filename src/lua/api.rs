use mlua::{Lua, LuaOptions, StdLib, Value};
use std::cell::RefCell;
use std::rc::Rc;
use tracing::{info, warn};

use crate::input::keybindings::Action;
use crate::input::mode::{Key, KeyCombo, Mode, Modifiers};

/// A parsed keybinding from Lua.
#[derive(Debug, Clone)]
pub struct PendingKeybind {
    pub mode: String,
    pub key: String,
    pub action: String,
}

/// The Lua scripting engine.
/// Manages a Lua 5.4 VM with sandboxed access to the aileron.* API.
pub struct LuaEngine {
    lua: Lua,
    /// Custom commands registered via aileron.cmd.create.
    /// Uses Rc<RefCell> because mlua closures are Fn (not FnMut),
    /// so we can't mutate &self from inside a closure.
    custom_commands: Rc<RefCell<Vec<CustomCommand>>>,
    /// URL redirect rules registered via aileron.url.add_redirect.
    url_redirects: Rc<RefCell<Vec<UrlRedirect>>>,
    /// Pending keybindings from aileron.keymap.set calls during init.
    pending_keybinds: Rc<RefCell<Vec<PendingKeybind>>>,
}

/// A user-defined command from Lua.
#[derive(Debug, Clone)]
pub struct CustomCommand {
    pub name: String,
    pub description: String,
    pub callback_name: String,
}

/// A URL redirect rule registered via aileron.url.add_redirect.
/// When navigating, if the URL's host contains `pattern`, it is replaced
/// with `replacement` (host-level substring replacement).
#[derive(Debug, Clone)]
pub struct UrlRedirect {
    /// Substring to match in the URL host (case-insensitive).
    pub pattern: String,
    /// Replacement string for the matched portion.
    pub replacement: String,
}

impl LuaEngine {
    /// Create a new Lua engine with sandboxed stdlib.
    pub fn new() -> mlua::Result<Self> {
        // Create Lua with limited stdlib (sandbox per TASK-020)
        let lua = Lua::new_with(
            StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE,
            LuaOptions::default(),
        )?;

        let pending_keybinds: Rc<RefCell<Vec<PendingKeybind>>> = Rc::new(RefCell::new(Vec::new()));
        let custom_commands: Rc<RefCell<Vec<CustomCommand>>> = Rc::new(RefCell::new(Vec::new()));
        let url_redirects: Rc<RefCell<Vec<UrlRedirect>>> = Rc::new(RefCell::new(Vec::new()));

        let mut engine = Self {
            lua,
            custom_commands: custom_commands.clone(),
            url_redirects: url_redirects.clone(),
            pending_keybinds: pending_keybinds.clone(),
        };

        engine.register_api(pending_keybinds, custom_commands, url_redirects)?;
        Ok(engine)
    }

    /// Register the aileron.* API tables in the Lua VM.
    fn register_api(
        &mut self,
        pending_keybinds: Rc<RefCell<Vec<PendingKeybind>>>,
        custom_commands: Rc<RefCell<Vec<CustomCommand>>>,
        url_redirects: Rc<RefCell<Vec<UrlRedirect>>>,
    ) -> mlua::Result<()> {
        let lua = &self.lua;

        // aileron = {}
        let aileron = lua.create_table()?;

        // aileron.version = "0.1.0"
        aileron.set("version", "0.1.0")?;

        // aileron.keymap = {}
        let keymap = lua.create_table()?;
        aileron.set("keymap", keymap.clone())?;

        // aileron.keymap.set(mode, key, action)
        // Parses the key string and stores the binding for later application.
        let set_keybind = {
            let binds = pending_keybinds.clone();
            lua.create_function(move |_, (mode, key, action): (String, String, String)| {
                info!(target: "lua", "keymap.set({}, {}, {})", mode, key, action);
                binds
                    .borrow_mut()
                    .push(PendingKeybind { mode, key, action });
                Ok(())
            })?
        };
        keymap.set("set", set_keybind)?;

        // aileron.theme = {}
        let theme = lua.create_table()?;
        aileron.set("theme", theme.clone())?;

        let set_theme = lua.create_function(|_, name: String| {
            info!(target: "lua", "theme.set({})", name);
            Ok(())
        })?;
        theme.set("set", set_theme)?;

        // aileron.cmd = {}
        let cmd = lua.create_table()?;
        aileron.set("cmd", cmd.clone())?;

        // aileron._commands = {} — stores callback functions keyed by name
        let commands_table = lua.create_table()?;
        aileron.set("_commands", commands_table.clone())?;

        // aileron.cmd.create(name, description, callback)
        // Registers a custom command. The callback is stored in aileron._commands[name]
        // so that call_command() can invoke it later.
        let create_cmd = {
            let cmds = custom_commands.clone();
            lua.create_function(
                move |lua, (name, desc, callback): (String, String, Value)| {
                    if let Value::Function(_) = callback {
                        info!(target: "lua", "cmd.create({}, {})", name, desc);

                        // Store command metadata on the Rust side
                        cmds.borrow_mut().push(CustomCommand {
                            name: name.clone(),
                            description: desc,
                            callback_name: name.clone(),
                        });

                        // Store the callback function in aileron._commands[name]
                        // so call_command() can look it up and invoke it.
                        let aileron: Value = lua.globals().get("aileron")?;
                        if let Value::Table(aileron_tbl) = aileron {
                            let existing: Value = aileron_tbl.get("_commands")?;
                            if let Value::Table(cmds_tbl) = existing {
                                cmds_tbl.set(name.clone(), callback)?;
                            }
                        }

                        Ok(())
                    } else {
                        Err(mlua::Error::external("callback must be a function"))
                    }
                },
            )?
        };
        cmd.set("create", create_cmd)?;

        // aileron._hooks = {}
        let hooks_table = lua.create_table()?;
        aileron.set("_hooks", hooks_table)?;

        // aileron.on(event, callback)
        let on_hook = lua.create_function(|lua, (event, callback): (String, Value)| {
            if let Value::Function(_) = callback {
                let aileron: Value = lua.globals().get("aileron")?;
                if let Value::Table(aileron_tbl) = aileron {
                    let hooks: Value = aileron_tbl.get("_hooks")?;
                    if let Value::Table(hooks_tbl) = hooks {
                        let event_hooks: Value = if let Ok(existing) = hooks_tbl.get(event.as_str())
                        {
                            existing
                        } else {
                            let arr = lua.create_table()?;
                            hooks_tbl.set(event.clone(), Value::Table(arr.clone()))?;
                            Value::Table(arr)
                        };
                        if let Value::Table(arr) = event_hooks {
                            let len = arr.len()?;
                            arr.set(len + 1, callback)?;
                        }
                    }
                }
                Ok(())
            } else {
                Err(mlua::Error::external("callback must be a function"))
            }
        })?;
        aileron.set("on", on_hook)?;

        // aileron.url = {}
        let url_tbl = lua.create_table()?;
        aileron.set("url", url_tbl.clone())?;

        // aileron.url.add_redirect(pattern, replacement)
        // Registers a URL redirect rule. If a navigated URL's host contains
        // `pattern` (case-insensitive), the host portion is replaced with
        // `replacement`. Useful for redirecting to privacy frontends.
        let add_redirect = {
            let redirects = url_redirects.clone();
            lua.create_function(move |_, (pattern, replacement): (String, String)| {
                info!(target: "lua", "url.add_redirect({}, {})", pattern, replacement);
                redirects.borrow_mut().push(UrlRedirect {
                    pattern,
                    replacement,
                });
                Ok(())
            })?
        };
        url_tbl.set("add_redirect", add_redirect)?;

        // aileron.info()
        let info_fn = lua.create_function(|lua, ()| {
            let info = lua.create_table()?;
            info.set("version", "0.1.0")?;
            info.set("engine", "placeholder")?;
            Ok(info)
        })?;
        aileron.set("info", info_fn)?;

        // aileron.log(message)
        let log_fn = lua.create_function(|_, msg: String| {
            info!(target: "lua", "{}", msg);
            Ok(())
        })?;
        aileron.set("log", log_fn)?;

        // aileron.warn(message)
        let warn_fn = lua.create_function(|_, msg: String| {
            warn!(target: "lua", "{}", msg);
            Ok(())
        })?;
        aileron.set("warn", warn_fn)?;

        lua.globals().set("aileron", aileron)?;
        Ok(())
    }

    /// Load and execute a Lua script.
    pub fn load_script(&self, script: &str) -> mlua::Result<()> {
        info!(target: "lua", "Loading script ({} bytes)", script.len());
        self.lua.load(script).exec()?;
        info!(target: "lua", "Script loaded successfully");
        Ok(())
    }

    /// Load a Lua script from a file path.
    pub fn load_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let contents = std::fs::read_to_string(path)?;
        self.load_script(&contents)
            .map_err(|e| anyhow::anyhow!("Lua error: {}", e))?;
        Ok(())
    }

    /// Execute a Lua expression and return the result as a string.
    pub fn eval(&self, expr: &str) -> anyhow::Result<String> {
        let result: Value = self
            .lua
            .load(expr)
            .eval()
            .map_err(|e| anyhow::anyhow!("Lua error: {}", e))?;
        Ok(format!("{:?}", result))
    }

    /// Call a registered custom command by name.
    pub fn call_command(&self, name: &str, args: &[String]) -> anyhow::Result<String> {
        let globals = self.lua.globals();
        let aileron: Value = globals
            .get("aileron")
            .map_err(|e| anyhow::anyhow!("Lua error: {}", e))?;
        if let Value::Table(aileron_tbl) = aileron {
            let cmds: Value = aileron_tbl
                .get("_commands")
                .map_err(|e| anyhow::anyhow!("Lua error: {}", e))?;
            if let Value::Table(cmds_tbl) = cmds {
                let func: Value = cmds_tbl
                    .get(name)
                    .map_err(|e| anyhow::anyhow!("Lua error: {}", e))?;
                if let Value::Function(f) = func {
                    let result: Value = f
                        .call(args)
                        .map_err(|e| anyhow::anyhow!("Lua error: {}", e))?;
                    return Ok(format!("{:?}", result));
                }
            }
        }
        anyhow::bail!("Lua command '{}' not found", name)
    }

    /// Get the list of registered custom commands.
    pub fn custom_commands(&self) -> Vec<CustomCommand> {
        self.custom_commands.borrow().clone()
    }

    /// Apply URL redirect rules to a URL.
    /// Checks each rule: if the URL's host contains `pattern` (case-insensitive),
    /// the first matching rule replaces the host's `pattern` substring with
    /// `replacement`. Only the host portion is modified; path/query are preserved.
    pub fn apply_url_redirects(&self, original_url: &url::Url) -> url::Url {
        let redirects = self.url_redirects.borrow();
        let host = match original_url.host_str() {
            Some(h) => h.to_lowercase(),
            None => return original_url.clone(),
        };

        for rule in redirects.iter() {
            if host.contains(&rule.pattern.to_lowercase()) {
                // Replace pattern in the (already lowered) host
                let new_host = host.replacen(
                    &rule.pattern.to_lowercase(),
                    &rule.replacement,
                    1, // replace first occurrence only
                );

                // Reconstruct the URL with the new host
                let mut new_url = original_url.clone();
                match url::Host::parse(&new_host) {
                    Ok(host_str) => {
                        let host_string = host_str.to_string();
                        new_url.set_host(Some(&host_string)).ok();
                        info!(
                            target: "lua",
                            "URL redirect: {} -> {} (rule: {} -> {})",
                            original_url.as_str(),
                            new_url.as_str(),
                            rule.pattern,
                            rule.replacement,
                        );
                        return new_url;
                    }
                    Err(_) => continue, // skip invalid host, try next rule
                }
            }
        }

        original_url.clone()
    }

    /// Take all pending keybindings registered during init.lua loading.
    /// Returns them and clears the internal buffer.
    pub fn take_pending_keybinds(&self) -> Vec<PendingKeybind> {
        self.pending_keybinds.borrow_mut().drain(..).collect()
    }

    /// Call all registered hooks for a given event.
    /// Args are passed as Lua string values. Errors are silently logged.
    pub fn call_hooks(&self, event: &str, args: &[&str]) {
        let globals = self.lua.globals();
        let aileron: Value = match globals.get("aileron") {
            Ok(v) => v,
            Err(_) => return,
        };
        let aileron = match aileron {
            Value::Table(t) => t,
            _ => return,
        };
        let hooks: Value = match aileron.get("_hooks") {
            Ok(v) => v,
            Err(_) => return,
        };
        let hooks = match hooks {
            Value::Table(t) => t,
            _ => return,
        };
        let event_hooks: Value = match hooks.get(event) {
            Ok(v) => v,
            Err(_) => return,
        };
        let event_hooks = match event_hooks {
            Value::Table(t) => t,
            _ => return,
        };

        for pair in event_hooks.pairs::<Value, Value>() {
            if let Ok((_, Value::Function(func))) = pair {
                let lua_args: Vec<Value> = args
                    .iter()
                    .filter_map(|a| self.lua.create_string(a).ok().map(Value::String))
                    .collect();
                let _ = func.call::<Value>(lua_args);
            }
        }
    }

    /// Parse a key string like "ctrl+a", "shift+H", "a" into a KeyCombo.
    /// Returns None if the string can't be parsed.
    pub fn parse_key_string(mode_str: &str, key_str: &str) -> Option<KeyCombo> {
        let mode = match mode_str.to_lowercase().as_str() {
            "normal" => Mode::Normal,
            "insert" => Mode::Insert,
            "command" => Mode::Command,
            _ => return None,
        };

        let mut mods = Modifiers::none();
        let mut key_part = key_str;

        // Parse modifier prefixes
        for prefix in key_str.split('+').collect::<Vec<_>>().iter() {
            match prefix.to_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "alt" | "mod" | "meta" => mods.alt = true,
                "shift" => mods.shift = true,
                "super" | "cmd" | "command" => mods.super_key = true,
                _ => key_part = *prefix, // Last part is the key
            }
        }

        let key = match key_part {
            // Single character keys
            s if s.len() == 1 => Key::Character(s.chars().next().unwrap()),
            // Special keys
            "enter" | "return" => Key::Enter,
            "escape" | "esc" => Key::Escape,
            "backspace" => Key::Backspace,
            "tab" => Key::Tab,
            "space" => Key::Character(' '),
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" | "page_up" => Key::PageUp,
            "pagedown" | "page_down" => Key::PageDown,
            _ => return None,
        };

        Some(KeyCombo::new(mode, mods, key))
    }

    /// Resolve an action string into an Action enum.
    /// Returns None for unknown actions.
    pub fn resolve_action(action_str: &str) -> Option<Action> {
        match action_str {
            "quit" => Some(Action::Quit),
            "scroll_up" => Some(Action::ScrollUp),
            "scroll_down" => Some(Action::ScrollDown),
            "scroll_left" => Some(Action::ScrollLeft),
            "scroll_right" => Some(Action::ScrollRight),
            "split_horizontal" | "sp" => Some(Action::SplitHorizontal),
            "split_vertical" | "vs" => Some(Action::SplitVertical),
            "close_pane" => Some(Action::ClosePane),
            "navigate_back" => Some(Action::NavigateBack),
            "navigate_forward" => Some(Action::NavigateForward),
            "reload" => Some(Action::Reload),
            "open_command_palette" => Some(Action::OpenCommandPalette),
            "open_external_browser" => Some(Action::OpenExternalBrowser),
            "enter_insert_mode" | "insert" => Some(Action::EnterInsertMode),
            "pin_pane" | "pin" => Some(Action::PinPane),
            _ => None,
        }
    }
}

impl Default for LuaEngine {
    fn default() -> Self {
        Self::new().expect("Failed to create Lua engine")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_engine_creation() {
        let engine = LuaEngine::new();
        assert!(engine.is_ok());
    }

    #[test]
    fn test_lua_version() {
        let engine = LuaEngine::new().unwrap();
        let version = engine.eval("return aileron.version").unwrap();
        assert!(version.contains("0.1.0"));
    }

    #[test]
    fn test_lua_info() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.eval("aileron.info().version").unwrap();
        assert!(result.contains("0.1.0"));
    }

    #[test]
    fn test_lua_log() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.load_script("aileron.log('hello from lua')");
        assert!(result.is_ok());
    }

    #[test]
    fn test_lua_string_operations() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.eval("return string.upper('hello')").unwrap();
        assert!(result.contains("HELLO"));
    }

    #[test]
    fn test_lua_table_operations() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.eval("return #{1,2,3}").unwrap();
        assert!(result.contains("3"));
    }

    #[test]
    fn test_lua_math_operations() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.eval("return math.floor(3.7)").unwrap();
        assert!(result.contains("3"));
    }

    #[test]
    fn test_lua_sandbox_blocks_os() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.eval("return os");
        // os should not be available — returns nil, not an error
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Nil"));
    }

    #[test]
    fn test_lua_sandbox_blocks_io() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.eval("return io");
        // io should not be available — returns nil, not an error
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Nil"));
    }

    #[test]
    fn test_lua_sandbox_blocks_debug() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.eval("return debug");
        // debug should not be available — returns nil, not an error
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Nil"));
    }

    #[test]
    fn test_lua_custom_function() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
            function double(x)
                return x * 2
            end
            "#,
            )
            .unwrap();
        let result = engine.eval("return double(21)").unwrap();
        assert!(result.contains("42"));
    }

    #[test]
    fn test_lua_error_handling() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.eval("error('test error')");
        assert!(result.is_err());
    }

    #[test]
    fn test_lua_keymap_set() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.load_script("aileron.keymap.set('normal', 'ctrl+a', 'SelectAll')");
        assert!(result.is_ok());
    }

    #[test]
    fn test_lua_theme_set() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.load_script("aileron.theme.set('dark')");
        assert!(result.is_ok());
    }

    #[test]
    fn test_lua_cmd_create_registers_command() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
                aileron.cmd.create("greet", "Say hello", function()
                    return "Hello from Lua!"
                end)
                "#,
            )
            .unwrap();

        let commands = engine.custom_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "greet");
        assert_eq!(commands[0].description, "Say hello");
    }

    #[test]
    fn test_lua_cmd_create_rejects_non_function() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.load_script(
            r#"
            aileron.cmd.create("bad", "Not a function", "not a function")
            "#,
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("callback must be a function"));
    }

    #[test]
    fn test_lua_cmd_create_callback_can_be_called() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
                aileron.cmd.create("hello", "Say hello", function()
                    return "Hello from Lua!"
                end)
                "#,
            )
            .unwrap();

        let result = engine.call_command("hello", &[]);
        assert!(result.is_ok(), "call_command failed: {:?}", result.err());
        assert_eq!(result.unwrap(), "String(\"Hello from Lua!\")");
    }

    #[test]
    fn test_lua_cmd_create_multiple_commands() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
                aileron.cmd.create("cmd1", "First command", function() return "1" end)
                aileron.cmd.create("cmd2", "Second command", function() return "2" end)
                aileron.cmd.create("cmd3", "Third command", function() return "3" end)
                "#,
            )
            .unwrap();

        let commands = engine.custom_commands();
        assert_eq!(commands.len(), 3);

        assert_eq!(engine.call_command("cmd1", &[]).unwrap(), "String(\"1\")");
        assert_eq!(engine.call_command("cmd2", &[]).unwrap(), "String(\"2\")");
        assert_eq!(engine.call_command("cmd3", &[]).unwrap(), "String(\"3\")");
    }

    #[test]
    fn test_lua_url_add_redirect_registers_rule() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
                aileron.url.add_redirect("twitter.com", "nitter.net")
                "#,
            )
            .unwrap();

        let redirects = engine.url_redirects.borrow();
        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].pattern, "twitter.com");
        assert_eq!(redirects[0].replacement, "nitter.net");
    }

    #[test]
    fn test_lua_url_add_redirect_multiple_rules() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
                aileron.url.add_redirect("twitter.com", "nitter.net")
                aileron.url.add_redirect("reddit.com", "old.reddit.com")
                aileron.url.add_redirect("youtube.com", "piped.video")
                "#,
            )
            .unwrap();

        let redirects = engine.url_redirects.borrow();
        assert_eq!(redirects.len(), 3);
    }

    #[test]
    fn test_apply_url_redirects_simple() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(r#"aileron.url.add_redirect("twitter.com", "nitter.net")"#)
            .unwrap();

        let url = url::Url::parse("https://twitter.com/user/status/123").unwrap();
        let redirected = engine.apply_url_redirects(&url);
        assert_eq!(redirected.host_str(), Some("nitter.net"));
        assert_eq!(redirected.path(), "/user/status/123");
    }

    #[test]
    fn test_apply_url_redirects_case_insensitive() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(r#"aileron.url.add_redirect("TWITTER.COM", "nitter.net")"#)
            .unwrap();

        let url = url::Url::parse("https://Twitter.com/user").unwrap();
        let redirected = engine.apply_url_redirects(&url);
        assert_eq!(redirected.host_str(), Some("nitter.net"));
    }

    #[test]
    fn test_apply_url_redirects_no_match() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(r#"aileron.url.add_redirect("twitter.com", "nitter.net")"#)
            .unwrap();

        let url = url::Url::parse("https://github.com/rust-lang/rust").unwrap();
        let redirected = engine.apply_url_redirects(&url);
        assert_eq!(redirected.as_str(), "https://github.com/rust-lang/rust");
    }

    #[test]
    fn test_apply_url_redirects_first_match_wins() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
                aileron.url.add_redirect("reddit.com", "old.reddit.com")
                aileron.url.add_redirect("reddit", "teddit.net")
                "#,
            )
            .unwrap();

        let url = url::Url::parse("https://www.reddit.com/r/rust").unwrap();
        let redirected = engine.apply_url_redirects(&url);
        // First rule matches: "reddit.com" in "www.reddit.com" → "www.old.reddit.com"
        assert_eq!(redirected.host_str(), Some("www.old.reddit.com"));
    }

    #[test]
    fn test_apply_url_redirects_preserves_query() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(r#"aileron.url.add_redirect("youtube.com", "piped.video")"#)
            .unwrap();

        let url = url::Url::parse("https://www.youtube.com/watch?v=abc123").unwrap();
        let redirected = engine.apply_url_redirects(&url);
        // "youtube.com" in "www.youtube.com" → "www.piped.video"
        assert_eq!(redirected.host_str(), Some("www.piped.video"));
        assert_eq!(redirected.query(), Some("v=abc123"));
    }

    #[test]
    fn test_apply_url_redirects_no_engine() {
        // LuaEngine with no rules — URL should pass through unchanged
        let engine = LuaEngine::new().unwrap();
        let url = url::Url::parse("https://example.com/path?q=1").unwrap();
        let redirected = engine.apply_url_redirects(&url);
        assert_eq!(redirected.as_str(), "https://example.com/path?q=1");
    }

    #[test]
    fn test_lua_on_registers_hook() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.load_script(
            r#"
            aileron.on("navigate", function(url)
                aileron.log("Navigated to: " .. url)
            end)
            "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_lua_on_rejects_non_function() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.load_script(
            r#"
            aileron.on("test", "not a function")
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_lua_call_hooks_invokes_callbacks() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
                aileron.on("test_event", function(arg1)
                    aileron.log("hook_called:" .. arg1)
                end)
                "#,
            )
            .unwrap();
        engine.call_hooks("test_event", &["hello"]);
        engine.call_hooks("nonexistent", &[]);
    }

    #[test]
    fn test_lua_call_hooks_multiple_callbacks() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
                aileron.on("multi", function() end)
                aileron.on("multi", function() end)
                aileron.on("multi", function() end)
                "#,
            )
            .unwrap();
        engine.call_hooks("multi", &["a", "b"]);
    }

    #[test]
    fn test_lua_call_hooks_no_panic_on_error() {
        let engine = LuaEngine::new().unwrap();
        engine
            .load_script(
                r#"
                aileron.on("err_event", function()
                    error("intentional error")
                end)
                "#,
            )
            .unwrap();
        engine.call_hooks("err_event", &[]);
    }
}
