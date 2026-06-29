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
    /// Glossary terms (or aliases) that must NOT be tooltipped while this
    /// question is unanswered — for definitional questions, the definition of
    /// the term under test would give the answer away.
    #[serde(default, rename = "glossaryExclude")]
    pub glossary_exclude: Vec<String>,
    /// Primary source citation added by the validation workflow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<QuestionSource>,
}

#[derive(Debug, Deserialize, Serialize, Clone, TS)]
#[ts(export)]
pub struct QuestionSource {
    pub url: String,
    pub quote: String,
    pub confidence: String,
    #[serde(default)]
    pub issues: Vec<String>,
}

/// A term of art defined once per bank and surfaced as a tooltip wherever the
/// term appears in question text. `source_url` is rendered as an outbound link,
/// so validation restricts it to http(s).
#[derive(Debug, Deserialize, Serialize, Clone, TS)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryEntry {
    pub term: String,
    /// Other surface forms of the same concept (abbreviations, plurals).
    #[serde(default)]
    pub aliases: Vec<String>,
    pub definition: String,
    pub source_url: String,
    #[serde(default)]
    pub source_title: Option<String>,
}

impl Question {
    /// Permute which letter holds which option, so the correct letter is
    /// uniform across a bank instead of whatever the bank author (often an
    /// LLM) favored. Seeded by the question id alone, so a question's layout
    /// is identical on every load — review history (`selected_letter`) and
    /// resumed sessions stay coherent across fetches and restarts.
    fn shuffle_options(&mut self) {
        let mut keys: Vec<String> = self.options.keys().cloned().collect();
        keys.sort();

        // FNV-1a over the id, then xorshift64* — both fixed here rather than
        // pulled from a crate so the permutation can never change under a
        // dependency upgrade.
        let mut state = keys
            .iter()
            .flat_map(|k| k.bytes())
            .chain(self.id.bytes())
            .fold(0xcbf29ce484222325u64, |h, b| {
                (h ^ b as u64).wrapping_mul(0x100000001b3)
            })
            | 1;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545F4914F6CDD1D)
        };

        // Fisher-Yates on a copy of the key list: slot `keys[i]` receives the
        // option that lived at `perm[i]`.
        let mut perm = keys.clone();
        for i in (1..perm.len()).rev() {
            perm.swap(i, (next() % (i as u64 + 1)) as usize);
        }

        self.options = keys
            .iter()
            .zip(&perm)
            .map(|(slot, source)| (slot.clone(), self.options[source].clone()))
            .collect();
        self.answer = keys
            .into_iter()
            .zip(perm)
            .find_map(|(slot, source)| (source == self.answer).then_some(slot))
            .expect("answer key validated to exist in options");
    }
}

