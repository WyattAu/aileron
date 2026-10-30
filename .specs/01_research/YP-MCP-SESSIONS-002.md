---
document_id: YP-MCP-SESSIONS-002
version: 1.0.0
status: DRAFT
supersedes: (extends YP-MCP-SESSION-001, the session-typed relay)
domain: MCP — Multi-Agent Session Algebra
subdomains: [Session Types, Concurrency, Capability Delegation, Isolation]
created: 2026-10-30
author: DeepThought
confidence_level: 0.84
tqa_level: 3
research_iterations: 1
citations: [arXiv:2601.17549, arXiv:2505.02279, ACM 3796519]
---

# YP-MCP-SESSIONS-002: Concurrent Agent Sessions over One Browser

## YP-2: Executive Summary

**Problem statement:** today's browser MCP servers assume one client per browser
(Playwright MCP launches a browser per session). Multi-agent operation — a planner
delegating to worker agents, parallel research agents, CI agents beside a human's
live browser — requires **typed concurrent sessions** with isolation of intent,
capability, and observation, over one shared browser process.

**Survey:** MCP security analyses (2601.17549: session hijack and confused-deputy
classes; ACM 3796519: lifecycle threats); interoperability survey (2505.02279:
MCP = tool layer, A2A/ACP/ANP = agent layers — a browser sits under all of them);
session-type theory (Honda/Yoshida/Carbone — channel discipline as types).

**Novel:** the relay's session typing (YP-MCP-SESSION-001) is extended from a
single-session discipline to an **n-session algebra**: each agent session is a
typed channel with its own pane focus, epoch view, capability set, and workspace
namespace; the browser mediates contention.

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| $S = \{s_1..s_n\}$ | concurrent agent sessions |
| $\kappa(s)$ | capability set granted to session $s$ |
| $\phi(s)$ | pane focus set of $s$ (which panes it may observe/actuate) |
| $\omega(s)$ | workspace namespace of $s$ |
| $\rightarrow_s$ | session $s$'s action stream |

## YP-4: Theoretical Foundation

**Definition (session):** $s$ = (channel, $\kappa$, $\phi$, $\omega$, view of
epochs). The relay multiplexes $S$ over one TCP bridge; commands carry $s.id$.

**Safety property (isolation):** without $\text{pane} \in \phi(s)$, no command
from $s$ observes or actuates that pane — enforcement at the router (the same
dispatch point as epoch gating), Lean-statable as a routing theorem
(EXTENDS PROP-MCP-002).

**Safety property (no confused deputy):** a capability introduced by session
$s_i$ (e.g., a cookie, a login, a grant) cannot be exercised by $s_j$ unless
$\kappa(s_i) \cap \kappa(s_j)$ sanctions it. This directly answers the
confused-deputy class in 2601.17549.

**Contention (focus arbitration):** two sessions acting on one pane serialize
per-pane through the existing epoch gate; observation consistency per session
is read-your-writes *for that session's own navigations* — epoch counters
become per-(session, pane) pairs, preserving YP-MCP-NAV-EPOCH-001's theorem
shape.

**Delegation:** a session may spawn a child with $\kappa(\text{child})
\subseteq \kappa(\text{parent})$ (monotone capability narrowing — never
amplification), the standard capability-system lattice rule.

## YP-5: Algorithm Specification

```
connect(agent, requested κ) -> session s | DENY (policy)
  router: for each command c from s:
    1. wait_mcp_nav_settled(s, c.pane)      # per-(s,pane) epochs
    2. assert c.pane ∈ φ(s)                  # isolation
    3. assert c.tool ∈ κ(s)                  # capability gate (YP-SEC-CONFINEMENT-001)
    4. dispatch as today
```

Human co-tenancy: the human profile's live panes are panes with
$\phi(s) = \emptyset$ by default; an agent session granted a pane is granted a
*view* — the human's tabs are never silently actuated (consent tool on first grant).

## YP-6: Test Vector Specification

`test_vectors_sessions.toml`: two sessions, disjoint focus sets — zero
cross-observation; same-pane concurrent navigations — per-session
read-your-writes holds; capability narrowing down a 3-deep delegation chain —
no amplification; revocation mid-flight — in-doubt command fails closed.

## YP-7: Domain Constraints

MCP wire protocol has no native session concept beyond the transport
connection; aileron's relay assigns identity per connection and multiplexes —
standard MCP clients work unmodified.

## YP-8: Bibliography

1. Honda, Yoshida, Carbone — multiparty asynchronous session types.
2. arXiv:2601.17549 — MCP security analysis (session/confused-deputy classes).
3. arXiv:2505.02279 — agent interoperability protocol layers.
4. ACM 3796519 — MCP lifecycle threat model.

## YP-9: Knowledge Graph Concepts

session_types, multi_agent, capability_delegation, isolation, confused_deputy,
pane_focus, contention_arbitration

## YP-10: Quality Checklist

- [x] Session algebra defined over the existing relay (no protocol break)
- [x] Isolation + no-confused-deputy properties stated as routing theorems
- [x] Delegation = monotone narrowing
- [ ] Lean: EXT-PROP-MCP-002 (isolation routing) — Year-1 formal work
- [ ] Multi-session QA matrix — Year-1
