use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use ts_rs::TS;

#[derive(Debug, Deserialize, Serialize, Clone, TS)]
pub struct Question {
    pub id: String,
    pub domain: u32,
    pub scenario: String,
    pub question: String,
    pub options: HashMap<String, String>,
    pub answer: String,
    pub explanation: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Bank {
    #[allow(dead_code)]
    pub cert: String,
    pub name: String,
    pub domains: HashMap<String, String>,
    pub questions: Vec<Question>,
}

impl Bank {
    pub fn load(questions_dir: &Path, cert: &str) -> Result<Self> {
        let path = questions_dir.join(format!("{cert}.json"));
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("Could not read question file: {}", path.display()))?;
        Self::parse(&raw)
    }

    /// Parse and validate a bank from a raw JSON string. Shared by `load`
    /// (reading from disk) and the upload endpoint (validating uploaded text),
    /// so both enforce the exact same rules.
    pub fn parse(raw: &str) -> Result<Self> {
        let bank: Bank = serde_json::from_str(raw).context("Failed to parse question JSON")?;
        bank.validate()?;
        Ok(bank)
    }

    fn validate(&self) -> Result<()> {
        let mut ids = HashSet::new();
        for q in &self.questions {
            if q.id.trim().is_empty() {
                bail!("Question IDs cannot be empty");
            }
            if !ids.insert(q.id.as_str()) {
                bail!("Duplicate question ID: {}", q.id);
            }
            if !self.domains.contains_key(&q.domain.to_string()) {
                bail!("Question {} references unknown domain {}", q.id, q.domain);
            }
            if q.options.is_empty() {
                bail!("Question {} must have at least one option", q.id);
            }
            if !q.options.contains_key(&q.answer) {
                bail!("Question {} answer {} is not in options", q.id, q.answer);
            }
        }
        Ok(())
    }

