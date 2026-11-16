//! The cards zoomed out while the ring moves between them: the per-pixel
//! work is fledge-pixels' (built for speed), the choices for this unit here.

pub use fledge_pixels::zoom::{compose, reach, round, shrink, zoomed};

/// The chosen card's rim and its neighbours' shading (fledge_pixels::zoom::
/// focus): with a soft halo, except on the gen 2, whose core does floating
/// point slowly (it cost 30 ms a frame).
pub fn focus(out: &mut [u8], at: f32, z: f32, accent: Option<[u8; 3]>) {
    fledge_pixels::zoom::focus(out, at, z, accent, crate::device::current().generation != 2);
}
