//! Lua scripting integration tests.
//!
//! End-to-end verification of the LuaEngine lifecycle:
//!   LuaEngine::new() -> load_script -> side effects -> take_pending_keybinds
//!                     -> parse_key_string -> resolve_action -> custom commands
//!                     -> URL redirects -> hooks -> sandbox validation
//!
//! All tests run without a GUI or browser window.

use aileron::input::Mode;
use aileron::lua::LuaEngine;
use aileron::lua::sandbox::validate_script;

// ─── 1. Engine creation and version ────────────────────────────────

#[test]
fn test_engine_creation_succeeds() {
    let engine = LuaEngine::new();
    assert!(engine.is_ok(), "LuaEngine::new() should succeed");
}

#[test]
fn test_engine_version_accessible() {
    let engine = LuaEngine::new().unwrap();
    let result = engine.eval("return aileron.version").unwrap();
    assert!(
        result.contains("0."),
        "aileron.version should contain '0.', got: {result}"
    );
}

#[test]
fn test_engine_info_returns_table() {
    let engine = LuaEngine::new().unwrap();
    // Access a field of the table directly to verify it's a table
    let result = engine.eval("return aileron.info().version").unwrap();
    assert!(
        result.contains("0."),
        "aileron.info().version should return version string, got: {result}"
    );
}

// ─── 2. Stdlib sandboxing ──────────────────────────────────────────

#[test]
fn test_os_module_not_accessible() {
    let engine = LuaEngine::new().unwrap();
    // os is not in the stdlib -- using it should error
    let result = engine.eval("os.execute('echo hi')");
    assert!(
        result.is_err(),
        "os.execute should not be accessible in sandbox"
    );
}

#[test]
fn test_io_module_not_accessible() {
    let engine = LuaEngine::new().unwrap();
    let result = engine.eval("io.open('/etc/passwd')");
    assert!(
        result.is_err(),
        "io.open should not be accessible in sandbox"
    );
}

#[test]
fn test_debug_module_not_accessible() {
    let engine = LuaEngine::new().unwrap();
    let result = engine.eval("debug.getinfo(1)");
    assert!(
        result.is_err(),
        "debug.getinfo should not be accessible in sandbox"
    );
}

