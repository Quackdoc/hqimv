//! The binding table and the incremental matcher for multi-key sequences.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::actions::Action;
use super::parser::{Chord, KeySequence, ParseError};

#[derive(Debug, Clone)]
struct Entry {
    sequence: KeySequence,
    actions: Vec<Action>,
}

#[derive(Debug, Clone)]
pub struct BindingTable {
    by_first: HashMap<Chord, Vec<Entry>>,
    timeout: Duration,
    pending: Vec<Chord>,
    pending_since: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Match {
    Pending,
    Actions(Vec<Action>),
    None,
}

/// How long a key sequence waits for its next key, unless configured.
pub const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(750);

impl Default for BindingTable {
    fn default() -> Self {
        Self {
            by_first: HashMap::new(),
            timeout: SEQUENCE_TIMEOUT,
            pending: Vec::new(),
            pending_since: None,
        }
    }
}

impl BindingTable {
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Inserts, replacing any identical sequence.
    pub fn insert(&mut self, sequence: KeySequence, actions: Vec<Action>) {
        let entry = Entry { sequence, actions };
        let bucket = self.by_first.entry(entry.sequence.chords[0]).or_default();
        // The shorter of two sequences sharing a start fires first, so the
        // longer one can never be reached.
        for other in bucket.iter().filter(|e| e.sequence != entry.sequence) {
            let (short, long) = if entry.sequence.len() <= other.sequence.len() {
                (&entry.sequence, &other.sequence)
            } else {
                (&other.sequence, &entry.sequence)
            };
            if short.len() < long.len() && long.chords.starts_with(&short.chords) {
                tracing::warn!(
                    "binding `{}` shadows `{}`: it fires first, so the longer sequence is unreachable",
                    short.describe(),
                    long.describe()
                );
            }
        }
        match bucket.iter_mut().find(|e| e.sequence == entry.sequence) {
            Some(existing) => *existing = entry,
            None => bucket.push(entry),
        }
        self.clear_pending();
    }

    pub fn insert_spec(&mut self, spec: &str) -> Result<(), ParseError> {
        let parsed = super::parser::parse(spec)?;
        self.insert(parsed.sequence, parsed.actions);
        Ok(())
    }

    pub fn press(&mut self, chord: Chord) -> Match {
        self.expire();

        self.pending.push(chord);
        // The timeout is per key: each press restarts it.
        self.pending_since = Some(Instant::now());

        // Bucketed by the first chord of the sequence, not the one just pressed.
        let first = self.pending[0];
        let Some(bucket) = self.by_first.get(&first) else {
            let pending = std::mem::take(&mut self.pending);
            self.pending_since = None;
            tracing::trace!("no binding starts with {:?}", pending.first());
            return Match::None;
        };

        for entry in bucket {
            if entry.sequence.chords == self.pending {
                let actions = entry.actions.clone();
                self.clear_pending();
                return Match::Actions(actions);
            }
        }

        let extends = bucket.iter().any(|e| {
            e.sequence.len() > self.pending.len() && e.sequence.chords.starts_with(&self.pending)
        });
        if extends {
            Match::Pending
        } else {
            // Drop dead end so a fumbled sequence can be retried.
            self.clear_pending();
            Match::None
        }
    }

    /// Drops a half-typed sequence once its timeout has passed.
    pub fn expire(&mut self) {
        if self.timed_out() {
            self.clear_pending();
        }
    }

    fn timed_out(&self) -> bool {
        self.pending_since
            .is_some_and(|since| since.elapsed() > self.timeout)
    }

    fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_since = None;
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}
