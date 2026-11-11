//! Window events: key presses, scrolling, resizes and redraw requests.

use std::time::Instant;

use winit::event::ElementState;
use winit::event::{KeyEvent, MouseScrollDelta};
use winit::keyboard::{Key, KeyCode, PhysicalKey};

use super::App;
use crate::input::parser::key_from_char;
use crate::input::{BindingTable, Chord, Match};

/// Key bindings, and the modifier state winit reports separately.
pub struct Input {
    pub bindings: BindingTable,
    pub modifiers: winit::event::Modifiers,
}

impl App {
    pub fn on_key(&mut self, event: KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        if !self.host.screen.shown
            && let Some(wl) = self.host.screen.wl.as_ref()
        {
            wl.window.set_visible(true);
            self.host.screen.shown = true;
        }
        let Some(key) = pressed_key(event) else {
            return;
        };

        match self
            .host
            .input
            .bindings
            .press(Chord::new(key, self.host.input.modifiers.state()))
        {
            Match::Actions(actions) => {
                for action in &actions {
                    self.run_action(action);
                }
            }
            Match::Pending => {}
            Match::None => {
                tracing::debug!("no binding for {key:?}");
            }
        }
    }

    pub fn on_scroll(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => f64::from(y),
            MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
        };
        if lines == 0.0 {
            return;
        }
        let factor = if lines > 0.0 { 1.1f64 } else { 1.0 / 1.1 };
        self.session.view.zoom_by(factor);
        self.invalidate();
    }

    pub fn resize(&mut self) {
        let Some(wl) = self.host.screen.wl.as_mut() else {
            return;
        };
        wl.pending_resize = Some(super::render::window_px(&wl.window));
        self.invalidate();
    }

    pub fn redraw(&mut self) {
        // `pre_present_notify` fires inside `draw`, just before the commit.
        if !self.host.screen.dirty {
            return;
        }
        if self.draw() {
            self.host.screen.retry_at = Some(Instant::now() + self.setup.settings.present_retry);
            self.invalidate();
        }
    }
}

// The logical key's mapping, or its physical position when there is none.
fn pressed_key(event: KeyEvent) -> Option<KeyCode> {
    let KeyEvent {
        logical_key,
        physical_key,
        ..
    } = event;
    let logical = match logical_key {
        Key::Character(c) => c.chars().next().and_then(|c| key_from_char(c).ok()),
        Key::Named(named) => named_key(named),
        _ => None,
    };
    logical.or(match physical_key {
        PhysicalKey::Code(code) => Some(code),
        PhysicalKey::Unidentified(_) => None,
    })
}

fn named_key(named: winit::keyboard::NamedKey) -> Option<KeyCode> {
    use winit::keyboard::NamedKey as N;
    macro_rules! same {
        ($($key:ident),* $(,)?) => {
            match named { $(N::$key => KeyCode::$key,)* _ => return None }
        };
    }
    Some(same!(
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        ArrowDown,
        Home,
        End,
        PageUp,
        PageDown,
        Insert,
        Delete,
        Backspace,
        Tab,
        Enter,
        Escape,
        Space,
        ContextMenu,
        BrowserHome,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
    ))
}
