//! Pure text/time calculations. No IO, native media, Tauri or frontend state.
use super::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rate {
    Finite { value: f64 },
    Unavailable { reason: Unavailable },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Unavailable {
    EmptyText,
    InvalidDuration,
    ReadingAllowance,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    TooSlow,
    SlowAcceptable,
    ABitSlow,
    GoodSlow,
    Perfect,
    GoodFast,
    ABitFast,
    FastAcceptable,
    TooFast,
}

impl Rating {
    pub const ALL: [Self; 9] = [
        Self::TooSlow,
        Self::SlowAcceptable,
        Self::ABitSlow,
        Self::GoodSlow,
        Self::Perfect,
        Self::GoodFast,
        Self::ABitFast,
        Self::FastAcceptable,
        Self::TooFast,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::TooSlow => "TOO SLOW",
            Self::SlowAcceptable => "Slow, acceptable",
            Self::ABitSlow => "A bit slow",
            Self::GoodSlow => "Good (slow side)",
            Self::Perfect => "Perfect",
            Self::GoodFast => "Good (fast side)",
            Self::ABitFast => "A bit fast",
            Self::FastAcceptable => "Fast, acceptable",
            Self::TooFast => "TOO FAST",
        }
    }
}
impl Rate {
    pub fn value(&self) -> Option<f64> {
        match self {
            Self::Finite { value } => Some(*value),
            Self::Unavailable { .. } => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextMetrics {
    pub count: u32,
    pub lines: Vec<u32>,
    pub empty: bool,
}

/// Independently implemented from the pinned host behavior described in the
/// contract. A retained newline counts as CRLF for speed and zero within a line.
/// Cancellation is checked at least every 4096 input bytes, including inside tags.
pub fn text(text: &str, mut checkpoint: impl FnMut() -> Result<()>) -> Result<TextMetrics> {
    if text.len() > super::MAX_TEXT_BYTES {
        return Err(super::Error::Limit("Quality text exceeds 16 MiB".into()));
    }
    let bytes = text.as_bytes();
    let mut stripped = String::with_capacity(text.len());
    let mut at = 0;
    let mut checked = 0;
    while at < bytes.len() {
        if at >= checked {
            checkpoint()?;
            checked = at.saturating_add(4096);
        }
        let closer = if bytes[at] == b'<' {
            Some(b'>')
        } else if bytes[at..].starts_with(b"{\\") && text[at..].encode_utf16().take(4).count() == 4
        {
            Some(b'}')
        } else {
            None
        };
        if let Some(closer) = closer {
            let mut end = at + 1;
            while end < bytes.len() && bytes[end] != closer {
                if end >= checked {
                    checkpoint()?;
                    checked = end.saturating_add(4096);
                }
                end += 1;
            }
            if end < bytes.len() {
                at = end + 1;
                continue;
            }
            // The legacy host preserves the entire tail after an unmatched opener.
            stripped.push_str(&text[at..]);
            break;
        }
        // Walk UTF-8 boundaries; ASCII tag delimiters cannot split a code point.
        let c = text[at..].chars().next().expect("at is inside UTF-8 input");
        stripped.push(c);
        at += c.len_utf8();
    }
    let mut count = 0u32;
    let mut lines = vec![0u32];
    let mut empty = true;
    let mut chars = stripped.char_indices().peekable();
    checked = 0;
    while let Some((at, c)) = chars.next() {
        if at >= checked {
            checkpoint()?;
            checked = at.saturating_add(4096);
        }
        if c == '\r' || c == '\n' {
            if c == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                chars.next();
            }
            count += 2;
            lines.push(0);
        } else {
            let units = c.len_utf16() as u32;
            count += units;
            *lines.last_mut().expect("one initial line") += units;
            empty &= c.is_whitespace();
        }
    }
    Ok(TextMetrics {
        count,
        lines,
        empty,
    })
}

pub fn rates(count: u32, duration_ms: u64, empty: bool) -> (Rate, Rate, Option<Rating>) {
    if empty {
        return (
            Rate::Unavailable {
                reason: Unavailable::EmptyText,
            },
            Rate::Unavailable {
                reason: Unavailable::EmptyText,
            },
            None,
        );
    }
    if duration_ms == 0 {
        return (
            Rate::Unavailable {
                reason: Unavailable::InvalidDuration,
            },
            Rate::Unavailable {
                reason: Unavailable::InvalidDuration,
            },
            None,
        );
    }
    let numerator = u64::from(count) * 1000;
    let cps = Rate::Finite {
        value: numerator as f64 / duration_ms as f64,
    };
    if duration_ms <= 500 {
        return (
            cps,
            Rate::Unavailable {
                reason: Unavailable::ReadingAllowance,
            },
            None,
        );
    }
    let denominator = duration_ms - 500;
    let rating = [
        (5, Rating::TooSlow),
        (10, Rating::SlowAcceptable),
        (13, Rating::ABitSlow),
        (15, Rating::GoodSlow),
        (23, Rating::Perfect),
        (27, Rating::GoodFast),
        (31, Rating::ABitFast),
        (35, Rating::FastAcceptable),
    ]
    .into_iter()
    .find(|(limit, _)| u128::from(numerator) < (*limit as u128) * u128::from(denominator))
    .map_or(Rating::TooFast, |(_, r)| r);
    (
        cps,
        Rate::Finite {
            value: numerator as f64 / denominator as f64,
        },
        Some(rating),
    )
}

/// ceil(N*1000/34.95 + 500), using exact integer arithmetic (34.95 = 699/20).
pub fn reading_duration(count: u32) -> u64 {
    (u64::from(count) * 20_000).div_ceil(699) + 500
}

/// Prior chronological cue data after any accepted proposal-local timing changes.
#[derive(Clone, Copy)]
pub(super) struct ReadingPrior {
    pub start: u64,
    pub end: u64,
    pub count: u32,
}

/// Reading start before clamping to the current end. The unclamped floor lets
/// endpoint repairs find a finite reading interval even when the old end is too
/// early. The checker clamps it to the actual end when reporting unavailable rates.
pub(super) fn reading_floor(
    basis: super::profile::SpeedBasis,
    start: u64,
    prior: Option<ReadingPrior>,
) -> u64 {
    if basis == super::profile::SpeedBasis::VssOverlapV1
        && let Some(prior) = prior
        && prior.end > start
    {
        (prior.start + reading_duration(prior.count))
            .min(prior.end)
            .max(start)
    } else {
        start
    }
}
