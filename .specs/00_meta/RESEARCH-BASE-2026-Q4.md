# Research Base — 2026 Q4 (foundation for Architecture v2 and ROADMAP-2027-2031)

**Status:** ADOPTED as the literature ground truth for all new specs.
**Rule:** no algorithm or architecture enters a YP without a citation here or a
measured reason it departs from the literature.

## 1. Browser agents: the state of the art

| Source | Finding we build on |
|---|---|
| **Building Browser Agents: Architecture, Security, and Practical Solutions** (arXiv:2511.19477, prod-operated agent, 85% WebGames vs ~50% prior, human 95.7%) | (a) **"Architecture, not model capability, determines success."** (b) Hybrid context: a11y-tree snapshot primary + selective vision for canvas/occlusion. (c) **Versioned element refs** (`ref=snapshot:element`) with fail-closed mismatch checks. (d) **Bulk actions**: 74% fewer tool calls, 57% faster, 41% fewer tokens on a 28-field form. (e) Semantic safety at the tool layer (block "delete"/"refund" without confirmation). (f) Aggressive history compression for long-horizon workflows. |
| **A Survey of WebAgents** (arXiv:2503.23350) | Agent loop decomposition: perception → memory → planning → execution → learning. Aileron owns **perception + execution + memory substrate**; the LLM owns planning. This boundary is the product. |
| **Visual Survey of Computer Use Agents** (ICLR blog 2026) | Convergence of visual and structured approaches; grounding (instruction→element) is the reliability bottleneck. |
| **Grounding Computer Use Agents on Human Demonstrations** (arXiv:2511.07332) | Grounding quality dominates task success; browser-side grounding beats screenshot-side. |
| **An Open Agentic Framework...** (arXiv:2410.08164) | "ACI ensures grounding by providing a vision-augmented accessibility tree containing all valid GUI elements and constraining actions" — the ACI is a designed artifact (Yang et al. lineage). |
| **The Human-AI gap** (2511.19477 §3) | Agents lack: visual hierarchy, timing/motion sensing, audio, continuous motor control. Canvas apps (Sheets/Figma) invisible to DOM/AX — vision fallback mandatory. |

