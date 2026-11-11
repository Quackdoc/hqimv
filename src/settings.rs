//! The settings both the command line and the config file can give, resolved
//! once: command line over config over built-in default. The rest of the app
//! reads these and never looks at `Cli` or `Config` for them.

use std::fmt::Display;
use std::time::Duration;

use crate::cli::Cli;
use crate::config::{Config, parse_hex_color};
use crate::image::{Filter, LoadOptions, ScaleMode};
use crate::names::Named;
use crate::playlist::{EndAction, ScanOptions, default_extensions, parse_extensions};

pub struct Settings {
    pub scan: ScanOptions,
    pub scale_mode: ScaleMode,
    pub zoom: f64,
    pub filter: Filter,
    pub background: [f32; 4],
    /// `None` fits the window to the first image.
    pub window_size: Option<(u32, u32)>,
    pub fullscreen: bool,
    pub decorations: bool,
    /// A `wl_shm` format to force, by name; `None` negotiates one.
    pub format: Option<String>,
    pub status_line: bool,
    pub info_overlay: bool,
    pub play_delay: Duration,
    pub end_action: EndAction,
    pub preload: usize,
    /// How often the slideshow timer is checked.
    pub play_tick: Duration,
    /// How often the spinner and animations advance.
    pub loading_tick: Duration,
    /// How long a notice stays up.
    pub notice_ttl: Duration,
    /// Delay before retrying a frame the compositor refused.
    pub present_retry: Duration,
    /// Font size of the on-screen text, in pixels.
    pub overlay_px: f32,
    /// Gap between the on-screen text and the window edge, in pixels.
    pub overlay_margin: i32,
    /// Decode budgets and timing.
    pub load: LoadOptions,
}

/// The first layer whose value parses. A layer that is set but wrong is
/// reported, with what would have been accepted, and skipped.
fn pick<V: Copy + Display, T, const N: usize>(
    layers: [(&str, Option<V>); N],
    expected: &str,
    parse: impl Fn(V) -> Option<T>,
) -> Option<T> {
    layers.into_iter().find_map(|(setting, value)| {
        let value = value?;
        let parsed = parse(value);
        if parsed.is_none() {
            tracing::warn!("{setting}: `{value}` is not valid (expected {expected}); ignoring it");
        }
        parsed
    })
}

/// [`pick`] for a value chosen by name.
fn pick_named<T: Named, const N: usize>(layers: [(&str, Option<&str>); N]) -> Option<T> {
    pick(layers, &T::expected(), T::parse)
}

impl Settings {
    pub fn resolve(cli: &Cli, config: &Config) -> Self {
        let (view, scan, limits) = (&config.view, &config.scan, &config.limits);

        let from_cli = cli.extensions.as_deref().map(parse_extensions);
        let extensions = [from_cli.unwrap_or_default(), config.extensions.clone()]
            .into_iter()
            .find(|list| !list.is_empty())
            .unwrap_or_else(default_extensions);

        // At least 1 ms: a zero tick would spin the loop or divide by zero.
        let ms = |v: u64| Duration::from_millis(v.max(1));
        Self {
            scan: ScanOptions {
                recursive: cli.recursive || scan.recursive,
                sort: pick_named([
                    ("--sort", cli.sort.as_deref()),
                    ("scan.sort", scan.sort.as_deref()),
                ])
                .unwrap_or_default(),
                reverse: cli.reverse || scan.reverse,
                extensions,
            },
            scale_mode: pick_named([
                ("--scale-mode", cli.scale_mode.as_deref()),
                ("view.scale_mode", Some(&view.scale_mode)),
            ])
            .unwrap_or_default(),
            zoom: pick(
                [("--zoom", cli.zoom), ("view.zoom", view.zoom)],
                "a positive number",
                |z| (z.is_finite() && z > 0.0).then_some(z),
            )
            .unwrap_or(1.0),
            filter: pick_named([
                ("--filter", cli.filter.as_deref()),
                ("view.filter", Some(&view.filter)),
            ])
            .unwrap_or_default(),
            background: pick(
                [
                    ("--background", cli.background.as_deref()),
                    ("view.background", Some(&view.background)),
                ],
                "#rgb, #rgba, #rrggbb or #rrggbbaa",
                parse_hex_color,
            )
            .unwrap_or([0.0; 4]),
            window_size: pick(
                [
                    ("--size", cli.size.as_deref()),
                    ("view.size", view.size.as_deref()),
                ],
                "WIDTHxHEIGHT of positive numbers, such as 1280x720",
                |raw| {
                    let (w, h) = raw.split_once(['x', 'X'])?;
                    let size = (w.trim().parse().ok()?, h.trim().parse().ok()?);
                    (size.0 > 0 && size.1 > 0).then_some(size)
                },
            ),
            fullscreen: cli.fullscreen || view.fullscreen,
            decorations: view.decorations && !cli.no_decorations,
            format: cli.format.clone().or_else(|| view.format.clone()),
            status_line: view.status_line && !cli.no_status_line,
            info_overlay: view.info_overlay || cli.info_overlay,
            play_delay: Duration::from_millis(cli.play_delay.or(view.play_delay).unwrap_or(0)),
            end_action: pick_named([
                ("--end-of-files-action", cli.end_of_files_action.as_deref()),
                ("view.end_of_files_action", Some(&view.end_of_files_action)),
            ])
            .unwrap_or_default(),
            preload: cli.preload.unwrap_or(view.preload),
            play_tick: ms(view.play_tick),
            loading_tick: ms(view.loading_tick),
            notice_ttl: ms(view.notice_ttl),
            present_retry: ms(view.present_retry),
            overlay_px: view.overlay_size.max(1.0),
            overlay_margin: view.overlay_margin.max(0),
            load: LoadOptions {
                max_pixels: limits.max_pixels,
                max_dimension: limits.max_dimension,
                max_animation_frames: limits.max_animation_frames,
                max_animation_pixels: limits.max_animation_pixels,
                max_download_bytes: limits.max_download_bytes,
                cms_scratch: limits.cms_scratch_bytes,
                download_timeout: ms(limits.download_timeout),
                min_frame_delay: ms(view.min_frame_delay),
            },
        }
    }
}
