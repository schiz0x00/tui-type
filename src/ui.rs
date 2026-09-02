use chrono::Local;
use ratatui::prelude::*;
use ratatui::widgets::*;
use unicode_width::UnicodeWidthStr;

use crate::app::{typing_counts, App, AppState, TypingState};
use crate::modes::{Mode, ModeConfig};

const MIN_WIDTH: u16 = 32;
const MIN_HEIGHT: u16 = 10;
const TAB_STOP: usize = 4;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.is_empty() {
        return;
    }
    frame.render_widget(Block::default().bg(app.theme.bg), area);
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_small_terminal(frame, app, area);
        return;
    }

    match app.state {
        AppState::Menu => render_menu(frame, app, area),
        AppState::ModeSelect => render_mode_select(frame, app, area),
        AppState::ThemeSelect => render_theme_select(frame, app, area),
        AppState::Typing => {
            if let Some(typing) = &app.typing {
                render_typing(frame, app, typing, area);
            }
        }
        AppState::Results => {
            if let Some(result) = &app.last_result {
                render_results(frame, app, result, area);
            }
        }
        AppState::History => render_history(frame, app, area),
        AppState::Quit => {}
    }
}

fn render_small_terminal(frame: &mut Frame, app: &App, area: Rect) {
    let message = format!(
        "cmd-type\n\nTerminal: {}×{}\nNeed at least {MIN_WIDTH}×{MIN_HEIGHT}\n\nResize or press Esc.",
        area.width, area.height
    );
    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(app.theme.fg).bg(app.theme.bg))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn render_menu(frame: &mut Frame, app: &App, area: Rect) {
    let notice_height = u16::from(app.notice.is_some()) * 2;
    let chunks = Layout::vertical([
        Constraint::Min(4),
        Constraint::Length(notice_height),
        Constraint::Length(1),
    ])
    .split(area);

    let labels = [
        "Start Test".to_string(),
        format!("Mode [{}]", app.mode.short_label()),
        format!("Theme [{}]", app.theme.name),
        "History".to_string(),
    ];
    let items: Vec<ListItem> = labels
        .into_iter()
        .map(|label| ListItem::new(Line::from(format!("  {label}  "))).style(fg(app)))
        .collect();
    let list = List::new(items)
        .block(titled_block(app, " cmd-type "))
        .highlight_style(selected(app))
        .highlight_symbol("› ")
        .repeat_highlight_symbol(true);
    let mut state = ListState::default().with_selected(Some(app.menu_index));
    frame.render_stateful_widget(list, chunks[0], &mut state);

    if let Some(notice) = &app.notice {
        frame.render_widget(
            Paragraph::new(notice.as_str())
                .style(Style::default().fg(app.theme.incorrect))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
            chunks[1],
        );
    }
    frame.render_widget(
        Paragraph::new("↑/↓ or j/k navigate  •  Enter select  •  q quit")
            .style(subtle(app))
            .alignment(Alignment::Center),
        chunks[2],
    );
}

fn render_mode_select(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).split(area);
    let items: Vec<ListItem> = Mode::all()
        .into_iter()
        .map(|mode| ListItem::new(format!("  {}  ", mode.label())).style(fg(app)))
        .collect();
    let list = List::new(items)
        .block(titled_block(app, " Select Mode "))
        .highlight_style(selected(app))
        .highlight_symbol("› ");
    let mut state = ListState::default().with_selected(Some(app.mode_index));
    frame.render_stateful_widget(list, chunks[0], &mut state);
    frame.render_widget(
        centered_hint(app, "↑/↓ or j/k navigate  •  Enter select  •  Esc back"),
        chunks[1],
    );
}

fn render_theme_select(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).split(area);
    let items: Vec<ListItem> = crate::themes::Theme::all()
        .into_iter()
        .map(|theme| {
            ListItem::new(format!("  {}  ", theme.name))
                .style(Style::default().fg(theme.accent).bg(app.theme.bg))
        })
        .collect();
    let list = List::new(items)
        .block(titled_block(app, " Select Theme "))
        .highlight_style(selected(app))
        .highlight_symbol("› ");
    let mut state = ListState::default().with_selected(Some(app.theme_index));
    frame.render_stateful_widget(list, chunks[0], &mut state);
    frame.render_widget(
        centered_hint(app, "↑/↓ or j/k navigate  •  Enter select  •  Esc back"),
        chunks[1],
    );
}

fn render_typing(frame: &mut Frame, app: &App, typing: &TypingState, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(3),
    ])
    .split(area);
    render_typing_header(frame, app, typing, chunks[0]);
    render_typing_body(frame, app, typing, chunks[1]);
    render_typing_footer(frame, app, typing, chunks[2]);
}