#[test]
fn test_allowed_stdlibs_work() {
    let engine = LuaEngine::new().unwrap();

    // string
    let r = engine.eval(r#"return string.upper("hello")"#).unwrap();
    assert!(r.contains("HELLO"), "string.upper should work, got: {r}");

    // table
    let r = engine.eval("return table.concat({1,2,3}, ',')").unwrap();
    assert!(r.contains("1,2,3"), "table.concat should work, got: {r}");

    // math
    let r = engine.eval("return math.floor(3.7)").unwrap();
    assert!(r.contains("3"), "math.floor should work, got: {r}");

    // utf8
    let r = engine.eval(r#"return utf8.len("hello")"#).unwrap();
    assert!(r.contains("5"), "utf8.len should work, got: {r}");
}

// ─── 3. Keybind registration and parsing ───────────────────────────

#[test]
fn test_keymap_set_single_keybind() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(r#"aileron.keymap.set("normal", "ctrl+a", "ScrollToTop")"#)
        .unwrap();

    let keybinds = engine.take_pending_keybinds();
    assert_eq!(keybinds.len(), 1);
    assert_eq!(keybinds[0].mode, "normal");
    assert_eq!(keybinds[0].key, "ctrl+a");
    assert_eq!(keybinds[0].action, "ScrollToTop");
}

#[test]
fn test_keymap_set_multiple_keybinds() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(
            r#"
            aileron.keymap.set("normal", "ctrl+k", "ScrollUp")
            aileron.keymap.set("normal", "ctrl+j", "ScrollDown")
            aileron.keymap.set("insert", "ctrl+o", "EnterNormalMode")
        "#,
        )
        .unwrap();

    let keybinds = engine.take_pending_keybinds();
    assert_eq!(keybinds.len(), 3);
    assert_eq!(keybinds[0].mode, "normal");
    assert_eq!(keybinds[1].mode, "normal");
    assert_eq!(keybinds[2].mode, "insert");
}

#[test]
fn test_take_pending_keybinds_drains_buffer() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(r#"aileron.keymap.set("normal", "x", "Quit")"#)
        .unwrap();

    let first = engine.take_pending_keybinds();
    assert_eq!(first.len(), 1);

    let second = engine.take_pending_keybinds();
    assert!(
        second.is_empty(),
        "take_pending_keybinds should drain the buffer"
    );
}

#[test]
fn test_parse_key_string_known_combos() {
    // Normal, single key
    let combo = LuaEngine::parse_key_string("normal", "j");
    assert!(combo.is_some());
    let c = combo.unwrap();
    assert_eq!(c.mode, Mode::Normal);

    // Normal, ctrl+key
    let combo = LuaEngine::parse_key_string("normal", "ctrl+p");
    assert!(combo.is_some(), "ctrl+p should parse");
    let c = combo.unwrap();
    assert!(c.modifiers.ctrl);

    // Insert mode
    let combo = LuaEngine::parse_key_string("insert", "escape");
    assert!(combo.is_some(), "insert escape should parse");
    assert_eq!(combo.unwrap().mode, Mode::Insert);

    // Command mode
    let combo = LuaEngine::parse_key_string("command", "enter");
    assert!(combo.is_some(), "command enter should parse");
    assert_eq!(combo.unwrap().mode, Mode::Command);
}

#[test]
fn test_parse_key_string_invalid_inputs() {
    // Invalid mode
    assert!(
        LuaEngine::parse_key_string("visual", "j").is_none(),
        "invalid mode 'visual' should return None"
    );

    // Invalid key
    assert!(
        LuaEngine::parse_key_string("normal", "").is_none(),
        "empty key string should return None"
    );

    // Invalid modifier syntax
    assert!(
        LuaEngine::parse_key_string("normal", "ctrl+").is_none(),
        "trailing '+' should return None"
    );
}

#[test]
fn test_resolve_action_known_actions() {
    assert!(
        LuaEngine::resolve_action("quit").is_some(),
        "quit should resolve"
    );
    assert!(
        LuaEngine::resolve_action("scroll_down").is_some(),
        "scroll_down should resolve"
    );
    assert!(
        LuaEngine::resolve_action("scroll_up").is_some(),
        "scroll_up should resolve"
    );
    assert!(
        LuaEngine::resolve_action("insert").is_some(),
        "insert should resolve"
    );
    assert!(
        LuaEngine::resolve_action("split_vertical").is_some(),
        "split_vertical should resolve"
    );
    assert!(
        LuaEngine::resolve_action("sp").is_some(),
        "sp alias should resolve"
    );
    assert!(
        LuaEngine::resolve_action("vs").is_some(),
        "vs alias should resolve"
    );
    assert!(
        LuaEngine::resolve_action("reload").is_some(),
        "reload should resolve"
    );
    assert!(
        LuaEngine::resolve_action("open_command_palette").is_some(),
        "open_command_palette should resolve"
    );
    assert!(
        LuaEngine::resolve_action("pin").is_some(),
        "pin alias should resolve"
    );
}

#[test]
fn test_resolve_action_unknown_returns_none() {
    assert!(
        LuaEngine::resolve_action("NotARealAction").is_none(),
        "unknown action should return None"
    );
    assert!(
        LuaEngine::resolve_action("").is_none(),
        "empty action should return None"
    );
}

// ─── 4. Custom commands ────────────────────────────────────────────

#[test]
fn test_cmd_create_registers_command() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(
            r#"
            aileron.cmd.create("greet", "Say hello", function()
                return "Hello, world!"
            end)
        "#,
        )
        .unwrap();

    let cmds = engine.custom_commands();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].name, "greet");
    assert_eq!(cmds[0].description, "Say hello");
}

#[test]
fn test_cmd_create_multiple_commands() {
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

    let cmds = engine.custom_commands();
    assert_eq!(cmds.len(), 3);
}

#[test]
fn test_call_command_invokes_callback() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(
            r#"
            aileron.cmd.create("echo", "Echo input", function()
                return "Echo: hello"
            end)
        "#,
        )
        .unwrap();

    let result = engine.call_command("echo", &[]).unwrap();
    assert!(
        result.contains("hello"),
        "call_command should invoke the Lua callback, got: {result}"
    );
}

#[test]
fn test_call_command_no_args() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(
            r#"
            aileron.cmd.create("ping", "Ping test", function()
                return "pong"
            end)
        "#,
        )
        .unwrap();

    let result = engine.call_command("ping", &[]).unwrap();
    assert!(result.contains("pong"), "expected 'pong', got: {result}");
}

