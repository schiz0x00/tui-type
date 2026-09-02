# cmd-type implementation map

This file describes the current architecture; it is not a list of aspirational features.

## Runtime flow

1. `cli.rs` parses bounded arguments, rejects conflicting options, and reads bounded custom input without trimming it.
2. `config.rs` loads a versioned configuration from the platform-native directory, applies defaults, and validates the selected mode and theme.
3. `text.rs` validates an explicit word list or loads the compile-time embedded corpora.
4. `main.rs` opens the controlling terminal, enters raw/alternate-screen mode, and blocks on events. Active tests wake at most four times per second for live statistics; idle screens do not redraw continuously.
5. `app.rs` owns the state machine and grapheme-based typing session. Deadlines are checked before events, generated timed/endless targets refill, and all input attempts remain in the score after backspacing.
6. `ui.rs` renders clipped layouts, a resize message on terminals smaller than 32×10, explicit line/tab markers, actual incorrect input, and terminal-cell-aware grapheme wrapping.
7. `stats.rs` saves at most 1,000 results through a cross-process lock and same-directory atomic replacement. Invalid history is preserved before recovery.

## Source layout

```text
src/main.rs    terminal lifecycle and event scheduling
src/cli.rs     arguments, conflicts, and bounded custom input
src/config.rs  portable versioned configuration
src/modes.rs   modes, labels, and numeric limits
src/app.rs     state machine, input attempts, deadlines, refills
src/ui.rs      size-safe Ratatui rendering
src/stats.rs   scoring and durable bounded history
src/text.rs    embedded/explicit corpus validation and sampling
src/themes.rs  accessible built-in palettes
src/sound.rs   feature-gated synthesized click
assets/        embedded MIT-licensed text with provenance
```

## Invariants covered by tests

- A word-count test completes only after the entire generated target.
- Newlines, tabs, combining marks, emoji sequences, and wide glyphs remain typeable and render safely.
- Backspace removes one grapheme but never erases an earlier attempt from statistics.
- A timed test refills before target exhaustion and rejects post-deadline input.
- Tiny terminal sizes cannot create out-of-frame rectangles.
- History corruption is preserved, concurrent appends do not lose records, and loaded history is bounded.
- CLI numeric boundaries, option conflicts, custom whitespace, and embedded asset validity are deterministic.

## Release gates

Formatting, tests, Clippy with warnings denied, documentation, packaging, all-feature compilation, and `cargo audit --deny warnings` run in CI. Rust and the audit tool are pinned; dependency updates are proposed weekly.
