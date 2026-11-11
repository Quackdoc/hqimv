//! Turning command-line arguments and config into a playlist.

use std::io::Read;
use std::path::PathBuf;

use super::error::AppError;
use crate::cli::Cli;
use crate::config::Config;
use crate::playlist::{
    Playlist, PlaylistEntry, ScanOptions, parse_path_list, path_matches_extensions,
};

pub fn build_playlist(
    cli: &Cli,
    config: &Config,
    opts: &ScanOptions,
) -> Result<Playlist, AppError> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // Config first so CLI paths land last, where the viewer starts.
    for raw in &config.playlist {
        expand_argument(raw, &mut paths)?;
    }
    for raw in &cli.files {
        expand_argument(raw, &mut paths)?;
    }

    // Dirs append after explicit files so they act as a tail.
    let (explicit, dirs): (Vec<PathBuf>, Vec<PathBuf>) =
        paths.into_iter().partition(|p| !p.is_dir());

    // Explicit files use the same filter; rejections are reported, not silent.
    let mut kept: Vec<PlaylistEntry> = Vec::with_capacity(explicit.len());
    for path in explicit {
        if path_matches_extensions(&path, &opts.extensions) {
            kept.push(PlaylistEntry::new(path));
        } else {
            tracing::warn!(
                "skipping {}: extension is not in the configured list",
                path.display()
            );
        }
    }

    // Entries are carried over as scanned, not rebuilt, so nothing is stat'd twice.
    for dir in dirs {
        match Playlist::scan_dir(&dir, opts) {
            Ok(scanned) => kept.extend(scanned.into_entries()),
            Err(err) => tracing::warn!("cannot scan {}: {err}", dir.display()),
        }
    }

    if let Some(file) = &cli.playlist_file {
        let text = std::fs::read(file).map_err(|source| AppError::ReadList {
            what: file.display().to_string(),
            source,
        })?;
        let all = parse_path_list(&text);
        let list: Vec<PlaylistEntry> = all
            .iter()
            .filter(|p| path_matches_extensions(p, &opts.extensions))
            .cloned()
            .map(PlaylistEntry::new)
            .collect();
        let dropped = all.len() - list.len();
        if dropped > 0 {
            tracing::info!("skipped {dropped} of {} paths by extension", all.len());
        }
        tracing::info!("loaded {} paths from {}", list.len(), file.display());
        if !kept.is_empty() {
            tracing::warn!(
                "--playlist-file replaces the {} file(s) from the command line and config; \
                 they are not loaded",
                kept.len()
            );
        }
        kept = list;
    }

    let playlist = Playlist::from_entries(kept);
    if playlist.is_empty() {
        tracing::warn!("the playlist is empty");
    }
    Ok(playlist)
}

/// `-` reads stdin.
pub fn expand_argument(raw: &str, out: &mut Vec<PathBuf>) -> Result<(), AppError> {
    if raw == "-" {
        let mut text = Vec::new();
        std::io::stdin()
            .read_to_end(&mut text)
            .map_err(|source| AppError::ReadList {
                what: "stdin".to_string(),
                source,
            })?;
        out.extend(parse_path_list(&text));
        return Ok(());
    }
    out.push(PathBuf::from(raw));
    Ok(())
}
