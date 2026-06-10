use crate::db::CardState;
use crate::questions::{Bank, Question};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainProgress {
    pub id: u32,
    pub name: String,
    pub total: u32,
    pub mastered: u32,
    pub review_total: u32,
    pub review_correct: u32,
    pub accuracy: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagProgress {
    pub tag: String,
    pub correct: u32,
    pub total: u32,
    pub accuracy: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProgress {
    pub date: String,
    pub total: u32,
    pub correct: u32,
    pub accuracy: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressSummary {
    pub total: u32,
    pub introduced: u32,
    pub due_today: u32,
    pub next_due: Option<String>,
    pub new_available: u32,
    pub mastered: u32,
    pub domains: Vec<DomainProgress>,
    pub tags: Vec<TagProgress>,
    pub sessions: Vec<SessionProgress>,
}

pub fn accuracy(correct: u32, total: u32) -> u32 {
    if total > 0 { correct * 100 / total } else { 0 }
}

pub fn session_progress(date: String, total: u32, correct: u32) -> SessionProgress {
    SessionProgress {
        accuracy: accuracy(correct, total),
        date,
        total,
        correct,
    }
}

pub fn summarize_progress(
    bank: &Bank,
    questions: &[&Question],
    cards: &[CardState],
    reviews: &[(String, bool, u32)],
    sessions: &[(String, u32, u32)],
    today: &str,
) -> ProgressSummary {
    let card_map: HashMap<&str, &CardState> = cards.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut reviews_by_card: HashMap<&str, (u32, u32)> = HashMap::new();

    for (card_id, correct, _) in reviews {
        let entry = reviews_by_card.entry(card_id.as_str()).or_default();
        entry.1 += 1;
        if *correct {
            entry.0 += 1;
        }
    }

    let total = questions.len() as u32;
    let introduced = questions
        .iter()
        .filter(|q| card_map.get(q.id.as_str()).is_some_and(|c| c.due.is_some()))
        .count() as u32;
    let due_today = questions
        .iter()
        .filter(|q| {
            card_map
                .get(q.id.as_str())
                .and_then(|c| c.due.as_deref())
                .is_some_and(|due| due <= today)
        })
        .count() as u32;
    let next_due = if due_today > 0 {
        Some(today.to_string())
    } else {
        questions
            .iter()
            .filter_map(|q| card_map.get(q.id.as_str()).and_then(|c| c.due.as_deref()))
            .filter(|due| *due > today)
            .min()
            .map(str::to_string)
    };
    let mastered = questions
        .iter()
        .filter(|q| card_map.get(q.id.as_str()).is_some_and(|c| c.reps >= 3))
        .count() as u32;

    let mut domain_ids: Vec<u32> = bank.domains.keys().filter_map(|k| k.parse().ok()).collect();
    domain_ids.sort();
    let domains = domain_ids
        .into_iter()
        .map(|id| {
            let domain_questions: Vec<&Question> = questions
                .iter()
                .copied()
                .filter(|q| q.domain == id)
                .collect();
            let mastered = domain_questions
                .iter()
                .filter(|q| card_map.get(q.id.as_str()).is_some_and(|c| c.reps >= 3))
                .count() as u32;
            let (review_correct, review_total) =
                domain_questions.iter().fold((0u32, 0u32), |(c, t), q| {
                    let (qc, qt) = reviews_by_card
                        .get(q.id.as_str())
                        .copied()
                        .unwrap_or_default();
                    (c + qc, t + qt)
                });

            DomainProgress {
                id,
                name: bank.domain_name(id).to_string(),
                total: domain_questions.len() as u32,
                mastered,
                review_total,
                review_correct,
                accuracy: accuracy(review_correct, review_total),
            }
        })
        .collect();

    let mut tag_stats: HashMap<&str, (u32, u32)> = HashMap::new();
    for &q in questions {
        let Some(&(correct, total)) = reviews_by_card.get(q.id.as_str()) else {
            continue;
        };
        for tag in &q.tags {
            let entry = tag_stats.entry(tag.as_str()).or_default();
            entry.0 += correct;
            entry.1 += total;
        }
    }
    let mut tags: Vec<TagProgress> = tag_stats
        .into_iter()
        .filter(|(_, (_, total))| *total >= 1)
        .map(|(tag, (correct, total))| TagProgress {
            tag: tag.to_string(),
            correct,
            total,
            accuracy: accuracy(correct, total),
        })
        .collect();
    tags.sort_by_key(|t| t.accuracy);

    ProgressSummary {
        total,
        introduced,
        due_today,
        next_due,
        new_available: total - introduced,
        mastered,
        domains,
        tags,
        sessions: sessions
            .iter()
            .map(|(date, total, correct)| session_progress(date.clone(), *total, *correct))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, due: Option<&str>, reps: u32) -> CardState {
        CardState {
            id: id.to_string(),
            cert: "test".to_string(),
            due: due.map(str::to_string),
            reps,
            ..Default::default()
        }
    }

    #[test]
    fn accuracy_zero_total_returns_zero() {
        assert_eq!(accuracy(0, 0), 0);
        assert_eq!(accuracy(5, 0), 0);
    }

    #[test]
    fn summarize_progress_computes_cards_domains_tags_and_sessions() {
        let bank = crate::questions::tests::test_bank();
        let questions = bank.filter(None, None);
        let cards = vec![
            card("q1", Some("2026-06-04"), 3),
            card("q2", Some("2999-01-01"), 1),
        ];
        let reviews = vec![
            ("q1".to_string(), true, 4),
            ("q1".to_string(), true, 4),
            ("q2".to_string(), false, 1),
        ];
        let sessions = vec![("2026-06-05".to_string(), 3, 2)];

        let summary =
            summarize_progress(&bank, &questions, &cards, &reviews, &sessions, "2026-06-05");

        assert_eq!(summary.total, 3);
        assert_eq!(summary.introduced, 2);
        assert_eq!(summary.new_available, 1);
        assert_eq!(summary.due_today, 1);
        assert_eq!(summary.next_due.as_deref(), Some("2026-06-05"));
        assert_eq!(summary.mastered, 1);
        assert_eq!(summary.domains[0].review_correct, 2);
        assert_eq!(summary.domains[0].review_total, 3);
        assert_eq!(summary.domains[0].accuracy, 66);
        // Tags surface after a single review; sorted by ascending accuracy.
        // tag-b has one review (q2, wrong) so it appears at 0% — it would have
        // been hidden under the old `>= 2` threshold.
        assert_eq!(summary.tags.len(), 3);
        assert_eq!(summary.tags[0].tag, "tag-b");
        assert_eq!(summary.tags[0].accuracy, 0);
        assert_eq!(summary.tags[1].tag, "shared");
        assert_eq!(summary.tags[1].accuracy, 66);
        assert_eq!(summary.sessions[0].accuracy, 66);
    }

    #[test]
    fn summarize_progress_reports_earliest_future_due_when_nothing_due_today() {
        let bank = crate::questions::tests::test_bank();
        let questions = bank.filter(None, None);
        let cards = vec![
            card("q1", Some("2026-06-12"), 1),
            card("q2", Some("2026-06-08"), 1),
        ];

        let summary = summarize_progress(&bank, &questions, &cards, &[], &[], "2026-06-05");

        assert_eq!(summary.due_today, 0);
        assert_eq!(summary.next_due.as_deref(), Some("2026-06-08"));
    }

    #[test]
    fn summarize_progress_has_no_next_due_without_scheduled_cards() {
        let bank = crate::questions::tests::test_bank();
        let questions = bank.filter(None, None);

        let summary = summarize_progress(&bank, &questions, &[], &[], &[], "2026-06-05");

        assert_eq!(summary.next_due, None);
    }
}
