//! Frame-loop tweening used for viewport transitions and layout animation.
//!
//! Animations drive real signal updates (rather than CSS transitions) so
//! that everything derived from positions — edges, the minimap, the
//! background pattern — stays perfectly in sync every frame.

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
pub(crate) async fn sleep_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sleep_ms(ms: u64) {
    futures_timer::Delay::new(std::time::Duration::from_millis(ms)).await;
}

pub(crate) fn ease_in_out_cubic(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Increment the epoch, cancelling any running tween that was started under a
/// previous epoch. Returns the new value.
pub(crate) fn bump_epoch(mut epoch: Signal<u64>) -> u64 {
    let next = *epoch.peek() + 1;
    epoch.set(next);
    next
}

/// Run `apply` with an eased progress in `0..=1` every frame for
/// `duration_ms`. The tween stops early (without completing) if `epoch`
/// changes — any new interaction or animation cancels it.
pub(crate) fn tween(epoch: Signal<u64>, duration_ms: u64, mut apply: impl FnMut(f64) + 'static) {
    if duration_ms == 0 {
        apply(1.0);
        return;
    }
    let my_epoch = bump_epoch(epoch);
    spawn(async move {
        let start = web_time::Instant::now();
        loop {
            sleep_ms(16).await;
            if *epoch.peek() != my_epoch {
                return;
            }
            let t = (start.elapsed().as_secs_f64() * 1000.0 / duration_ms as f64).min(1.0);
            apply(ease_in_out_cubic(t));
            if t >= 1.0 {
                return;
            }
        }
    });
}
