//! hqimv: a high-quality CPU image viewer for Wayland.
//!
//! The crate is organised so each stage of the pipeline owns exactly one concern:
//!
//! - [`cli`] and [`config`] turn arguments and files into [`settings`].
//! - [`names`] is how enums such as the scale mode are chosen by name.
//! - [`playlist`] owns which image is current; it never touches pixels.
//! - [`image`] decodes into linear `f32` and resamples on the CPU.
//! - [`input`] parses `--bind-key` specs and matches them against key presses.
//! - [`present`] encodes a linear image into the best `wl_shm` format available.
//! - [`app`] wires the above together and runs the event loop.

mod app;
mod cli;
mod config;
mod image;
mod input;
mod names;
mod playlist;
mod present;
mod settings;

use std::io::IsTerminal;

use anyhow::{Context, Result};
use tracing_subscriber::{EnvFilter, fmt};

use crate::cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.log.as_deref());

    jxl_image_rs_integration::register_image_decoding_hook();

    let config = load_config(&cli)?;

    let event_loop = winit::event_loop::EventLoop::new()
        .context("cannot create the event loop; is WAYLAND_DISPLAY set?")?;
    let proxy = event_loop.create_proxy();
    let mut app = app::App::new(cli, config, proxy).context("cannot start the viewer")?;
    event_loop.run_app(&mut app).context("event loop failed")?;
    if app.failed() {
        // Already reported; only the exit status is left to set.
        drop(app);
        std::process::exit(1);
    }
    Ok(())
}

fn load_config(cli: &Cli) -> Result<config::Config> {
    if cli.config.is_none()
        && let Err(err) = config::ensure_default_config()
    {
        tracing::warn!("{err}");
    }
    let Some(path) = cli.config.clone().or_else(config::config_path) else {
        return Ok(config::Config::default());
    };
    if cli.config.is_some() && !path.exists() {
        tracing::warn!(
            "--config: {} does not exist; using defaults",
            path.display()
        );
    }
    config::Config::load(&path).with_context(|| format!("cannot load {}", path.display()))
}

/// Events go to stderr, so stdout stays for data (`output_file_list`, command
/// output). The level is `RUST_LOG` if set, else `--log`, else `warn`.
fn init_tracing(level: Option<&str>) {
    let level = level.map_or(tracing::Level::WARN, |raw| {
        raw.parse().unwrap_or_else(|_| {
            eprintln!(
                "--log: `{raw}` is not a level (error, warn, info, debug, trace); using warn"
            );
            tracing::Level::WARN
        })
    });
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    // Colour only on a terminal, and never when NO_COLOR is set.
    let ansi = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();

    let _ = fmt()
        .with_env_filter(filter)
        .with_ansi(ansi)
        .with_target(false)
        .without_time()
        .with_writer(std::io::stderr)
        .try_init();
}
