//! Blending two frames' rows.

/// `to` over `from` at `alpha` of 256, a pixel at a time, two channels to a
/// multiply (blue and red, then green and the unused byte), into `out`.
pub fn mix(out: &mut [u8], from: &[u8], to: &[u8], alpha: u16) {
    let alpha = alpha.min(256) as u32;
    for ((out, a), b) in out.as_chunks_mut::<4>().0.iter_mut().zip(from.as_chunks::<4>().0).zip(to.as_chunks::<4>().0) {
        let (a, b) = (u32::from_le_bytes(*a), u32::from_le_bytes(*b));
        let even = ((a & 0x00ff_00ff) * (256 - alpha) + (b & 0x00ff_00ff) * alpha) >> 8 & 0x00ff_00ff;
        let odd = (((a >> 8) & 0x00ff_00ff) * (256 - alpha) + ((b >> 8) & 0x00ff_00ff) * alpha) & 0xff00_ff00;
        *out = (even | odd).to_le_bytes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ends_are_one_frame_or_the_other() {
        let (a, b) = ([10u8, 20, 30, 0], [200u8, 100, 50, 0]);
        let mut out = [0u8; 4];
        mix(&mut out, &a, &b, 0);
        assert_eq!(out, a);
        mix(&mut out, &a, &b, 256);
        assert_eq!(out, b);
        mix(&mut out, &a, &b, 128);
        assert_eq!(out, [105, 60, 40, 0]);
    }
}
