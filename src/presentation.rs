use crate::controller::Intent;
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

pub struct PresentationClock {
    last: Instant,
    paused: bool,
}

impl PresentationClock {
    pub fn new(now: Instant) -> Self {
        Self {
            last: now,
            paused: true,
        }
    }
    pub fn elapsed(&mut self, now: Instant, paused: bool) -> Duration {
        let elapsed = if paused || self.paused {
            Duration::ZERO
        } else {
            now.saturating_duration_since(self.last)
        };
        self.last = now;
        self.paused = paused;
        elapsed
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
}

/// Presentation-only state. Byte offsets are always extended grapheme boundaries.
#[derive(Default)]
pub struct Typewriter {
    text: String,
    boundaries: Vec<usize>,
    visible: usize,
    fractional: f64,
}

impl Typewriter {
    pub fn new(text: &str, complete: bool) -> Self {
        let boundaries = text
            .grapheme_indices(true)
            .map(|(index, g)| index + g.len())
            .collect();
        let mut state = Self {
            text: text.to_owned(),
            boundaries,
            ..Self::default()
        };
        if complete {
            state.finish();
        }
        state
    }

    pub fn tick(&mut self, elapsed: Duration, graphemes_per_second: f32) {
        if graphemes_per_second == 0.0 {
            self.finish();
            return;
        }
        if !graphemes_per_second.is_finite() || graphemes_per_second < 0.0 {
            return;
        }
        self.fractional += elapsed.as_secs_f64() * f64::from(graphemes_per_second);
        let count = self.fractional.floor() as usize;
        self.fractional -= count as f64;
        self.visible = self
            .visible
            .saturating_add(count)
            .min(self.boundaries.len());
    }

    pub fn finish(&mut self) {
        self.visible = self.boundaries.len();
        self.fractional = 0.0;
    }
    pub fn complete(&self) -> bool {
        self.visible == self.boundaries.len()
    }
    pub fn visible_text(&self) -> &str {
        &self.text[..self
            .visible
            .checked_sub(1)
            .map(|index| self.boundaries[index])
            .unwrap_or(0)]
    }
}

pub enum GatedIntent {
    Reveal,
    Dispatch(Intent),
    Ignore,
}

pub fn gate(intent: Intent, complete: bool) -> GatedIntent {
    match (intent, complete) {
        (Intent::Advance, false) => GatedIntent::Reveal,
        (Intent::Choose(_), false) => GatedIntent::Ignore,
        (intent, true) => GatedIntent::Dispatch(intent),
    }
}

/// One shot per entry. User input always wins arbitration in App.
#[derive(Default)]
pub struct AutoPlay {
    pub enabled: bool,
    elapsed: f32,
}
impl AutoPlay {
    pub fn set(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.enter();
    }
    pub fn enter(&mut self) {
        self.elapsed = 0.0;
    }
    pub fn tick(&mut self, seconds: f32, ready: bool, advance_route: bool, delay: f32) -> bool {
        if !advance_route {
            self.set(false);
            return false;
        }
        if !self.enabled || !ready {
            self.elapsed = 0.0;
            return false;
        }
        self.elapsed += seconds.max(0.0);
        if self.elapsed >= delay {
            self.elapsed = 0.0;
            true
        } else {
            false
        }
    }
}
