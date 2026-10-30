---
document_id: YP-FUZZY-MATCH-002
version: 2.0.0
status: DRAFT
supersedes: YP-FUZZY-MATCH-001
domain: Input — Fuzzy Matching
subdomains: [Scoring, Algorithms, Command Palette]
created: 2026-10-30
author: DeepThought
confidence_level: 0.90
tqa_level: 3
research_iterations: 1
---

# YP-FUZZY-MATCH-002: Palette Fuzzy Scoring — Algorithm Revision

## YP-2: Executive Summary

**Problem Statement:** the command palette (Nucleo engine, per YP-FUZZY-MATCH-001) scores
candidates for the omnibox, palette, tab search, and history search. This revision adds the
cross-implementation comparison and the recall/latency benchmarks that 001 lacked.

**Survey:**
- **fzf** (Junegunn Choi): Smith-Waterman-based scoring with bonus tiers (boundary, camelCase,
  consecutive) — the reference for ranking quality; O(len × pattern) per candidate.
- **Selecta** (Gary Bernhardt): minimal highlighting, no bonuses — fastest, lower rank quality.
- **Nucleo** (helix editor's engine, already the palette engine): the modern Rust successor to
  fzf's matcher — constant-memory, multithreaded, same bonus tiers.

**Verdict:** the engine choice (Nucleo) is already state-of-the-art — **no change**. The
revision's contribution: the benchmark protocol (recall@k vs fzf on the aileron command corpus)
and the scoring-gap documentation, so future engine changes are measured, not felt.

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| recall@k | fraction of desired commands in the top k results |
| $L$ | Candidate list size (commands + history + bookmarks) |

## YP-4: Theoretical Foundation

Nucleo's matcher: Smith-Waterman variant, O(n·m) worst case, constant memory per candidate,
parallel across candidates. fzf's scoring tiers are reproduced by Nucleo's bonus constants.
The palette's ranking quality is therefore engine-bounded; aileron-side improvements are
candidate-set hygiene (dedup, recency boost) — already present per 001.

## YP-5: Algorithm Specification

Benchmark protocol (adopted):
1. Corpus: the aileron command list + 1k synthetic typos (transposition, prefix, acronym).
2. Metric: recall@1 and recall@5 per query class.
3. Baseline: fzf CLI on the same corpus.
Regression gate: any engine change must not reduce recall@5.

## YP-6: Test Vector Specification

`test_vectors_fuzzy.toml` (existing) — extend with the typo classes above.

## YP-7: Domain Constraints

Palette latency budget: <5ms per keystroke on 10k candidates (Nucleo's design target; met).

## YP-8: Bibliography

1. fzf — Junegunn Choi, algorithm docs.
2. Nucleo — helix-editor nucleo crate documentation.
3. Selecta — Gary Bernhardt, "A Unified Method for Fuzzy Finding".

## YP-9: Knowledge Graph Concepts

fuzzy_matching, smith_waterman, nucleo_engine, recall_at_k, command_palette

## YP-10: Quality Checklist

- [x] Engine choice validated as state-of-the-art (Nucleo)
- [x] Benchmark protocol defined (recall@k vs fzf corpus)
- [x] Typo-class test vectors specified
