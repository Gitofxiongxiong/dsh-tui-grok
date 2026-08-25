//! Deterministic conversion from monotonic runtime time to Grok render ticks.

use std::time::Duration;

pub const DEFAULT_ANIMATION_FPS: u32 = 30;
pub const DEFAULT_WAVE_ROWS: u16 = 32;
pub const GROK_WAVE_SPEED: f32 = 0.15;

/// Convert elapsed monotonic time to a frame tick without using redraw count.
pub fn tick_from_elapsed(elapsed: Duration, fps: u32) -> u64 {
    let fps = fps.max(1);
    let whole = elapsed.as_secs().saturating_mul(u64::from(fps));
    let fractional =
        u64::from(elapsed.subsec_nanos()).saturating_mul(u64::from(fps)) / 1_000_000_000;
    whole.saturating_add(fractional)
}

pub fn animation_tick(elapsed: Duration) -> u64 {
    tick_from_elapsed(elapsed, DEFAULT_ANIMATION_FPS)
}

#[cfg(test)]
mod tests {
    use super::{animation_tick, tick_from_elapsed};
    use std::time::Duration;

    #[test]
    fn default_tick_uses_elapsed_time_boundaries() {
        assert_eq!(animation_tick(Duration::ZERO), 0);
        assert_eq!(animation_tick(Duration::from_millis(33)), 0);
        assert_eq!(animation_tick(Duration::from_millis(34)), 1);
        assert_eq!(animation_tick(Duration::from_millis(66)), 1);
        assert_eq!(animation_tick(Duration::from_millis(67)), 2);
        assert_eq!(animation_tick(Duration::from_secs(1)), 30);
    }

    #[test]
    fn tick_saturates_for_extreme_elapsed_time() {
        assert_eq!(tick_from_elapsed(Duration::MAX, u32::MAX), u64::MAX);
    }
}