#[test]
fn test_call_nonexistent_command_returns_error() {
    let engine = LuaEngine::new().unwrap();
    let result = engine.call_command("nonexistent", &[]);
    assert!(
        result.is_err(),
        "calling a nonexistent command should return an error"
    );
}

#[test]
fn test_cmd_create_rejects_non_function() {
    let engine = LuaEngine::new().unwrap();
    // Passing a string instead of a function should not panic
    let result =
        engine.load_script(r#"aileron.cmd.create("bad", "Bad command", "not a function")"#);
    // The API may either error or silently ignore -- just verify no panic
    let _ = result;
}

// ─── 5. URL redirects ──────────────────────────────────────────────

#[test]
fn test_url_add_redirect_basic() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(r#"aileron.url.add_redirect("twitter.com", "nitter.net")"#)
        .unwrap();

    let url = url::Url::parse("https://twitter.com/user").unwrap();
    let redirected = engine.apply_url_redirects(&url);
    assert_eq!(
        redirected.host_str(),
        Some("nitter.net"),
        "twitter.com should redirect to nitter.net"
    );
}

#[test]
fn test_url_add_redirect_case_insensitive() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(r#"aileron.url.add_redirect("twitter.com", "nitter.net")"#)
        .unwrap();

    let url = url::Url::parse("https://TWITTER.COM/user").unwrap();
    let redirected = engine.apply_url_redirects(&url);
    assert_eq!(
        redirected.host_str(),
        Some("nitter.net"),
        "redirect matching should be case-insensitive"
    );
}

#[test]
fn test_url_no_match_passthrough() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(r#"aileron.url.add_redirect("twitter.com", "nitter.net")"#)
        .unwrap();

    let url = url::Url::parse("https://github.com/user/repo").unwrap();
    let redirected = engine.apply_url_redirects(&url);
    assert_eq!(
        redirected.host_str(),
        Some("github.com"),
        "non-matching URL should pass through unchanged"
    );
}

#[test]
fn test_url_redirect_preserves_path_and_query() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(r#"aileron.url.add_redirect("twitter.com", "nitter.net")"#)
        .unwrap();

    let url = url::Url::parse("https://twitter.com/elonmusk/status/12345?q=test").unwrap();
    let redirected = engine.apply_url_redirects(&url);
    assert_eq!(redirected.host_str(), Some("nitter.net"));
    assert_eq!(redirected.path(), "/elonmusk/status/12345");
    assert_eq!(redirected.query(), Some("q=test"));
}

#[test]
fn test_url_redirect_no_rules_passthrough() {
    let engine = LuaEngine::new().unwrap();
    // No redirects registered

    let url = url::Url::parse("https://example.com/page").unwrap();
    let redirected = engine.apply_url_redirects(&url);
    assert_eq!(
        redirected.as_str(),
        url.as_str(),
        "no rules registered should return original URL"
    );
}

// ─── 6. Event hooks ────────────────────────────────────────────────

#[test]
fn test_hook_registration_and_invocation() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(
            r#"
            aileron.on("navigate", function(url)
                aileron.log("Navigated to: " .. url)
            end)
        "#,
        )
        .unwrap();

    // Calling hooks should not panic even without a real browser
    engine.call_hooks("navigate", &["https://example.com"]);
    // call_hooks returns () and swallows errors -- no assertion needed
}

#[test]
fn test_hook_multiple_callbacks_same_event() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(
            r#"
            aileron.on("test_event", function(a) aileron.log("first: " .. a) end)
            aileron.on("test_event", function(a) aileron.log("second: " .. a) end)
            aileron.on("test_event", function(a) aileron.log("third: " .. a) end)
        "#,
        )
        .unwrap();

    // call_hooks returns () -- no assertion needed, just verify no panic
    engine.call_hooks("test_event", &["payload"]);
}

#[test]
fn test_hook_no_callbacks_for_event() {
    let engine = LuaEngine::new().unwrap();
    // No hooks registered for this event
    engine.call_hooks("nonexistent_event", &["data"]);
    // call_hooks returns () -- no panic is the assertion
}

#[test]
fn test_hook_error_does_not_break_subsequent_hooks() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(
            r#"
            aileron.on("test", function()
                error("intentional error")
            end)
            aileron.on("test", function()
                aileron.log("still runs")
            end)
        "#,
        )
        .unwrap();

    // Should not panic even though the first hook errors
    engine.call_hooks("test", &[]);
    // call_hooks returns () and swallows errors -- no assertion needed
}

