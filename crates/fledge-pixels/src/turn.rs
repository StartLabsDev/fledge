//! A frame turned back a quarter onto the gen 2's panel.

/// Panel rows turned at a time: 16 rows of 320 pixels, 20 KB, stay in the
/// gen 2's 32 KB data cache.
const BAND: usize = 16;

/// Writes the panel's columns that the frame's rows `first..=last` make into
/// `page` (rows `stride` bytes apart), turned back a quarter: the panel's
/// pixel (x, y) is the frame's row x, column side - 1 - y.
///
/// For one x, panel rows y.. are frame pixels running back along row x. A
/// band of BAND panel rows at a time, so each frame row is read a run at a
/// time (not a pixel per cache line), into lines that stay in the cache,
/// each panel row's run then written whole (the display's memory is slow a
/// pixel at a time).
pub fn back(page: &mut [u8], stride: usize, side: usize, frame: &[u8], first: usize, last: usize) {
    let row = side * 4;
    let last = last.min(side - 1);
    let mut band = vec![0u8; BAND * row];
    for top in (0..side).step_by(BAND) {
        let rows = BAND.min(side - top);
        for x in first..=last {
            // Frame row x, columns side - top - rows ..= side - 1 - top.
            let start = x * row + (side - top - rows) * 4;
            let run = &frame[start..start + rows * 4];
            for (n, pixel) in run.as_chunks::<4>().0.iter().rev().enumerate() {
                band[n * row + x * 4..n * row + x * 4 + 4].copy_from_slice(pixel);
            }
        }
        for n in 0..rows {
            let y = top + n;
            page[y * stride + first * 4..y * stride + (last + 1) * 4]
                .copy_from_slice(&band[n * row + first * 4..n * row + (last + 1) * 4]);
        }
    }
}
