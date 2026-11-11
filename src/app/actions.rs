//! What a key binding does once it has been matched.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use super::App;
use super::host::CommandResult;
use crate::image::transform::Turn;
use crate::input::{Action, CommandContext, quote_positionals};
use crate::playlist::{Playlist, Step, absolutize, display_name, is_url, path_matches_extensions};

impl App {
    pub fn nav(&mut self, move_position: impl FnOnce(&mut Playlist) -> bool) {
        let prev = self.session.playlist.current_owned();
        if move_position(&mut self.session.playlist) {
            if let Some(prev) = prev {
                self.session.playlist.push_history(prev);
            }
            self.load_current_async();
            self.notify_current();
        }
    }

    /// Names the picture just switched to, with its place in the playlist.
    fn notify_current(&mut self) {
        if let Some(msg) = self.current_label() {
            self.notify(msg);
        }
    }

    /// Restarts the notice timer if it is the current image's name.
    pub fn notify_current_if_shown(&mut self) {
        if let Some(label) = self.current_label()
            && self
                .session
                .chrome
                .notice
                .as_ref()
                .is_some_and(|(m, _)| *m == label)
        {
            self.notify(label);
        }
    }

    /// "name [pos/total]" for the current entry.
    pub fn current_label(&self) -> Option<String> {
        let path = self.session.playlist.current_path()?;
        let pos = self.session.playlist.position() + 1;
        let total = self.session.playlist.len();
        Some(format!("{} [{pos}/{total}]", display_name(path)))
    }

    fn notify_zoom(&mut self) {
        let msg = format!("zoom {:.2}x", self.session.view.zoom);
        self.notify(msg);
    }

    fn notify_scale_mode(&mut self) {
        let msg = format!("scale {:?}", self.session.view.scale_mode);
        self.notify(msg);
    }

    pub fn notify(&mut self, msg: String) {
        self.session.chrome.notice = Some((msg, Instant::now()));
    }

    // "added x to playlist" / "removed N images from playlist".
    pub fn list_msg(verb: &str, prep: &str, paths: &[PathBuf]) -> String {
        if let [only] = paths {
            format!("{verb} {} {prep} playlist", display_name(only))
        } else {
            format!("{verb} {} images {prep} playlist", paths.len())
        }
    }

    /// A relative move that honours `--end-of-files-action` at either end.
    pub fn nav_step(&mut self, step: impl FnOnce(&mut Playlist) -> Step) {
        let mut quit = false;
        self.nav(|p| match step(p) {
            Step::Moved => true,
            Step::Stay => false,
            Step::Quit => {
                quit = true;
                false
            }
        });
        if quit {
            self.quit();
        }
    }

    pub fn step_relative(&mut self, delta: isize) {
        let end = self.setup.settings.end_action;
        self.nav_step(|p| p.step_relative(delta, end));
    }

    /// After entries were removed by index or name: reload only if that took
    /// the current image away, otherwise just refresh the title's position.
    fn after_removal(&mut self, before: Option<PathBuf>) {
        if self.session.playlist.current_path() != before.as_deref() {
            self.load_current_async();
        } else {
            self.refresh();
        }
    }

    fn notify_removed(&mut self, label: &str) {
        tracing::info!("removed {label} from the playlist");
        self.notify(format!("removed {label} from playlist"));
    }

    // Rotation and mirroring reach every frame, or the next one undoes them.
    fn transform_image(&mut self, turn: Turn, label: &str) {
        if self.session.content.apply(turn) {
            tracing::debug!("{label}");
            self.notify(label.to_string());
            self.invalidate();
        }
    }

    pub fn absorb_added_paths(&mut self, paths: Vec<PathBuf>) {
        let mut seen = HashSet::new();
        let fresh: Vec<PathBuf> = paths
            .into_iter()
            .map(|p| if is_url(&p) { p } else { absolutize(&p) })
            .filter(|p| {
                !self.session.playlist.contains(p)
                    && path_matches_extensions(p, &self.setup.settings.scan.extensions)
                    && seen.insert(p.clone())
            })
            .collect();
        if fresh.is_empty() {
            return;
        }
        // A URL has no directory on disk to watch.
        for p in fresh.iter().filter(|p| !is_url(p)) {
            self.host.fs.watch(super::watch::dir_of(p));
        }
        tracing::info!("added {} image(s) to playlist", fresh.len());
        let msg = Self::list_msg("added", "to", &fresh);
        self.session.playlist.add_paths(fresh);
        self.notify(msg);
        self.refresh();
    }

