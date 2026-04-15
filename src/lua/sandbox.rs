/// Sandbox configuration for the Lua engine.
///
/// Security measures implemented:
/// 1. Limited stdlib: no os, io, debug, or load/require
/// 2. No access to the Rust filesystem from Lua
/// 3. No access to environment variables
/// 4. No access to subprocess execution
/// 5. Memory and execution time limits (where supported)
///
/// Per STRIDE threat T-TAMP-001 (Tampering) and security tests
/// ST-SEC-008 (os.execute blocked) and ST-SEC-009 (io.open blocked).
/// List of blocked Lua stdlib modules that could be dangerous.
pub const BLOCKED_MODULES: &[&str] = &[
    "os",      // os.execute, os.rename, os.remove, os.getenv
    "io",      // io.open, io.popen (file I/O and subprocess)
    "debug",   // debug.getinfo, debug.sethook (introspection)
    "package", // require, package.loaders (code loading)
];

/// List of dangerous global functions that should be blocked.
pub const BLOCKED_GLOBALS: &[&str] = &[
    "dofile",   // Execute a Lua file
    "loadfile", // Load a Lua file
    "load",     // Load a Lua chunk
    "require",  // Load a module
    "dostring", // Execute a Lua string (legacy)
];

/// Validate that a Lua script does not attempt to access blocked APIs.
/// This is a best-effort check; the actual sandboxing is done by
/// not loading the dangerous stdlib modules into the Lua VM.
pub fn validate_script(script: &str) -> Result<(), SandboxViolation> {
    // Check for obvious attempts to access blocked modules
    for blocked in BLOCKED_MODULES {
        // Look for patterns like "os.execute", "require('os')", etc.
        let patterns = [
            format!("{}.", blocked),
            format!("require('{}')", blocked),
            format!("require(\"{}\")", blocked),
        ];
        for pattern in &patterns {
            if script.contains(pattern.as_str()) {
                return Err(SandboxViolation {
                    kind: SandboxViolationKind::BlockedModule(blocked.to_string()),
                    pattern: pattern.clone(),
                    position: find_pattern_position(script, pattern),
                });
            }
        }
    }

    // Check for dangerous globals
    for blocked in BLOCKED_GLOBALS {
        if script.contains(blocked) {
            // Only flag if it looks like a function call, not just a substring
            let call_pattern = format!("{}(", blocked);
            if script.contains(&call_pattern) {
                return Err(SandboxViolation {
                    kind: SandboxViolationKind::BlockedGlobal(blocked.to_string()),
                    pattern: call_pattern.clone(),
                    position: find_pattern_position(script, &call_pattern),
                });
            }
        }
    }

    Ok(())
}

/// A sandbox violation detected in a Lua script.
#[derive(Debug, Clone)]
pub struct SandboxViolation {
    /// What kind of violation was detected.
    pub kind: SandboxViolationKind,
    /// The pattern that triggered the violation.
    pub pattern: String,
    /// Approximate character position in the script.
    pub position: usize,
}

impl std::fmt::Display for SandboxViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Sandbox violation at position {}: {:?} (pattern: '{}')",
            self.position, self.kind, self.pattern
        )
    }
}

impl std::error::Error for SandboxViolation {}

/// The kind of sandbox violation.
#[derive(Debug, Clone)]
pub enum SandboxViolationKind {
    /// Attempted to access a blocked module (os, io, debug, package).
    BlockedModule(String),
    /// Attempted to call a blocked global function (dofile, load, require).
    BlockedGlobal(String),
}

impl std::fmt::Display for SandboxViolationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxViolationKind::BlockedModule(name) => {
                write!(f, "blocked module '{}' access", name)
            }
            SandboxViolationKind::BlockedGlobal(name) => {
                write!(f, "blocked global '{}' call", name)
            }
        }
    }
}

/// Find the approximate position of a pattern in a string.
fn find_pattern_position(text: &str, pattern: &str) -> usize {
    text.find(pattern).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_script_passes() {
        let script = r#"
            local x = 10
            local y = string.upper("hello")
            print(y)
        "#;
        assert!(validate_script(script).is_ok());
    }

    #[test]
    fn test_os_execute_blocked() {
        let script = r#"os.execute("rm -rf /")"#;
        let result = validate_script(script);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err.kind, SandboxViolationKind::BlockedModule(_)));
    }

    #[test]
    fn test_os_getenv_blocked() {
        let script = r#"local home = os.getenv("HOME")"#;
        let result = validate_script(script);
        assert!(result.is_err());
    }

    #[test]
    fn test_io_open_blocked() {
        let script = r#"local f = io.open("/etc/passwd", "r")"#;
        let result = validate_script(script);
        assert!(result.is_err());
    }

    #[test]
    fn test_require_os_blocked() {
        let script = r#"local os = require("os")"#;
        let result = validate_script(script);
        assert!(result.is_err());
    }

    #[test]
    fn test_dofile_blocked() {
        let script = r#"dofile("/etc/passwd")"#;
        let result = validate_script(script);
        assert!(result.is_err());
    }

    #[test]
    fn test_loadfile_blocked() {
        let script = r#"loadfile("malicious.lua")"#;
        let result = validate_script(script);
        assert!(result.is_err());
    }

    #[test]
    fn test_require_blocked() {
        let script = r#"require("socket")"#;
        let result = validate_script(script);
        assert!(result.is_err());
    }

    #[test]
    fn test_debug_blocked() {
        let script = r#"debug.getinfo(1)"#;
        let result = validate_script(script);
        assert!(result.is_err());
    }

    #[test]
    fn test_math_allowed() {
        let script = r#"local x = math.floor(3.14)"#;
        assert!(validate_script(script).is_ok());
    }

    #[test]
    fn test_string_allowed() {
        let script = r#"local x = string.match("hello", "hel")"#;
        assert!(validate_script(script).is_ok());
    }

    #[test]
    fn test_table_allowed() {
        let script = r#"local t = {1, 2, 3}"#;
        assert!(validate_script(script).is_ok());
    }

    #[test]
    fn test_violation_display() {
        let script = r#"os.execute("evil")"#;
        let result = validate_script(script).unwrap_err();
        let msg = format!("{}", result);
        assert!(msg.contains("os"));
    }

    #[test]
    fn test_false_positive_safe_word() {
        // "host" contains "os" but shouldn't trigger
        let script = r#"local host = "localhost""#;
        assert!(validate_script(script).is_ok());
    }
}