fn render_typing_header(frame: &mut Frame, app: &App, typing: &TypingState, area: Rect) {
    let elapsed = typing.elapsed().as_secs_f64();
    let (wpm, accuracy) = compute_live_stats(typing, elapsed);
    let mode = match typing.mode_config {
        ModeConfig::Timed(seconds) => {
            format!("time {:.1}s", (f64::from(seconds) - elapsed).max(0.0))
        }
        ModeConfig::Words(total) => {
            format!("words {}/{}", typing.completed_words().min(total), total)
        }
        _ => app.mode.label(),
    };
    let speed = wpm.map_or_else(|| "-- wpm".to_string(), |value| format!("{value:.1} wpm"));
    let line = Line::from(vec![
        Span::styled(format!(" {mode} "), accent_bold(app)),
        Span::raw("  "),
        Span::styled(speed, Style::default().fg(app.theme.correct)),
        Span::raw("  "),
        Span::styled(
            format!("{accuracy:.1}%"),
            Style::default().fg(app.theme.accent),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .block(plain_block(app))
            .alignment(Alignment::Center),
        area,
    );
}

fn render_typing_body(frame: &mut Frame, app: &App, typing: &TypingState, area: Rect) {
    let block = plain_block(app).padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    let lines = visible_typing_lines(app, typing, inner.width as usize, inner.height as usize);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn visible_typing_lines(
    app: &App,
    typing: &TypingState,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let width = width.max(1);
    let height = height.max(1);
    let cursor = typing.input.len().min(typing.target.len());
    let capacity = width.saturating_mul(height).max(1);
    let start = cursor.saturating_sub(capacity.saturating_mul(2));
    let end = cursor
        .saturating_add(capacity.saturating_mul(3))
        .min(typing.target.len());

    let mut rows: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut column = 0usize;
    let mut cursor_row = 0usize;

    for index in start..end {
        let expected = &typing.target[index];
        let actual = typing.input.get(index);
        let value = actual.unwrap_or(expected);
        let style = if let Some(actual) = actual {
            if actual == expected {
                Style::default().fg(app.theme.correct)
            } else {
                Style::default()
                    .fg(app.theme.incorrect)
                    .add_modifier(Modifier::UNDERLINED)
            }
        } else if index == cursor {
            Style::default()
                .fg(app.theme.cursor)
                .add_modifier(Modifier::REVERSED)
        } else {
            subtle(app)
        };

        let is_newline = value == "\n";
        let display = if is_newline {
            "↵".to_string()
        } else if value == "\t" {
            let cells = TAB_STOP - (column % TAB_STOP);
            format!("⇥{}", " ".repeat(cells.saturating_sub(1)))
        } else {
            value.clone()
        };
        let display_width = UnicodeWidthStr::width(display.as_str()).max(1);
        if !is_newline && column > 0 && column.saturating_add(display_width) > width {
            rows.push(Vec::new());
            column = 0;
        }
        if index == cursor {
            cursor_row = rows.len() - 1;
        }
        rows.last_mut()
            .expect("typing rows always has one row")
            .push(Span::styled(display, style));
        column = column.saturating_add(display_width);
        if is_newline {
            rows.push(Vec::new());
            column = 0;
        }
    }

    if cursor == typing.target.len() {
        if column >= width {
            rows.push(Vec::new());
        }
        cursor_row = rows.len() - 1;
        rows.last_mut()
            .expect("typing rows always has one row")
            .push(Span::styled(
                " ",
                Style::default()
                    .fg(app.theme.cursor)
                    .add_modifier(Modifier::REVERSED),
            ));
    }

    let scroll = cursor_row.saturating_sub(height / 3);
    rows.into_iter()
        .skip(scroll)
        .take(height)
        .map(Line::from)
        .collect()
}

fn render_typing_footer(frame: &mut Frame, app: &App, typing: &TypingState, area: Rect) {
    let hint = if matches!(typing.mode_config, ModeConfig::Endless) {
        "Type to test  •  Enter finish  •  Esc menu"
    } else {
        "Type to test  •  Backspace correct  •  Esc menu"
    };
    frame.render_widget(
        Paragraph::new(hint)
            .style(subtle(app))
            .block(plain_block(app))
            .alignment(Alignment::Center),
        area,
    );
}

fn render_results(frame: &mut Frame, app: &App, result: &crate::stats::TestResult, area: Rect) {
    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("wpm         ", subtle_bold(app)),
            Span::styled(
                format!("{:.1}", result.wpm),
                Style::default().fg(app.theme.correct),
            ),
        ]),
        Line::from(vec![
            Span::styled("raw wpm     ", subtle_bold(app)),
            Span::styled(
                format!("{:.1}", result.raw_wpm),
                Style::default().fg(app.theme.accent),
            ),
        ]),
        Line::from(vec![
            Span::styled("accuracy    ", subtle_bold(app)),
            Span::styled(
                format!("{:.1}%", result.accuracy),
                Style::default().fg(app.theme.accent),
            ),
        ]),
        Line::from(vec![
            Span::styled("keystrokes  ", subtle_bold(app)),
            Span::styled(
                format!(
                    "{} correct, {} incorrect",
                    result.correct_chars, result.incorrect_chars
                ),
                fg(app),
            ),
        ]),
        Line::from(vec![
            Span::styled("time        ", subtle_bold(app)),
            Span::styled(format!("{:.2}s", result.duration), fg(app)),
        ]),
    ];
    if let Some(notice) = &app.notice {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            notice.clone(),
            Style::default().fg(app.theme.incorrect),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(titled_block(app, " Results "))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true }),
        chunks[0],
    );
    frame.render_widget(
        centered_hint(app, "Tab/Enter new test  •  Esc menu  •  q quit"),
        chunks[1],
    );
}

