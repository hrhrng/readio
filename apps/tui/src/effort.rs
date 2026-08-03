//! Reading pace, dressed as an agent's reasoning effort.
//!
//! A coding agent lets you pick how hard the model should think, and the honest
//! consequence of asking for more is that output arrives more slowly. readio
//! borrows the control and keeps the consequence: `minimal` skims, `max` savours.
//! Six levels, the same six a coding agent offers.
//!
//! Every level is a **multiplier**, and one multiplier drives both worlds:
//! the reveal speed is `reading.speed × multiplier`, and read-aloud plays at
//! `multiplier` — so "slower" means the same thing whether the text is being
//! typed out or spoken. The numbers live in `~/.readio/config.yaml`, because the
//! pace someone reads at is not something a program should have opinions about.

use serde::{Deserialize, Serialize};

use crate::i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

/// Every level, fastest first, which is the order `/effort` lists them in.
pub const LEVELS: [Effort; 6] = [
    Effort::Minimal,
    Effort::Low,
    Effort::Medium,
    Effort::High,
    Effort::Xhigh,
    Effort::Max,
];

impl Default for Effort {
    /// `high`, whose multiplier is 1.0: the pace readio has always read at.
    fn default() -> Self {
        Effort::High
    }
}

impl Effort {
    /// The spelling used in the config file, on the command line and in the menu.
    pub fn code(self) -> &'static str {
        match self {
            Effort::Minimal => "minimal",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }

    /// `Minimal`, the way a menu of levels writes it.
    pub fn label(self) -> &'static str {
        match self {
            Effort::Minimal => "Minimal",
            Effort::Low => "Low",
            Effort::Medium => "Medium",
            Effort::High => "High",
            Effort::Xhigh => "Xhigh",
            Effort::Max => "Max",
        }
    }

    /// What choosing this level means for a reader, in their own language.
    pub fn about(self) -> &'static str {
        match self {
            Effort::Minimal => t("effort.minimal"),
            Effort::Low => t("effort.low"),
            Effort::Medium => t("effort.medium"),
            Effort::High => t("effort.high"),
            Effort::Xhigh => t("effort.xhigh"),
            Effort::Max => t("effort.max"),
        }
    }

    /// Generous about spelling: a level should never be refused over a case or a
    /// hyphen, and `x-high`, `xhigh` and `extra` are the same idea.
    pub fn parse(arg: &str) -> Option<Self> {
        let key: String = arg
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        match key.as_str() {
            "minimal" | "min" | "skim" | "0" => Some(Effort::Minimal),
            "low" | "1" => Some(Effort::Low),
            "medium" | "med" | "mid" | "2" => Some(Effort::Medium),
            "high" | "normal" | "3" => Some(Effort::High),
            "xhigh" | "extrahigh" | "extra" | "veryhigh" | "4" => Some(Effort::Xhigh),
            "max" | "maximum" | "ultra" | "5" => Some(Effort::Max),
            _ => None,
        }
    }

    /// The next level `^r` should move to, wrapping at the top.
    pub fn next(self) -> Self {
        let at = LEVELS.iter().position(|l| *l == self).unwrap_or(3);
        LEVELS[(at + 1) % LEVELS.len()]
    }
}

/// The multiplier each level stands for, straight from the config file.
///
/// Higher effort is slower, which is why the defaults descend. Nothing enforces
/// that order: a reader who wants `max` to mean "fastest" is allowed to be wrong
/// in their own config.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ladder {
    pub minimal: f32,
    pub low: f32,
    pub medium: f32,
    pub high: f32,
    pub xhigh: f32,
    pub max: f32,
}

impl Default for Ladder {
    fn default() -> Self {
        Self {
            minimal: 2.5,
            low: 2.0,
            medium: 1.5,
            high: 1.0,
            xhigh: 0.85,
            max: 0.7,
        }
    }
}

impl Ladder {
    pub fn get(&self, level: Effort) -> f32 {
        let raw = match level {
            Effort::Minimal => self.minimal,
            Effort::Low => self.low,
            Effort::Medium => self.medium,
            Effort::High => self.high,
            Effort::Xhigh => self.xhigh,
            Effort::Max => self.max,
        };
        clamp(raw)
    }

    pub fn set(&mut self, level: Effort, multiplier: f32) {
        let value = clamp(multiplier);
        match level {
            Effort::Minimal => self.minimal = value,
            Effort::Low => self.low = value,
            Effort::Medium => self.medium = value,
            Effort::High => self.high = value,
            Effort::Xhigh => self.xhigh = value,
            Effort::Max => self.max = value,
        }
    }

    /// Pull hand-edited numbers into a range that still produces sound.
    pub fn clamp_all(&mut self) {
        for level in LEVELS {
            let value = self.get(level);
            self.set(level, value);
        }
    }
}

/// The embedded player and the text pacer share this supported range, and a
/// reveal speed of zero is a hang, so every multiplier lands inside the same
/// range `/rate` accepts.
fn clamp(multiplier: f32) -> f32 {
    if multiplier.is_finite() {
        multiplier.clamp(0.5, 3.0)
    } else {
        1.0
    }
}

/// `1.5×`, as the interface writes a multiplier.
pub fn times(multiplier: f32) -> String {
    let trimmed = format!("{multiplier:.2}");
    let trimmed = trimmed.trim_end_matches('0').trim_end_matches('.');
    format!("{trimmed}×")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_level_changes_nothing() {
        assert_eq!(Effort::default(), Effort::High);
        assert_eq!(Ladder::default().get(Effort::High), 1.0);
    }

    #[test]
    fn more_effort_means_a_slower_read() {
        let ladder = Ladder::default();
        let speeds: Vec<f32> = LEVELS.iter().map(|level| ladder.get(*level)).collect();
        for pair in speeds.windows(2) {
            assert!(pair[0] > pair[1], "{speeds:?} should descend");
        }
    }

    #[test]
    fn every_spelling_a_reader_might_try_lands() {
        assert_eq!(Effort::parse("Minimal"), Some(Effort::Minimal));
        assert_eq!(Effort::parse("  MIN "), Some(Effort::Minimal));
        assert_eq!(Effort::parse("x-high"), Some(Effort::Xhigh));
        assert_eq!(Effort::parse("extra high"), Some(Effort::Xhigh));
        assert_eq!(Effort::parse("ultra"), Some(Effort::Max));
        assert_eq!(Effort::parse("3"), Some(Effort::High));
        assert_eq!(Effort::parse("sideways"), None);
    }

    #[test]
    fn ctrl_r_visits_every_level_and_comes_back() {
        let mut level = Effort::Minimal;
        let mut seen = vec![level];
        for _ in 0..LEVELS.len() {
            level = level.next();
            seen.push(level);
        }
        assert_eq!(seen.first(), seen.last(), "the ladder should wrap");
        assert_eq!(seen.len(), LEVELS.len() + 1);
    }

    #[test]
    fn a_hand_edited_impossible_multiplier_is_pulled_into_range() {
        let mut ladder = Ladder {
            minimal: 99.0,
            max: -1.0,
            ..Ladder::default()
        };
        ladder.clamp_all();
        assert_eq!(ladder.get(Effort::Minimal), 3.0);
        assert_eq!(ladder.get(Effort::Max), 0.5);
    }

    #[test]
    fn multipliers_are_written_the_way_a_reader_says_them() {
        assert_eq!(times(1.0), "1×");
        assert_eq!(times(1.25), "1.25×");
        assert_eq!(times(0.7), "0.7×");
    }
}
