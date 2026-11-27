use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Equal,
    Removed,
    Added,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TokenChange {
    pub kind: TokenKind,
    pub text: String,
}
// Test the script property, rather than broad blocks which include other scripts.
fn unspaced(c: char) -> bool {
    static SCRIPT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"[\p{scx:Han}\p{scx:Hiragana}\p{scx:Katakana}\p{Thai}\p{Lao}\p{Khmer}\p{Myanmar}]",
        )
        .expect("valid Unicode script properties")
    });
    !c.is_ascii() && SCRIPT.is_match(c.encode_utf8(&mut [0; 4]))
}

fn base_with_marks(text: &str, first: char) -> usize {
    first.len_utf8()
        + text[first.len_utf8()..]
            .chars()
            .take_while(|&c| unicode_normalization::char::is_combining_mark(c))
            .map(char::len_utf8)
            .sum::<usize>()
}

pub(super) fn reading_tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split_whitespace().flat_map(|mut word| {
        std::iter::from_fn(move || {
            let first = word.chars().next()?;
            let end = if unspaced(first) {
                base_with_marks(word, first)
            } else {
                word.find(unspaced).unwrap_or(word.len())
            };
            let (token, rest) = word.split_at(end);
            word = rest;
            Some(token)
        })
    })
}
fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let tail = &text[pos..];
        let first = tail.chars().next().unwrap();
        let end = match first {
            '<' => tail.find('>').map(|i| i + 1),
            '{' => tail.find('}').map(|i| i + 1),
            _ => None,
        }
        .unwrap_or_else(|| {
            if unspaced(first) {
                return base_with_marks(tail, first);
            }
            if first == '\n' || (!first.is_alphanumeric() && !first.is_whitespace()) {
                return first.len_utf8();
            }
            tail.char_indices()
                .skip(1)
                .find(|(_, c)| {
                    *c == '\n'
                        || unspaced(*c)
                        || c.is_alphanumeric() != first.is_alphanumeric()
                        || c.is_whitespace() != first.is_whitespace()
                })
                .map_or(tail.len(), |(i, _)| i)
        });
        out.push(&text[pos..pos + end]);
        pos += end;
    }
    out
}
/// Myers shortest edit script over words, spacing, punctuation, breaks and tags.
/// A 512-edit search cap bounds pathological cue memory; beyond it both complete
/// texts are retained as removed/added blocks, never silently truncated.
pub fn word_diff(a: &str, b: &str) -> Vec<TokenChange> {
    if a == b {
        return vec![TokenChange {
            kind: TokenKind::Equal,
            text: a.into(),
        }];
    }
    let (a, b) = (tokens(a), tokens(b));
    let limit = (a.len() + b.len()).min(512);
    let offset = limit as isize + 1;
    let mut v = vec![0isize; 2 * limit + 3];
    let mut trace = Vec::new();
    for d in 0..=limit {
        for k in (-(d as isize)..=d as isize).step_by(2) {
            let idx = (offset + k) as usize;
            let mut x = if k == -(d as isize) || (k != d as isize && v[idx - 1] < v[idx + 1]) {
                v[idx + 1]
            } else {
                v[idx - 1] + 1
            };
            let mut y = x - k;
            while x < a.len() as isize && y < b.len() as isize && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx] = x;
            if x >= a.len() as isize && y >= b.len() as isize {
                let mut result = Vec::new();
                let (mut x, mut y) = (a.len() as isize, b.len() as isize);
                for depth in (1..=d).rev() {
                    let previous: &Vec<isize> = &trace[depth - 1];
                    let k = x - y;
                    let idx = (offset + k) as usize;
                    let pk = if k == -(depth as isize)
                        || (k != depth as isize && previous[idx - 1] < previous[idx + 1])
                    {
                        k + 1
                    } else {
                        k - 1
                    };
                    let px = previous[(offset + pk) as usize];
                    let py = px - pk;
                    while x > px && y > py {
                        x -= 1;
                        y -= 1;
                        result.push((TokenKind::Equal, a[x as usize]));
                    }
                    if x == px {
                        y -= 1;
                        result.push((TokenKind::Added, b[y as usize]));
                    } else {
                        x -= 1;
                        result.push((TokenKind::Removed, a[x as usize]));
                    }
                }
                while x > 0 && y > 0 {
                    x -= 1;
                    y -= 1;
                    result.push((TokenKind::Equal, a[x as usize]));
                }
                result.reverse();
                return coalesce(result);
            }
        }
        trace.push(v.clone());
    }
    coalesce(
        a.into_iter()
            .map(|s| (TokenKind::Removed, s))
            .chain(b.into_iter().map(|s| (TokenKind::Added, s)))
            .collect(),
    )
}
fn coalesce(parts: Vec<(TokenKind, &str)>) -> Vec<TokenChange> {
    let mut result: Vec<TokenChange> = Vec::new();
    for (kind, text) in parts {
        if let Some(last) = result.last_mut().filter(|last| last.kind == kind) {
            last.text.push_str(text);
        } else {
            result.push(TokenChange {
                kind,
                text: text.into(),
            });
        }
    }
    result
}
