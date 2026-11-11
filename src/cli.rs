//! Command-line parsing.
//!
//! `usage-rs` derives the parse tables from this struct, so the spec in the attribute
//! is the same thing `--help` prints. The fields are the raw flags; defaults and
//! the config file are applied in `settings`, which is what the app reads.

use std::path::PathBuf;

use usage::Cli;

#[derive(Debug, Clone, Cli)]
#[usage(
    // The derive emits paths against `::usage_rs` by default. This crate depends on
    // the package under the name `usage`, so the facade path has to be pointed at it.
    crate = usage,
    bin = "hqimv",
    // A mistyped `--flag` is an error, not a file name. `--` still lets a file
    // start with a dash.
    unknown_flags = "error",
    version,
    about = "High-quality CPU image viewer for Wayland",
    long_about = "hqimv shows images on Wayland at the highest bit depth the compositor \
                  supports, using pure CPU resampling.\n\n\
                  Files may be paths, directories, http(s) URLs, or `-` to read a \
                  newline-separated list from stdin. pqiv-style bindings are configurable \
                  with --bind-key and in the config file."
)]
pub struct Cli {
    /// Add a key binding: "<Right> { goto_file_relative(1) }". Repeatable.
    #[allow(rustdoc::invalid_html_tags)]
    #[usage(short = 'b', long = "bind-key", value_name = "SPEC")]
    pub bind_key: Vec<String>,

    /// Config file to use instead of ~/.config/hqimv/config.toml.
    #[usage(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Playlist sort order: name, alpha, path, mtime, size, extension, random, none.
    #[usage(long, value_name = "ORDER")]
    pub sort: Option<String>,

    /// Reverse the sort order.
    #[usage(long)]
    pub reverse: bool,

    /// Scan directories recursively.
    #[usage(short = 'r', long)]
    pub recursive: bool,

    /// Only load files with these extensions, comma-separated.
    #[usage(long, value_name = "LIST")]
    pub extensions: Option<String>,

    /// Scale mode: fit, 1:1, fill.
    #[usage(long, value_name = "MODE")]
    pub scale_mode: Option<String>,

    /// Zoom multiplier on top of the scale mode.
    #[usage(short = 'z', long, value_name = "FACTOR")]
    pub zoom: Option<f64>,

    /// Resampling kernel: lanczos3, mitchell, bilinear.
    #[usage(long, value_name = "KERNEL")]
    pub filter: Option<String>,

    /// Force a format instead of negotiating.
    #[usage(long, value_name = "FORMAT")]
    pub format: Option<String>,

    /// Background colour behind the image, as #rrggbb or #rrggbbaa.
    #[usage(long, value_name = "COLOR")]
    pub background: Option<String>,

    /// Window size as WIDTHxHEIGHT; default fits image.
    #[usage(long, value_name = "WxH")]
    pub size: Option<String>,

    /// Start hidden and present on the first key press.
    #[usage(long)]
    pub hidden: bool,

    /// Auto-advance delay in ms; 0 disables.
    #[usage(long, value_name = "MS")]
    pub play_delay: Option<u64>,

    /// Start with the slideshow playing (needs --play-delay).
    #[usage(long)]
    pub slideshow: bool,

    /// Start fullscreen.
    #[usage(long)]
    pub fullscreen: bool,

    /// What stepping past the playlist ends does: loop, stop, quit.
    #[usage(long, value_name = "ACTION")]
    pub end_of_files_action: Option<String>,

    /// Decode this many following images ahead of time.
    #[usage(long, value_name = "N")]
    pub preload: Option<usize>,

    /// Start with the info overlay shown.
    #[usage(long)]
    pub info_overlay: bool,

    /// Keep the image name and position out of the window title.
    #[usage(long)]
    pub no_status_line: bool,

    /// Open without window decorations (title bar and borders).
    #[usage(long)]
    pub no_decorations: bool,

    /// Load the playlist from this file instead of scanning directories.
    #[usage(long, value_name = "PATH")]
    pub playlist_file: Option<PathBuf>,

    /// Write the playlist here on exit.
    #[usage(long, value_name = "PATH")]
    pub save_playlist: Option<PathBuf>,

    /// Log events to stderr at this level: error, warn, info, debug, trace.
    /// The RUST_LOG environment variable overrides it.
    #[usage(long, value_name = "LEVEL")]
    pub log: Option<String>,

    /// Print the negotiated presentation format and exit.
    #[usage(long)]
    pub print_format: bool,

    /// Paths, directories, URLs, or `-` for a list on stdin.
    #[usage(value_name = "FILES", var_min = 0)]
    pub files: Vec<String>,
}