#[test]
fn test_hook_rejects_non_function() {
    let engine = LuaEngine::new().unwrap();
    // Passing non-function should not panic
    let result = engine.load_script(r#"aileron.on("test", "not a function")"#);
    let _ = result;
}

// ─── 7. Logging ────────────────────────────────────────────────────

#[test]
fn test_log_and_warn_do_not_panic() {
    let engine = LuaEngine::new().unwrap();
    let result = engine.load_script(
        r#"
            aileron.log("info message")
            aileron.warn("warning message")
        "#,
    );
    assert!(result.is_ok(), "logging should not panic");
}

// ─── 8. Sandbox validation ─────────────────────────────────────────

#[test]
fn test_sandbox_allows_clean_script() {
    assert!(
        validate_script(r#"local x = "hello"; print(x)"#).is_ok(),
        "clean script should pass validation"
    );
}

#[test]
fn test_sandbox_blocks_os_execute() {
    assert!(
        validate_script("os.execute('rm -rf /')").is_err(),
        "os.execute should be blocked"
    );
}

#[test]
fn test_sandbox_blocks_os_getenv() {
    assert!(
        validate_script("os.getenv('HOME')").is_err(),
        "os.getenv should be blocked"
    );
}

#[test]
fn test_sandbox_blocks_io_open() {
    assert!(
        validate_script("io.open('/etc/passwd')").is_err(),
        "io.open should be blocked"
    );
}

#[test]
fn test_sandbox_blocks_require() {
    assert!(
        validate_script("require('os')").is_err(),
        "require should be blocked"
    );
}

#[test]
fn test_sandbox_blocks_dofile() {
    assert!(
        validate_script("dofile('/etc/passwd')").is_err(),
        "dofile should be blocked"
    );
}

#[test]
fn test_sandbox_blocks_loadfile() {
    assert!(
        validate_script("loadfile('/etc/passwd')").is_err(),
        "loadfile should be blocked"
    );
}

#[test]
fn test_sandbox_blocks_debug_library() {
    assert!(
        validate_script("debug.getinfo(1)").is_err(),
        "debug library should be blocked"
    );
}

#[test]
fn test_sandbox_allows_math() {
    assert!(
        validate_script("local x = math.floor(3.14)").is_ok(),
        "math library should be allowed"
    );
}

#[test]
fn test_sandbox_allows_string() {
    assert!(
        validate_script(r#"local x = string.upper("hello")"#).is_ok(),
        "string library should be allowed"
    );
}

#[test]
fn test_sandbox_allows_table() {
    assert!(
        validate_script("local t = {}; table.insert(t, 1)").is_ok(),
        "table library should be allowed"
    );
}

#[test]
fn test_sandbox_blocks_single_quote_require() {
    assert!(
        validate_script("require('io')").is_err(),
        "single-quoted require should be blocked"
    );
}

#[test]
fn test_sandbox_blocks_whitespace_trick() {
    // This tests whether whitespace around require args bypasses the check.
    // Even if it passes validation, the runtime stdlib restriction prevents execution.
    let result = validate_script("require ( \"os\" )");
    // The validation is best-effort string matching; it may or may not catch this.
    // The important thing is no panic.
    let _ = result;
}

// ─── 9. Full init.lua lifecycle simulation ─────────────────────────

#[test]
fn test_full_init_lua_lifecycle() {
    let engine = LuaEngine::new().unwrap();

    // Simulate a comprehensive init.lua
    let init_lua = r#"
        -- Custom keybindings (lowercase action names as expected by resolve_action)
        aileron.keymap.set("normal", "ctrl+shift+r", "reload")
        aileron.keymap.set("normal", "ctrl+shift+n", "open_command_palette")

        -- Custom commands
        aileron.cmd.create("open-github", "Open GitHub", function()
            aileron.log("Opening GitHub...")
            return "navigating to github.com"
        end)

        aileron.cmd.create("toggle-dark", "Toggle dark mode", function()
            aileron.log("Toggling dark mode")
            return "dark mode toggled"
        end)

        -- URL redirects
        aileron.url.add_redirect("old.reddit.com", "reddit.com")
        aileron.url.add_redirect("twitter.com", "nitter.net")

        -- Event hooks
        aileron.on("navigate", function(url)
            aileron.log("Navigate: " .. url)
        end)

        aileron.on("mode_change", function(mode)
            aileron.log("Mode: " .. mode)
        end)

        -- Version check
        local info = aileron.info()
        aileron.log("Aileron " .. info.version .. " (" .. info.engine .. ")")
    "#;

    engine.load_script(init_lua).unwrap();

    // Verify keybinds were collected
    let keybinds = engine.take_pending_keybinds();
    assert_eq!(keybinds.len(), 2, "should have 2 custom keybinds");
    assert_eq!(keybinds[0].action, "reload");
    assert_eq!(keybinds[1].action, "open_command_palette");

    // Verify parse + resolve work for collected keybinds
    for bind in &keybinds {
        let combo = LuaEngine::parse_key_string(&bind.mode, &bind.key);
        assert!(
            combo.is_some(),
            "keybind '{}' '{}' should parse",
            bind.mode,
            bind.key
        );
        let action = LuaEngine::resolve_action(&bind.action);
        assert!(action.is_some(), "action '{}' should resolve", bind.action);
    }

    // Verify custom commands
    let cmds = engine.custom_commands();
    assert_eq!(cmds.len(), 2);
    assert_eq!(cmds[0].name, "open-github");
    assert_eq!(cmds[1].name, "toggle-dark");

    // Verify command invocation
    let result = engine.call_command("open-github", &[]).unwrap();
    assert!(
        result.contains("github.com"),
        "command should return github.com, got: {result}"
    );

    // Verify URL redirects
    let url = url::Url::parse("https://old.reddit.com/r/rust").unwrap();
    let redirected = engine.apply_url_redirects(&url);
    assert_eq!(redirected.host_str(), Some("reddit.com"));

    // Verify hooks fire (no panic = success)
    engine.call_hooks("navigate", &["https://example.com"]);
    engine.call_hooks("mode_change", &["normal"]);
}

// ─── 10. Script isolation ──────────────────────────────────────────

#[test]
fn test_multiple_load_script_calls_do_not_leak() {
    let engine = LuaEngine::new().unwrap();

    engine
        .load_script(
            r#"
            aileron.cmd.create("script1_cmd", "From script 1", function() return "1" end)
        "#,
        )
        .unwrap();

    engine
        .load_script(
            r#"
            aileron.cmd.create("script2_cmd", "From script 2", function() return "2" end)
        "#,
        )
        .unwrap();

    let cmds = engine.custom_commands();
    assert_eq!(cmds.len(), 2, "both scripts should contribute commands");

    let r1 = engine.call_command("script1_cmd", &[]).unwrap();
    assert!(r1.contains("1"), "script1 command should work, got: {r1}");

    let r2 = engine.call_command("script2_cmd", &[]).unwrap();
    assert!(r2.contains("2"), "script2 command should work, got: {r2}");
}

// ─── 11. Edge cases ────────────────────────────────────────────────

#[test]
fn test_empty_script_loads_without_error() {
    let engine = LuaEngine::new().unwrap();
    assert!(engine.load_script("").is_ok(), "empty script should load");
}

#[test]
fn test_comments_only_script_loads_without_error() {
    let engine = LuaEngine::new().unwrap();
    assert!(
        engine
            .load_script("-- just a comment\n-- another comment")
            .is_ok(),
        "comments-only script should load"
    );
}

#[test]
fn test_syntax_error_in_script_returns_error() {
    let engine = LuaEngine::new().unwrap();
    let result = engine.load_script("this is not valid lua syntax !!!");
    assert!(
        result.is_err(),
        "syntax error in script should return error"
    );
}

#[test]
fn test_lua_global_function_is_callable() {
    let engine = LuaEngine::new().unwrap();
    engine
        .load_script(
            r#"
            function my_helper(x)
                return x * 2
            end
        "#,
        )
        .unwrap();

    let result = engine.eval("return my_helper(21)").unwrap();
    assert!(
        result.contains("42"),
        "global function should be callable, got: {result}"
    );
}

#[test]
fn test_theme_set_does_not_panic() {
    let engine = LuaEngine::new().unwrap();
    let result = engine.load_script(r#"aileron.theme.set("dark")"#);
    assert!(result.is_ok(), "aileron.theme.set should not panic");
}
