//! The XDG config file: loading, the default file, and the key-binding table.
//!
//! Every key is optional, so a config file only needs the ones it changes. The
//! other settings are merged with the command-line flags in `settings`.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::image::LoadOptions;
use crate::input::{BindingTable, SEQUENCE_TIMEOUT, default_bindings, default_table};
use crate::playlist::is_content_line;

fn xdg_config_file(name: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("hqimv").join(name))
}

pub fn config_path() -> Option<PathBuf> {
    xdg_config_file("config.toml")
}

/// Bindings file, separate so shells can source it.
pub fn bindings_path() -> Option<PathBuf> {
    xdg_config_file("bindings")
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> ConfigError {
    move |source| ConfigError::Io {
        path: path.display().to_string(),
        source,
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// Key bindings, in `--bind-key` syntax.
    pub bindings: Vec<String>,
    /// Extra playlist args, appended to CLI ones.
    pub playlist: Vec<String>,
    /// Only load files with these extensions; empty means every supported format.
    pub extensions: Vec<String>,
    pub scan: ScanConfig,
    pub view: ViewConfig,
    pub limits: LimitConfig,
}

/// Decode budgets, with the built-in ones as defaults.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LimitConfig {
    /// Largest single image, in pixels.
    pub max_pixels: u64,
    /// Largest edge of a single image, in pixels; unset means no edge cap.
    pub max_dimension: Option<u32>,
    /// Largest number of frames in one animation.
    pub max_animation_frames: usize,
    /// Largest total area across an animation's frames, in pixels.
    pub max_animation_pixels: u64,
    /// Largest remote response body downloaded, in bytes.
    pub max_download_bytes: u64,
    /// Ceiling on the colour conversion's scratch memory, in bytes.
    pub cms_scratch_bytes: usize,
    /// Whole-request budget for a remote image, in ms.
    pub download_timeout: u64,
}

impl Default for LimitConfig {
    fn default() -> Self {
        let d = LoadOptions::default();
        Self {
            max_pixels: d.max_pixels,
            max_dimension: d.max_dimension,
            max_animation_frames: d.max_animation_frames,
            max_animation_pixels: d.max_animation_pixels,
            max_download_bytes: d.max_download_bytes,
            cms_scratch_bytes: d.cms_scratch,
            download_timeout: d.download_timeout.as_millis() as u64,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ScanConfig {
    pub recursive: bool,
    /// Sort key: name, alpha, path, mtime, size, extension, random, or none.
    pub sort: Option<String>,
    /// Reverse the sort order.
    pub reverse: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ViewConfig {
    /// Initial scale mode: fit, 1:1, or fill.
    pub scale_mode: String,
    /// Initial zoom multiplier on top of the scale mode.
    pub zoom: Option<f64>,
    /// Resampling kernel: lanczos3, mitchell, or bilinear.
    pub filter: String,
    /// Background colour as sRGB hex, with optional alpha.
    pub background: String,
    /// Window size as `WIDTHxHEIGHT`; unset fits the image.
    pub size: Option<String>,
    pub fullscreen: bool,
    /// Window decorations (title bar and borders).
    pub decorations: bool,
    /// Force a `wl_shm` format instead of negotiating one.
    pub format: Option<String>,
    pub status_line: bool,
    pub info_overlay: bool,
    /// Auto-advance delay in ms; 0 disables.
    pub play_delay: Option<u64>,
    /// What stepping past the playlist ends does: loop, stop, or quit.
    pub end_of_files_action: String,
    pub preload: usize,
    /// Key-sequence timeout in ms.
    pub sequence_timeout: u64,
    /// Floor for a frame an animation marks as instant, in ms.
    pub min_frame_delay: u64,
    /// How often the slideshow timer is checked, in ms.
    pub play_tick: u64,
    /// How often the loading spinner and animations advance, in ms.
    pub loading_tick: u64,
    /// How long a notice stays on screen, in ms.
    pub notice_ttl: u64,
    /// Delay before retrying a frame the compositor refused, in ms.
    pub present_retry: u64,
    /// Font size of the on-screen text, in pixels.
    pub overlay_size: f32,
    /// Gap between the on-screen text and the window edge, in pixels.
    pub overlay_margin: i32,
}

impl Default for ViewConfig {
    fn default() -> Self {
        Self {
            scale_mode: "fit".to_string(),
            zoom: None,
            filter: "lanczos3".to_string(),
            background: "#00000000".to_string(),
            status_line: true,
            info_overlay: false,
            play_delay: Some(0),
            preload: 0,
            size: None,
            fullscreen: false,
            decorations: true,
            format: None,
            end_of_files_action: "loop".to_string(),
            sequence_timeout: SEQUENCE_TIMEOUT.as_millis() as u64,
            min_frame_delay: LoadOptions::default().min_frame_delay.as_millis() as u64,
            play_tick: 50,
            loading_tick: 250,
            notice_ttl: 2000,
            present_retry: 4,
            overlay_size: 24.0,
            overlay_margin: 16,
        }
    }
}

impl Config {
    /// Missing file returns defaults; malformed file errors.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            tracing::debug!("no config at {}, using defaults", path.display());
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).map_err(io_err(path))?;
        let mut unknown: Vec<String> = Vec::new();
        let parse_err = |e: toml::de::Error| ConfigError::Parse {
            path: path.display().to_string(),
            message: e.to_string(),
        };
        let mut table: toml::Table = text.parse().map_err(parse_err)?;

        // A key of the wrong type is dropped with a warning rather than stopping
        // the viewer; only a file that is not TOML at all is fatal. Each key is
        // tried alone, since the deserializer reports just the first failure.
        let mut rejected: Vec<(String, Option<String>, String)> = Vec::new();
        let rejection = |key: &str, sub: Option<&str>, value: &toml::Value| {
            let value = match sub {
                Some(sub) => {
                    toml::Value::Table([(sub.to_string(), value.clone())].into_iter().collect())
                }
                None => value.clone(),
            };
            let probe: toml::Table = [(key.to_string(), value)].into_iter().collect();
            Config::deserialize(probe)
                .err()
                .map(|e| e.message().to_string())
        };
        for (key, value) in &table {
            match value.as_table() {
                Some(inner) if matches!(key.as_str(), "scan" | "view" | "limits") => {
                    for (sub, v) in inner {
                        if let Some(msg) = rejection(key, Some(sub), v) {
                            rejected.push((key.clone(), Some(sub.clone()), msg));
                        }
                    }
                }
                _ => {
                    if let Some(msg) = rejection(key, None, value) {
                        rejected.push((key.clone(), None, msg));
                    }
                }
            }
        }
        for (key, sub, msg) in rejected {
            let name = sub.as_ref().map_or(key.clone(), |s| format!("{key}.{s}"));
            tracing::warn!("{}: {name}: {msg}; ignoring it", path.display());
            match sub {
                Some(sub) => {
                    if let Some(inner) = table.get_mut(&key).and_then(toml::Value::as_table_mut) {
                        inner.remove(&sub);
                    }
                }
                None => {
                    table.remove(&key);
                }
            }
        }

        let parsed = serde_ignored::deserialize(table, |key| unknown.push(key.to_string()))
            .map_err(parse_err)?;
        for key in unknown {
            tracing::warn!("{}: unknown setting `{key}`; ignoring it", path.display());
        }
        Ok(parsed)
    }

    /// Precedence: defaults, bindings file, config, CLI.
    pub fn build_bindings(&self, cli_bindings: &[String]) -> Result<BindingTable, ConfigError> {
        let mut table = default_table();

        if let Some(path) = bindings_path()
            && path.exists()
        {
            let text = std::fs::read_to_string(&path).map_err(io_err(&path))?;
            apply_binding_lines(&mut table, &text, &path.display().to_string());
        }

        // Config typos warn and skip; CLI errors below are fatal.
        for (i, spec) in self.bindings.iter().enumerate() {
            if let Err(err) = table.insert_spec(spec) {
                tracing::warn!("invalid binding at config[{i}]: {err}; skipping");
            }
        }
        for spec in cli_bindings {
            table
                .insert_spec(spec)
                .map_err(|err| ConfigError::Binding {
                    origin: "--bind-key".to_string(),
                    message: err.to_string(),
                })?;
        }

        Ok(table.with_timeout(Duration::from_millis(self.view.sequence_timeout.max(1))))
    }
}

/// Writes defaults to the XDG path if missing; never touches --config.
pub fn ensure_default_config() -> Result<(), ConfigError> {
    let Some(path) = config_path().filter(|p| !p.exists()) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    let defaults = Config {
        bindings: default_bindings().into_iter().map(str::to_string).collect(),
        ..Config::default()
    };
    let mut text = String::from(
        "# hqimv defaults, written on first run. Edit freely; anything removed\n\
         # falls back to the built-in default.\n",
    );
    text.push_str(
        &toml::to_string_pretty(&defaults).map_err(|source| ConfigError::Serialize {
            path: path.display().to_string(),
            message: source.to_string(),
        })?,
    );
    std::fs::write(&path, text).map_err(io_err(&path))?;
    tracing::info!("wrote default config to {}", path.display());
    Ok(())
}

/// One binding per line; blanks and `#` comments skipped, bad lines warn.
pub fn apply_binding_lines(table: &mut BindingTable, text: &str, origin: &str) {
    for (n, line) in text.lines().enumerate() {
        let Some(line) = is_content_line(line) else {
            continue;
        };
        if let Err(err) = table.insert_spec(line) {
            tracing::warn!("invalid binding at {origin}:{}: {err}; skipping", n + 1);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("invalid config {path}: {message}")]
    Parse { path: String, message: String },
    #[error("cannot write default config {path}: {message}")]
    Serialize { path: String, message: String },
    #[error("invalid binding at {origin}: {message}")]
    Binding { origin: String, message: String },
}

/// Parses `#rgb`, `#rgba`, `#rrggbb`, or `#rrggbbaa` into normalized sRGB
/// components, with alpha defaulting to 1 when omitted.
pub fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    let hex = s.trim().strip_prefix('#')?;
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<_>>()?;

    // Each shorthand digit is doubled: 0xf -> 0xff.
    let pairs: Vec<u8> = match digits.len() {
        3 | 4 => digits.iter().flat_map(|&d| [d, d]).collect(),
        6 | 8 => digits,
        _ => return None,
    };
    let channel = |i: usize| (pairs[2 * i] * 16 + pairs[2 * i + 1]) as f32 / 255.0;
    let alpha = if pairs.len() == 8 { channel(3) } else { 1.0 };
    Some([channel(0), channel(1), channel(2), alpha])
}
