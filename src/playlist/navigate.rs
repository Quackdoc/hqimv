//! Moving through the playlist: stepping, jumping, and what happens at the ends.

use std::collections::HashMap;
use std::path::Path;

use super::{Playlist, PlaylistEntry};
use crate::names::Named;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndAction {
    #[default]
    Loop,
    Stop,
    Quit,
}

impl Named for EndAction {
    const NAMES: &'static [(Self, &'static [&'static str])] = &[
        (Self::Loop, &["loop", "cycle", "wrap"]),
        (Self::Stop, &["stop", "clamp"]),
        (Self::Quit, &["quit", "exit"]),
    ];
}

/// What a relative move did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Moved,
    /// Nowhere to go, or already there.
    Stay,
    /// Stepped past an end with `EndAction::Quit`.
    Quit,
}

/// Where `delta` steps from index `from` land in a list of `len`, or `None` to quit.
fn land(from: isize, delta: isize, len: usize, end: EndAction) -> Option<usize> {
    let len = len as isize;
    let target = from.saturating_add(delta);
    match end {
        EndAction::Loop => Some(target.rem_euclid(len) as usize),
        EndAction::Stop => Some(target.clamp(0, len - 1) as usize),
        EndAction::Quit if (0..len).contains(&target) => Some(target as usize),
        EndAction::Quit => None,
    }
}

impl Playlist {
    pub fn jump_to_path(&mut self, path: &Path) -> bool {
        self.jump_to_entry(|e| e.path == path)
    }

    pub fn jump_to_name(&mut self, name: &str) -> bool {
        self.jump_to_entry(|e| e.display_name() == name)
    }

    fn jump_to_entry(&mut self, pred: impl FnMut(&PlaylistEntry) -> bool) -> bool {
        let found = self.entries.iter().position(pred);
        found.is_some_and(|i| self.jump_to(i))
    }

    /// Steps `delta` entries, honouring what happens past either end.
    pub fn step_relative(&mut self, delta: isize, end: EndAction) -> Step {
        if delta == 0 || self.entries.is_empty() {
            return Step::Stay;
        }
        match land(self.current as isize, delta, self.entries.len(), end) {
            Some(next) => self.moved(next),
            None => Step::Quit,
        }
    }

    /// Steps `delta` entries through the marked ones only. From an unmarked
    /// entry it goes to the first marked one, or the last when going backwards.
    pub fn step_marked(&mut self, delta: isize, end: EndAction) -> Step {
        let marked: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.marked)
            .map(|(pos, _)| pos)
            .collect();
        if marked.is_empty() || delta == 0 {
            return Step::Stay;
        }
        let next = match marked.iter().position(|&p| p == self.current) {
            Some(i) => match land(i as isize, delta, marked.len(), end) {
                Some(next) => marked[next],
                None => return Step::Quit,
            },
            None if delta > 0 => marked[0],
            None => marked[marked.len() - 1],
        };
        self.moved(next)
    }

    /// Steps `delta` directories, landing on the first entry of one. Directories
    /// are taken in the order they first appear, so this holds for any sort.
    pub fn step_directory(&mut self, delta: isize, end: EndAction) -> Step {
        if self.entries.is_empty() || delta == 0 {
            return Step::Stay;
        }
        let mut index_of: HashMap<&Path, usize> = HashMap::new();
        let mut first: Vec<usize> = Vec::new();
        for (pos, entry) in self.entries.iter().enumerate() {
            let dir = entry.path.parent().unwrap_or(Path::new(""));
            index_of.entry(dir).or_insert_with(|| {
                first.push(pos);
                first.len() - 1
            });
        }
        let here = self
            .current_path()
            .and_then(Path::parent)
            .and_then(|dir| index_of.get(dir).copied())
            .unwrap_or(0);
        match land(here as isize, delta, first.len(), end) {
            Some(next) => {
                let target = first[next];
                self.moved(target)
            }
            None => Step::Quit,
        }
    }

    fn moved(&mut self, pos: usize) -> Step {
        if self.jump_to(pos) {
            Step::Moved
        } else {
            Step::Stay
        }
    }

    pub fn first(&mut self) -> bool {
        self.jump_to(0)
    }

    pub fn last(&mut self) -> bool {
        self.jump_to(usize::MAX)
    }

    /// `pos` is clamped.
    pub fn jump_to(&mut self, pos: usize) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        let next = pos.min(self.entries.len() - 1);
        if next == self.current {
            return false;
        }
        self.current = next;
        true
    }

    pub fn random(&mut self) -> bool {
        if self.entries.len() < 2 {
            return false;
        }
        // Over the other entries, so it always goes somewhere.
        let mut next = rand::random_range(0..self.entries.len() - 1);
        if next >= self.current {
            next += 1;
        }
        self.jump_to(next)
    }
}
