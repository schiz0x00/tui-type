use std::process::Command;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cmd-type"))
}

#[test]
fn zero_word_count_exits_unsuccessfully() {
    let output = command()
        .args(["--mode", "words", "--words", "0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("between 1 and 10000"));
}

#[test]
fn irrelevant_wordlist_is_an_explicit_error() {
    let output = command()
        .args(["--mode", "quote", "--wordlist", "does-not-matter.txt"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("--wordlist is only valid with timed, words, or endless mode"));
}

#[cfg(target_os = "linux")]
#[test]
fn corrupt_history_cli_error_has_a_failure_exit_status() {
    let directory = tempfile::tempdir().unwrap();
    let config_directory = directory.path().join("cmd-type");
    std::fs::create_dir(&config_directory).unwrap();
    std::fs::write(config_directory.join("history.json"), "not json").unwrap();

    let output = command()
        .arg("--history")
        .env("XDG_CONFIG_HOME", directory.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not read history"));
}
