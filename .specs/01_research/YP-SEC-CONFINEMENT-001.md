---
document_id: YP-SEC-CONFINEMENT-001
version: 1.0.0
status: DRAFT
domain: Security — Capability Confinement at the Browser Enforcement Point
subdomains: [Prompt Injection, Information Flow, Typed Preconditions]
created: 2026-10-30
author: DeepThought
confidence_level: 0.82
tqa_level: 3
research_iterations: 2
citations: [arXiv:2601.17549, arXiv:2606.26479, arXiv:2511.19477, BrowseSafe-Bench]
---

# YP-SEC-CONFINEMENT-001: The Browser as the Enforcement Point

## YP-2: Executive Summary

**Problem statement:** indirect prompt injection is the failure class that makes
autonomous browser agents unsafe (Brave's Comet analysis: hidden-text injection
→ cross-site OTP/banking exfiltration; 2026 field reports: every major AI browser
agent hijacked; Anthropic's residual ~1% attack success). The 2024→2026
literature converges: **detection/filtering cannot reach zero** (an impossibility
argument now exists); the viable class is **out-of-band structural confinement**
(2606.26479) — bound what the agent can *reach*, independent of what it reads.

**Survey:** 2601.17549 (no formal MCP security analysis — a standing gap);
2606.26479 (out-of-band defenses convergence); 2511.19477 §8 (programmatic
constraints over LLM judgment: deterministic boundaries, allowlists, action
restriction, specialization); reachability-based confinement; BrowseSafe-Bench
(browser-grounded injection evaluation).

**Novel position:** every competitor enforces policy in the *agent* or *MCP
server* layer — both are inside the model's influence. Aileron enforces in the
**browser process** — below the agent stack, on the only path to real effects
(DOM actuation, network, storage) — with **machine-checked preconditions**
(Lean theorems over the routing/gating model), not prompt text. The browser is
also the only component that can see *which page content influenced which
action address* (epoch+ref provenance), enabling structural taint rules.

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| $\Gamma$ | capability grant: (session, tool-class, scope, consent) |
| $\tau(a)$ | provenance of action $a$: (epoch, ref, pane, session) |
| $W(\text{page})$ | untrusted-content witness set of a page (form fields, comments, hidden text) |

## YP-4: Theoretical Foundation

**Principle (effects at the kernel):** all irreversible effects (navigate,
submit, click, cookie write, download, clipboard, payment-class actions) pass
through one dispatcher; policy is evaluated there. The agent's *reasoning* is
never trusted; the dispatcher is.

**Definition (structural taint):** content from $W(\text{page})$ cannot create
or widen grants. Formally: $\Gamma$ is modified only by (a) the human operator,
(b) static config, (c) narrowing delegation (YP-MCP-SESSIONS-002). Web content
is read-only with respect to $\Gamma$ — injection cannot amplify reach *by
construction*, not by detection.

**Definition (sensitivity-classed actions):** actions are classed
(read / mutate / irreversible); irreversible ones require
$\text{consent} \in \Gamma$ captured from the human channel, and semantic
guards (label/classifier preconditions: "refund", "delete", "pay" classes) as
fallback allow/deny lists — the 2511.19477 tool-layer pattern, moved into the
kernel dispatcher with typed preconditions.

**Theorem shape (confinement):** absent a grant from (a)/(b)/(c), no sequence
of agent actions crosses a sensitivity boundary. Statable in Lean over the
dispatcher model (ACT-CONF-001); the epoch/ref provenance makes the
hyperproperty checkable per action.

**Honest limit:** confinement bounds *effects*, not *beliefs* — an injected
instruction can still make the agent waste effort or read the wrong page.
Impossibility results apply to detection; structural confinement's claim is
narrower and achievable: **no exfiltration or irreversible cross-scope action
without a grant.**

## YP-5: Algorithm Specification

```
dispatch(a, τ(a)):
  class = classify(a.tool, a.target)                    # read|mutate|irreversible
  if class ≠ read and ¬grant_covers(Γ, session, a):     # structural check
      DENY(reason=CAPABILITY, audit_log)
  if class = irreversible:
      require consent_token(Γ) or human_channel         # never web content
  semantic_guards(a.target.label, class)                # denylist fallback
  epoch_gate(τ(a))                                      # YP-AGENT-ACI-001
  execute(a)
```

## YP-6: Test Vector Specification

BrowseSafe-Bench classes as vectors: hidden-text injection → attempt
cross-origin submit → must DENY(CAPABILITY); legitimate same-scope flow with
grant → must PASS; consent captured from page content → must be REJECTED
(structural taint); revocation mid-session → next irreversible action DENY.

## YP-7: Domain Constraints

Semantic guards inherit NLP fragility — they are the *fallback*, not the
mechanism; the mechanism is grant presence. False denials (utility loss) are
the cost; policy profiles (strict/balanced) expose the tradeoff.

## YP-8: Bibliography

1. arXiv:2606.26479 — out-of-band defense convergence.
2. arXiv:2601.17549 — MCP formal-analysis gap (this YP is a direct answer).
3. arXiv:2511.19477 §8 — programmatic constraints, production evidence.
4. Brave Security — Comet injection analysis (hidden-text class).
5. BrowseSafe-Bench — browser-grounded injection evaluation.

## YP-9: Knowledge Graph Concepts

capability_confinement, enforcement_point, prompt_injection, structural_taint,
irreversible_actions, consent, sensitivity_classes, denial_as_utility_cost

## YP-10: Quality Checklist

- [x] Enforcement point below the agent stack (browser dispatcher)
- [x] Structural taint: web content cannot widen grants
- [x] Sensitivity classes + consent, denylist as fallback only
- [x] Honest limits stated (effects not beliefs; utility cost)
- [ ] Lean ACT-CONF-001 (confinement theorem) — Year-1..2 formal work
- [ ] BrowseSafe-Bench evaluation harness — Year-2