#[derive(Debug, Deserialize)]
pub struct Bank {
    #[allow(dead_code)]
    pub cert: String,
    pub name: String,
    pub domains: HashMap<String, String>,
    #[serde(default)]
    pub glossary: Vec<GlossaryEntry>,
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
        let mut bank: Bank = serde_json::from_str(raw).context("Failed to parse question JSON")?;
        bank.validate()?;
        for q in &mut bank.questions {
            q.shuffle_options();
        }
        Ok(bank)
    }

    fn validate(&self) -> Result<()> {
        // Every surface form (term or alias) a glossary entry can be matched
        // or excluded by, lowercased. Built first so question-level
        // `glossaryExclude` lists can be checked against it.
        let mut surfaces = HashSet::new();
        for entry in &self.glossary {
            if entry.term.trim().is_empty() {
                bail!("Glossary terms cannot be empty");
            }
            if entry.definition.trim().is_empty() {
                bail!("Glossary term '{}' has an empty definition", entry.term);
            }
            if !entry.source_url.starts_with("https://") && !entry.source_url.starts_with("http://")
            {
                bail!(
                    "Glossary term '{}' source URL must start with http:// or https://",
                    entry.term
                );
            }
            for surface in std::iter::once(&entry.term).chain(&entry.aliases) {
                if surface.trim().is_empty() {
                    bail!("Glossary term '{}' has an empty alias", entry.term);
                }
                if !surfaces.insert(surface.trim().to_lowercase()) {
                    bail!("Duplicate glossary term or alias: {surface}");
                }
            }
        }

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
            for excl in &q.glossary_exclude {
                if !surfaces.contains(&excl.trim().to_lowercase()) {
                    bail!(
                        "Question {} excludes unknown glossary term '{}'",
                        q.id,
                        excl
                    );
                }
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
        "glossary": [
            {
                "term": "widget",
                "aliases": ["widgets"],
                "definition": "A small reusable part.",
                "sourceUrl": "https://example.com/widget",
                "sourceTitle": "Widget docs"
            }
        ],
        "questions": [
            {
                "id": "q1",
                "domain": 1,
                "scenario": "System is configured",
                "question": "What is the primary benefit?",
                "options": {"A": "Opt A", "B": "Opt B", "C": "Opt C", "D": "Opt D"},
                "answer": "A",
                "explanation": "A solves the problem.",
                "tags": ["tag-a", "shared"],
                "glossaryExclude": ["widget"]
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
    fn shuffle_options_is_deterministic_across_parses() {
        let a = Bank::parse(TEST_BANK_JSON).unwrap();
        let b = Bank::parse(TEST_BANK_JSON).unwrap();
        for (qa, qb) in a.questions.iter().zip(&b.questions) {
            assert_eq!(qa.answer, qb.answer);
            assert_eq!(qa.options, qb.options);
        }
    }

    #[test]
    fn shuffle_preserves_answer_text_and_option_set() {
        let authored: Bank = serde_json::from_str(TEST_BANK_JSON).unwrap();
        let parsed = Bank::parse(TEST_BANK_JSON).unwrap();
        for (orig, shuf) in authored.questions.iter().zip(&parsed.questions) {
            // The letter may move, but it must still point at the same text.
            assert_eq!(orig.options[&orig.answer], shuf.options[&shuf.answer]);

            let mut orig_keys: Vec<_> = orig.options.keys().collect();
            let mut shuf_keys: Vec<_> = shuf.options.keys().collect();
            orig_keys.sort();
            shuf_keys.sort();
            assert_eq!(orig_keys, shuf_keys);

            let mut orig_vals: Vec<_> = orig.options.values().collect();
            let mut shuf_vals: Vec<_> = shuf.options.values().collect();
            orig_vals.sort();
            shuf_vals.sort();
            assert_eq!(orig_vals, shuf_vals);
        }
    }

    #[test]
    fn shuffle_breaks_answer_letter_skew() {
        // A bank authored with every answer on "B" — the failure mode that
        // motivated the shuffle — must not come out all-"B".
        let questions: Vec<String> = (0..26)
            .map(|i| {
                format!(
                    r#"{{"id": "skew-{i}", "domain": 1, "scenario": "s", "question": "q",
                        "options": {{"A": "a{i}", "B": "b{i}", "C": "c{i}", "D": "d{i}"}},
                        "answer": "B", "explanation": "e"}}"#
                )
            })
            .collect();
        let raw = format!(
            r#"{{"cert": "t", "name": "T", "domains": {{"1": "One"}}, "questions": [{}]}}"#,
            questions.join(",")
        );

        let bank = Bank::parse(&raw).unwrap();
        let on_b = bank.questions.iter().filter(|q| q.answer == "B").count();
        assert!(on_b < 26, "shuffle left every answer on B");
        // And every remapped letter still names the authored correct text.
        for q in &bank.questions {
            assert!(q.options[&q.answer].starts_with('b'));
        }
    }

    #[test]
    fn shuffle_single_option_question_is_noop() {
        let raw = r#"{
            "cert": "t", "name": "T",
            "domains": {"1": "One"},
            "questions": [
                {"id": "q1", "domain": 1, "scenario": "s", "question": "q",
                 "options": {"A": "only"}, "answer": "A", "explanation": "e"}
            ]
        }"#;
        let bank = Bank::parse(raw).unwrap();
        assert_eq!(bank.questions[0].answer, "A");
        assert_eq!(bank.questions[0].options["A"], "only");
    }

    #[test]
    fn parse_keeps_glossary_and_excludes() {
        let bank = Bank::parse(TEST_BANK_JSON).unwrap();
        assert_eq!(bank.glossary.len(), 1);
        assert_eq!(bank.glossary[0].term, "widget");
        assert_eq!(bank.glossary[0].aliases, vec!["widgets"]);
        assert_eq!(bank.glossary[0].source_title.as_deref(), Some("Widget docs"));
        assert_eq!(bank.questions[0].glossary_exclude, vec!["widget"]);
        assert!(bank.questions[1].glossary_exclude.is_empty());
    }

    #[test]
    fn parse_bank_without_glossary_defaults_empty() {
        let raw = r#"{
            "cert": "t", "name": "T",
            "domains": {"1": "One"},
            "questions": [
                {"id": "q1", "domain": 1, "scenario": "s", "question": "q",
                 "options": {"A": "a"}, "answer": "A", "explanation": "e"}
            ]
        }"#;
        let bank = Bank::parse(raw).unwrap();
        assert!(bank.glossary.is_empty());
        assert!(bank.questions[0].glossary_exclude.is_empty());
    }

    #[test]
    fn parse_rejects_empty_glossary_term() {
        let raw = TEST_BANK_JSON.replace(r#""term": "widget""#, r#""term": "  ""#);
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("Glossary terms cannot be empty"));
    }

    #[test]
    fn parse_rejects_empty_glossary_definition() {
        let raw = TEST_BANK_JSON.replace(r#""definition": "A small reusable part.""#, r#""definition": """#);
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("empty definition"));
    }

    #[test]
    fn parse_rejects_non_http_source_url() {
        let raw = TEST_BANK_JSON.replace(
            r#""sourceUrl": "https://example.com/widget""#,
            r#""sourceUrl": "javascript:alert(1)""#,
        );
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("must start with http"));
    }

    #[test]
    fn parse_rejects_empty_glossary_alias() {
        let raw = TEST_BANK_JSON.replace(r#""aliases": ["widgets"]"#, r#""aliases": [" "]"#);
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("empty alias"));
    }

    #[test]
    fn parse_rejects_duplicate_glossary_surface() {
        // An alias colliding with the term (case-insensitively) is ambiguous.
        let raw = TEST_BANK_JSON.replace(r#""aliases": ["widgets"]"#, r#""aliases": ["Widget"]"#);
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("Duplicate glossary term or alias"));
    }

    #[test]
    fn parse_rejects_unknown_glossary_exclude() {
        let raw = TEST_BANK_JSON.replace(
            r#""glossaryExclude": ["widget"]"#,
            r#""glossaryExclude": ["gadget"]"#,
        );
        let err = Bank::parse(&raw).unwrap_err();
        assert!(err.to_string().contains("unknown glossary term 'gadget'"));
    }

    #[test]
    fn exclude_may_name_an_alias() {
        let raw = TEST_BANK_JSON.replace(
            r#""glossaryExclude": ["widget"]"#,
            r#""glossaryExclude": ["WIDGETS"]"#,
        );
        let bank = Bank::parse(&raw).unwrap();
        assert_eq!(bank.questions[0].glossary_exclude, vec!["WIDGETS"]);
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
