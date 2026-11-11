//! The actions a binding can run, and parsing them from their spec text.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::image::ScaleMode;
use crate::names::Named;
use crate::playlist::{display_name, is_url_str, url_dir};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Relative move, wrapping.
    GotoFileRelative(i64),
    /// 1-based absolute position.
    GotoFileAbsolute(usize),
    /// Relative move through marked entries only.
    GotoFileMarked(i64),
    GotoFileRandom,
    GotoFileFirst,
    GotoFileLast,
    ToggleMark,
    MarkAll,
    UnmarkAll,
    ResetImage,
    /// Image px per window px.
    SetZoom(f64),
    /// Zoom multiplier.
    ZoomBy(f64),
    SetScaleMode(ScaleMode),
    /// Cycles fit -> 1:1 -> fill.
    CycleScaleMode,
    ToggleFullscreen,
    /// Shell command; $1/$2/$3 substituted.
    Command(String),
    RemoveFromPlaylist,
    DeleteFile,
    CopyPath,
    ToggleInfoOverlay,
    /// Pan by (x, y) px.
    Pan(i32, i32),
    ToggleStatusLine,
    Reload,
    SavePlaylist(PathBuf),
    /// Jump by display name.
    GotoFileByName(String),
    /// First entry of nth next/prev dir; wraps.
    GotoDirectoryRelative(i64),
    GotoEarlierFile,
    RotateLeft,
    RotateRight,
    FlipHorizontal,
    FlipVertical,
    AddFile(PathBuf),
    /// 1-based visible index.
    RemoveFileByIndex(usize),
    /// Remove by display name.
    RemoveFileByName(String),
    OutputFileList,
    TogglePlay,
    Quit,
    /// No-op that still consumes the sequence.
    Noop,
}

impl Action {
    pub fn parse(spec: &str) -> Result<Self, ActionError> {
        let spec = spec.trim();
        let (verb, args) = match split_call(spec) {
            Some((verb, args)) => (verb, args),
            None => (spec, ""),
        };

        let arg_i64 = |s: &str| parse_num::<i64>(verb, s);

        Ok(match verb {
            "goto_file_relative" => Action::GotoFileRelative(arg_i64(args)?),
            // One step by default; `prev` counts backwards.
            "next" => Action::GotoFileRelative(steps(verb, args)?),
            "prev" => Action::GotoFileRelative(steps(verb, args)?.saturating_neg()),
            "goto_file_absolute" => Action::GotoFileAbsolute(parse_num::<usize>(verb, args)?),
            "goto_file_marked" => Action::GotoFileMarked(arg_i64(args)?),
            "goto_file_random" => Action::GotoFileRandom,
            "goto_file_first" => Action::GotoFileFirst,
            "goto_file_last" => Action::GotoFileLast,
            "goto_file_by_name" | "goto_file_byname" => {
                Action::GotoFileByName(args.trim().to_string())
            }
            "goto_directory_relative" => Action::GotoDirectoryRelative(arg_i64(args)?),
            "goto_earlier_file" => Action::GotoEarlierFile,
            "rotate_left" => Action::RotateLeft,
            "rotate_right" => Action::RotateRight,
            "flip_horizontally" | "flip_horizontal" => Action::FlipHorizontal,
            "flip_vertically" | "flip_vertical" => Action::FlipVertical,
            "add_file" => Action::AddFile(PathBuf::from(args.trim())),
            "remove_file_by_index" | "remove_file_byindex" => {
                Action::RemoveFileByIndex(parse_num::<usize>(verb, args)?)
            }
            "remove_file_by_name" | "remove_file_byname" => {
                Action::RemoveFileByName(args.trim().to_string())
            }
            "output_file_list" => Action::OutputFileList,
            "toggle_mark" | "mark" => Action::ToggleMark,
            "mark_all" => Action::MarkAll,
            "unmark_all" => Action::UnmarkAll,
            "set_zoom" => Action::SetZoom(parse_f64(verb, args)?),
            "zoom_by" | "zoom" => Action::ZoomBy(parse_f64(verb, args)?),
            "set_scale_mode" => Action::SetScaleMode(
                ScaleMode::parse(args.trim()).ok_or_else(|| bad_arg(verb, args))?,
            ),
            "cycle_scale_mode" => Action::CycleScaleMode,
            "toggle_fullscreen" => Action::ToggleFullscreen,
            "command" => Action::Command(args.to_string()),
            "remove_from_playlist" => Action::RemoveFromPlaylist,
            "delete_file" => Action::DeleteFile,
            "copy_path" => Action::CopyPath,
            "toggle_info_overlay" => Action::ToggleInfoOverlay,
            "pan" => {
                let (x, y) = args.split_once(',').ok_or_else(|| bad_arg(verb, args))?;
                Action::Pan(parse_num(verb, x)?, parse_num(verb, y)?)
            }
            "toggle_status_line" => Action::ToggleStatusLine,
            "reload" => Action::Reload,
            "save_playlist" => Action::SavePlaylist(PathBuf::from(args.trim())),
            "toggle_play" => Action::TogglePlay,
            "quit" => Action::Quit,
            "reset_image" => Action::ResetImage,
            "noop" | "" => Action::Noop,
            other => return Err(ActionError::UnknownAction(other.to_string())),
        })
    }
}

