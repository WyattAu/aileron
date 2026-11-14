//! aileron-headed: the headed shell layer (S4 stage 3 step 2).
//!
//! GPU compositor, terminal, input, chrome bridge, popup — everything
//! that needs a display. Depends on aileron-core (leaf) and
//! aileron-engine (wry/WebKit panes). The agent binary never links
//! this crate.

pub mod gfx;

#[cfg(feature = "terminal")]
pub mod terminal;
