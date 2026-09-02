use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fs4::FileExt;
use serde::{Deserialize, Serialize};

use crate::modes::Mode;

pub const MAX_HISTORY_ENTRIES: usize = 1_000;
pub const MAX_HISTORY_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub wpm: f64,
    pub raw_wpm: f64,
    pub accuracy: f64,
    pub correct_chars: usize,
    pub incorrect_chars: usize,
    pub total_chars: usize,
    pub duration: f64,
    pub mode: Mode,
    pub timestamp: DateTime<Utc>,
}

impl TestResult {
    pub fn from_counts(correct: usize, incorrect: usize, duration: f64, mode: &Mode) -> Self {
        let total = correct.saturating_add(incorrect);
        let minutes = duration / 60.0;
        let wpm = if minutes > 0.0 {
            (correct as f64 / 5.0) / minutes
        } else {
            0.0
        };
        let raw_wpm = if minutes > 0.0 {
            (total as f64 / 5.0) / minutes
        } else {
            0.0
        };
        let accuracy = if total > 0 {
            (correct as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        TestResult {
            wpm,
            raw_wpm,
            accuracy,
            correct_chars: correct,
            incorrect_chars: incorrect,
            total_chars: total,
            duration,
            mode: mode.clone(),
            timestamp: Utc::now(),
        }
    }

    fn validate(&self) -> io::Result<()> {
        if !self.wpm.is_finite()
            || !self.raw_wpm.is_finite()
            || !self.accuracy.is_finite()
            || !self.duration.is_finite()
            || self.wpm < 0.0
            || self.raw_wpm < 0.0
            || !(0.0..=100.0).contains(&self.accuracy)
            || self.duration < 0.0
            || self.total_chars != self.correct_chars.saturating_add(self.incorrect_chars)
        {
            return invalid("history contains an invalid test result");
        }
        self.mode
            .validate()
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))
    }
}

#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: PathBuf,
}

#[derive(Debug)]
pub struct SaveOutcome {
    pub history: Vec<TestResult>,
    pub recovered_backup: Option<PathBuf>,
}

impl HistoryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> io::Result<Vec<TestResult>> {
        load_history_file(&self.path)
    }

    pub fn append(&self, result: &TestResult) -> io::Result<SaveOutcome> {
        result.validate()?;
        let directory = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("history path has no parent directory"))?;
        fs::create_dir_all(directory)?;

        let lock_path = directory.join("history.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        FileExt::lock(&lock)?;

        let mut recovered_backup = None;
        let mut history = match load_history_file(&self.path) {
            Ok(history) => history,
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                let backup = quarantine_path(&self.path)?;
                fs::rename(&self.path, &backup).map_err(|rename_error| {
                    io::Error::new(
                        rename_error.kind(),
                        format!(
                            "history is invalid ({error}) and could not be preserved as {}: {rename_error}",
                            backup.display()
                        ),
                    )
                })?;
                recovered_backup = Some(backup);
                Vec::new()
            }
            Err(error) => return Err(error),
        };

        history.push(result.clone());
        if history.len() > MAX_HISTORY_ENTRIES {
            history.drain(..history.len() - MAX_HISTORY_ENTRIES);
        }
        write_history_atomically(directory, &self.path, &history)?;

        Ok(SaveOutcome {
            history,
            recovered_backup,
        })
    }
}

fn load_history_file(path: &Path) -> io::Result<Vec<TestResult>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let size = file.metadata()?.len();
    if size > MAX_HISTORY_BYTES {
        return invalid(format!(
            "{} exceeds the {} byte history limit",
            path.display(),
            MAX_HISTORY_BYTES
        ));
    }

    let mut bytes = Vec::with_capacity(size as usize);
    file.take(MAX_HISTORY_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_HISTORY_BYTES {
        return invalid(format!(
            "{} exceeds the {} byte history limit",
            path.display(),
            MAX_HISTORY_BYTES
        ));
    }
    let mut history: Vec<TestResult> = serde_json::from_slice(&bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not parse {}: {error}", path.display()),
        )
    })?;
    for result in &history {
        result.validate()?;
    }
    if history.len() > MAX_HISTORY_ENTRIES {
        history.drain(..history.len() - MAX_HISTORY_ENTRIES);
    }
    Ok(history)
}

fn write_history_atomically(
    directory: &Path,
    path: &Path,
    history: &[TestResult],
) -> io::Result<()> {
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer_pretty(&mut temporary, history).map_err(io::Error::other)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .map(|_| ())
}

fn quarantine_path(path: &Path) -> io::Result<PathBuf> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("history path has no parent directory"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("history.json");
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    for sequence in 0..1_000 {
        let candidate = directory.join(format!(
            "{file_name}.corrupt-{timestamp}-{}-{sequence}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not choose a unique corrupt-history backup name",
    ))
}

fn invalid<T>(message: impl Into<String>) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;

    #[test]
    fn production_scoring_formula_is_correct() {
        let result = TestResult::from_counts(25, 5, 60.0, &Mode::Timed(60));
        assert!((result.wpm - 5.0).abs() < 0.001);
        assert!((result.raw_wpm - 6.0).abs() < 0.001);
        assert!((result.accuracy - 83.333).abs() < 0.01);
    }

    #[test]
    fn corrupt_history_is_preserved_before_a_new_result_is_saved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        fs::write(&path, b"{ definitely not json").unwrap();
        let store = HistoryStore::new(path.clone());
        let outcome = store
            .append(&TestResult::from_counts(1, 0, 1.0, &Mode::Quote))
            .unwrap();
        let backup = outcome.recovered_backup.unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "{ definitely not json");
        assert_eq!(store.load().unwrap().len(), 1);
    }

    #[test]
    fn concurrent_appends_do_not_lose_results() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(HistoryStore::new(directory.path().join("history.json")));
        let barrier = Arc::new(Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    store
                        .append(&TestResult::from_counts(1, 0, 1.0, &Mode::Quote))
                        .unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(store.load().unwrap().len(), 8);
    }

    #[test]
    fn history_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(directory.path().join("history.json"));
        let sample = TestResult::from_counts(1, 0, 1.0, &Mode::Quote);
        let oversized = vec![sample; MAX_HISTORY_ENTRIES + 10];
        write_history_atomically(directory.path(), store.path(), &oversized).unwrap();
        assert_eq!(store.load().unwrap().len(), MAX_HISTORY_ENTRIES);
    }
}
