//! Indexed colour materialisation. Arithmetic cube/ramp, not a theme table.

use rill_vt_types::{Palette, Rgb};

pub(crate) fn indexed(n: u8, palette: &Palette) -> Rgb {
    match n {
        0..=15 => palette.ansi[n as usize],
        16..=231 => {
            let i = n - 16;
            let r = i / 36;
            let g = (i % 36) / 6;
            let b = i % 6;
            Rgb {
                r: cube(r),
                g: cube(g),
                b: cube(b),
            }
        }
        232..=255 => {
            let v = 8 + 10 * (n - 232);
            Rgb { r: v, g: v, b: v }
        }
    }
}

fn cube(level: u8) -> u8 {
    // SPEC-VT-COLOR §3: 0, 95, 135, 175, 215, 255.
    if level == 0 {
        0
    } else {
        55 + 40 * level
    }
}
