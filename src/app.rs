use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

use crate::config::Config;
use crate::modes::{Mode, ModeConfig};
use crate::sound::Ticker;
use crate::stats::{HistoryStore, TestResult};
use crate::text::TextGenerator;
use crate::themes::Theme;

const REFILL_THRESHOLD: usize = 300;
const REFILL_WORDS: usize = 100;
const BUFFER_HIGH_WATER: usize = 800;
const BUFFER_RETAIN: usize = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppState {
    Menu,
    ModeSelect,
    ThemeSelect,
    Typing,
    Results,
    History,
    Quit,
}

pub struct App {
    pub state: AppState,
    pub theme: Theme,
    /// Theme shown while browsing the theme list. Set from arrow keys,
    /// promoted to `theme` on Enter, and dropped on Esc so the committed
    /// theme is restored. Always `None` outside `AppState::ThemeSelect`.
    pub preview_theme: Option<Theme>,
    pub mode: Mode,
    pub menu_index: usize,
    pub mode_index: usize,
    pub theme_index: usize,
    pub typing: Option<TypingState>,
    pub last_result: Option<TestResult>,
    pub history: Vec<TestResult>,
    pub custom_text: Option<String>,
    pub sound_enabled: bool,
    pub notice: Option<String>,
    pub text_generator: TextGenerator,
    ticker: Ticker,
    history_store: HistoryStore,
}

pub struct TypingState {
    /// Extended grapheme clusters, not Unicode scalar values.
    pub target: Vec<String>,
    /// The currently visible/editable input buffer, also stored as graphemes.
    pub input: Vec<String>,
    pub start_time: Option<Instant>,
    pub mode_config: ModeConfig,
    correct_attempts: usize,
    incorrect_attempts: usize,
    /// Correctness for input still in the editable buffer. This lets a
    /// combining scalar update its grapheme's attempt without retaining an
    /// unbounded event log in endless mode.
    input_attempt_correct: Vec<bool>,
}

impl TypingState {
    pub fn elapsed(&self) -> Duration {
        self.start_time
            .map_or(Duration::ZERO, |start| start.elapsed())
    }

    pub fn completed_words(&self) -> usize {
        let cursor = self.input.len().min(self.target.len());
        let mut words = 0;
        let mut in_word = false;
        for grapheme in &self.target[..cursor] {
            if grapheme.chars().all(char::is_whitespace) {
                if in_word {
                    words += 1;
                    in_word = false;
                }
            } else {
                in_word = true;
            }
        }
        if cursor == self.target.len() && in_word {
            words += 1;
        }
        words
    }

    pub fn deadline_remaining(&self) -> Option<Duration> {
        let ModeConfig::Timed(seconds) = self.mode_config else {
            return None;
        };
        let duration = Duration::from_secs(u64::from(seconds));
        Some(duration.saturating_sub(self.elapsed()))
    }

    fn deadline_reached(&self) -> bool {
        self.deadline_remaining()
            .is_some_and(|remaining| remaining.is_zero() && self.start_time.is_some())
    }

    fn ensure_started(&mut self) {
        self.start_time.get_or_insert_with(Instant::now);
    }

    fn push_token(&mut self, token: String) {
        self.ensure_started();
        let position = self.input.len();
        let correct = self.target.get(position) == Some(&token);
        if correct {
            self.correct_attempts = self.correct_attempts.saturating_add(1);
        } else {
            self.incorrect_attempts = self.incorrect_attempts.saturating_add(1);
        }
        self.input.push(token);
        self.input_attempt_correct.push(correct);
    }

    fn push_scalar(&mut self, scalar: char) {
        self.ensure_started();
        let normalized: String = scalar.to_string().nfd().collect();
        if let Some(last) = self.input.last_mut() {
            let mut combined = last.clone();
            combined.push_str(&normalized);
            if UnicodeSegmentation::graphemes(combined.as_str(), true).count() == 1 {
                *last = combined;
                let position = self.input.len() - 1;
                let was_correct = self.input_attempt_correct[position];
                let is_correct = self.target.get(position) == self.input.get(position);
                if was_correct != is_correct {
                    if is_correct {
                        self.incorrect_attempts = self.incorrect_attempts.saturating_sub(1);
                        self.correct_attempts = self.correct_attempts.saturating_add(1);
                    } else {
                        self.correct_attempts = self.correct_attempts.saturating_sub(1);
                        self.incorrect_attempts = self.incorrect_attempts.saturating_add(1);
                    }
                    self.input_attempt_correct[position] = is_correct;
                }
                return;
            }
        }
        self.push_token(normalized);
    }