**Adopted:** hybrid observation (AX-first, vision on demand), versioned refs,
bulk/batched actions, tool-layer semantic safety, compressed history.
**Rejected:** pure-vision interaction (bounding-box fragility on dense grids,
canvas blindness, no semantic safety handle); grid-based coordinate mapping
(abandoned in production per 2511.19477 §4.3); agent-side safety reasoning
("programmatic constraints over LLM judgment" — the paper's central claim).

## 2. Observation reduction (context economy)

| Source | Finding |
|---|---|
| **FocusAgent** (via axtree studies) | AX tree ≈ **10x smaller** than raw DOM at equal actionability. |
| **Prune4Web** (AAAI) | DOM pruning as *program synthesis*: an LLM writes the pruning program once per site class; O(1) at runtime. |
| **LineRetriever** (arXiv:2507.00210) | Planning-aware observation reduction: keep lines relevant to the *current plan*, not generic importance. |

**Adopted:** aileron-agent serves observations in **tiers** (T0 title/url,
T1 pruned AX snapshot, T2 full AX, T3 DOM/text, T4 screenshot) with the agent
requesting the tier; server-side reduction is deterministic and auditable —
unlike agent-side summarization, it never lies to the planner.

## 3. Protocol theory and security

| Source | Finding |
|---|---|
| **Breaking the Protocol: Security Analysis of the MCP Spec** (arXiv:2601.17549) | "**No formal security analysis** of MCP exists." Tool-description injection, session hijack, confused-deputy across the lifecycle. |
| **MCP: Landscape, Security Threats** (ACM 3796519) | Full lifecycle threat model; deployment-stage attacks. |
| **Survey of Agent Interoperability Protocols** (arXiv:2505.02279) | MCP (tool layer) vs ACP/A2A/ANP (agent-to-agent layers) — aileron lives at the tool layer but must not leak assumptions upward. |
| **WebMCP** (W3C proposal; Google I/O 2026; Mozilla position: neutral; Meta shipping in Wearables) | *Pages* expose tools to browser-based agents. A browser that already speaks MCP to agents is one hop from brokering WebMCP — and Chromium will gate it behind Gemini. **Strategic window.** |
| **Indirect prompt injection — impossibility argument** (May 2026 arXiv, cipherssecurity summary) | Detection-based defenses cannot reach zero without breaking utility; **structural confinement** is the only class that composes with an impossibility result. |
| **Reachability-based capability confinement** (2025-26) | Constrain *what an agent can reach*, not what it reads. |
| **Adaptive Evaluation of Out-of-Band Defenses** (arXiv:2606.26479) | Field converged 2024→2026: stop filtering inputs, bound capabilities out-of-band. |
| **BrowseSafe / BrowseSafe-Bench** (OpenReview) | Real-world injection detection in browsers fails; benchmark exists for browser-level defense. |

**Adopted:** aileron positions as **the enforcement point** — capability
confinement implemented in the browser process (the only component that can
guarantee it), exposed as typed tool preconditions (Lean-checked), with
per-session capability grants. This is YP-SEC-CONFINEMENT-001.
**Positioning:** the first browser with a formally specified MCP surface
(direct answer to 2601.17549's "no formal analysis").

## 4. Memory, persistence, recoverability

| Source | Finding |
|---|---|
| **Mem0** (arXiv:2504.19413) | Memory = extraction pipeline + conflict resolution; formalized two-phase architecture. |
| **Are We Ready For An Agent-Native Memory System?** (arXiv:2606.24775) | Agent-native memory needs *stateful execution* substrate, not vector stores. |
| **PRO-LONG** (arXiv:2607.20064) | **Programmatic memory**: agents address memory through code (queries, not context dumps) — scales long-horizon reasoning. |
| **Remember When It Matters** (arXiv:2607.08716) | Proactive memory intervention: maintain *decision-relevant* execution state. |
| **AgentRewind** (arXiv:2608.14380) | **Recoverable execution**: aligned checkpoints of agent + environment enable rewind/repair — the environment half of this is *exactly* workspace snapshots + nav epochs. |

**Adopted:** aileron's workspaces become the **environment-side memory** of the
literature: named, versioned, restorable page-states with typed side-channels
(bookmarks/history/notes), checkpoint-aligned with agent sessions (AgentRewind
pattern), queryable programmatically (PRO-LONG pattern) via MCP tools
(`workspace_diff`, `workspace_checkpoint`, `workspace_restore`).

## 5. Evaluation

| Source | Finding |
|---|---|
| **Online-Mind2Web / "An Illusion of Progress?"** (arXiv:2504.01382) | Many reported wins don't survive realistic online evaluation; 300 tasks / 136 sites methodology. |
| **CAP** (arXiv:2608.08392) | Cross-site realistic browsing benchmark; anti-cheat evaluation design. |
| **WebGames** (2511.19477 §9) | Interactive benchmark; 53 challenges; human baseline 95.7%. |
| **BrowseSafe-Bench** | Injection-defense evaluation grounded in real browser states. |

**Adopted:** aileron-agent is evaluated on Online-Mind2Web (task success),
CAP (cross-site robustness), WebGames (interactivity), BrowseSafe-Bench
(defense), plus our own latency/memory/freshness metrics — full matrix in
ROADMAP-2027-2031 P4.

## 6. What remains novel (no prior art found)

1. **Epoch-unified element references** — snapshot-versioned refs (2511.19477)
   unified with navigation epochs (YP-MCP-NAV-EPOCH-001) into one staleness
   algebra: any read or action carries an epoch bound; the browser enforces it.
   Prior art versions refs per-snapshot; nobody ties them to load-commit epochs
   with formal read-your-writes.
2. **Browser-enforced capability confinement with machine-checked preconditions**
   — the enforcement point moves into the browser; preconditions are Lean
   theorems, not prompt text.
3. **Session-typed multi-agent browser relay** — concurrent agent sessions over
   one browser with typed channel discipline (prior MCP servers: one client,
   untyped).
4. **Environment-side memory as workspaces** — persistence is the browser's job;
   agents get checkpoint/restore/diff as tools (AgentRewind's environment half
   as a product).

These four are the research contributions embedded in the product plan.
