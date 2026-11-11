# hqimv

A high-quality CPU image viewer for Wayland, with pqiv inspired commands. It presents at
the highest bit depth your compositor offers and resamples on the CPU
(Lanczos3, Mitchell or bilinear).

Formats: Image-rs, Jxl-rs, DDS. Files can be paths, directories, `http(s)` URLs, or `-` for a
list on stdin.

## AI Use Disclosure

Much of the initial code was generated via an AI for POC handling. All code has been human reviewed.
The vast majority of documentation and code comments has been AI generated.

## Build

Needs Rust 1.89+ and `libwayland-client`.

```sh
cargo build --release        # target/release/hqimv
./build-release.sh           # same, with build-machine paths removed
```

## Usage

```sh
hqimv photo.jpg
hqimv -r ~/Pictures
hqimv --slideshow --play-delay 3000 --background '#00000000' *.png
```

Run `hqimv --help` for all options, or see [docs/usage.md](docs/usage.md).
Left/Right, `space` and `backspace` step through the playlist, Up/Down zoom,
and `i` toggles the info overlay.

Config is `~/.config/hqimv/config.toml`, written with the default key
bindings on first run. Add bindings with `--bind-key` or in that file, e.g.
`"x { command(rm -- \"$1\") }"`; `$1`, `$2` and `$3` are the path, index and
count.

## License

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
Third-party licences are in `THIRD_PARTY_LICENSES` (regenerate with
`./update-licenses.sh`, needs `cargo-about`).

## Notes

The application does not currently support HDR as cosmic does not currently
support it. Colormanagement is done via moxcms. Image-rs is used for most image 
formats, jxl-rs is used for jxl images. If there is corruption on viewing images
first check image-rs and jxl-rs for corruption.
