# cmd-type

`cmd-type` is a terminal typing test inspired by [Monkeytype](https://monkeytype.com), built with Rust, Ratatui, and Crossterm.

## Features

- Timed, word-count, quote, custom-text, and endless tests
- Live WPM and accuracy with correct, incorrect, and pending graphemes
- Multiline, tab, combining-mark, emoji-sequence, and double-width text support
- Six built-in themes whose ordinary text colors meet WCAG AA contrast
- Locked, atomic history persistence with local-time display
- Optional synthesized keystroke sound without audio decoder dependencies

## Install

```bash
cargo build --release --locked
install -m 755 target/release/cmd-type ~/.local/bin/cmd-type
```

For sound on Debian or Ubuntu, install ALSA's development package and enable the feature:

```bash
sudo apt-get install libasound2-dev
cargo build --release --locked --features sound
```

If `sound = true` is configured but the feature was not compiled, or no audio device can be initialized, startup returns an actionable error.

## Usage

```text
Usage: cmd-type [OPTIONS]

Options:
      --mode <MODE>       timed, words, quote, custom, or endless
      --words <WORDS>     Words-mode count: 1..=10000 (default 25)
      --time <TIME>       Timed-mode seconds: 1..=3600 (default 30)
      --theme <THEME>     Built-in theme name
      --file <PATH>       Custom UTF-8 text, at most 1 MiB
      --wordlist <PATH>   UTF-8 words for generated modes, at most 1 MiB
      --history           Print the latest 20 results and exit
  -h, --help              Print help
  -V, --version           Print version
```

Examples:

```bash
cmd-type --mode timed --time 30
cmd-type --mode words --words 100
cmd-type --file ./practice.txt
cmd-type --mode endless --wordlist ./words.txt
printf 'first line\n\tsecond line  ' | cmd-type --mode custom
cmd-type --history
```

Piped custom text is read before the UI opens; on Unix, keyboard input and rendering then use the controlling terminal (`/dev/tty`). Consequently the pipe form must be launched from an interactive terminal. Leading and trailing whitespace is preserved, while CRLF is normalized to LF.

Conflicting options fail explicitly. In particular, `--time` requires `--mode timed`, `--words` requires `--mode words`, and a word list cannot be combined with quote or custom mode. `--wordlist` by itself customizes interactive selection but does not automatically start the configured mode.

## Keys

| Screen | Key | Action |
|---|---|---|
| Menus | arrows or `j`/`k` | Navigate |
| Menus | `Enter` | Select or start |
| Typing | `Backspace` | Remove one grapheme |
| Endless typing | `Enter` | Finish and save |
| Typing | `Esc` | Return to menu |
| Results | `Tab` or `Enter` | Start a new randomized test |
| Results | `q` | Quit |
| History | `Esc` or `q` | Return to menu |
| Anywhere | `Ctrl+C` | Quit |

`q` is ordinary typeable text on the typing screen; it is not a global quit key.

## Configuration and history

Configuration follows the operating system's standard application directory. On Linux this is `$XDG_CONFIG_HOME/cmd-type/config.toml`, or `$HOME/.config/cmd-type/config.toml` when `XDG_CONFIG_HOME` is unset. macOS and Windows use their native conventions.

```toml
schema_version = 1
theme = "catppuccin_mocha"
sound = false

[default_mode]
Timed = 30
```

Missing fields receive defaults. Unknown themes, unsupported schema versions, invalid ranges, and malformed TOML are reported rather than silently replaced.

Each completed test attempts to save to `history.json`. Save failures are shown on the results screen and never overwrite existing history. The file is capped at 1,000 results; the UI and `--history` show the latest 20. Corrupt data is reported on startup and preserved under a unique backup name when the next result is saved. Timestamps are stored as UTC RFC 3339 values and displayed in local time with a timezone abbreviation.

## Text assets

The default word and practice-sentence corpora are embedded in the executable, so installed binaries never depend on a build-machine source path. Use `--wordlist` for a runtime override. See [`assets/README.md`](assets/README.md) for provenance and licensing.

## Development

The repository pins Rust in `rust-toolchain.toml` and commits `Cargo.lock` for reproducible application builds.

```bash
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo doc --no-deps --document-private-items --locked
cargo audit --deny warnings
cargo package --locked
```

CI additionally builds and tests the optional sound feature after installing ALSA headers. Dependabot checks Cargo and GitHub Actions dependencies weekly. See [`SECURITY.md`](SECURITY.md) for the dependency and data-integrity policy.

## License

MIT. See [`LICENSE`](LICENSE).
