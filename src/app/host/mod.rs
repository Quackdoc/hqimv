//! Everything that binds the app to the platform.

mod commands;
mod fs;
mod loader;
mod screen;

use super::input::Input;

pub use self::commands::{CommandResult, Commands};
pub use self::fs::Fs;
pub use self::loader::Loader;
pub use self::screen::{Presentation, Screen, Wl, flush};

pub struct Host {
    pub input: Input,
    pub loader: Loader,
    pub screen: Screen,
    pub fs: Fs,
    pub commands: Commands,
}
