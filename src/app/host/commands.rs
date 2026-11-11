//! The command worker's channel and the results it sends back.

use std::sync::mpsc::{self, Receiver};

pub struct Commands {
    pub rx: Receiver<CommandResult>,
    pub tx: mpsc::Sender<CommandResult>,
}

/// What a finished `sh -c` run reported.
#[derive(Debug)]
pub struct CommandResult {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}
