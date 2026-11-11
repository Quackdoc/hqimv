//! Enums chosen by name, on the command line and in the config file.

/// A value picked from a fixed set by name. The table is the one place that
/// says which names each value answers to, so parsing, the "expected one of"
/// warnings and displaying the value cannot disagree.
pub trait Named: Copy + PartialEq + 'static {
    /// Each value with its names, case-insensitive. The first is canonical.
    const NAMES: &'static [(Self, &'static [&'static str])];

    fn parse(s: &str) -> Option<Self> {
        let s = s.to_ascii_lowercase();
        Self::NAMES
            .iter()
            .find(|(_, names)| names.contains(&s.as_str()))
            .map(|&(value, _)| value)
    }

    /// The canonical name.
    fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(value, _)| *value == self)
            .map_or("", |(_, names)| names[0])
    }

    /// For warnings: `one of: fit, 1:1, fill`.
    fn expected() -> String {
        let names: Vec<_> = Self::NAMES.iter().map(|(_, names)| names[0]).collect();
        format!("one of: {}", names.join(", "))
    }
}
