//! Launch-directory integration tests.
//!
//! These spawn the real binary with `current_dir` set to a directory that is
//! NOT the repo root, the exact condition behind the Windows "os error 3"
//! bank-loading bug: any cwd-relative lookup (the `questions/` dev layout)
//! silently misses, and the home-config fallback must either work or fail
//! with a readable message. HOME/USERPROFILE are pointed at a temp dir so
//! nothing touches the real user database.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const BANK_JSON: &str = r#"{
    "cert": "testcert",
    "name": "Launch Dir Test",
    "domains": {"1": "Domain One"},
    "questions": [
        {
            "id": "q1",
            "domain": 1,
            "scenario": "S",
            "question": "Q?",
            "options": {"A": "a", "B": "b"},
            "answer": "A",
            "explanation": "Because.",
            "tags": ["t"]
        }
    ]
}"#;

/// Binary invocation hermetically sandboxed: foreign cwd, temp home.
fn cmd(cwd: &TempDir, home: &TempDir) -> Command {
    let mut c = Command::cargo_bin("study-engine").unwrap();
    c.current_dir(cwd.path())
        .env_remove("STUDY_ENGINE_QUESTIONS_DIR")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path());
    c
}

#[test]
fn explicit_questions_dir_works_from_any_cwd() {
    let cwd = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let banks = TempDir::new().unwrap();
    std::fs::write(banks.path().join("testcert.json"), BANK_JSON).unwrap();

    cmd(&cwd, &home)
        .args(["--cert", "testcert", "--questions-dir"])
        .arg(banks.path())
        .arg("stats")
        .assert()
        .success();
}

#[test]
fn questions_dir_flag_is_accepted_after_subcommand() {
    // boot.mjs passes the flag after `serve`; clap only allows that when the
    // arg is global. Guard against the regression with the cheaper `stats`.
    let cwd = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let banks = TempDir::new().unwrap();
    std::fs::write(banks.path().join("testcert.json"), BANK_JSON).unwrap();

    cmd(&cwd, &home)
        .args(["--cert", "testcert", "stats", "--questions-dir"])
        .arg(banks.path())
        .assert()
        .success();
}

#[test]
fn missing_bank_from_foreign_cwd_fails_with_readable_error() {
    // No flag, no env, empty home: the home-config fallback has no bank.
    // The failure must name the file it looked for, not surface a bare
    // os error like the original Windows report.
    let cwd = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    cmd(&cwd, &home)
        .arg("stats")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not read question file"));
}

#[test]
fn env_var_questions_dir_works_from_any_cwd() {
    let cwd = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let banks = TempDir::new().unwrap();
    std::fs::write(banks.path().join("testcert.json"), BANK_JSON).unwrap();

    cmd(&cwd, &home)
        .env("STUDY_ENGINE_QUESTIONS_DIR", banks.path())
        .args(["--cert", "testcert", "stats"])
        .assert()
        .success();
}
