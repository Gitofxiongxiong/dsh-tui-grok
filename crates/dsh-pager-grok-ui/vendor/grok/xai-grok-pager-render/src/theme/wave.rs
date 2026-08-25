//! Grok Build traveling-wave and pulse primitives.
//!
//! Extracted without formula changes from
//! `xai-grok-pager-render/src/theme/tokyonight.rs` at source revision
//! `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`. Keeping this small slice
//! avoids importing palette constructors before the Appearance tranche.

/// Compute animated brightness for a traveling wave effect.
///
/// Creates a wave that travels along the accent line. Each row has a fixed phase
/// offset so the wave appears to move smoothly regardless of block height.
///
/// # Arguments
/// - `tick`: Frame counter (increments each render tick)
/// - `row`: Current row within the block (0 = top)
/// - `wave_rows`: Rows per full wave cycle (e.g., 32)
/// - `speed`: Wave speed (radians per tick, e.g., 0.15)
///
/// # Returns
/// Brightness value in [0.0, 1.0] for this row at this tick.
pub fn wave_brightness(tick: u64, row: u16, wave_rows: u16, speed: f32) -> f32 {
    use std::f32::consts::PI;

    let rows_per_wave = wave_rows.max(1) as f32;
    let phase = (row as f32 / rows_per_wave) * 2.0 * PI;

    // Time-based oscillation
    let t = tick as f32 * speed;

    // sin²(t + phase) gives smooth 0-1 oscillation
    let sin_val = (t + phase).sin();
    sin_val * sin_val
}

/// Compute a smooth pulsing brightness for a single element (icon, indicator).
///
/// Unlike [`wave_brightness`] which creates a spatial wave across rows,
/// this is a simple temporal pulse: all elements sharing the same tick
/// pulse in unison.
///
/// # Arguments
/// - `tick`: Frame counter (increments each render tick, ~30fps)
/// - `speed`: Pulse speed (radians per tick). The returned value uses
///   `sin²`, which has period π, so the visible bright→dim→bright cycle
///   is `π / (speed * fps)`. At 30fps, `speed = 0.08` ≈ 1.3s per cycle;
///   for a 2.5s cycle pass `speed ≈ 0.042`.
///
/// # Returns
/// Brightness value in [0.0, 1.0].
pub fn pulse_brightness(tick: u64, speed: f32) -> f32 {
    let t = tick as f32 * speed;
    let sin_val = t.sin();
    sin_val * sin_val
}

#[cfg(test)]
mod tests {
    use super::{pulse_brightness, wave_brightness};

    #[test]
    fn wave_uses_fixed_spatial_phase() {
        let top = wave_brightness(0, 0, 32, 0.15);
        let quarter = wave_brightness(0, 8, 32, 0.15);
        let half = wave_brightness(0, 16, 32, 0.15);

        assert!(top.abs() < f32::EPSILON);
        assert!((quarter - 1.0).abs() < 1e-6);
        assert!(half < 1e-12);
    }

    #[test]
    fn wave_and_pulse_stay_in_unit_interval() {
        for tick in 0..500 {
            let wave = wave_brightness(tick, (tick % 97) as u16, 32, 0.15);
            let pulse = pulse_brightness(tick, 0.08);
            assert!((0.0..=1.0).contains(&wave));
            assert!((0.0..=1.0).contains(&pulse));
        }
    }
}
