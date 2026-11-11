//! Auto-advance through the playlist, on its own timer.

use std::time::{Duration, Instant};

pub struct Slideshow {
    play_delay: Duration,
    next_advance: Option<Instant>,
}

impl Slideshow {
    /// `autoplay` is ignored, with a note, when there is no delay to play at.
    pub fn new(autoplay: bool, play_delay: Duration) -> Self {
        if autoplay && play_delay.is_zero() {
            tracing::warn!(
                "--slideshow does nothing without a play delay (--play-delay or view.play_delay)"
            );
        }
        Self {
            play_delay,
            next_advance: (autoplay && !play_delay.is_zero()).then(|| Instant::now() + play_delay),
        }
    }

    /// Flips play mode; a zero delay means there is nothing to play.
    pub fn toggle(&mut self) {
        if self.play_delay.is_zero() {
            tracing::warn!("play mode needs a play delay (--play-delay or view.play_delay)");
            return;
        }
        let playing = self.next_advance.is_none();
        self.next_advance = playing.then(|| Instant::now() + self.play_delay);
        tracing::info!("play mode {}", if playing { "on" } else { "off" });
    }

    /// True once the step is due; re-arms for the one after it.
    pub fn due(&mut self) -> bool {
        let Some(deadline) = self.next_advance else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }
        self.next_advance = Some(Instant::now() + self.play_delay);
        true
    }

    /// The next step, while playing.
    pub fn deadline(&self) -> Option<Instant> {
        self.next_advance
    }
}
