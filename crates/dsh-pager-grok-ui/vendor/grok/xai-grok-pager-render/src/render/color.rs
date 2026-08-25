//! Grok Build color blending primitive.
//!
//! This A1 slice is extracted from
//! `xai-grok-pager-render/src/render/color.rs` at source revision
//! `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`. It retains the upstream
//! RGB/Indexed conversion and quantization used by `blend_color`; the
//! line/buffer fading helpers remain with the later renderer tranche.

use ratatui::style::Color;

/// The 6 channel values in the 256-color 6×6×6 cube.
const CUBE_VALUES: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// Convert a 256-color indexed color to its (R, G, B) components.
pub fn indexed_to_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0 => (0, 0, 0),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        16..=231 => {
            let n = index - 16;
            let r = CUBE_VALUES[(n / 36) as usize];
            let g = CUBE_VALUES[((n % 36) / 6) as usize];
            let b = CUBE_VALUES[(n % 6) as usize];
            (r, g, b)
        }
        232..=255 => {
            let v = 8 + (index - 232) * 10;
            (v, v, v)
        }
    }
}

/// Map an RGB triplet to the nearest 256-color palette index (16–255).
pub fn nearest_indexed(r: u8, g: u8, b: u8) -> u8 {
    let ri = nearest_cube_channel(r);
    let gi = nearest_cube_channel(g);
    let bi = nearest_cube_channel(b);
    let cube_idx = 16 + 36 * ri as u16 + 6 * gi as u16 + bi as u16;
    let cube_dist = sq_dist(
        r,
        g,
        b,
        CUBE_VALUES[ri as usize],
        CUBE_VALUES[gi as usize],
        CUBE_VALUES[bi as usize],
    );

    let lum = (r as u16 + g as u16 + b as u16) / 3;
    let gray_step = if lum <= 3 {
        0u8
    } else if lum >= 243 {
        23
    } else {
        ((lum as i16 - 8 + 5) / 10).clamp(0, 23) as u8
    };
    let gv = (8 + gray_step as u16 * 10) as u8;
    let gray_dist = sq_dist(r, g, b, gv, gv, gv);

    if gray_dist < cube_dist {
        232 + gray_step
    } else {
        cube_idx as u8
    }
}

fn nearest_cube_channel(v: u8) -> u8 {
    let mut best = 0u8;
    let mut best_d = v.abs_diff(CUBE_VALUES[0]) as u16;
    for i in 1..6u8 {
        let d = v.abs_diff(CUBE_VALUES[i as usize]) as u16;
        if d < best_d {
            best = i;
            best_d = d;
        }
    }
    best
}

fn sq_dist(r1: u8, g1: u8, b1: u8, r2: u8, g2: u8, b2: u8) -> u32 {
    let dr = r1 as i32 - r2 as i32;
    let dg = g1 as i32 - g2 as i32;
    let db = b1 as i32 - b2 as i32;
    (dr * dr + dg * dg + db * db) as u32
}

fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Indexed(n) => Some(indexed_to_rgb(n)),
        _ => None,
    }
}

/// Blend a single color channel: lerp from base toward original based on opacity.
#[inline]
pub fn blend_channel(base: u8, original: u8, opacity: f32) -> u8 {
    let result = base as f32 * (1.0 - opacity) + original as f32 * opacity;
    result.round() as u8
}

/// Blend a color toward a base color based on opacity.
///
/// RGB inputs stay RGB. If either input is Indexed, the result is quantized
/// back to the nearest xterm-256 palette entry. Named ANSI colors return
/// `None` because their RGB values depend on terminal configuration.
pub fn blend_color(base: Color, original: Color, opacity: f32) -> Option<Color> {
    let (base_r, base_g, base_b) = color_to_rgb(base)?;
    let (orig_r, orig_g, orig_b) = color_to_rgb(original)?;

    let r = blend_channel(base_r, orig_r, opacity);
    let g = blend_channel(base_g, orig_g, opacity);
    let b = blend_channel(base_b, orig_b, opacity);

    Some(match (base, original) {
        (Color::Indexed(_), _) | (_, Color::Indexed(_)) => Color::Indexed(nearest_indexed(r, g, b)),
        _ => Color::Rgb(r, g, b),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_colors_round_trip() {
        for &idx in &[16u8, 141, 149, 210, 234, 243, 245, 255] {
            let (r, g, b) = indexed_to_rgb(idx);
            assert_eq!(nearest_indexed(r, g, b), idx);
        }
    }

    #[test]
    fn blend_color_rgb_matches_upstream_lerp() {
        let base = Color::Rgb(0, 0, 0);
        let original = Color::Rgb(100, 150, 200);
        assert_eq!(blend_color(base, original, 0.0), Some(base));
        assert_eq!(blend_color(base, original, 1.0), Some(original));
        assert_eq!(
            blend_color(base, original, 0.5),
            Some(Color::Rgb(50, 75, 100))
        );
    }

    #[test]
    fn indexed_or_mixed_input_stays_indexed() {
        let indexed = Color::Indexed(232);
        let rgb = Color::Rgb(238, 238, 238);
        assert!(matches!(
            blend_color(indexed, Color::Indexed(255), 0.5),
            Some(Color::Indexed(_))
        ));
        assert!(matches!(
            blend_color(indexed, rgb, 0.5),
            Some(Color::Indexed(_))
        ));
        assert!(matches!(
            blend_color(rgb, indexed, 0.5),
            Some(Color::Indexed(_))
        ));
    }

    #[test]
    fn named_color_is_not_resolved_implicitly() {
        let rgb = Color::Rgb(100, 100, 100);
        assert_eq!(blend_color(Color::Red, rgb, 0.5), None);
        assert_eq!(blend_color(rgb, Color::Red, 0.5), None);
    }
}
