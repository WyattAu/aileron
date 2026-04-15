---
document_id: YP-INPUT-MODES-001
version: 1.0.0
status: DRAFT
domain: Input & Event Handling
subdomains: [State Machines, Event Routing, Keyboard Handling]
applicable_standards: [IEEE 1016-2009]
created: 2026-04-11
author: DeepThought
confidence_level: 0.93
tqa_level: 3
---

# YP-INPUT-MODES-001: Modal Input State Machine & Event Routing

## YP-2: Executive Summary

**Problem Statement:**
Define a deterministic finite automaton (DFA) $M = (Q, \Sigma, \delta, q_0, F)$ that routes keyboard/mouse events from winit to the correct subsystem (Servo, egui, or Lua command handler) based on the current mode, with $O(1)$ per-event dispatch latency.

**Scope:**
- In-scope: Mode transitions, event routing, keybinding dispatch, mode indicator display
- Out-of-scope: Mouse gesture recognition, touch input, IME (Input Method Editor) composition
- Assumptions: Events arrive sequentially on the main thread; no concurrent event sources

## YP-3: Nomenclature

| Symbol | Description | Units | Domain | Source |
|--------|-------------|-------|--------|--------|
| $M$ | Modal state machine (DFA) | — | Automata theory | — |
| $Q$ | Set of states (modes) | — | $\{\text{Normal}, \text{Insert}, \text{Command}\}$ | — |
| $\Sigma$ | Input alphabet (events) | — | $\text{winit::Event}$ | winit docs |
| $\delta$ | Transition function | — | $Q \times \Sigma \to Q$ | — |
| $q_0$ | Initial state | — | $\text{Normal}$ | — |
| $F$ | Accepting states | — | All states | — |
| $K$ | Keybinding registry | — | $\text{Map<KeyCombo, Action>}$ | — |

## YP-4: Theoretical Foundation

### Axioms

**AX-MODE-001 (Determinism):** For any state $q \in Q$ and event $e \in \Sigma$, the transition function yields exactly one next state: $|\delta(q, e)| = 1$.
*Justification:* Non-deterministic mode transitions would cause unpredictable behavior.
*Verification:* Exhaustive unit tests for all state×event combinations.

**AX-MODE-002 (Event Exclusivity):** Each input event is routed to exactly one subsystem: either Servo, egui, or the command handler. No event is processed by multiple subsystems.
*Justification:* Duplicate event processing causes double-typing, double-scrolling, or conflicting actions.
*Verification:* Event counter assertions in tests.

**AX-MODE-003 (Mode Persistence):** The current mode persists until an explicit transition event is received. Window focus/blur events do not change the mode.
*Justification:* Losing and regaining focus should not reset the user's workflow state.
*Verification:* Focus cycle test.

### Definitions

**DEF-MODE-001 (Mode Enum):**
$$
\text{Mode} = \{ \text{Normal}, \text{Insert}, \text{Command} \}
$$

**DEF-MODE-002 (Transition Function):**
$$
\delta(q, e) = \begin{cases}
\text{Insert} & \text{if } q = \text{Normal} \land e = \text{Key}(i) \\
\text{Command} & \text{if } q = \text{Normal} \land e = \text{Key}(:) \\
\text{Normal} & \text{if } q = \text{Insert} \land e = \text{Key}(\text{Esc}) \\
\text{Normal} & \text{if } q = \text{Command} \land e = \text{Key}(\text{Esc}) \\
q & \text{otherwise (no transition)}
\end{cases}
$$

**DEF-MODE-003 (Event Routing):**
$$
\text{route}(q, e) = \begin{cases}
\text{ToServo}(e) & \text{if } q = \text{Insert} \land \text{is\_input\_event}(e) \\
\text{ToCommandHandler}(e) & \text{if } q = \text{Command} \land \text{is\_input\_event}(e) \\
\text{ToKeybindingHandler}(e) & \text{if } q = \text{Normal} \land \text{is\_input\_event}(e) \\
\text{ToEgui}(e) & \text{if } \text{is\_mouse\_event}(e) \lor \text{is\_window\_event}(e) \\
\text{Discard}(e) & \text{otherwise}
\end{cases}
$$

**DEF-MODE-004 (KeyCombo):** A key combination $c = (\text{modifiers}, \text{key})$ where modifiers $\subseteq \{\text{Ctrl}, \text{Alt}, \text{Shift}, \text{Super}\}$ and key is a physical or logical key.

### Lemmas

**LEM-MODE-001 (Transition Latency):** The transition function $\delta$ executes in $O(1)$ time.
*Proof:* The transition function is a pattern match on the mode enum (3 variants) and the event type (finite cases). Pattern matching on enums is $O(1)$ in Rust. ∎

**LEM-MODE-002 (Keybinding Lookup):** Looking up a keybinding in a HashMap-based registry takes $O(1)$ expected time.
*Proof:* Rust's HashMap is based on SwissTable (hash table with Robin Hood hashing). Average lookup is $O(1)$. Worst case $O(n)$ for hash collisions, but negligible for typical keybinding counts (< 200). ∎