    fn backspace(&mut self) {
        self.input.pop();
        self.input_attempt_correct.pop();
        // Attempts deliberately remain: corrections must not erase mistakes
        // from accuracy and raw-speed statistics.
    }

    fn is_fixed_target_complete(&self) -> bool {
        if !matches!(
            self.mode_config,
            ModeConfig::Words(_) | ModeConfig::Quote | ModeConfig::Custom
        ) || self.input.len() < self.target.len()
        {
            return false;
        }
        if self.input.len() > self.target.len() {
            return true;
        }

        // A terminal emits an emoji ZWJ sequence or decomposed accent as
        // several scalar key events. If the final input grapheme is still a
        // prefix of the target grapheme, wait for the rest of that cluster.
        let Some((actual, expected)) = self.input.last().zip(self.target.last()) else {
            return true;
        };
        actual == expected || !expected.starts_with(actual)
    }
}

impl App {
    pub fn new(
        config: &Config,
        custom_text: Option<String>,
        text_generator: TextGenerator,
        history_store: HistoryStore,
    ) -> io::Result<Self> {
        config.validate()?;
        let theme = Theme::by_name(&config.theme)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown theme {:?}", config.theme),
                )
            })?
            .clone();
        let mode = config.default_mode.clone();
        let mode_index = Mode::all()
            .iter()
            .position(|candidate| candidate == &mode)
            .unwrap_or(0);
        let theme_index = Theme::all()
            .iter()
            .position(|candidate| candidate.name == theme.name)
            .unwrap_or(0);
        let ticker = if config.sound {
            Ticker::new()?
        } else {
            Ticker::disabled()
        };
        let (history, notice) = match history_store.load() {
            Ok(history) => (history, None),
            Err(error) => (
                Vec::new(),
                Some(format!(
                    "History could not be read and was left untouched: {error}"
                )),
            ),
        };

        Ok(App {
            state: AppState::Menu,
            theme,
            preview_theme: None,
            mode,
            menu_index: 0,
            mode_index,
            theme_index,
            typing: None,
            last_result: None,
            history,
            custom_text,
            sound_enabled: config.sound,
            notice,
            text_generator,
            ticker,
            history_store,
        })
    }

    /// Theme that should drive rendering right now: the live preview when
    /// browsing, otherwise the committed theme.
    pub fn active_theme(&self) -> &Theme {
        self.preview_theme.as_ref().unwrap_or(&self.theme)
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.state {
            AppState::Menu => self.handle_menu_key(key),
            AppState::ModeSelect => self.handle_mode_select_key(key),
            AppState::ThemeSelect => self.handle_theme_select_key(key),
            AppState::Typing => self.handle_typing_key(key),
            AppState::Results => self.handle_results_key(key),
            AppState::History => self.handle_history_key(key),
            AppState::Quit => {}
        }
    }

    fn handle_menu_key(&mut self, key: KeyEvent) {
        const ITEMS: usize = 4;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.state = AppState::Quit,
            KeyCode::Up | KeyCode::Char('k') => {
                self.menu_index = (self.menu_index + ITEMS - 1) % ITEMS;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.menu_index = (self.menu_index + 1) % ITEMS;
            }
            KeyCode::Enter => match self.menu_index {
                0 => {
                    self.start_test();
                }
                1 => self.state = AppState::ModeSelect,
                2 => self.state = AppState::ThemeSelect,
                3 => self.state = AppState::History,
                _ => {}
            },
            _ => {}
        }
    }

    fn handle_mode_select_key(&mut self, key: KeyEvent) {
        let items = Mode::all().len();
        match key.code {
            KeyCode::Esc => self.state = AppState::Menu,
            KeyCode::Up | KeyCode::Char('k') => {
                self.mode_index = (self.mode_index + items - 1) % items;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.mode_index = (self.mode_index + 1) % items;
            }
            KeyCode::Enter => {
                self.mode = Mode::all()[self.mode_index].clone();
                self.state = AppState::Menu;
            }
            _ => {}
        }
    }

    fn handle_theme_select_key(&mut self, key: KeyEvent) {
        let palette = Theme::all();
        let items = palette.len();
        match key.code {
            KeyCode::Esc => {
                self.preview_theme = None;
                self.state = AppState::Menu;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.theme_index = (self.theme_index + items - 1) % items;
                self.preview_theme = Some(palette[self.theme_index].clone());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.theme_index = (self.theme_index + 1) % items;
                self.preview_theme = Some(palette[self.theme_index].clone());
            }
            KeyCode::Enter => {
                self.theme = self
                    .preview_theme
                    .take()
                    .unwrap_or_else(|| palette[self.theme_index].clone());
                self.state = AppState::Menu;
            }
            _ => {}
        }
    }

    fn handle_typing_key(&mut self, key: KeyEvent) {
        if self
            .typing
            .as_ref()
            .is_some_and(TypingState::deadline_reached)
        {
            self.finish_test();
            return;
        }

        if key.code == KeyCode::Esc {
            self.state = AppState::Menu;
            return;
        }

        let endless = self
            .typing
            .as_ref()
            .is_some_and(|typing| matches!(typing.mode_config, ModeConfig::Endless));
        if endless && key.code == KeyCode::Enter {
            if self
                .typing
                .as_ref()
                .is_some_and(|typing| typing.start_time.is_some())
            {
                self.finish_test();
            }
            return;
        }

        let mut inserted = false;
        if let Some(typing) = self.typing.as_mut() {
            match key.code {
                KeyCode::Backspace => typing.backspace(),
                KeyCode::Enter => {
                    typing.push_token("\n".to_string());
                    inserted = true;
                }
                KeyCode::Tab => {
                    typing.push_token("\t".to_string());
                    inserted = true;
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && !character.is_control() =>
                {
                    typing.push_scalar(character);
                    inserted = true;
                }
                _ => {}
            }
        }

        if inserted && self.sound_enabled {
            self.ticker.click();
        }
        self.refill_generated_if_needed();
        if self
            .typing
            .as_ref()
            .is_some_and(TypingState::is_fixed_target_complete)
        {
            self.finish_test();
        }
    }

    fn handle_results_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::Enter => self.start_test(),
            KeyCode::Esc => self.state = AppState::Menu,
            KeyCode::Char('q') => self.state = AppState::Quit,
            _ => {}
        }
    }

    fn handle_history_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            self.state = AppState::Menu;
        }
    }

    pub fn start_test(&mut self) {
        let mode_config = self.mode.to_config();
        let target = match &mode_config {
            ModeConfig::Timed(_) => self.text_generator.random_words(REFILL_WORDS),
            ModeConfig::Words(count) => self.text_generator.random_words(*count),
            ModeConfig::Quote => self.text_generator.random_quote(),
            ModeConfig::Custom => match self.custom_text.clone() {
                Some(text) => text,
                None => {
                    self.notice = Some(
                        "Custom mode requires --file PATH or text piped to --mode custom."
                            .to_string(),
                    );
                    self.state = AppState::Menu;
                    return;
                }
            },
            ModeConfig::Endless => self.text_generator.random_words(REFILL_WORDS),
        };

        self.typing = Some(TypingState {
            target: graphemes(&target),
            input: Vec::new(),
            start_time: None,
            mode_config,
            correct_attempts: 0,
            incorrect_attempts: 0,
            input_attempt_correct: Vec::new(),
        });
        self.state = AppState::Typing;
    }

    /// Advances the deadline before input is handled. Returns true when the
    /// visible application state changed.
    pub fn tick(&mut self) -> bool {
        if self.state == AppState::Typing
            && self
                .typing
                .as_ref()
                .is_some_and(TypingState::deadline_reached)
        {
            self.finish_test();
            true
        } else {
            false
        }
    }

    fn finish_test(&mut self) {
        let Some(typing) = self.typing.as_ref() else {
            return;
        };
        let elapsed = match typing.mode_config {
            ModeConfig::Timed(seconds) => typing.elapsed().as_secs_f64().min(f64::from(seconds)),
            _ => typing.elapsed().as_secs_f64(),
        };
        let (correct, incorrect) = typing_counts(typing);
        let result = TestResult::from_counts(correct, incorrect, elapsed, &self.mode);
        self.last_result = Some(result.clone());
        match self.history_store.append(&result) {
            Ok(outcome) => {
                self.history = outcome.history;
                self.notice = outcome.recovered_backup.map(|backup| {
                    format!(
                        "Invalid history was preserved as {}; this result was saved.",
                        backup.display()
                    )
                });
            }
            Err(error) => {
                self.notice = Some(format!(
                    "This result could not be saved; existing history was left untouched: {error}"
                ));
            }
        }
        self.state = AppState::Results;
    }

    fn refill_generated_if_needed(&mut self) {
        let Some(typing) = self.typing.as_mut() else {
            return;
        };
        if !matches!(
            typing.mode_config,
            ModeConfig::Timed(_) | ModeConfig::Endless
        ) {
            return;
        }

        if typing.target.len().saturating_sub(typing.input.len()) < REFILL_THRESHOLD {
            let suffix = self.text_generator.random_words(REFILL_WORDS);
            if !typing.target.is_empty()
                && !typing
                    .target
                    .last()
                    .is_some_and(|value| value.chars().all(char::is_whitespace))
            {
                typing.target.push(" ".to_string());
            }
            typing.target.extend(graphemes(&suffix));
        }

        if typing.input.len() > BUFFER_HIGH_WATER {
            let drop_count = typing
                .input
                .len()
                .saturating_sub(BUFFER_RETAIN)
                .min(typing.target.len());
            typing.target.drain(..drop_count);
            typing.input.drain(..drop_count);
            typing.input_attempt_correct.drain(..drop_count);
        }
    }
}

