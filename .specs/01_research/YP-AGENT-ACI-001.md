---
document_id: YP-AGENT-ACI-001
version: 1.0.0
status: DRAFT
domain: Agent-Computer Interface — Page Abstraction & Actuation
subdomains: [Observation Tiers, Versioned References, Bulk Transactions, Grounding]
created: 2026-10-30
author: DeepThought
confidence_level: 0.86
tqa_level: 3
research_iterations: 2
citations: [arXiv:2511.19477, arXiv:2503.23350, arXiv:2507.00210, arXiv:2410.08164]
---

# YP-AGENT-ACI-001: The Page Abstraction — Observation, Reference, Actuation

## YP-2: Executive Summary

**Problem statement:** an LLM planner cannot see pixels or DOM; it needs a compact,
honest, *addressable* abstraction of page state, and a way to act on addresses that
cannot silently go stale. Production evidence (2511.19477): architecture, not model
capability, determines agent success; stale refs and dense-page blindness are the
dominant failure classes.

**Survey:** AX-tree-first agents (Playwright MCP, Chrome DevTools MCP), vision-first
(CUA-style), grid overlays (abandoned), DOM pruning (Prune4Web), planning-aware
reduction (LineRetriever), element-ref versioning (2511.19477 §5.1.1).

**Novel:** aileron unifies snapshot-versioned refs with **navigation epochs**
(YP-MCP-NAV-EPOCH-001) into one staleness algebra, and defines **observation tiers**
served deterministically by the browser (reduction never lies to the planner).

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| $\mathcal{P}$ | Page state (DOM + AX + layout + epoch) |
| $\alpha(\mathcal{P}, t)$ | Observation function: tier $t \in \{0..4\}$ over page state |
| $\rho(\mathcal{P})$ | Reference map: ref → DOM node, versioned by epoch |
| $\epsilon$ | Epoch: monotone counter per pane (load commit generation) |
| $\beta$ | Bulk action transaction: ordered action list, one precondition check |

## YP-4: Theoretical Foundation

**Definition (epoch-unified ref):** a reference is the pair
$r = (\epsilon, e)$ where $e$ is an element handle created under epoch
$\epsilon$. An action on $r$ is valid iff the pane's current epoch equals
$\epsilon$ **and** $e$ is still attached and role-compatible. Invalid ⇒ the
action **fails closed** with a fresh observation attached.

**Theorem (no-stale-actuation):** given per-action epoch checking, no action
issued under epoch $\epsilon_1$ can mutate a DOM produced under
$\epsilon_2 \neq \epsilon_1$. *Proof sketch:* epochs advance only through
`navigate_settled` (YP-MCP-NAV-EPOCH-001, Lean-checked commit discipline); the
action gate compares before dispatch; DOM replacement implies epoch advance.
Formal statement lands in `ProofACI.lean` (ACT-ACI-001).

**Definition (observation tiers, served not summarized):**

| Tier | Content | Typical size |
|------|---------|--------------|
| T0 | title, url, epoch, loading state | <100 B |
| T1 | pruned AX snapshot (interactive + labeled only) | 1–4 kB |
| T2 | full AX snapshot | 4–40 kB |
| T3 | readable text / DOM text | 1–100 kB |
| T4 | screenshot (viewport/element/full) | 10²–10³ kB |

Reduction is **deterministic and auditable**: the browser computes tiers with
fixed rules (AX role/label filters, per Prune4Web-style pruning, FocusAgent's
10× evidence); the planner never trusts an LLM-summarized page.

**Bulk transactions ($\beta$):** an ordered list of (action, precondition)
pairs executed atomically-with-report: each action's ref validity is checked at
its execution point; failures do not abort the batch, they return per-action
status (measured: 74% fewer calls, 57% faster, 41% fewer tokens — 2511.19477 §5.3).

## YP-5: Algorithm Specification

```
snapshot(pane, tier):
  wait epoch settled (YP-MCP-NAV-EPOCH-001)
  inject walker script → nodes[(ref, epoch, role, name, states, bounds)]
  apply tier filter → serialize
  cache (ref→handle) under epoch ϵ

act(pane, (ϵ, e), action):
  if pane.epoch ≠ ϵ → fail(STALE_EPOCH, attach fresh T1)
  if ¬attached(e) → fail(DETACHED, attach fresh T1)
  if ¬policy.allows(session, action, e) → fail(FORBIDDEN, reason)  # YP-SEC-CONFINEMENT-001
  dispatch; return per-action result
```

## YP-6: Test Vector Specification

`test_vectors_aci.toml`: per tier — size bounds on 10 canonical pages;
stale-ref fail-closed on synthetic DOM swap; bulk transaction partial-failure
reporting; walker coverage vs Chrome AX tree on 50 sites (≥95% interactive-element
recall); canvas-page behavior (T4 fallback present).

## YP-7: Domain Constraints

WebKitGTK exposes no CDP-grade AX serialization: the walker is an **in-page JS
program** (computed roles from ARIA + tag heuristics) — must match Chrome's AX
tree on the test corpus within tolerance. Occlusion/z-order is not in the AX
tree; T4 (screenshot) is the fallback, per hybrid context (2511.19477 §4.4).

## YP-8: Bibliography

1. arXiv:2511.19477 — Building Browser Agents (refs, bulk, hybrid context).
2. arXiv:2503.23350 — Survey of WebAgents (loop decomposition).
3. arXiv:2507.00210 — LineRetriever (planning-aware reduction).
4. Prune4Web (AAAI) — DOM pruning by program synthesis.
5. arXiv:2410.08164 — vision-augmented AX-tree grounding.

## YP-9: Knowledge Graph Concepts

agent_computer_interface, observation_tiers, element_refs, epoch_binding,
bulk_transactions, axtree, grounding, fail_closed

## YP-10: Quality Checklist

- [x] Staleness algebra defined (epoch-unified refs) with proof sketch
- [x] Observation tiers deterministic and browser-served
- [x] Bulk transactions with per-action status
- [x] Walker fidelity risk identified with tolerance test
- [ ] `ProofACI.lean` ACT-ACI-001 (no-stale-actuation) — P2 roadmap item
- [ ] Walker implementation + 50-site corpus validation — P2
