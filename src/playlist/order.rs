//! Sort keys.

use std::cmp::Ordering;
use std::path::Path;

use rand::seq::SliceRandom;

use super::PlaylistEntry;
use crate::names::Named;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// File name, case-insensitive, digit runs by value (`img2` before
    /// `img10`); the default.
    #[default]
    Name,
    /// File name, case-insensitive, strictly character by character.
    Alpha,
    /// Whole path, one directory level at a time, so a directory's files stay
    /// together.
    Path,
    /// Oldest first.
    Mtime,
    /// Smallest first.
    Size,
    /// File extension, then name.
    Extension,
    /// Shuffled once, when the playlist is built.
    Random,
    /// The order files were found in.
    None,
}

impl Named for SortKey {
    const NAMES: &'static [(Self, &'static [&'static str])] = &[
        (Self::Name, &["name", "filename", "natural"]),
        (Self::Alpha, &["alpha", "lexical", "ascii"]),
        (Self::Path, &["path"]),
        (Self::Mtime, &["mtime", "date", "time"]),
        (Self::Size, &["size"]),
        (Self::Extension, &["extension", "ext", "type"]),
        (Self::Random, &["random", "shuffle"]),
        (Self::None, &["none", "unsorted", "scan"]),
    ];
}

impl SortKey {
    /// Sorts `entries` in place by this key, newest or largest first when
    /// `reverse`. Ties always fall back to name and then path, so the order is
    /// the same on every run. Keys are built once per entry, not per comparison.
    pub fn sort(self, entries: &mut [PlaylistEntry], reverse: bool) {
        // Ties fall back to natural name, then path.
        let tie = |e: &PlaylistEntry| (Natural(e.sort_name()), e.path.clone());
        match self {
            Self::None => {}
            Self::Random => {
                entries.shuffle(&mut rand::rng());
                // A reversed shuffle is still a shuffle.
                return;
            }
            Self::Name => entries.sort_by_cached_key(tie),
            Self::Alpha => entries.sort_by_cached_key(|e| (e.sort_name(), e.path.clone())),
            Self::Path => entries.sort_by_cached_key(|e| path_key(&e.path)),
            Self::Mtime => entries.sort_by_cached_key(|e| (e.mtime, tie(e))),
            Self::Size => entries.sort_by_cached_key(|e| (e.size, tie(e))),
            Self::Extension => entries.sort_by_cached_key(|e| {
                let ext = e.path.extension();
                (ext.map(|x| x.to_string_lossy().to_lowercase()), tie(e))
            }),
        }
        if reverse {
            entries.reverse();
        }
    }
}

/// A string that orders naturally; wrapped so it can be a sort key.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Natural(String);

impl Ord for Natural {
    fn cmp(&self, other: &Self) -> Ordering {
        natural_cmp(&self.0, &other.0)
    }
}

impl PartialOrd for Natural {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Each directory level compared naturally, then the shorter path first.
fn path_key(path: &Path) -> Vec<Natural> {
    path.components()
        .map(|c| Natural(c.as_os_str().to_string_lossy().to_lowercase()))
        .collect()
}

/// Compares byte by byte, except that a run of digits is compared as a number.
/// Equal numbers with different zero padding order the shorter run first, so
/// the result is a total order.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    let mut padding = Ordering::Equal;
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let run_a = digits(&a[i..]);
            let run_b = digits(&b[j..]);
            let (num_a, num_b) = (strip_zeros(run_a), strip_zeros(run_b));
            // More digits after the leading zeros is a bigger number.
            let by_value = num_a.len().cmp(&num_b.len()).then_with(|| num_a.cmp(num_b));
            if by_value != Ordering::Equal {
                return by_value;
            }
            if padding == Ordering::Equal {
                padding = run_a.len().cmp(&run_b.len());
            }
            i += run_a.len();
            j += run_b.len();
        } else {
            match a[i].cmp(&b[j]) {
                Ordering::Equal => {
                    i += 1;
                    j += 1;
                }
                other => return other,
            }
        }
    }
    (a.len() - i).cmp(&(b.len() - j)).then(padding)
}

/// The leading run of ASCII digits.
fn digits(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|b| b.is_ascii_digit()).count();
    &s[..n]
}

fn strip_zeros(run: &[u8]) -> &[u8] {
    let n = run.iter().take_while(|&&b| b == b'0').count();
    &run[n..]
}
