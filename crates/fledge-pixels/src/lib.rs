//! The thermostat UI's per-pixel loops, apart from the rest of nestlocal so
//! that they alone are built for speed (opt-level 3, the workspace's
//! thermostat profile) while nestlocal is built for size: a gen 2's root
//! slot has room for little more than one nestlocal and its update.
//!
//! Frames are BGRX, square, in the framebuffer's layout (nestlocal's
//! ui::device::Frame).

pub mod blend;
pub mod turn;
pub mod zoom;

/// A screen as the framebuffer holds it (nestlocal's ui::device::Frame).
pub type Frame = Vec<u8>;
