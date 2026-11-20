# PATTERN_ADOPTION: aileron

Relationship to the WyattAu pattern canon
(`github.com/WyattAu/pattern-standards`), validated by
`scripts/adoption-check.py` per `references/adoption-format.md`.

aileron is a keyboard-driven browser shell over WebKitGTK. Untrusted text
(EasyList rules, config) enters through typed parsers. LLM agents consume
aileron through the MCP server it exposes; aileron does not generate or
decode LLM output.

| Pattern | Status | Evidence |
|---|---|---|
| T1 | adopted | src/net/filter_list.rs:79 (EasyList content parsed into typed NetworkFilter and CosmeticFilter values; parse_line at :108 returns Options for malformed lines) |
| T48 | n/a | (aileron exposes MCP tools with inputSchema at src/mcp/server.rs:153; grammar-constrained decoding constrains LLM generation, which aileron has no surface for) |

Gaps observed, no row claimed: P23 listener teardown. The MCP TCP bridge
documents graceful shutdown as future work (src/mcp/tcp_bridge.rs:255),
which is the listener-leak surface P23 guards; there is no AbortScope-style
teardown anywhere under src/mcp or src/arp. The crate split separating a
core from headed shells lives in aileron-gate1, not here.
