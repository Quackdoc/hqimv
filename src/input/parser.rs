//! Parsing `--bind-key` specs into key sequences and actions.

use winit::keyboard::{KeyCode, ModifiersState as Modifiers};

use super::actions::{Action, ActionError};
use crate::playlist::is_content_line;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

impl Chord {
    pub fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct KeySequence {
    pub chords: Vec<Chord>,
}

impl KeySequence {
    pub fn len(&self) -> usize {
        self.chords.len()
    }

    /// The keys as they would be typed in a binding, for messages.
    pub fn describe(&self) -> String {
        self.chords
            .iter()
            .map(|c| {
                let mut parts = Vec::new();
                for (flag, name) in [
                    (Modifiers::CONTROL, "Ctrl"),
                    (Modifiers::ALT, "Alt"),
                    (Modifiers::SHIFT, "Shift"),
                    (Modifiers::SUPER, "Super"),
                ] {
                    if c.modifiers.contains(flag) {
                        parts.push(name.to_string());
                    }
                }
                parts.push(format!("{:?}", c.key));
                format!("<{}>", parts.join("-"))
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedBinding {
    pub sequence: KeySequence,
    pub actions: Vec<Action>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("binding `{spec}` has no action block; write it as `<Key> {{ action }}`")]
    MissingActions { spec: String },
    #[error("binding `{spec}` has text after the closing `}}`")]
    TrailingText { spec: String },
    #[error("unknown key `{0}`")]
    UnknownKey(String),
    #[error("`{0}` is only modifiers; add a key")]
    ModifiersOnly(String),
    #[error(transparent)]
    Action(#[from] ActionError),
}

pub fn parse(spec: &str) -> Result<ParsedBinding, ParseError> {
    let spec = spec.trim();
    let missing = || ParseError::MissingActions {
        spec: spec.to_string(),
    };
    let open = spec.find('{').ok_or_else(missing)?;
    let close = spec.rfind('}').filter(|&c| c > open).ok_or_else(missing)?;

    if !spec[close + 1..].trim().is_empty() {
        return Err(ParseError::TrailingText {
            spec: spec.to_string(),
        });
    }

    let keys = spec[..open].trim();
    let body = &spec[open + 1..close];

    let sequence = parse_sequence(keys)?;
    let actions = parse_actions(body)?;

    if actions.is_empty() {
        return Err(missing());
    }

    Ok(ParsedBinding { sequence, actions })
}

fn parse_sequence(keys: &str) -> Result<KeySequence, ParseError> {
    let mut chords = Vec::new();
    for step in keys.split_whitespace() {
        chords.extend(parse_chord(step)?);
    }
    if chords.is_empty() {
        return Err(ParseError::ModifiersOnly(keys.to_string()));
    }
    Ok(KeySequence { chords })
}

fn parse_chord(step: &str) -> Result<Vec<Chord>, ParseError> {
    let mut out: Vec<Chord> = Vec::new();
    let mut rest = step;

    while let Some(open) = rest.find('<') {
        // `a<Right>` is the keys `a` then `<Right>`, not just the latter.
        out.extend(chars_to_chords(&rest[..open])?);
        let Some(close) = rest[open..].find('>') else {
            return Err(ParseError::UnknownKey(rest.to_string()));
        };
        let inner = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];

        let mut modifiers = Modifiers::empty();
        let mut key: Option<KeyCode> = None;
        for part in inner.split(['-', '+']) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some(m) = parse_modifier(part) {
                modifiers |= m;
                continue;
            }
            // `plus` names the shifted `=`; the key it maps to is unshifted.
            if part.eq_ignore_ascii_case("plus") {
                modifiers |= Modifiers::SHIFT;
            }
            key = Some(parse_key(part)?);
        }

        match key {
            Some(k) => out.push(Chord::new(k, modifiers)),
            None => return Err(ParseError::ModifiersOnly(inner.to_string())),
        }
    }

    out.extend(chars_to_chords(rest)?);
    Ok(out)
}

/// Plain characters, one chord each.
fn chars_to_chords(text: &str) -> Result<Vec<Chord>, ParseError> {
    text.chars()
        .map(|c| {
            // An uppercase character in a spec means the shifted key.
            let modifiers = if c.is_ascii_uppercase() {
                Modifiers::SHIFT
            } else {
                Modifiers::empty()
            };
            Ok(Chord::new(key_from_char(c)?, modifiers))
        })
        .collect()
}

fn parse_modifier(name: &str) -> Option<Modifiers> {
    let lower = name.to_ascii_lowercase();
    Some(match lower.as_str() {
        "ctrl" | "control" => Modifiers::CONTROL,
        "alt" | "meta" | "option" => Modifiers::ALT,
        "shift" => Modifiers::SHIFT,
        "super" | "win" | "cmd" | "command" | "logo" => Modifiers::SUPER,
        _ => return None,
    })
}

/// `F1`..`F24`; the caller bounds-checks `n`.
fn function_key(n: u8) -> KeyCode {
    use KeyCode::*;
    const F_KEYS: [KeyCode; 24] = [
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20,
        F21, F22, F23, F24,
    ];
    F_KEYS[n as usize - 1]
}

fn parse_key(name: &str) -> Result<KeyCode, ParseError> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return key_from_char(c);
    }