pub fn graphemes(text: &str) -> Vec<String> {
    let normalized: String = text.nfd().collect();
    UnicodeSegmentation::graphemes(normalized.as_str(), true)
        .map(str::to_owned)
        .collect()
}

pub fn typing_counts(typing: &TypingState) -> (usize, usize) {
    (typing.correct_attempts, typing.incorrect_attempts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    struct Harness {
        app: App,
        _directory: tempfile::TempDir,
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app_for(mode: Mode, custom_text: Option<&str>) -> Harness {
        let config = Config {
            default_mode: mode,
            ..Config::default()
        };
        let directory = tempfile::tempdir().unwrap();
        let history = HistoryStore::new(directory.path().join("history.json"));
        let app = App::new(
            &config,
            custom_text.map(str::to_owned),
            TextGenerator::new(None).unwrap(),
            history,
        )
        .unwrap();
        Harness {
            app,
            _directory: directory,
        }
    }

    fn type_text(app: &mut App, text: &str) {
        for grapheme in UnicodeSegmentation::graphemes(text, true) {
            match grapheme {
                "\n" => app.handle_key(key(KeyCode::Enter)),
                "\t" => app.handle_key(key(KeyCode::Tab)),
                _ => {
                    for character in grapheme.chars() {
                        app.handle_key(key(KeyCode::Char(character)));
                    }
                }
            }
        }
    }

    #[test]
    fn menu_and_mode_navigation_work() {
        let mut harness = app_for(Mode::Quote, None);
        harness.app.handle_key(key(KeyCode::Down));
        assert_eq!(harness.app.menu_index, 1);
        harness.app.handle_key(key(KeyCode::Enter));
        assert_eq!(harness.app.state, AppState::ModeSelect);
        assert!(Mode::all().contains(&Mode::Custom));
    }

    #[test]
    fn fixed_target_requires_every_grapheme_not_just_the_last_word_start() {
        let mut harness = app_for(Mode::Custom, Some("camp good"));
        harness.app.start_test();
        type_text(&mut harness.app, "camp g");
        assert_eq!(harness.app.state, AppState::Typing);
        type_text(&mut harness.app, "ood");
        assert_eq!(harness.app.state, AppState::Results);
        assert_eq!(harness.app.last_result.as_ref().unwrap().accuracy, 100.0);
    }

    #[test]
    fn multiline_and_tabbed_custom_text_can_be_typed() {
        let mut harness = app_for(Mode::Custom, Some("one\n\ttwo"));
        harness.app.start_test();
        type_text(&mut harness.app, "one\n\ttwo");
        assert_eq!(harness.app.state, AppState::Results);
        assert_eq!(harness.app.last_result.as_ref().unwrap().incorrect_chars, 0);
    }

    #[test]
    fn corrected_errors_remain_in_statistics() {
        let mut harness = app_for(Mode::Custom, Some("abc"));
        harness.app.start_test();
        harness.app.handle_key(key(KeyCode::Char('x')));
        harness.app.handle_key(key(KeyCode::Backspace));
        type_text(&mut harness.app, "abc");
        let result = harness.app.last_result.as_ref().unwrap();
        assert_eq!((result.correct_chars, result.incorrect_chars), (3, 1));
        assert_eq!(result.accuracy, 75.0);
        assert_eq!(harness.app.history.len(), 1);
    }

    #[test]
    fn backspace_removes_an_extended_grapheme_cluster() {
        let mut harness = app_for(Mode::Custom, Some("e\u{301} x"));
        harness.app.start_test();
        harness.app.handle_key(key(KeyCode::Char('e')));
        harness.app.handle_key(key(KeyCode::Char('\u{301}')));
        assert_eq!(harness.app.typing.as_ref().unwrap().input.len(), 1);
        harness.app.handle_key(key(KeyCode::Backspace));
        assert!(harness.app.typing.as_ref().unwrap().input.is_empty());
    }

    #[test]
    fn final_emoji_sequence_waits_for_the_complete_grapheme() {
        let mut harness = app_for(Mode::Custom, Some("👩‍💻"));
        harness.app.start_test();
        harness.app.handle_key(key(KeyCode::Char('👩')));
        assert_eq!(harness.app.state, AppState::Typing);
        harness.app.handle_key(key(KeyCode::Char('\u{200d}')));
        assert_eq!(harness.app.state, AppState::Typing);
        harness.app.handle_key(key(KeyCode::Char('💻')));
        assert_eq!(harness.app.state, AppState::Results);
        assert_eq!(harness.app.last_result.as_ref().unwrap().accuracy, 100.0);
    }

    #[test]
    fn canonically_equivalent_accent_input_matches() {
        let mut harness = app_for(Mode::Custom, Some("é"));
        harness.app.start_test();
        harness.app.handle_key(key(KeyCode::Char('e')));
        assert_eq!(harness.app.state, AppState::Typing);
        harness.app.handle_key(key(KeyCode::Char('\u{301}')));
        assert_eq!(harness.app.state, AppState::Results);
        assert_eq!(harness.app.last_result.as_ref().unwrap().accuracy, 100.0);
    }

    #[test]
    fn timed_target_refills_before_it_can_be_exhausted() {
        let mut harness = app_for(Mode::Timed(30), None);
        harness.app.start_test();
        harness.app.typing.as_mut().unwrap().target = graphemes("a");
        for _ in 0..1_000 {
            harness.app.handle_key(key(KeyCode::Char('a')));
        }
        let typing = harness.app.typing.as_ref().unwrap();
        assert!(typing.target.len() > typing.input.len());
    }

    #[test]
    fn plain_enter_finishes_endless_mode() {
        let mut harness = app_for(Mode::Endless, None);
        harness.app.start_test();
        harness.app.handle_key(key(KeyCode::Char('a')));
        harness.app.handle_key(key(KeyCode::Enter));
        assert_eq!(harness.app.state, AppState::Results);
    }

    #[test]
    fn a_key_after_the_timed_deadline_is_not_counted() {
        let mut harness = app_for(Mode::Timed(1), None);
        harness.app.start_test();
        harness.app.typing.as_mut().unwrap().start_time =
            Some(Instant::now() - Duration::from_secs(2));
        harness.app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(harness.app.state, AppState::Results);
        assert_eq!(harness.app.last_result.as_ref().unwrap().total_chars, 0);
    }

    #[test]
    fn theme_select_previews_each_theme_on_navigation_and_commits_on_enter() {
        let mut harness = app_for(Mode::Timed(30), None);
        let original = harness.app.theme.clone();
        harness.app.state = AppState::ThemeSelect;

        harness.app.handle_key(key(KeyCode::Down));
        assert_eq!(harness.app.state, AppState::ThemeSelect);
        assert!(harness.app.preview_theme.is_some());
        assert_ne!(harness.app.active_theme().name, original.name);
        assert_eq!(harness.app.theme.name, original.name);

        let previewed = harness.app.active_theme().clone();
        harness.app.handle_key(key(KeyCode::Enter));
        assert_eq!(harness.app.state, AppState::Menu);
        assert!(harness.app.preview_theme.is_none());
        assert_eq!(harness.app.theme.name, previewed.name);
    }

    #[test]
    fn theme_select_j_and_k_keys_preview_in_their_directions() {
        let mut harness = app_for(Mode::Timed(30), None);
        let original = harness.app.theme.clone();
        harness.app.state = AppState::ThemeSelect;

        harness.app.handle_key(key(KeyCode::Char('j')));
        let down_target = harness.app.active_theme().name.clone();
        assert_ne!(down_target, original.name);

        harness.app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(harness.app.active_theme().name, original.name);

        harness.app.handle_key(key(KeyCode::Char('k')));
        let wrapped = harness.app.active_theme().name.clone();
        assert_ne!(wrapped, original.name);
        assert_eq!(
            wrapped,
            Theme::all().get(Theme::all().len() - 1).unwrap().name
        );
    }

    #[test]
    fn theme_select_enter_without_arrows_commits_the_highlighted_theme() {
        let mut harness = app_for(Mode::Timed(30), None);
        let original = harness.app.theme.clone();
        harness.app.state = AppState::ThemeSelect;
        harness.app.theme_index = 3;

        harness.app.handle_key(key(KeyCode::Enter));
        assert_eq!(harness.app.state, AppState::Menu);
        assert!(harness.app.preview_theme.is_none());
        assert_eq!(harness.app.theme.name, Theme::all().get(3).unwrap().name);
        assert_ne!(harness.app.theme.name, original.name);
    }

    #[test]
    fn theme_select_reverts_on_escape_without_committing() {
        let mut harness = app_for(Mode::Timed(30), None);
        let original = harness.app.theme.clone();
        harness.app.state = AppState::ThemeSelect;

        harness.app.handle_key(key(KeyCode::Down));
        harness.app.handle_key(key(KeyCode::Down));
        assert_ne!(harness.app.active_theme().name, original.name);

        harness.app.handle_key(key(KeyCode::Esc));
        assert_eq!(harness.app.state, AppState::Menu);
        assert!(harness.app.preview_theme.is_none());
        assert_eq!(harness.app.theme.name, original.name);
    }

    #[test]
    fn preview_theme_is_cleared_when_leaving_theme_select_by_any_key() {
        let mut harness = app_for(Mode::Timed(30), None);
        harness.app.state = AppState::ThemeSelect;
        harness.app.handle_key(key(KeyCode::Down));
        assert!(harness.app.preview_theme.is_some());

        harness.app.handle_key(key(KeyCode::Esc));
        assert!(harness.app.preview_theme.is_none());

        harness.app.state = AppState::ThemeSelect;
        harness.app.handle_key(key(KeyCode::Down));
        harness.app.handle_key(key(KeyCode::Enter));
        assert!(harness.app.preview_theme.is_none());
    }

    #[test]
    fn active_theme_returns_preview_when_set_and_committed_otherwise() {
        let mut harness = app_for(Mode::Timed(30), None);
        let committed = harness.app.theme.clone();
        assert_eq!(harness.app.active_theme().name, committed.name);

        let next = Theme::all()
            .iter()
            .find(|candidate| candidate.name != committed.name)
            .expect("at least one alternative theme")
            .clone();
        harness.app.preview_theme = Some(next.clone());
        assert_eq!(harness.app.active_theme().name, next.name);
    }

    #[test]
    fn repeat_events_are_regular_typing_events() {
        let mut harness = app_for(Mode::Custom, Some("aa"));
        harness.app.start_test();
        let event =
            KeyEvent::new_with_kind(KeyCode::Char('a'), KeyModifiers::NONE, KeyEventKind::Repeat);
        harness.app.handle_key(event);
        assert_eq!(harness.app.typing.as_ref().unwrap().input, ["a"]);
    }
}
