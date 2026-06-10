use crate::ansi::*;
use crate::db::{CardState, Db};
use crate::questions::{Bank, Question};
use crate::study_plan::plan_study_session;
use anyhow::{Context, Result, bail};
use chrono::{Local, NaiveDate};
use fsrs::{FSRS, MemoryState};
use std::collections::HashMap;

#[cfg(not(tarpaulin_include))]
fn prompt(msg: &str) -> Option<String> {
    use std::io::{self, Write};
    print!("{msg}");
    io::stdout().flush().ok();
    let mut buf = String::new();
    match io::stdin().read_line(&mut buf) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(buf.trim().to_string()),
    }
}

#[cfg(not(tarpaulin_include))]
fn ask_rating(correct: bool) -> u32 {
    if !correct {
        return 1; // Again
    }
    loop {
        let r = prompt(&format!("\n  {DIM}Confident or unsure?  [c/u]:{RST} ")).unwrap_or_default();
        match r.to_lowercase().as_str() {
            "c" | "confident" | "" => return 4, // Easy
            "u" | "unsure" => return 3,         // Good
            _ => {}
        }
    }
}

/// Computes the next FSRS memory state and due date for a review. `today` is
/// injected so this is a pure function of (card, rating, today): no clock read
/// happens here, only at the boundaries that call it.
pub fn fsrs_next(
    card: &CardState,
    fsrs: &FSRS,
    rating: u32,
    today: NaiveDate,
) -> Result<(f32, f32, String)> {
    let memory = card
        .stability
        .zip(card.difficulty)
        .map(|(s, d)| MemoryState {
            stability: s,
            difficulty: d,
        });

    let days = card.days_since_review(today);
    let states = fsrs.next_states(memory, 0.9, days)?;

    let chosen = match rating {
        1 => &states.again,
        3 => &states.good,
        4 => &states.easy,
        _ => bail!("Unsupported FSRS rating: {rating}"),
    };

    let interval = chosen.interval.round().max(1.0) as i64;
    let due = (today + chrono::Duration::days(interval))
        .format("%Y-%m-%d")
        .to_string();

    tracing::debug!(
        card_id = %card.id,
        rating,
        interval_days = interval,
        stability = chosen.memory.stability,
        difficulty = chosen.memory.difficulty,
        %due,
        "fsrs_next"
    );

    Ok((chosen.memory.stability, chosen.memory.difficulty, due))
}

#[derive(Debug, Clone)]
pub struct ScheduledReview {
    pub stability: f32,
    pub difficulty: f32,
    pub due: String,
}

pub fn apply_review(
    card: &CardState,
    scheduled: ScheduledReview,
    reviewed_at: String,
    correct: bool,
) -> CardState {
    CardState {
        id: card.id.clone(),
        cert: card.cert.clone(),
        stability: Some(scheduled.stability),
        difficulty: Some(scheduled.difficulty),
        due: Some(scheduled.due),
        last_review: Some(reviewed_at),
        reps: if correct {
            card.reps.saturating_add(1)
        } else {
            0
        },
    }
}

#[cfg(not(tarpaulin_include))]
fn display_question(i: usize, total: usize, q: &Question, bank: &Bank, card: &CardState) {
    let domain = bank.domain_name(q.domain);
    let card_label = if card.is_new() {
        "new".to_string()
    } else {
        format!(
            "reps={}  due={}",
            card.reps,
            card.due.as_deref().unwrap_or("?")
        )
    };

    println!("\n{BLD}{}─{RST}", "─".repeat(62));
    println!(
        "{BLD}Question {i}/{total}{RST}  {DIM}D{}: {}{RST}  {DIM}[{card_label}]{RST}",
        q.domain,
        &domain[..domain.len().min(36)],
    );
    println!("{CYN}Scenario: {}{RST}", q.scenario);
    println!("\n{}", q.question);
    println!();
    for letter in ["A", "B", "C", "D"] {
        if let Some(text) = q.options.get(letter) {
            println!("  {BLD}{letter}){RST} {text}");
        }
    }
    println!();
}

