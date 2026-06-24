use crate::ansi::*;
use crate::db::Db;
use crate::progress::summarize_progress;
use crate::questions::{Bank, Question};
use anyhow::Result;

fn bar(pct: u32, width: usize) -> String {
    let filled = (pct as usize * width / 100).min(width);
    let color = if pct >= 80 {
        GRN
    } else if pct >= 50 {
        YLW
    } else {
        RED
    };
    format!(
        "{color}{}{}{RST}",
        "█".repeat(filled),
        "░".repeat(width - filled)
    )
}

pub fn show(questions: &[&Question], bank: &Bank, db: &Db, cert: &str) -> Result<()> {
    println!("\n{BLD}{CYN}{}={RST}", "=".repeat(62));
    println!("  {} — Progress", bank.name);
    println!("{BLD}{CYN}{}={RST}\n", "=".repeat(62));

    let cards = db.all_cards("default", cert)?;
    let reviews = db.all_reviews("default", cert)?;
    let sessions = db.recent_sessions("default", cert, 5)?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let summary = summarize_progress(bank, questions, &cards, &reviews, &sessions, &today);

    println!(
        "  Cards:    {BLD}{total}{RST} total  |  {introduced} introduced  |  {} new",
        summary.new_available,
        total = summary.total,
        introduced = summary.introduced
    );
    println!("  Due now:  {BLD}{}{RST}", summary.due_today);
    println!(
        "  Mastered: {BLD}{}{RST}  (3+ correct reps)\n",
        summary.mastered
    );

    // Domain breakdown
    println!("{BLD}By Domain:{RST}");
    for domain in &summary.domains {
        if domain.total == 0 {
            continue;
        }
        let name_trunc = &domain.name[..domain.name.len().min(36)];
        println!(
            "  D{d_id} {name_trunc:<36} {} {pct:3}%  {d_mastered}/{} mastered",
            bar(domain.accuracy, 12),
            domain.total,
            d_id = domain.id,
            pct = domain.accuracy,
            d_mastered = domain.mastered
        );
    }

    // Tag breakdown
    println!("\n{BLD}Concept Mastery (tags with ≥2 reviews):{RST}");
    if summary.tags.is_empty() {
        println!("  {DIM}No concept data yet.{RST}");
    } else {
        for tag in &summary.tags {
            let label = if tag.accuracy < 60 {
                format!("{RED}▼ needs work{RST}")
            } else if tag.accuracy < 85 {
                format!("{YLW}~ ok{RST}")
            } else {
                format!("{GRN}✓ strong{RST}")
            };
            println!(
                "  #{:<40} {} {:3}%  {label}",
                tag.tag,
                bar(tag.accuracy, 10),
                tag.accuracy
            );
        }
    }

    // Recent sessions
    if !summary.sessions.is_empty() {
        println!("\n{BLD}Recent Sessions:{RST}");
        for session in &summary.sessions {
            println!(
                "  {}  {}/{} ({}%)  {}",
                session.date,
                session.correct,
                session.total,
                session.accuracy,
                bar(session.accuracy, 8)
            );
        }
    }
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::CardState;
    use crate::questions::tests::test_bank;

    fn reviewed_card(id: &str, cert: &str, reps: u32, due: &str) -> CardState {
        CardState {
            id: id.to_string(),
            cert: cert.to_string(),
            stability: Some(1.0),
            difficulty: Some(5.0),
            due: Some(due.to_string()),
            last_review: Some("2026-06-04T12:00:00".to_string()),
            reps,
        }
    }

    #[test]
    fn bar_zero_pct_is_all_empty_and_red() {
        let result = bar(0, 8);
        assert!(
            result.contains(&"░".repeat(8)),
            "should be all empty blocks"
        );
        assert!(result.contains(RED), "should be red at 0%");
        assert!(!result.contains("█"), "should have no filled blocks");
    }

    #[test]
    fn bar_full_pct_is_all_filled_and_green() {
        let result = bar(100, 8);
        assert!(
            result.contains(&"█".repeat(8)),
            "should be all filled blocks"
        );
        assert!(result.contains(GRN), "should be green at 100%");
        assert!(!result.contains("░"), "should have no empty blocks");
    }

    #[test]
    fn bar_50_pct_is_half_filled_and_yellow() {
        let result = bar(50, 10);
        assert!(result.contains("█████░░░░░"), "should be half filled");
        assert!(result.contains(YLW), "should be yellow at 50%");
    }

    #[test]
    fn bar_79_pct_is_yellow() {
        let result = bar(79, 10);
        assert!(
            result.contains(YLW),
            "79% should be yellow (below 80 threshold)"
        );
        assert!(!result.contains(GRN), "79% should not be green");
    }

    #[test]
    fn bar_80_pct_is_green() {
        let result = bar(80, 10);
        assert!(result.contains(GRN), "80% should be green");
    }

    #[test]
    fn bar_49_pct_is_red() {
        let result = bar(49, 10);
        assert!(result.contains(RED), "49% should be red");
        assert!(!result.contains(YLW), "49% should not be yellow");
    }

    #[test]
    fn bar_width_zero_produces_color_and_reset_only() {
        let result = bar(50, 0);
        assert!(!result.contains("█"), "zero width has no blocks");
        assert!(!result.contains("░"), "zero width has no empty blocks");
        assert!(result.contains(RST), "should still contain reset code");
    }

    #[test]
    fn show_handles_empty_progress() {
        let mut bank = test_bank();
        bank.domains
            .insert("3".to_string(), "Empty Domain".to_string());
        let db = Db::open_in_memory().unwrap();
        let questions = bank.filter(None, None);

        show(&questions, &bank, &db, "test").unwrap();
    }

    #[test]
    fn show_handles_reviews_tags_mastery_and_sessions() {
        let bank = test_bank();
        let db = Db::open_in_memory().unwrap();
        let questions = bank.filter(None, None);

        let mastered = reviewed_card("q1", "test", 3, "2020-01-01");
        let learning = reviewed_card("q2", "test", 1, "2099-01-01");
        db.record_review("default", &mastered, "q1", "test", true, 4, None).unwrap();
        db.record_review("default", &mastered, "q1", "test", true, 4, None).unwrap();
        db.record_review("default", &learning, "q2", "test", true, 3, None).unwrap();
        db.record_review("default", &learning, "q2", "test", false, 1, None).unwrap();
        db.insert_session("default", "test", 4, 3).unwrap();

        show(&questions, &bank, &db, "test").unwrap();
    }
}