fn render_history(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);
    let mut lines = vec![Line::styled(
        "WPM      ACC       MODE       LOCAL TIME",
        subtle_bold(app),
    )];
    if app.history.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::styled("No test history yet.", subtle(app)));
    } else {
        for result in app
            .history
            .iter()
            .rev()
            .take(20.min(chunks[0].height.saturating_sub(3) as usize))
        {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<8.1} ", result.wpm),
                    Style::default().fg(app.theme.correct),
                ),
                Span::styled(
                    format!("{:<9.1}% ", result.accuracy),
                    Style::default().fg(app.theme.accent),
                ),
                Span::styled(format!("{:<10} ", result.mode.short_label()), fg(app)),
                Span::styled(
                    result
                        .timestamp
                        .with_timezone(&Local)
                        .format("%Y-%m-%d %H:%M %Z")
                        .to_string(),
                    subtle(app),
                ),
            ]));
        }
    }
    if let Some(notice) = &app.notice {
        lines.push(Line::styled(
            notice.clone(),
            Style::default().fg(app.theme.incorrect),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(titled_block(app, " History "))
            .wrap(Wrap { trim: true }),
        chunks[0],
    );
    frame.render_widget(centered_hint(app, "Esc or q back to menu"), chunks[1]);
}

fn compute_live_stats(typing: &TypingState, elapsed: f64) -> (Option<f64>, f64) {
    let (correct, incorrect) = typing_counts(typing);
    let total = correct + incorrect;
    let accuracy = if total == 0 {
        100.0
    } else {
        correct as f64 / total as f64 * 100.0
    };
    let wpm = (elapsed >= 1.0).then(|| (correct as f64 / 5.0) / (elapsed / 60.0));
    (wpm, accuracy)
}

fn titled_block<'a>(app: &App, title: &'a str) -> Block<'a> {
    plain_block(app).title(title).title_style(accent_bold(app))
}

fn plain_block(app: &App) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border))
        .bg(app.theme.bg)
}

fn centered_hint<'a>(app: &App, text: &'a str) -> Paragraph<'a> {
    Paragraph::new(text)
        .style(subtle(app))
        .alignment(Alignment::Center)
}

fn fg(app: &App) -> Style {
    Style::default().fg(app.theme.fg).bg(app.theme.bg)
}

fn subtle(app: &App) -> Style {
    Style::default().fg(app.theme.subtle).bg(app.theme.bg)
}

fn subtle_bold(app: &App) -> Style {
    subtle(app).add_modifier(Modifier::BOLD)
}

fn accent_bold(app: &App) -> Style {
    Style::default()
        .fg(app.theme.accent)
        .bg(app.theme.bg)
        .add_modifier(Modifier::BOLD)
}

fn selected(app: &App) -> Style {
    Style::default()
        .fg(app.theme.bg)
        .bg(app.theme.accent)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::stats::{HistoryStore, TestResult};
    use crate::text::TextGenerator;
    use ratatui::backend::TestBackend;

    fn app() -> (App, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let app = App::new(
            &Config::default(),
            Some("wide 世界\n\tline 👩‍💻".to_string()),
            TextGenerator::new(None).unwrap(),
            HistoryStore::new(directory.path().join("history.json")),
        )
        .unwrap();
        (app, directory)
    }

    fn draw_at(app: &App, width: u16, height: u16) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
    }

    #[test]
    fn every_screen_is_safe_on_tiny_terminals() {
        let (mut app, _directory) = app();
        let states = [
            AppState::Menu,
            AppState::ModeSelect,
            AppState::ThemeSelect,
            AppState::Typing,
            AppState::Results,
            AppState::History,
        ];
        app.start_test();
        app.last_result = Some(TestResult::from_counts(1, 1, 1.0, &Mode::Timed(30)));
        for state in states {
            app.state = state;
            draw_at(&app, 80, 5);
            draw_at(&app, 80, 8);
            draw_at(&app, 1, 1);
        }
    }

    #[test]
    fn grapheme_and_cell_width_rendering_is_safe() {
        let (mut app, _directory) = app();
        app.mode = Mode::Custom;
        app.start_test();
        draw_at(&app, 40, 12);
    }

    #[test]
    fn live_wpm_is_suppressed_during_the_first_second() {
        let (mut app, _directory) = app();
        app.start_test();
        assert_eq!(
            compute_live_stats(app.typing.as_ref().unwrap(), 0.01).0,
            None
        );
    }
}