#[cfg(not(tarpaulin_include))]
fn display_result(correct: bool, answer: &str, q: &Question, rating: u32) {
    if correct {
        let label = if rating == 4 {
            format!("{GRN}{BLD}Correct — confident{RST}")
        } else {
            format!("{YLW}{BLD}Correct — unsure{RST}")
        };
        println!("{label}");
    } else {
        println!("{RED}{BLD}Incorrect.{RST} Correct answer: {BLD}{answer}{RST}");
    }
    println!("\n{DIM}Explanation:{RST}");
    println!("{}", q.explanation);
    if !q.tags.is_empty() {
        let tags: Vec<String> = q.tags.iter().map(|t| format!("#{t}")).collect();
        println!("\n{DIM}{}{RST}", tags.join("  "));
    }
}

#[cfg(not(tarpaulin_include))]
fn run_cards(questions: &[&Question], bank: &Bank, db: &Db, cert: &str) -> Result<(u32, u32)> {
    let mut total = 0u32;
    let mut correct_count = 0u32;
    let fsrs = FSRS::new(&[]).context("FSRS init failed")?;
    let today = Local::now().date_naive();

    for (i, q) in questions.iter().enumerate() {
        let card = db.get_card(cert, &q.id)?;
        display_question(i + 1, questions.len(), q, bank, &card);

        let answer = loop {
            let a = match prompt("Answer (A/B/C/D) or Q to quit: ") {
                None => return Ok((total, correct_count)),
                Some(s) => s.to_uppercase(),
            };
            if a == "Q" {
                return Ok((total, correct_count));
            }
            if matches!(a.as_str(), "A" | "B" | "C" | "D") {
                break a;
            }
        };

        let correct = answer == q.answer;
        let rating = ask_rating(correct);
        display_result(correct, &q.answer, q, rating);

        let (stability, difficulty, due) = fsrs_next(&card, &fsrs, rating, today)?;
        let reviewed_at = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let updated_card = apply_review(
            &card,
            ScheduledReview {
                stability,
                difficulty,
                due,
            },
            reviewed_at,
            correct,
        );

        db.record_review(&updated_card, &q.id, cert, correct, rating, None)?;

        total += 1;
        if correct {
            correct_count += 1;
        }

        let _ = prompt(&format!("\n{DIM}[Enter to continue]{RST}"));
    }

    Ok((total, correct_count))
}

#[cfg(not(tarpaulin_include))]
pub fn study(
    questions: &[&Question],
    bank: &Bank,
    db: &Db,
    cert: &str,
    max_new: usize,
) -> Result<()> {
    let mut card_cache = HashMap::new();
    for &q in questions {
        card_cache.insert(q.id.clone(), db.get_card(cert, &q.id)?);
    }
    let today = Local::now().format("%Y-%m-%d").to_string();
    let plan = plan_study_session(
        questions,
        |q| card_cache.get(q.id.as_str()).cloned(),
        &today,
        max_new,
    );

    tracing::info!(
        %cert,
        due_count = plan.due.len(),
        new_count = plan.new.len().min(max_new),
        "study session started"
    );

    if plan.due.is_empty() && plan.new.is_empty() {
        println!("\n{GRN}{BLD}Nothing due today.{RST}");
        return Ok(());
    }

    println!(
        "\n{BLD}Session:{RST} {} due  +  {} new  =  {} cards",
        plan.due.len(),
        plan.new.len() - plan.new_remaining,
        plan.session.len()
    );
    if plan.new_remaining > 0 {
        println!("{DIM}({} more new cards queued){RST}", plan.new_remaining);
    }

    let (total, correct) = run_cards(&plan.session, bank, db, cert)?;
    if total > 0 {
        db.insert_session(cert, total, correct)?;
        let pct = correct * 100 / total;
        println!("\n{BLD}Session done:{RST} {correct}/{total} ({pct}%)");
    }
    Ok(())
}

