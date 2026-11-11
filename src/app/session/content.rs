//! What is on screen: a still frame, or the current frame of an animation.

use std::time::Instant;

use crate::image::transform::Turn;
use crate::image::{Animation, Decoded, WorkingImage};

/// Animation playback: which frame is up and when the next one is due.
struct Playback {
    animation: Animation,
    frame_index: usize,
    advance_at: Instant,
    cycles_done: u32,
}

impl Playback {
    fn new(animation: Animation) -> Self {
        let advance_at = Instant::now() + animation.delay(0);
        Self {
            animation,
            frame_index: 0,
            advance_at,
            cycles_done: 0,
        }
    }

    fn advance(&mut self) -> bool {
        let next = self.frame_index + 1;
        if next < self.animation.len() {
            self.frame_index = next;
        } else {
            self.cycles_done += 1;
            match self.animation.cycles() {
                Some(max) if self.cycles_done >= max => return false,
                _ => self.frame_index = 0,
            }
        }
        // Each frame gets its full delay from now, so a late frame shifts the
        // rest back instead of being skipped.
        self.advance_at = Instant::now() + self.animation.delay(self.frame_index);
        true
    }
}

#[derive(Default)]
pub struct Content {
    frame: Option<WorkingImage>,
    playback: Option<Playback>,
}

impl Content {
    /// Nothing on screen; the frame and any animation are dropped.
    pub fn clear(&mut self) {
        self.frame = None;
        self.playback = None;
    }

    /// Puts decoded content on screen; the frame is then its first one.
    pub fn set(&mut self, decoded: Decoded) {
        match decoded {
            Decoded::Still(image) => {
                self.frame = Some(image);
                self.playback = None;
            }
            Decoded::Animated(animation) => {
                self.frame = Some(animation.frame(0));
                // One frame is not an animation; leave it as a still rather
                // than repainting it forever.
                self.playback = (animation.len() > 1).then(|| Playback::new(animation));
            }
        }
    }

    pub fn frame(&self) -> Option<&WorkingImage> {
        self.frame.as_ref()
    }

    /// Rotates or mirrors every frame, or the next one undoes it.
    pub fn apply(&mut self, turn: Turn) -> bool {
        if let Some(playback) = self.playback.as_mut() {
            playback.animation.apply(turn);
            self.frame = Some(playback.animation.frame(playback.frame_index));
            true
        } else if let Some(frame) = self.frame.as_mut() {
            frame.apply(turn);
            true
        } else {
            false
        }
    }

    /// Shows the next animation frame once the one on screen is due; true when
    /// it was due, even if that spent the animation and froze the last frame.
    pub fn advance(&mut self) -> bool {
        let Some(playback) = self.playback.as_mut() else {
            return false;
        };
        if Instant::now() < playback.advance_at {
            return false;
        }
        match playback.advance() {
            true => self.frame = Some(playback.animation.frame(playback.frame_index)),
            // Spent: the last frame stays up.
            false => self.playback = None,
        }
        true
    }

    /// When the next animation frame is due, while one is playing.
    pub fn next_frame_at(&self) -> Option<Instant> {
        self.playback.as_ref().map(|playback| playback.advance_at)
    }
}
