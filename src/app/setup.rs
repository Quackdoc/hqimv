//! Launch state, fixed after startup: the raw flags and the resolved settings.

use crate::cli::Cli;
use crate::settings::Settings;

pub struct Setup {
    pub cli: Cli,
    pub settings: Settings,
}