#[cfg(not(tarpaulin_include))]
pub fn all(questions: &[&Question], bank: &Bank, db: &Db, cert: &str) -> Result<()> {
    use rand::seq::SliceRandom;
    let mut rng = rand::rng();
    let mut shuffled: Vec<&Question> = questions.to_vec();
    shuffled.shuffle(&mut rng);

    let (total, correct) = run_cards(&shuffled, bank, db, cert)?;
    if total > 0 {
        db.insert_session(cert, total, correct)?;
        let pct = correct * 100 / total;
        println!("\n{BLD}Done:{RST} {correct}/{total} ({pct}%)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_card() -> CardState {
        CardState {
            id: "test-card".to_string(),
            cert: "test".to_string(),
            ..Default::default()
        }
    }

    fn seen_card() -> CardState {
        CardState {
            id: "seen-card".to_string(),
            cert: "test".to_string(),
            stability: Some(2.0),
            difficulty: Some(5.0),
            // Reviewed two days before TODAY, so days_since_review == 2.
            due: Some("2026-06-03".to_string()),
            last_review: Some("2026-06-03T12:00:00".to_string()),
            reps: 2,
        }
    }

    fn fsrs() -> FSRS {
        FSRS::new(&[]).unwrap()
    }

    /// Fixed "today" for deterministic scheduling assertions.
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 5).unwrap()
    }

    #[test]
    fn new_card_easy_schedules_on_or_after_today() {
        let (stability, difficulty, due) = fsrs_next(&new_card(), &fsrs(), 4, today()).unwrap();
        assert!(stability > 0.0, "stability should be positive");
        assert!(difficulty > 0.0, "difficulty should be positive");
        // due is today + interval, and interval is clamped to at least 1 day.
        assert!(due.as_str() > "2026-06-05", "due={due} should be after today");
    }

    #[test]
    fn easy_rating_gives_longer_interval_than_again() {
        let fsrs = fsrs();
        let (_, _, due_again) = fsrs_next(&new_card(), &fsrs, 1, today()).unwrap();
        let (_, _, due_easy) = fsrs_next(&new_card(), &fsrs, 4, today()).unwrap();
        // Easy should schedule further out than Again
        assert!(
            due_easy >= due_again,
            "easy={due_easy} should be >= again={due_again}"
        );
    }

    #[test]
    fn good_rating_between_again_and_easy() {
        let fsrs = fsrs();
        let card = seen_card();
        let (_, _, due_again) = fsrs_next(&card, &fsrs, 1, today()).unwrap();
        let (_, _, due_good) = fsrs_next(&card, &fsrs, 3, today()).unwrap();
        let (_, _, due_easy) = fsrs_next(&card, &fsrs, 4, today()).unwrap();
        assert!(due_good >= due_again, "good should be >= again");
        assert!(due_easy >= due_good, "easy should be >= good");
    }

    #[test]
    fn seen_card_easy_schedules_further_than_new_card_easy() {
        // A card with existing stability should get a longer interval on Easy
        let fsrs = fsrs();
        let (_, _, due_new) = fsrs_next(&new_card(), &fsrs, 4, today()).unwrap();
        let (_, _, due_seen) = fsrs_next(&seen_card(), &fsrs, 4, today()).unwrap();
        // The seen card has stability=2.0 so it should schedule further out
        assert!(
            due_seen >= due_new,
            "seen card (stability=2) should schedule no earlier than new card: seen={due_seen}, new={due_new}"
        );
    }

    #[test]
    fn again_rating_returns_valid_state() {
        let (stability, difficulty, _) = fsrs_next(&new_card(), &fsrs(), 1, today()).unwrap();
        assert!(stability > 0.0);
        assert!(difficulty > 0.0);
    }

    #[test]
    fn unsupported_rating_errors() {
        let err = fsrs_next(&new_card(), &fsrs(), 2, today()).unwrap_err();
        assert!(err.to_string().contains("Unsupported FSRS rating"));
    }

    #[test]
    fn apply_review_increments_reps_for_correct_answers() {
        let card = CardState {
            reps: 2,
            ..seen_card()
        };

        let next = apply_review(
            &card,
            ScheduledReview {
                stability: 3.0,
                difficulty: 4.0,
                due: "2026-06-10".to_string(),
            },
            "2026-06-05T13:00:00".to_string(),
            true,
        );

        assert_eq!(next.id, card.id);
        assert_eq!(next.cert, card.cert);
        assert_eq!(next.stability, Some(3.0));
        assert_eq!(next.difficulty, Some(4.0));
        assert_eq!(next.due.as_deref(), Some("2026-06-10"));
        assert_eq!(next.last_review.as_deref(), Some("2026-06-05T13:00:00"));
        assert_eq!(next.reps, 3);
    }

    #[test]
    fn apply_review_resets_reps_for_incorrect_answers() {
        let next = apply_review(
            &seen_card(),
            ScheduledReview {
                stability: 1.0,
                difficulty: 8.0,
                due: "2026-06-06".to_string(),
            },
            "2026-06-05T13:00:00".to_string(),
            false,
        );

        assert_eq!(next.reps, 0);
    }
}
