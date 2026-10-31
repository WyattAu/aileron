# Spike Note: WebMCP Broker (Era-2 Q4, S48)

**Status:** design note — no runtime work; the standard is a proposal
(W3C Web Machine Learning CG; Google I/O 2026 first look; Mozilla
position: neutral; Meta shipping `document.modelContext.registerTool()`).

## What WebMCP is

Pages expose structured tools (JS functions, HTML forms) to the
*browser-hosted* agent via `navigator.modelContext` / `document.modelContext`.
The browser brokers discovery and invocation. Chromium will surface it
through Gemini; the neutral Mozilla position and the CG process leave room
for an independent implementation.

## Why aileron is positioned for it

Aileron already has the two halves a WebMCP broker needs:

1. **Agent-facing MCP surface** (44 tools, session tags, capability gates) —
   the broker maps page tools into a session's κ-filtered tool set.
2. **Per-page JS execution with epoch-stamped refs** (YP-AGENT-ACI-001) —
   page tools are invoked in-page, where the walker already runs; an
   invocation is `execute_js` scoped to the page's own registered function,
   subject to the SAME capability classes (a page-exposed tool inherits the
   most restrictive class of its effects — enforcement stays in the
   dispatcher, YP-SEC-CONFINEMENT-001).

## Design sketch (when the standard stabilizes)

- `page_tools_list` (T2-adjacent observation tier): reads
  `modelContext.registerTool` registrations via an init-script shim.
- `page_tools_call {tool, args}` → dispatcher: class = registered class
  ∩ session κ; provenance tag `(epoch, page_origin)`.
- Injection boundary: page-registered tool DESCRIPTIONS are untrusted
  content — they flow to the agent as observations, never as capability
  changes (structural taint, unchanged).
- Chromium gates WebMCP behind Gemini; aileron exposes it to ANY MCP
  client — the open-agent differentiator.

## Exit criteria for runtime work

Standard draft stability (W3C CG pattern solidified) + one interop
partner. Tracking: webmachinelearning/webmcp issues (a11y-tree redundancy
discussion #91 is directly relevant to our T1/T2 tiers).