### Theorems

**THM-MODE-001 (No Lost Events):** Every input event $e$ from winit is either routed to a subsystem or explicitly discarded. No event is silently dropped.
*Proof:* The event loop calls `route(q, e)` for every event. The routing function has exhaustive match arms covering all cases (AX-MODE-002). Therefore, every event reaches exactly one destination. ∎

**THM-MODE-002 (Mode Transition Correctness):** The mode transition function satisfies: for any sequence of events $e_1, e_2, \ldots, e_n$, the mode after processing is $\delta^n(q_0, e_1, \ldots, e_n) = \delta(\delta(\ldots\delta(q_0, e_1), e_2), \ldots, e_n)$.
*Proof:* By induction on $n$.
- Base case ($n=0$): Mode is $q_0 = \text{Normal}$. ✓
- Inductive step: Assume mode after $k$ events is correct. Event $e_{k+1}$ produces exactly one transition (AX-MODE-001). Therefore mode after $k+1$ events is correct. ∎

**THM-MODE-003 (Keybinding Override):** User-defined keybindings (from Lua) override default keybindings when both exist for the same KeyCombo.
*Proof:* The keybinding registry is layered: defaults → user config. Lookup checks user config first, then defaults. If a match is found in user config, the default is shadowed. ∎

## YP-5: Algorithm Specification

### ALG-MODE-001: Process Input Event

```
Algorithm: process_event
Input: state: AppState, event: winit::Event
Output: state': AppState

1:  function process_event(state, event)
2:    // Step 1: Check for global shortcuts (mode-independent)
3:    if is_global_shortcut(event):
4:      execute_global_action(state, event)
5:      return state
6:    
7:    // Step 2: Route mouse/window events to egui
8:    if is_mouse_or_window_event(event):
9:      egui::handle_event(state.egui_ctx, event)
10:     return state
11:   
12:   // Step 3: Mode-dependent routing
13:   match state.mode:
14:     case Normal =>
15:       // Check Lua-defined keybindings first
16:       if let Some(action) = state.lua_keybindings.get(event.key_combo):
17:         execute_action(state, action)
18:       else if let Some(action) = state.default_keybindings.get(event.key_combo):
19:         execute_action(state, action)
20:       // Mode transitions
21:       else if event.key == Key::Character('i'):
22:         state.mode = Insert
23:       else if event.key == Key::Character(':'):
24:         state.mode = Command
25:       else:
26:         discard(event)
27:     
28:     case Insert =>
29:       if event.key == Key::Escape:
30:         state.mode = Normal
31:       else:
32:         servo::send_event(state.active_pane, event)
33:     
34:     case Command =>
35:       if event.key == Key::Escape:
36:         state.mode = Normal
37:         state.command_palette.close()
38:       else:
39:         state.command_palette.handle_input(event)
40:   end match
41:   
42:   return state
43: end function
```

**Complexity:**
| Metric | Value | Derivation |
|--------|-------|------------|
| Time | $O(1)$ amortized | HashMap lookup + enum match |
| Space | $O(1)$ | No allocation per event |

## YP-6: Test Vector Specification

| Category | Description | Coverage Target |
|----------|-------------|-----------------|
| Nominal | Mode transitions, event routing, keybinding dispatch | 40% |
| Boundary | Rapid mode switching, all modifier combinations | 20% |
| Adversarial | Undefined key combos, events during mode transition | 15% |
| Regression | Focus/blur during Insert mode, Unicode input | 10% |
| Random | Property-based: event sequence preserves mode invariant | 15% |

## YP-7: Domain Constraints

- Maximum event processing latency: 1ms (must not cause frame drops)
- Keybinding registry size: < 500 entries (typical)
- Mode indicator update: < 1 frame (16.67ms at 60fps)

## YP-8: Bibliography

| ID | Citation | Relevance | TQA Level | Confidence |
|----|----------|-----------|-----------|------------|
| [^1] | Neovim mode system (neovim.io) | Reference modal implementation | 3 | 0.95 |
| [^2] | "Introduction to Automata Theory" — Hopcroft & Ullman | DFA formalization | 5 | 0.99 |
| [^3] | winit event types (docs.rs/winit) | Event type definitions | 3 | 0.95 |
| [^4] | egui input handling (docs.rs/egui) | Event consumption patterns | 3 | 0.90 |

## YP-9: Knowledge Graph Concepts

| ID | Concept | Language | Source | Confidence |
|----|---------|----------|--------|------------|
| CONCEPT-MODE-001 | Deterministic Finite Automaton | EN | [^2] | 0.99 |
| CONCEPT-MODE-002 | Modal Editing | EN | [^1] | 0.95 |
| CONCEPT-MODE-003 | Event Routing | EN | — | 0.90 |

## YP-10: Quality Checklist

- [x] Nomenclature table complete
- [x] All axioms have verification methods
- [x] All theorems have proofs
- [x] All algorithms have complexity analysis
- [x] Test vector categories defined
- [x] Domain constraints specified
- [x] Bibliography with TQA levels
