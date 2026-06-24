#![deny(warnings)]
mod ansi;
mod db;
mod paths;
mod progress;
mod questions;
mod serve;
mod session;
mod stats;
mod study_plan;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "study-engine",
    about = "Spaced-repetition study engine (CLI + web API)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,

    /// Certification to study
    #[arg(long, default_value = "cca-f")]
    cert: String,

    /// Filter to a specific domain number
    #[arg(long)]
    domain: Option<u32>,

    /// Filter to a concept tag (e.g. 'programmatic-enforcement')
    #[arg(long)]
    tag: Option<String>,

    /// Max new cards introduced per session
    #[arg(long, default_value = "5")]
    new: usize,

    /// Directory containing <cert>.json question files
    #[arg(long, global = true)]
    questions_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Daily study session: due cards first, then new ones (default)
    Study,
    /// Progress dashboard with domain and concept mastery
    Stats,
    /// Quiz all questions, ignoring the spaced-repetition schedule
    All,
    /// Start the HTTP API server for the web UI
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "3001")]
        port: u16,
    },
}

fn resolve_questions_dir(flag: Option<PathBuf>) -> PathBuf {
    if let Some(p) = flag {
        return p;
    }
    if let Ok(env) = std::env::var("STUDY_ENGINE_QUESTIONS_DIR") {
        return PathBuf::from(env);
    }
    // Try sibling questions/ relative to cwd (development layout)
    let dev = PathBuf::from("questions");
    if dev.exists() {
        return dev;
    }
    // Default XDG-style location
    paths::home_dir().join(".config/study-engine/questions")
}

#[cfg(not(tarpaulin_include))]
fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();
}

#[cfg(not(tarpaulin_include))]
fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let questions_dir = resolve_questions_dir(cli.questions_dir);

    // Serve runs its own async runtime and handles requests independently
    if let Some(Cmd::Serve { port }) = cli.command {
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(port);
        tokio::runtime::Runtime::new()?.block_on(serve::run(questions_dir, cli.cert, port))?;
        return Ok(());
    }

    let bank = questions::Bank::load(&questions_dir, &cli.cert)?;
    let db = db::Db::open()?;
    let filtered = bank.filter(cli.domain, cli.tag.as_deref());

    if filtered.is_empty() {
        eprintln!("No questions match the given filters.");
        std::process::exit(1);
    }

    match cli.command.unwrap_or(Cmd::Study) {
        Cmd::Study => session::study(&filtered, &bank, &db, &cli.cert, cli.new)?,
        Cmd::Stats => stats::show(&filtered, &bank, &db, &cli.cert)?,
        Cmd::All => session::all(&filtered, &bank, &db, &cli.cert)?,
        Cmd::Serve { .. } => unreachable!(),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_questions_dir_uses_flag_first() {
        let path = PathBuf::from("/tmp/questions");
        assert_eq!(resolve_questions_dir(Some(path.clone())), path);
    }

    #[test]
    fn resolve_questions_dir_uses_env_before_dev_layout() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join("env-questions");
        unsafe { std::env::set_var("STUDY_ENGINE_QUESTIONS_DIR", &env_path) };

        let resolved = resolve_questions_dir(None);

        unsafe { std::env::remove_var("STUDY_ENGINE_QUESTIONS_DIR") };
        assert_eq!(resolved, env_path);
    }

    #[test]
    fn resolve_questions_dir_falls_back_to_home_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let previous_cwd = std::env::current_dir().unwrap();
        let previous_home = std::env::var_os("HOME");
        unsafe {
            std::env::remove_var("STUDY_ENGINE_QUESTIONS_DIR");
            std::env::set_current_dir(dir.path()).unwrap();
            std::env::set_var("HOME", dir.path());
        }

        let resolved = resolve_questions_dir(None);

        unsafe {
            std::env::set_current_dir(previous_cwd).unwrap();
            match previous_home {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
        }
        assert_eq!(resolved, dir.path().join(".config/study-engine/questions"));
    }

    #[test]
    fn resolve_questions_dir_uses_dev_layout_before_home_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let previous_cwd = std::env::current_dir().unwrap();
        unsafe {
            std::env::remove_var("STUDY_ENGINE_QUESTIONS_DIR");
            std::env::set_current_dir(dir.path()).unwrap();
        }
        std::fs::create_dir_all(dir.path().join("questions")).unwrap();

        let resolved = resolve_questions_dir(None);

        std::env::set_current_dir(previous_cwd).unwrap();
        assert_eq!(resolved, PathBuf::from("questions"));
    }
}