    // Whitespace and `-` split a chord into steps/parts before this, so a key
    // name only ever needs `_` folded out.
    let canonical = name.to_ascii_lowercase().replace('_', "");
    let code = match canonical.as_str() {
        "left" | "arrowleft" => KeyCode::ArrowLeft,
        "right" | "arrowright" => KeyCode::ArrowRight,
        "up" | "arrowup" => KeyCode::ArrowUp,
        "down" | "arrowdown" => KeyCode::ArrowDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "prior" => KeyCode::PageUp,
        "pagedown" | "next" => KeyCode::PageDown,
        "insert" | "ins" => KeyCode::Insert,
        "delete" | "del" => KeyCode::Delete,
        "backspace" => KeyCode::Backspace,
        "tab" => KeyCode::Tab,
        "return" | "enter" => KeyCode::Enter,
        "escape" | "esc" => KeyCode::Escape,
        "space" | "spacebar" => KeyCode::Space,
        "plus" | "equal" => KeyCode::Equal,
        "minus" | "hyphen" => KeyCode::Minus,
        "comma" => KeyCode::Comma,
        "period" | "dot" => KeyCode::Period,
        "slash" => KeyCode::Slash,
        "backslash" => KeyCode::Backslash,
        "semicolon" => KeyCode::Semicolon,
        "apostrophe" | "quote" => KeyCode::Quote,
        "backtick" | "grave" => KeyCode::Backquote,
        "bracketleft" => KeyCode::BracketLeft,
        "bracketright" => KeyCode::BracketRight,
        "capslock" => KeyCode::CapsLock,
        "printscreen" => KeyCode::PrintScreen,
        "scrolllock" => KeyCode::ScrollLock,
        "pause" => KeyCode::Pause,
        "numlock" => KeyCode::NumLock,
        "menu" | "contextmenu" => KeyCode::ContextMenu,
        "kpleft" => KeyCode::ArrowLeft,
        "kpright" => KeyCode::ArrowRight,
        "kpup" => KeyCode::ArrowUp,
        "kpdown" => KeyCode::ArrowDown,
        "browserhome" | "xf86homepage" => KeyCode::BrowserHome,
        other => {
            let n = other
                .strip_prefix('f')
                .and_then(|d| d.parse::<u8>().ok())
                .filter(|n| (1..=24).contains(n))
                .ok_or_else(|| ParseError::UnknownKey(name.to_string()))?;
            return Ok(function_key(n));
        }
    };
    Ok(code)
}

/// Shared char->key mapping used by the key handler.
pub fn key_from_char(c: char) -> Result<KeyCode, ParseError> {
    use KeyCode::*;
    const LETTERS: [KeyCode; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO,
        KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    ];

    let lower = c.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        return Ok(LETTERS[(lower as u8 - b'a') as usize]);
    }
    if let Some(d) = lower.to_digit(10) {
        return Ok(DIGITS[d as usize]);
    }
    Ok(match lower {
        ' ' => Space,
        '-' => Minus,
        '=' => Equal,
        '[' => BracketLeft,
        ']' => BracketRight,
        '\\' => Backslash,
        ';' => Semicolon,
        '\'' => Quote,
        '`' => Backquote,
        ',' => Comma,
        '.' => Period,
        '/' => Slash,
        other => return Err(ParseError::UnknownKey(other.to_string())),
    })
}

fn parse_actions(body: &str) -> Result<Vec<Action>, ParseError> {
    let mut actions = Vec::new();
    for part in split_top_level(body, [';', '\n']) {
        let Some(part) = is_content_line(part) else {
            continue;
        };
        actions.push(Action::parse(part)?);
    }
    Ok(actions)
}

/// Splits on top-level separators, outside brackets and quotes.
///
/// A quote only opens at the start of a word, so the apostrophe in `it's` is
/// text. One that opens and never closes is retried as text too, instead of
/// swallowing every separator after it.
fn split_top_level(text: &str, separators: [char; 2]) -> Vec<&str> {
    let mut literal: Vec<usize> = Vec::new();
    loop {
        match try_split(text, separators, &literal) {
            Ok(parts) => return parts,
            Err(unclosed) => literal.push(unclosed),
        }
    }
}

/// `Err` carries the offset of a quote that never closed.
fn try_split<'a>(
    text: &'a str,
    separators: [char; 2],
    literal: &[usize],
) -> Result<Vec<&'a str>, usize> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<(char, usize)> = None;
    let mut start = 0usize;
    let mut prev: Option<char> = None;

    let mut it = text.char_indices();
    while let Some((i, c)) = it.next() {
        match quote {
            Some((q, _)) => {
                if c == q {
                    quote = None;
                }
                if c == '\\' {
                    it.next();
                }
            }
            None => match c {
                '\'' | '"'
                    if !literal.contains(&i) && !prev.is_some_and(|p| p.is_alphanumeric()) =>
                {
                    quote = Some((c, i));
                }
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                _ if depth == 0 && (c == separators[0] || c == separators[1]) => {
                    out.push(&text[start..i]);
                    start = i + c.len_utf8();
                }
                _ => {}
            },
        }
        prev = Some(c);
    }
    if let Some((_, at)) = quote {
        return Err(at);
    }
    out.push(&text[start..]);
    Ok(out)
}
