
## Stage 1a executed (2026-11-06): aileron-core leaf modules

The manifest's ordering was corrected by the dependency audit: **core
extraction precedes engine extraction** — servo/offscreen_webview sit ON
core modules (wm, extensions, db, net::privacy, debug_capturer), so
"engine out first" was impossible without core.

Shipped: `aileron-core` crate with config, color, db, debug_capturer,
extensions, git, i18n, net, platform/paths (dir half), profiling,
update_check, wm. aileron re-exports one-for-one — zero call-site churn.
Core Cargo deps: the 16 crates the moved modules import (extracted
mechanically, then iterated: thiserror, chrono, reqwest(blocking),
attohttpc, tempfile(dev) surfaced by the compiler).

Verified: core 540 tests, aileron lib 663, both profiles green, clippy
clean. paths.rs split at its real seam (ProjectDirs dir helpers = core;
PlatformOps OS helpers = aileron platform).

Lesson for stage 1b: run the crate-boundary grep BEFORE the manifest
asserts cleanliness — the pattern `crate::(platform|mcp|sync|…)` was
missed on the first sweep and the compiler caught platform::paths.
