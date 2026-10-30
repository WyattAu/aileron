---
document_id: YP-NET-ADBLOCK-002
version: 2.0.0
status: DRAFT
supersedes: YP-NET-ADBLOCK-001
domain: Network — Adblock Rule Matching
subdomains: [Indexing, Complexity, EasyList]
created: 2026-10-30
author: DeepThought
confidence_level: 0.88
tqa_level: 3
research_iterations: 1
---

# YP-NET-ADBLOCK-002: Rule Indexing & Match Complexity

## YP-2: Executive Summary

**Problem Statement:** EasyList-scale rule sets (100k+ rules) must evaluate per network request
at sub-millisecond cost. The naive scan is O(rules × request-attributes). The existing
implementation (domain constraints: `domain_constraints_net_adblock`) uses rule parsing +
network/cosmetic matching with `$redirect`/`$important`/`$badfilter` support and filter-list
auto-update; this revision adds the complexity analysis and the indexing roadmap.

**Survey:**
- **uBlock Origin** (gorhill): static-rule indexer with per-hostname buckets + tokenizer for
  URL-matching rules — O(bucket size) per request; the reference implementation.
- **Brave's adblock-rust**: the same architecture, Rust-native, benchmarked at >1M rules with
  sub-microsecond matches; the direct ecosystem neighbor (also Rust, also EasyList).
- **CSS-driven cosmetic filters**: keyed by hostname buckets, evaluated post-DOM.

**Verdict:** the indexing approach is solved in the ecosystem (adblock-rust is Rust, Apache-2.0,
and embeddable). **Adopt/reject:** aileron's own matcher is retained for the domain-constraint
integration and the `$redirect` semantics; the indexing upgrade (per-hostname buckets + a
tokenizer index) is the roadmap item, with adblock-rust as the comparison baseline for
benchmarks (adopt as a dev-dependency benchmark harness, reject as a replacement for now).

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| $R$ | Rule count (EasyList ≈ 100k) |
| $Q$ | Requests per page load (50–200) |
| $\mathrm{cost}(req)$ | Per-request match cost |

## YP-4: Theoretical Foundation

Naive: $\mathrm{cost}(req) = O(R)$. Indexed (per-hostname buckets + tokenizer):
$\mathrm{cost}(req) = O(\mathrm{bucket}(host))$ — buckets shrink 100k rules to hundreds per
hostname. The transactional rule: `$badfilter` negations must apply AFTER indexing (negation
removal pass) — the existing test vectors (`test_vectors_adblock.toml`) cover this.

## YP-5: Algorithm Specification

Roadmap (indexed matcher):
1. Parse rules → classify (network/cosmetic/redirect/important/badfilter)
2. Index network rules by hostname buckets + fallback bucket for pattern-only rules
3. Per request: bucket lookup → pattern match → priority resolution
   (important > normal; redirect > block)

## YP-6: Test Vector Specification

`test_vectors_adblock.toml` (existing) — extend with index-behavior vectors: bucket
assignment, badfilter negation, redirect priority.

## YP-7: Domain Constraints

HTTPS upgrade list (separate) is unaffected. Auto-update must invalidate the index atomically
(rebuild under the existing lock, swap on completion — the pattern used by the snapshot
backpressure fix).

## YP-8: Bibliography

1. gorhill — uBlock Origin static matcher design.
2. brave/adblock-rust — Rust implementation + benchmarks.
3. EasyList/EasyPrivacy specification.

## YP-9: Knowledge Graph Concepts

adblock, easylist, rule_indexing, hostname_buckets, tokenizer, redirect_semantics

## YP-10: Quality Checklist

- [x] Complexity model stated (naive vs indexed)
- [x] Ecosystem neighbor identified (adblock-rust) as benchmark baseline
- [x] Index-invalidation constraint (auto-update) documented
- [ ] Indexed matcher implementation (roadmap, benchmarked vs adblock-rust)
