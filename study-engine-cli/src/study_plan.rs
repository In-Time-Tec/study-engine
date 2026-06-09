use crate::db::CardState;
use crate::questions::Question;
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug)]
pub struct StudyPlan<'a> {
    pub due: Vec<&'a Question>,
    pub new: Vec<&'a Question>,
    pub session: Vec<&'a Question>,
    pub new_remaining: usize,
}

pub fn interleave_by_domain<'a>(questions: &[&'a Question]) -> Vec<&'a Question> {
    let mut by_domain: BTreeMap<u32, VecDeque<&'a Question>> = BTreeMap::new();
    for &q in questions {
        by_domain.entry(q.domain).or_default().push_back(q);
    }

    let mut interleaved = Vec::with_capacity(questions.len());
    while !by_domain.is_empty() {
        let domains: Vec<u32> = by_domain.keys().copied().collect();
        for domain in domains {
            let is_empty = {
                let Some(queue) = by_domain.get_mut(&domain) else {
                    continue;
                };
                if let Some(q) = queue.pop_front() {
                    interleaved.push(q);
                }
                queue.is_empty()
            };
            if is_empty {
                by_domain.remove(&domain);
            }
        }
    }

    interleaved
}

pub fn plan_study_session<'a, F>(
    questions: &[&'a Question],
    card_for: F,
    today: &str,
    max_new: usize,
) -> StudyPlan<'a>
where
    F: Fn(&Question) -> Option<CardState>,
{
    let mut due = vec![];
    let mut new = vec![];

    for &q in questions {
        match card_for(q) {
            None | Some(CardState { due: None, .. }) => new.push(q),
            Some(card) if card.is_due_on(today) => due.push(q),
            _ => {}
        }
    }

    let due = interleave_by_domain(&due);
    let new = interleave_by_domain(&new);
    let new_count = max_new.min(new.len());
    let session = due
        .iter()
        .chain(new.iter().take(new_count))
        .copied()
        .collect();

    StudyPlan {
        due,
        new_remaining: new.len() - new_count,
        new,
        session,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(due: Option<&str>, reps: u32) -> CardState {
        CardState {
            due: due.map(str::to_string),
            reps,
            ..Default::default()
        }
    }

    #[test]
    fn interleave_by_domain_round_robins_grouped_questions() {
        let bank = crate::questions::tests::test_bank();
        let questions: Vec<&Question> = bank.questions.iter().collect();

        let ids: Vec<&str> = interleave_by_domain(&questions)
            .iter()
            .map(|q| q.id.as_str())
            .collect();

        assert_eq!(ids, vec!["q1", "q3", "q2"]);
    }

    #[test]
    fn plan_study_session_returns_due_then_capped_new_cards() {
        let bank = crate::questions::tests::test_bank();
        let questions = bank.filter(None, None);

        let plan = plan_study_session(
            &questions,
            |q| match q.id.as_str() {
                "q1" => Some(card(Some("2026-06-04"), 1)),
                "q2" => Some(card(None, 0)),
                "q3" => Some(card(Some("2999-01-01"), 1)),
                _ => None,
            },
            "2026-06-05",
            1,
        );

        let session_ids: Vec<&str> = plan.session.iter().map(|q| q.id.as_str()).collect();
        assert_eq!(session_ids, vec!["q1", "q2"]);
        assert_eq!(plan.due.len(), 1);
        assert_eq!(plan.new.len(), 1);
        assert_eq!(plan.new_remaining, 0);
    }

    #[test]
    fn plan_study_session_counts_remaining_new_cards() {
        let bank = crate::questions::tests::test_bank();
        let questions = bank.filter(None, None);

        let plan = plan_study_session(&questions, |_| None, "2026-06-05", 2);

        assert_eq!(plan.due.len(), 0);
        assert_eq!(plan.new.len(), 3);
        assert_eq!(plan.session.len(), 2);
        assert_eq!(plan.new_remaining, 1);
    }
}