/// An optional step count, 1 when none is written.
fn steps(verb: &str, args: &str) -> Result<i64, ActionError> {
    if args.trim().is_empty() {
        Ok(1)
    } else {
        parse_num::<i64>(verb, args)
    }
}

fn split_call(spec: &str) -> Option<(&str, &str)> {
    let open = spec.find('(')?;
    if !spec.trim_end().ends_with(')') {
        return None;
    }
    let verb = spec[..open].trim();
    let args = &spec[open + 1..spec.trim_end().len() - 1];
    Some((verb, args))
}

fn parse_num<T>(verb: &str, args: &str) -> Result<T, ActionError>
where
    T: FromStr,
{
    args.trim().parse().map_err(|_| bad_arg(verb, args))
}

fn parse_f64(verb: &str, args: &str) -> Result<f64, ActionError> {
    let v = parse_num::<f64>(verb, args)?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(bad_arg(verb, args))
    }
}

fn bad_arg(verb: &str, args: &str) -> ActionError {
    ActionError::BadArgument {
        action: verb.to_string(),
        argument: args.to_string(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("unknown action `{0}`")]
    UnknownAction(String),
    #[error("action `{action}` got an unusable argument `{argument}`")]
    BadArgument { action: String, argument: String },
}

/// Placeholder values for `command(...)` bindings.
#[derive(Debug, Clone, Default)]
pub struct CommandContext {
    /// `$1` and `$HQIMV_PATH`: the current file's path.
    pub path: PathBuf,
    /// `$2` and `$HQIMV_INDEX`: 1-based playlist position.
    pub index: usize,
    /// `$3` and `$HQIMV_COUNT`: total number of entries.
    pub count: usize,
    /// `$HQIMV_DIR`: directory containing the current file.
    pub dir: PathBuf,
    /// `$HQIMV_NAME`: file name of the current file.
    pub name: String,
    /// `$HQIMV_MARKS`: how many entries are marked.
    pub marks: usize,
}

impl CommandContext {
    pub fn for_current(path: &Path, index: usize, count: usize, marks: usize) -> Self {
        // A URL has no parent on disk, so split it for the command environment.
        let dir = match path.to_str() {
            Some(url) if is_url_str(url) => PathBuf::from(url_dir(url)),
            _ => path.parent().unwrap_or(Path::new(".")).to_path_buf(),
        };
        let name = display_name(path);
        Self {
            path: path.to_path_buf(),
            index,
            count,
            dir,
            name,
            marks,
        }
    }

    pub fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("HQIMV_PATH", self.path.to_string_lossy().into_owned()),
            ("HQIMV_INDEX", self.index.to_string()),
            ("HQIMV_COUNT", self.count.to_string()),
            ("HQIMV_DIR", self.dir.to_string_lossy().into_owned()),
            ("HQIMV_NAME", self.name.clone()),
            ("HQIMV_MARKS", self.marks.to_string()),
        ]
    }

    /// The positional parameters `$1`/`$2`/`$3` handed to the shell. Passing
    /// them as arguments, not pasting them into the script, keeps a hostile
    /// file name from being parsed as shell syntax.
    pub fn positional_args(&self) -> [OsString; 3] {
        [
            self.path.as_os_str().to_owned(),
            self.index.to_string().into(),
            self.count.to_string().into(),
        ]
    }
}

/// Makes `$1`/`$2`/`$3` safe wherever the user wrote them: unquoted ones get
/// double quotes (no word splitting), ones inside double quotes are left as
/// they are, and ones inside single quotes, where the shell would not expand
/// them at all, step out of the quotes.
pub fn quote_positionals(command: &str) -> String {
    let mut out = String::with_capacity(command.len() + 16);
    let (mut single, mut double) = (false, false);
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if !single => {
                out.push(c);
                out.extend(chars.next());
            }
            '\'' if !double => {
                single = !single;
                out.push(c);
            }
            '"' if !single => {
                double = !double;
                out.push(c);
            }
            '$' if matches!(chars.peek(), Some('1'..='3')) => {
                let n = chars.next().unwrap_or('1');
                out.push_str(&match (single, double) {
                    (true, _) => format!("'\"${n}\"'"),
                    (_, true) => format!("${n}"),
                    _ => format!("\"${n}\""),
                });
            }
            _ => out.push(c),
        }
    }
    out
}