    fn add_file_action(&mut self, raw: &Path) {
        if raw.is_dir() && !is_url(raw) {
            match Playlist::scan_dir(raw, &self.setup.settings.scan) {
                Ok(scanned) => {
                    self.absorb_added_paths(scanned.paths().cloned().collect());
                }
                Err(err) => tracing::warn!("cannot scan {}: {err}", raw.display()),
            }
        } else if raw.is_file() || is_url(raw) {
            self.absorb_added_paths(vec![raw.to_path_buf()]);
        } else {
            tracing::warn!("cannot add {}: no such file or directory", raw.display());
        }
    }

    pub fn run_action(&mut self, action: &Action) {
        match action {
            Action::GotoFileRelative(n) => self.step_relative(*n as isize),
            Action::GotoFileAbsolute(n) => {
                // Bindings are 1-based, matching the status line.
                if *n > 0 {
                    self.nav(|p| p.jump_to(n.saturating_sub(1)));
                }
            }
            Action::GotoFileMarked(n) => {
                let end = self.setup.settings.end_action;
                self.nav_step(|p| p.step_marked(*n as isize, end));
            }
            Action::GotoFileRandom => self.nav(|p| p.random()),
            Action::GotoFileFirst => self.nav(|p| p.first()),
            Action::GotoFileLast => self.nav(|p| p.last()),
            Action::GotoFileByName(name) => {
                self.nav(|p| p.jump_to_name(name));
            }
            Action::GotoDirectoryRelative(n) => {
                let end = self.setup.settings.end_action;
                self.nav_step(|p| p.step_directory(*n as isize, end));
            }
            Action::GotoEarlierFile => {
                while let Some(prev) = self.session.playlist.pop_history() {
                    if self.session.playlist.jump_to_path(&prev) {
                        self.load_current_async();
                        self.notify_current();
                        break;
                    }
                }
            }
            Action::RotateLeft => self.transform_image(Turn::RotateCcw, "rotated left"),
            Action::RotateRight => self.transform_image(Turn::RotateCw, "rotated right"),
            Action::FlipHorizontal => {
                self.transform_image(Turn::FlipHorizontal, "flipped horizontally")
            }
            Action::FlipVertical => self.transform_image(Turn::FlipVertical, "flipped vertically"),
            Action::AddFile(path) => self.add_file_action(path),
            Action::RemoveFileByIndex(n) => {
                let before = self.session.playlist.current_owned();
                if let Some(pos) = n.checked_sub(1)
                    && let Some(path) = self.session.playlist.remove_at(pos)
                {
                    self.notify_removed(&display_name(&path));
                    self.after_removal(before);
                }
            }
            Action::RemoveFileByName(name) => {
                let before = self.session.playlist.current_owned();
                if self.session.playlist.remove_by_name(name) {
                    self.notify_removed(name);
                    self.after_removal(before);
                }
            }
            Action::OutputFileList => {
                super::out(self.session.playlist.save_lines());
            }
            Action::ToggleMark => {
                let marked = self.session.playlist.toggle_mark();
                if let Some(path) = self.session.playlist.current_path() {
                    let verb = if marked { "marked" } else { "unmarked" };
                    tracing::info!("{verb} {}", path.display());
                }
                self.refresh();
            }
            Action::MarkAll => {
                self.session.playlist.mark_all();
                self.refresh();
            }
            Action::UnmarkAll => {
                self.session.playlist.unmark_all();
                self.refresh();
            }
            Action::SetZoom(z) => {
                self.session.view.set_zoom(*z);
                self.notify_zoom();
                self.invalidate();
            }
            Action::ResetImage => {
                self.session.view.reset();
                self.notify("reset view".to_string());
                self.invalidate();
            }
            Action::Pan(dx, dy) => {
                self.session.view.pan(*dx, *dy);
                self.invalidate();
            }
            Action::ZoomBy(f) => {
                self.session.view.zoom_by(*f);
                self.notify_zoom();
                self.invalidate();
            }
            Action::SetScaleMode(mode) => {
                self.session.view.scale_mode = *mode;
                self.notify_scale_mode();
                self.invalidate();
            }
            Action::CycleScaleMode => {
                self.session.view.scale_mode = self.session.view.scale_mode.next();
                self.notify_scale_mode();
                self.invalidate();
            }
            Action::ToggleFullscreen => self.toggle_fullscreen(),
            Action::Command(spec) => self.run_command(spec),
            Action::RemoveFromPlaylist => {
                if let Some(path) = self.session.playlist.remove_current() {
                    self.notify_removed(&display_name(&path));
                    self.load_current_async();
                }
            }
            Action::DeleteFile => self.delete_current_file(),
            Action::CopyPath => self.copy_path(),
            Action::ToggleInfoOverlay => {
                self.session.view.info_overlay = !self.session.view.info_overlay;
                self.invalidate();
            }
            Action::ToggleStatusLine => {
                self.session.view.status_line = !self.session.view.status_line;
                self.refresh();
            }
            Action::Reload => {
                self.load_current_async();
            }
            Action::SavePlaylist(path) => self.save_playlist(path),
            Action::TogglePlay => self.session.slideshow.toggle(),
            Action::Quit => self.quit(),
            Action::Noop => {}
        }
    }

