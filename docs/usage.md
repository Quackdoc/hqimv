# hqimv usage

```
hqimv [FLAGS] [FILES]...
```

## Files

`FILES` may be any of:

- image file paths
- directories (scanned for images; `-r` recurses into subdirectories)
- `http(s)` URLs
- `-` to read a newline-separated list of paths from stdin

Use `--` before the files to pass a name that starts with a dash:

```
hqimv -- -strange-name.png
```

## Options

| Flag | Value | Description |
| --- | --- | --- |
| `-b`, `--bind-key` | `SPEC` | Add a key binding, e.g. `"<Right> { goto_file_relative(1) }"`. Repeatable. |
| `--config` | `PATH` | Use this config file instead of `~/.config/hqimv/config.toml`. |
| `--sort` | `ORDER` | Playlist sort order; see [Sorting](#sorting). |
| `--reverse` | | Reverse the sort order. |
| `-r`, `--recursive` | | Scan directories recursively. |
| `--extensions` | `LIST` | Only load files with these extensions, comma-separated. Also applies to files named on the command line, which are skipped with a warning. |
| `--scale-mode` | `MODE` | Scale mode: `fit`, `1:1`, `fill`. |
| `-z`, `--zoom` | `FACTOR` | Zoom multiplier on top of the scale mode. |
| `--filter` | `KERNEL` | Resampling kernel: `lanczos3`, `mitchell`, `bilinear`. |
| `--format` | `FORMAT` | Force a format instead of negotiating. |
| `--background` | `COLOR` | Background colour behind the image, as `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`. |
| `--size` | `WxH` | Window size as `WIDTHxHEIGHT`; default fits the image. |
| `--hidden` | | Start hidden and present on the first key press. |
| `--play-delay` | `MS` | Auto-advance delay in milliseconds; `0` disables. |
| `--slideshow` | | Start with the slideshow playing (needs `--play-delay`). |
| `--fullscreen` | | Start fullscreen. |
| `--end-of-files-action` | `ACTION` | What stepping past the end of the playlist does: `loop`, `stop`, `quit`. |
| `--preload` | `N` | Decode this many following images ahead of time. |
| `--info-overlay` | | Start with the info overlay shown. |
| `--no-status-line` | | Keep the image name and position out of the window title. |
| `--no-decorations` | | Open without window decorations (title bar and borders). |
| `--playlist-file` | `PATH` | Load the playlist from this file instead of scanning directories. |
| `--save-playlist` | `PATH` | Write the playlist to this file on exit. |
| `--log` | `LEVEL` | Log events to stderr at this level; see [Logging](#logging). |
| `--print-format` | | Print the negotiated presentation format and exit. |
| `-h`, `--help` | | Print help. |
| `-V`, `--version` | | Print version. |

## Sorting

`--sort` (or `scan.sort`) orders each directory that is scanned. Files named on the command line keep the order given and come first; scanned directories follow them. Ties always fall back to name, then path, so the order is the same on every run.

| Key | Order |
| --- | --- |
| `name` (default; also `natural`) | File name, case-insensitive, digit runs by value: `img2` before `img10`. |
| `alpha` (`lexical`, `ascii`) | File name, case-insensitive, character by character: `img10` before `img2`. |
| `path` | Whole path one directory level at a time, so each directory's files stay together. |
| `mtime` (`date`, `time`) | Oldest first. |
| `size` | Smallest first. |
| `extension` (`ext`, `type`) | Extension, then name. |
| `random` (`shuffle`) | Shuffled once when the playlist is built. |
| `none` (`unsorted`, `scan`) | The order the files were found in. |

`--reverse` flips any of them (except `random`, which is already unordered).

## Names and aliases

Values chosen by name are case-insensitive, and the same names work on the command line and in the config file. The first name is canonical (it is what hqimv prints); the others are accepted as aliases.

| Setting | Names |
| --- | --- |
| `--scale-mode`, `view.scale_mode` | `fit`; `1:1` (`one`, `one-to-one`, `actual`); `fill` |
| `--filter`, `view.filter` | `lanczos3` (`lanczos`); `mitchell` (`cubic`); `bilinear` (`linear`) |
| `--end-of-files-action`, `view.end_of_files_action` | `loop` (`cycle`, `wrap`); `stop` (`clamp`); `quit` (`exit`) |

`--sort` and `scan.sort` take the names in [Sorting](#sorting).

## Logging

Events go to stderr, so stdout carries only data (`output_file_list`, `copy_path`, command output). `--log LEVEL` picks how much: `error`, `warn` (the default), `info`, `debug` or `trace`. The `RUST_LOG` environment variable overrides it and takes the full `tracing` filter syntax, such as `RUST_LOG=hqimv=debug`.

| Level | What it shows |
| --- | --- |
| `error` | The viewer cannot start. |
| `warn` | Something you asked for did not happen: a setting or binding was ignored, a file or command failed. |
| `info` | What happened: each image loaded, entries added, removed or marked, files saved or trashed, commands run, the presentation format. |
| `debug` | Internals: preloading, buffer pools, unmatched keys. |
| `trace` | Key-sequence matching. |

## Warnings

A setting that is set but wrong is reported on stderr, naming the flag or config key and what was accepted, and the next source (config file, then built-in default) is used instead. This covers `--sort`, `--scale-mode`, `--filter`, `--end-of-files-action`, `--background`, `--zoom`, `--size` (and the matching config keys), an invalid `--log` level, a missing `--config` file, and keys in the config file that are unknown or of the wrong type (such as `zoom = "big"`); a wrong-typed key is ignored and keeps its default. Only a config file that is not valid TOML stops hqimv from starting. `--playlist-file` also warns when it replaces files given on the command line.

## Config file

hqimv reads `~/.config/hqimv/config.toml` (or `$XDG_CONFIG_HOME/hqimv/config.toml`). If the file does not exist, hqimv writes it on first run with the default bindings. Pass `--config PATH` to use another file; in that case the default file is neither read nor written.

Precedence, lowest to highest: built-in defaults, the bindings file `~/.config/hqimv/bindings` (one binding per line; blank lines and `#` comments are skipped), the `bindings` key in the config file, then `--bind-key` on the command line.

Every key is optional. Unset keys keep the built-in default. For every setting that has both a flag and a key (including `extensions`), the flag wins over the config file, which wins over the built-in default. The boolean flags only turn things on or off: `--recursive`, `--reverse`, `--fullscreen` and `--info-overlay` add to the config, and `--no-status-line` and `--no-decorations` turn off a `status_line` or `decorations` that is on. `--hidden`, `--slideshow` and `--playlist-file` apply to one run and have no config key.

Top-level keys:

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `bindings` | array of strings | default bindings (see below) | Key bindings in `--bind-key` syntax. |
| `playlist` | array of strings | `[]` | Extra entries (paths, directories, URLs, `-`). Prepended to the command-line `FILES`. |
| `extensions` | array of strings | `[]` (all supported formats) | Only load files with these extensions. |

`[scan]`:

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `recursive` | bool | `false` | Scan directories recursively. |
| `sort` | string | unset (`name`) | Sort key; see [Sorting](#sorting). |
| `reverse` | bool | `false` | Reverse the sort order. |

`[view]`:

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `scale_mode` | string | `"fit"` | Initial scale mode: `fit`, `1:1`, or `fill`. |
| `zoom` | float | unset (`1.0`) | Initial zoom multiplier on top of the scale mode. |
| `filter` | string | `"lanczos3"` | Resampling kernel: `lanczos3`, `mitchell`, or `bilinear`. |
| `background` | string | `"#00000000"` | Background colour as sRGB hex: `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`. |
| `size` | string | unset (fits the image) | Window size as `WIDTHxHEIGHT`. |
| `fullscreen` | bool | `false` | Start fullscreen. |
| `decorations` | bool | `true` | Window decorations (title bar and borders). `--no-decorations` turns them off. |
| `format` | string | unset (negotiated) | Force a `wl_shm` format instead of negotiating one. |
| `status_line` | bool | `true` | Show the image name and position in the window title. |
| `info_overlay` | bool | `false` | Start with the info overlay shown. |
| `play_delay` | integer (ms) | `0` | Auto-advance delay; `0` disables. |
| `end_of_files_action` | string | `"loop"` | What stepping past the playlist ends does: `loop`, `stop` or `quit`. |
| `preload` | integer | `0` | Number of following images to decode ahead of time. |
| `sequence_timeout` | integer (ms) | `750` | Timeout for multi-key binding sequences. |
| `min_frame_delay` | integer (ms) | `10` | Floor for an animation frame that asks for no delay. |
| `play_tick` | integer (ms) | `50` | How often the slideshow timer is checked. |
| `loading_tick` | integer (ms) | `250` | How often the loading spinner and animations advance. |
| `notice_ttl` | integer (ms) | `2000` | How long an on-screen notice stays up. |
| `present_retry` | integer (ms) | `4` | Delay before retrying a frame the compositor refused. |
| `overlay_size` | float (px) | `24.0` | Font size of the on-screen text (loading, notices, info overlay). Minimum 1. |
| `overlay_margin` | integer (px) | `16` | Gap between the on-screen text and the window edge. Minimum 0. |

`sequence_timeout`, `min_frame_delay` and the four ticks and delays above are clamped to at least 1 ms.
`[limits]` (decode budgets; an image over a limit is refused):

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `max_pixels` | integer | `500000000` | Largest single image, in pixels. |
| `max_dimension` | integer | unset | Largest edge of a single image, in pixels. |
| `max_animation_frames` | integer | `10000` | Largest number of frames in one animation. |
| `max_animation_pixels` | integer | `50000000` | Largest total area across an animation's frames, in pixels. |
| `max_download_bytes` | integer | `268435456` | Largest remote response body downloaded, in bytes. |
| `cms_scratch_bytes` | integer | `33554432` | Ceiling on the scratch memory used for colour conversion, in bytes. Smaller uses less memory but takes more passes over a large image. |
| `download_timeout` | integer (ms) | `30000` | Whole-request budget for a remote image: connect, transfer and each read. |

Example:

```toml
extensions = ["png", "jpg", "webp"]

[scan]
recursive = true
sort = "mtime"

[view]
scale_mode = "fill"
filter = "mitchell"
background = "#202020"
```

## Key bindings

A binding has the form:

```
"<Key> { action(args) }"
```

`<Key>` is a key name in angle brackets (for example `<Right>`, `<Ctrl-m>`, `<Shift-Left>`, `<space>`) or a single character. Keys can be chained into a sequence (`g h`, or `g<Right>`); each key must follow the previous one within the sequence timeout (`view.sequence_timeout`, 750 ms by default). A binding that is the start of a longer one fires first, so the longer one is unreachable; hqimv warns about this. The braces hold one or more actions separated by `;`. Bindings are given with `--bind-key`, in the `bindings` config key, or in the bindings file.

Default bindings:

| Key | Action |
| --- | --- |
| `<Right>` | `goto_file_relative(1)` |
| `<Left>` | `goto_file_relative(-1)` |
| `<Down>` | `zoom_by(0.8)` |
| `<Up>` | `zoom_by(1.25)` |
| `<Shift-Left>` | `pan(-10, 0)` |
| `<Shift-Right>` | `pan(10, 0)` |
| `<Shift-Up>` | `pan(0, -10)` |
| `<Shift-Down>` | `pan(0, 10)` |
| `<PageDown>` | `goto_file_relative(5)` |
| `<PageUp>` | `goto_file_relative(-5)` |
| `<Home>` | `goto_file_first` |
| `<End>` | `goto_file_last` |
| `r` | `goto_file_random` |
| `<space>` | `goto_file_relative(1)` |
| `<backspace>` | `goto_file_relative(-1)` |
| `m` | `toggle_mark` |
| `M` | `mark_all` |
| `<Ctrl-m>` | `unmark_all` |
| `<Ctrl-j>` | `goto_file_marked(1)` |
| `<Ctrl-k>` | `goto_file_marked(-1)` |
| `u` | `goto_earlier_file` |
| `o` | `output_file_list` |
| `k` | `rotate_right` |
| `l` | `rotate_left` |
| `h` | `flip_horizontally` |
| `v` | `flip_vertically` |
| `<plus>` | `zoom_by(1.25)` |
| `<equal>` | `zoom_by(1.25)` |
| `<minus>` | `zoom_by(0.8)` |
| `0` | `reset_image` |
| `f` | `cycle_scale_mode` |
| `1` | `set_scale_mode(1:1)` |
| `d` | `remove_from_playlist` |
| `R` | `reload` |
| `<f11>` | `toggle_fullscreen` |
| `i` | `toggle_info_overlay` |
| `y` | `copy_path` |
| `q` | `quit` |
| `<Escape>` | `quit` |
| `<Ctrl-q>` | `quit` |

Actions:

| Action | Description |
| --- | --- |
| `goto_file_relative(N)` | Move N entries (negative for backwards). End behaviour follows `--end-of-files-action`. |
| `next(N)` / `prev(N)` | Same as `goto_file_relative(N)` / `goto_file_relative(-N)`; N defaults to 1. |
| `goto_file_absolute(N)` | Go to the 1-based position N. |
| `goto_file_marked(N)` | Move N steps through marked entries only. From an unmarked entry, go to the first marked one (the last when N is negative). |
| `goto_file_random` | Go to a random entry other than the current one. |
| `goto_file_first` | Go to the first entry. |
| `goto_file_last` | Go to the last entry. |
| `goto_file_by_name(NAME)` | Jump to the entry with this display name. Alias: `goto_file_byname`. |
| `goto_directory_relative(N)` | Go to the first entry of the Nth next (or previous, if negative) directory. Directories are taken in the order they first appear in the playlist. Follows `--end-of-files-action`. |
| `goto_earlier_file` | Return to the previously visited file. |
| `toggle_mark` | Toggle the mark on the current entry. Alias: `mark`. |
| `mark_all` | Mark all entries. |
| `unmark_all` | Clear all marks. |
| `reset_image` | Reset the view (zoom and pan). |
| `set_zoom(F)` | Set the zoom factor to F (image pixels per window pixel). |
| `zoom_by(F)` | Multiply the zoom by F. Alias: `zoom`. |
| `set_scale_mode(MODE)` | Set the scale mode: `fit`, `1:1`, or `fill`. |
| `cycle_scale_mode` | Cycle fit, 1:1, fill. |
| `toggle_fullscreen` | Toggle fullscreen. |
| `command(CMD)` | Run a shell command; see [Commands](#commands). |
| `remove_from_playlist` | Remove the current entry from the playlist. |
| `delete_file` | Move the current file to the trash and remove it from the playlist. |
| `copy_path` | Print the current file's path to stdout (no clipboard). |
| `toggle_info_overlay` | Toggle the info overlay. |
| `pan(X, Y)` | Pan the image by X, Y pixels. |
| `toggle_status_line` | Toggle the image name and position in the window title. |
| `reload` | Reload the current image from disk. |
| `save_playlist(PATH)` | Write the playlist to PATH. |
| `rotate_left` | Rotate the image 90 degrees counter-clockwise. |
| `rotate_right` | Rotate the image 90 degrees clockwise. |
| `flip_horizontal` | Flip the image horizontally. Alias: `flip_horizontally`. |
| `flip_vertical` | Flip the image vertically. Alias: `flip_vertically`. |
| `add_file(PATH)` | Add a file to the playlist. |
| `remove_file_by_index(N)` | Remove the entry at the 1-based visible position N. Alias: `remove_file_byindex`. |
| `remove_file_by_name(NAME)` | Remove the entry with this display name. Alias: `remove_file_byname`. |
| `output_file_list` | Print the playlist to stdout. |
| `toggle_play` | Start or stop the slideshow. |
| `quit` | Quit. |
| `noop` | Do nothing; still consumes the key sequence. |

## Commands

`command(SHELL)` runs `SHELL` with `sh -c`. The text is run as a shell script, so it can use pipes, quoting and so on. It runs on a background thread, so the window does not block while it runs. Its stdin is empty.

Within the script, these positional parameters are set:

| Parameter | Environment variable | Value |
| --- | --- | --- |
| `$1` | `HQIMV_PATH` | Path of the current file. |
| `$2` | `HQIMV_INDEX` | 1-based playlist position. |
| `$3` | `HQIMV_COUNT` | Total number of playlist entries. |
| (none) | `HQIMV_DIR` | Directory containing the current file. |
| (none) | `HQIMV_NAME` | File name of the current file. |
| (none) | `HQIMV_MARKS` | Number of marked entries. |

`$1`, `$2` and `$3` are passed as arguments, not pasted into the script text. Unquoted occurrences are quoted automatically, and ones inside single quotes are still expanded, so file names with spaces are safe.

If the command prints anything to standard output, it is passed on to hqimv's standard output with trailing whitespace trimmed and one newline added. When the command fails, its exit status and standard error are logged as a warning.

## Exit status

| Code | Meaning |
| --- | --- |
| `0` | Normal exit. |
| `1` | Startup failure: a config file that is not valid TOML, an invalid binding, an unreadable `--playlist-file` or stdin list, an unknown `--format`, a window or Wayland failure, or a single file that cannot be loaded. |
| `2` | Usage error (for example an unknown flag). |

SIGTERM, SIGINT and SIGHUP make hqimv quit normally (saving the playlist if `--save-playlist` is set) and exit with `0`. A second signal exits immediately with `128 + signal number`.
