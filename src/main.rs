mod app;
mod cli;
mod config;
mod modes;
mod sound;
mod stats;
mod text;
mod themes;
mod ui;

use std::io::{self, Write};
use std::time::Duration;

use chrono::Local;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use app::{App, AppState};
use cli::Cli;
use config::Config;
use stats::HistoryStore;
use text::TextGenerator;

const ACTIVE_REFRESH_RATE: Duration = Duration::from_millis(250);
const INACTIVE_POLL_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

struct TerminalGuard {
    output: Box<dyn Write>,
}

impl TerminalGuard {
    fn enter(mut output: Box<dyn Write>) -> io::Result<Self> {
        enable_raw_mode().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("could not enable terminal raw mode: {error}"),
            )
        })?;
        if let Err(error) = execute!(output, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { output })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.output, LeaveAlternateScreen);
    }
}

fn main() -> io::Result<()> {
    run()
}

fn run() -> io::Result<()> {
    let cli = Cli::parse();
    cli.validate()?;

    if cli.history {
        return print_history();
    }

    let mut config = Config::load()?;
    cli.apply_overrides(&mut config)?;
    let custom_text = cli.custom_text()?;
    if cli.starts_immediately()
        && matches!(config.default_mode, modes::Mode::Custom)
        && custom_text.is_none()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "custom mode requires --file PATH or piped UTF-8 text",
        ));
    }

    let wordlist = cli.applicable_wordlist(&config.default_mode);
    let text_generator = TextGenerator::new(wordlist)?;
    let history_store = HistoryStore::new(Config::history_path()?);
    let mut app = App::new(&config, custom_text, text_generator, history_store)?;
    if cli.starts_immediately() {
        app.start_test();
    }

    let (terminal_output, guard_output) = terminal_outputs()?;
    let _terminal_guard = TerminalGuard::enter(guard_output)?;
    let backend = CrosstermBackend::new(terminal_output);
    let mut terminal = Terminal::new(backend)?;
    let mut dirty = true;

    loop {
        if app.tick() {
            dirty = true;
        }
        if dirty {
            terminal.draw(|frame| ui::render(frame, &app))?;
            dirty = false;
        }
        if app.state == AppState::Quit {
            break;
        }

        let timeout = next_poll_timeout(&app);
        if event::poll(timeout)? {
            // A deadline always wins over an event that became readable at
            // the same instant. Discard that event instead of counting it or
            // applying it to the newly opened results screen.
            if app.tick() {
                let _ = event::read()?;
                dirty = true;
                continue;
            }

            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        app.state = AppState::Quit;
                    } else {
                        app.handle_key(key);
                    }
                    dirty = true;
                }
                Event::Resize(_, _) => dirty = true,
                _ => {}
            }
        } else if app
            .typing
            .as_ref()
            .is_some_and(|typing| typing.start_time.is_some())
        {
            // Live time and speed change while an active test is otherwise idle.
            dirty = true;
        }
    }

    terminal.show_cursor()?;
    Ok(())
}

fn next_poll_timeout(app: &App) -> Duration {
    let Some(typing) = app
        .typing
        .as_ref()
        .filter(|_| app.state == AppState::Typing)
    else {
        return INACTIVE_POLL_TIMEOUT;
    };
    if typing.start_time.is_none() {
        return INACTIVE_POLL_TIMEOUT;
    }
    typing
        .deadline_remaining()
        .map_or(ACTIVE_REFRESH_RATE, |remaining| {
            remaining.min(ACTIVE_REFRESH_RATE)
        })
}

#[cfg(unix)]
fn terminal_outputs() -> io::Result<(Box<dyn Write>, Box<dyn Write>)> {
    use std::fs::OpenOptions;

    let terminal = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                format!(
                    "an interactive controlling terminal is required (could not open /dev/tty: {error})"
                ),
            )
        })?;
    let restore = terminal.try_clone()?;
    Ok((Box::new(terminal), Box::new(restore)))
}

#[cfg(not(unix))]
fn terminal_outputs() -> io::Result<(Box<dyn Write>, Box<dyn Write>)> {
    Ok((Box::new(io::stdout()), Box::new(io::stdout())))
}

fn print_history() -> io::Result<()> {
    let store = HistoryStore::new(Config::history_path()?);
    let history = store.load().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "could not read history at {}: {error}",
                store.path().display()
            ),
        )
    })?;
    if history.is_empty() {
        println!("No test history yet.");
        return Ok(());
    }
    println!(
        "{:<8} {:<10} {:<12} {:<10} LOCAL TIME",
        "WPM", "ACC", "MODE", "DUR(s)"
    );
    println!("{}", "-".repeat(72));
    for result in history.iter().rev().take(20) {
        println!(
            "{:<8.1} {:<9.1}% {:<12} {:<10.2} {}",
            result.wpm,
            result.accuracy,
            result.mode.short_label(),
            result.duration,
            result
                .timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M %Z"),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_screens_do_not_request_periodic_redraws() {
        let directory = tempfile::tempdir().unwrap();
        let app = App::new(
            &Config::default(),
            None,
            TextGenerator::new(None).unwrap(),
            HistoryStore::new(directory.path().join("history.json")),
        )
        .unwrap();
        assert_eq!(next_poll_timeout(&app), INACTIVE_POLL_TIMEOUT);
    }
}
