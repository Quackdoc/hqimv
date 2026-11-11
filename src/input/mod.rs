//! Key bindings: parsing `--bind-key` specs and matching key presses.

pub mod actions;
pub mod bindings;
pub mod parser;

pub use actions::{Action, CommandContext, quote_positionals};
pub use bindings::{BindingTable, Match, SEQUENCE_TIMEOUT};
pub use parser::Chord;

pub fn default_bindings() -> Vec<&'static str> {
    vec![
        "<Right> { goto_file_relative(1) }",
        "<Left> { goto_file_relative(-1) }",
        "<Down> { zoom_by(0.8) }",
        "<Up> { zoom_by(1.25) }",
        "<Shift-Left> { pan(-10, 0) }",
        "<Shift-Right> { pan(10, 0) }",
        "<Shift-Up> { pan(0, -10) }",
        "<Shift-Down> { pan(0, 10) }",
        "<PageDown> { goto_file_relative(5) }",
        "<PageUp> { goto_file_relative(-5) }",
        "<Home> { goto_file_first }",
        "<End> { goto_file_last }",
        "r { goto_file_random }",
        "<space> { goto_file_relative(1) }",
        "<backspace> { goto_file_relative(-1) }",
        "m { toggle_mark }",
        "M { mark_all }",
        "<Ctrl-m> { unmark_all }",
        "<Ctrl-j> { goto_file_marked(1) }",
        "<Ctrl-k> { goto_file_marked(-1) }",
        "u { goto_earlier_file }",
        "o { output_file_list }",
        "k { rotate_right }",
        "l { rotate_left }",
        "h { flip_horizontally }",
        "v { flip_vertically }",
        "<plus> { zoom_by(1.25) }",
        "<equal> { zoom_by(1.25) }",
        "<minus> { zoom_by(0.8) }",
        "0 { reset_image }",
        "f { cycle_scale_mode }",
        "1 { set_scale_mode(1:1) }",
        "d { remove_from_playlist }",
        "R { reload }",
        "<f11> { toggle_fullscreen }",
        "i { toggle_info_overlay }",
        "y { copy_path }",
        "q { quit }",
        "<Escape> { quit }",
        "<Ctrl-q> { quit }",
    ]
}

pub fn default_table() -> BindingTable {
    let mut table = BindingTable::default();
    for spec in default_bindings() {
        // Defaults are constants; a failure is a bug here, so panic.
        table
            .insert_spec(spec)
            .unwrap_or_else(|err| panic!("built-in binding `{spec}` is invalid: {err}"));
    }
    table
}