    pub fn filter<'a>(&'a self, domain: Option<u32>, tag: Option<&str>) -> Vec<&'a Question> {
        self.questions
            .iter()
            .filter(|q| domain.map_or(true, |d| q.domain == d))
            .filter(|q| tag.map_or(true, |t| q.tags.iter().any(|qt| qt == t)))
            .collect()
    }

    pub fn domain_name(&self, domain: u32) -> &str {
        self.domains
            .get(&domain.to_string())
            .map(|s| s.as_str())
            .unwrap_or("Unknown")
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const TEST_BANK_JSON: &str = r#"{
        "cert": "test",
        "name": "Test Certification",
        "domains": {
            "1": "Domain One",
            "2": "Domain Two"
        },
        "questions": [
            {
                "id": "q1",
                "domain": 1,
                "scenario": "System is configured",
                "question": "What is the primary benefit?",
                "options": {"A": "Opt A", "B": "Opt B", "C": "Opt C", "D": "Opt D"},
                "answer": "A",
                "explanation": "A solves the problem.",
                "tags": ["tag-a", "shared"]
            },
            {
                "id": "q2",
                "domain": 1,
                "scenario": "Another scenario",
                "question": "Which approach is best?",
                "options": {"A": "Opt A", "B": "Opt B", "C": "Opt C", "D": "Opt D"},
                "answer": "B",
                "explanation": "B provides best performance.",
                "tags": ["tag-b", "shared"]
            },
            {
                "id": "q3",
                "domain": 2,
                "scenario": "Domain two context",
                "question": "How does this work?",
                "options": {"A": "Opt A", "B": "Opt B", "C": "Opt C", "D": "Opt D"},
                "answer": "C",
                "explanation": "C is the mechanism.",
                "tags": ["tag-c"]
            }
        ]
    }"#;

    pub fn test_bank() -> Bank {
        serde_json::from_str(TEST_BANK_JSON).unwrap()
    }

    #[test]
    fn bank_loads_all_questions() {
        let bank = test_bank();
        assert_eq!(bank.questions.len(), 3);
        assert_eq!(bank.name, "Test Certification");
    }

    #[test]
    fn filter_by_domain_returns_matching() {
        let bank = test_bank();
        let filtered = bank.filter(Some(1), None);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|q| q.domain == 1));
    }

    #[test]
    fn filter_by_domain_two_returns_one() {
        let bank = test_bank();
        let filtered = bank.filter(Some(2), None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "q3");
    }

    #[test]
    fn filter_by_tag_returns_matching() {
        let bank = test_bank();
        let filtered = bank.filter(None, Some("shared"));
        assert_eq!(filtered.len(), 2);
        let ids: Vec<&str> = filtered.iter().map(|q| q.id.as_str()).collect();
        assert!(ids.contains(&"q1"));
        assert!(ids.contains(&"q2"));
    }

    #[test]
    fn filter_by_domain_and_tag_combined() {
        let bank = test_bank();
        let filtered = bank.filter(Some(1), Some("tag-b"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "q2");
    }

    #[test]
    fn filter_no_match_returns_empty() {
        let bank = test_bank();
        let filtered = bank.filter(Some(99), None);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_no_constraints_returns_all() {
        let bank = test_bank();
        let filtered = bank.filter(None, None);
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn domain_name_known_domain() {
        let bank = test_bank();
        assert_eq!(bank.domain_name(1), "Domain One");
        assert_eq!(bank.domain_name(2), "Domain Two");
    }

    #[test]
    fn domain_name_unknown_returns_fallback() {
        let bank = test_bank();
        assert_eq!(bank.domain_name(99), "Unknown");
    }

    #[test]
    fn bank_load_from_file() {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("mytest.json"), TEST_BANK_JSON).unwrap();
        let bank = Bank::load(dir.path(), "mytest").unwrap();
        assert_eq!(bank.cert, "test");
        assert_eq!(bank.questions.len(), 3);
    }

    #[test]
    fn bank_load_missing_file_errors() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let result = Bank::load(dir.path(), "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn bank_load_duplicate_question_ids_errors() {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let raw = TEST_BANK_JSON.replace(r#""id": "q2""#, r#""id": "q1""#);
        fs::write(dir.path().join("dup.json"), raw).unwrap();

        let err = Bank::load(dir.path(), "dup").unwrap_err();

        assert!(err.to_string().contains("Duplicate question ID"));
    }

    #[test]
    fn bank_load_answer_missing_from_options_errors() {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let raw = TEST_BANK_JSON.replace(r#""answer": "C""#, r#""answer": "Z""#);
        fs::write(dir.path().join("bad-answer.json"), raw).unwrap();

        let err = Bank::load(dir.path(), "bad-answer").unwrap_err();

        assert!(err.to_string().contains("is not in options"));
    }

    #[test]
    fn bank_load_unknown_domain_errors() {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let raw = TEST_BANK_JSON.replacen(r#""domain": 2"#, r#""domain": 99"#, 1);
        fs::write(dir.path().join("bad-domain.json"), raw).unwrap();

        let err = Bank::load(dir.path(), "bad-domain").unwrap_err();

        assert!(err.to_string().contains("unknown domain"));
    }

    #[test]
    fn parse_accepts_valid_bank() {
        let bank = Bank::parse(TEST_BANK_JSON).unwrap();
        assert_eq!(bank.questions.len(), 3);
        assert_eq!(bank.cert, "test");
    }

    #[test]
    fn parse_rejects_malformed_json() {
        let err = Bank::parse("{not valid json").unwrap_err();
        assert!(err.to_string().contains("parse question JSON"));
    }

    #[test]
    fn parse_rejects_empty_question_id() {
        let raw = TEST_BANK_JSON.replace(r#""id": "q1""#, r#""id": "  ""#);
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("IDs cannot be empty"));
    }

    #[test]
    fn parse_rejects_duplicate_ids() {
        let raw = TEST_BANK_JSON.replace(r#""id": "q2""#, r#""id": "q1""#);
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("Duplicate question ID"));
    }

    #[test]
    fn parse_rejects_unknown_domain() {
        let raw = TEST_BANK_JSON.replacen(r#""domain": 2"#, r#""domain": 99"#, 1);
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("unknown domain"));
    }

    #[test]
    fn parse_rejects_answer_not_in_options() {
        let raw = TEST_BANK_JSON.replace(r#""answer": "C""#, r#""answer": "Z""#);
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("is not in options"));
    }

    #[test]
    fn parse_rejects_empty_options() {
        let raw = r#"{
            "cert": "t", "name": "T",
            "domains": {"1": "One"},
            "questions": [
                {"id": "q1", "domain": 1, "scenario": "s", "question": "q",
                 "options": {}, "answer": "A", "explanation": "e"}
            ]
        }"#;
        let err = Bank::parse(raw).unwrap_err();
        assert!(err.to_string().contains("at least one option"));
    }
}