    fn toggle_fullscreen(&mut self) {
        let Some(wl) = self.host.screen.wl.as_mut() else {
            return;
        };
        let next = wl.window.fullscreen().is_none();
        wl.window
            .set_fullscreen(next.then_some(winit::window::Fullscreen::Borderless(None)));
        self.invalidate();
    }

    fn run_command(&mut self, spec: &str) {
        let Some(path) = self.session.playlist.current_owned() else {
            return;
        };
        let ctx = CommandContext::for_current(
            &path,
            self.session.playlist.position() + 1,
            self.session.playlist.len(),
            self.session.playlist.marked_count(),
        );
        if spec.trim().is_empty() {
            return;
        }
        let script = quote_positionals(spec);
        let args = ctx.positional_args();

        let tx = self.host.commands.tx.clone();
        let env = ctx.env();
        tracing::info!("running `{spec}` on {}", path.display());

        // Slow commands must not freeze the window.
        std::thread::spawn(move || {
            let mut cmd = Command::new("sh");
            // `$0` is the program name; `$1`..`$3` follow.
            cmd.arg("-c").arg(&script).arg("hqimv").args(&args);
            cmd.stdin(Stdio::null());
            for (key, value) in env {
                cmd.env(key, value);
            }
            let _ = tx.send(match cmd.output() {
                Ok(out) => CommandResult {
                    status: out.status.code(),
                    stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                },
                Err(err) => CommandResult {
                    status: None,
                    stdout: String::new(),
                    stderr: err.to_string(),
                },
            });
        });
    }

    pub fn drain_commands(&mut self) {
        while let Ok(result) = self.host.commands.rx.try_recv() {
            match result.status {
                Some(0) => tracing::debug!("command finished"),
                Some(127) => tracing::warn!("{}", describe("command not found", &result.stderr)),
                Some(code) => tracing::warn!(
                    "{}",
                    describe(&format!("command exited with {code}"), &result.stderr)
                ),
                None => tracing::warn!("command did not run: {}", result.stderr),
            }
            if !result.stdout.trim().is_empty() {
                super::out(format!("{}\n", result.stdout.trim_end()));
            }
            self.invalidate();
        }
    }

    fn delete_current_file(&mut self) {
        let Some(path) = self.session.playlist.current_owned() else {
            return;
        };
        if is_url(&path) {
            tracing::warn!(
                "cannot trash {}: it is a URL, not a local file",
                path.display()
            );
            return;
        }
        match trash::delete(&path) {
            Ok(()) => {
                tracing::info!("trashed {}", path.display());
                let _ = self.session.playlist.remove_current();
                self.notify(format!("trashed {}", display_name(&path)));
                self.load_current_async();
            }
            Err(err) => tracing::warn!("cannot trash {}: {err}", path.display()),
        }
    }

    fn copy_path(&mut self) {
        if let Some(path) = self.session.playlist.current_path() {
            super::out(format!("{}\n", path.display()));
        }
    }

    fn save_playlist(&self, path: &Path) {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            tracing::warn!("cannot create {}: {err}", parent.display());
            return;
        }
        match std::fs::write(path, self.session.playlist.save_lines()) {
            Ok(()) => tracing::info!(
                "wrote {} entries to {}",
                self.session.playlist.len(),
                path.display()
            ),
            Err(err) => tracing::warn!("cannot write {}: {err}", path.display()),
        }
    }

    pub fn update_title(&mut self) -> bool {
        let title = self.window_title();
        if self.session.chrome.title == title {
            return true;
        }
        let Some(wl) = self.host.screen.wl.as_ref() else {
            return false;
        };
        let delivered = self
            .session
            .chrome
            .commit(title, &wl.window, &wl.presentation.conn);
        if delivered {
            self.invalidate();
        }
        delivered
    }

    pub fn quit(&mut self) {
        if let Some(path) = &self.setup.cli.save_playlist {
            self.save_playlist(path);
        }
        self.exit_requested = true;
    }
}

fn describe(prefix: &str, stderr: &str) -> String {
    match stderr.trim() {
        "" => prefix.to_string(),
        stderr => format!("{prefix}: {stderr}"),
    }
}
