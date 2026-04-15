# Security Test Plan

## Test Categories

### 1. Credential Handling Tests

| ID | Test | Method | Priority |
|----|------|--------|----------|
| ST-SEC-001 | Verify passwords never appear in log output | Run password injection, grep logs | Critical |
| ST-SEC-002 | Verify credentials are zeroized from memory after injection | Valgrind/ASan memory scan | Critical |
| ST-SEC-003 | Verify SensitiveString type prevents Display/Debug | Compile-time test | Critical |

### 2. MCP Server Security Tests

| ID | Test | Method | Priority |
|----|------|--------|----------|
| ST-SEC-004 | Verify unauthenticated MCP requests are rejected | Connect without token | Critical |
| ST-SEC-005 | Verify rate limiting on MCP tool calls | Flood with 1000 requests/sec | High |
| ST-SEC-006 | Verify `read_active_pane` redacts password fields | Inject password form, call tool | Critical |
| ST-SEC-007 | Verify `run_js` requires explicit user confirmation | Call run_js without prompt | Critical |

### 3. Lua Sandboxing Tests

| ID | Test | Method | Priority |
|----|------|--------|----------|
| ST-SEC-008 | Verify `os.execute` is blocked in Lua sandbox | Attempt `os.execute("rm -rf /")` | Critical |
| ST-SEC-009 | Verify `io.open` is blocked in Lua sandbox | Attempt `io.open("/etc/passwd")` | Critical |
| ST-SEC-010 | Verify only `aileron.*` API is exposed | Attempt `require("ffi")` | High |

### 4. Network Security Tests

| ID | Test | Method | Priority |
|----|------|--------|----------|
| ST-SEC-011 | Verify ad-blocker blocks tracking domains | Load page with known tracker | High |
| ST-SEC-012 | Verify HTTPS is enforced where possible | Attempt HTTP → HTTPS upgrade | High |

### 5. Session Isolation Tests

| ID | Test | Method | Priority |
|----|------|--------|----------|
| ST-SEC-013 | Verify separate pane sessions have isolated cookies | Set cookie in pane 1, check pane 2 | High |
| ST-SEC-014 | Verify localStorage is isolated per session | Set localStorage in pane 1, check pane 2 | High |

## Tools Required

| Tool | Purpose | Available |
|------|---------|-----------|
| cargo audit | Dependency vulnerability scan | ✅ |
| clippy | Unsafe code detection | ✅ |
| Valgrind | Memory leak detection | ❌ (add to flake.nix) |
| ASan (AddressSanitizer) | Memory safety | ✅ (rustc flag) |
| rustsec | Advisory database | ✅ (cargo audit) |
