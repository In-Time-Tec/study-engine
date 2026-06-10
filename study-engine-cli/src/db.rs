use anyhow::Result;
use chrono::{Local, NaiveDate};
use rusqlite::{Connection, params};
use std::path::Path;
use ts_rs::TS;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS cards (
    id          TEXT NOT NULL,
    cert        TEXT NOT NULL,
    stability   REAL,
    difficulty  REAL,
    due         TEXT,
    last_review TEXT,
    reps        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (cert, id)
);
CREATE TABLE IF NOT EXISTS reviews (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id         TEXT NOT NULL,
    cert            TEXT NOT NULL,
    ts              TEXT NOT NULL,
    correct         INTEGER NOT NULL,
    rating          INTEGER NOT NULL,
    selected_letter TEXT
);
CREATE TABLE IF NOT EXISTS sessions (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    cert    TEXT NOT NULL,
    date    TEXT NOT NULL,
    total   INTEGER NOT NULL,
    correct INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pending_sessions (
    cert            TEXT PRIMARY KEY,
    card_ids        TEXT NOT NULL,
    control_mode    TEXT NOT NULL DEFAULT 'due',
    control_domain  INTEGER,
    started_at      TEXT NOT NULL
);
";

pub struct Db(Connection);

#[derive(Debug, Default, Clone, serde::Serialize, TS)]
pub struct CardState {
    pub id: String,
    pub cert: String,
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    pub due: Option<String>,
    pub last_review: Option<String>,
    pub reps: u32,
}

impl CardState {
    pub fn is_new(&self) -> bool {
        self.due.is_none()
    }

    /// True when this card's due date is on or before `today` (a `%Y-%m-%d`
    /// string). Used by session planning, which injects `today` for testability.
    pub fn is_due_on(&self, today: &str) -> bool {
        self.due.as_deref().map_or(false, |d| d <= today)
    }

    /// Whole days elapsed between the last review and `today`. `today` is
    /// injected (rather than read from the clock here) so scheduling stays a
    /// pure function of its inputs and is deterministically testable.
    pub fn days_since_review(&self, today: NaiveDate) -> u32 {
        let Some(lr) = &self.last_review else {
            return 0;
        };
        if lr.len() < 10 {
            return 0;
        }
        let Ok(lr_date) = NaiveDate::parse_from_str(&lr[..10], "%Y-%m-%d") else {
            return 0;
        };
        (today - lr_date).num_days().max(0) as u32
    }
}

impl CardState {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(CardState {
            id: row.get(0)?,
            cert: row.get(1)?,
            stability: row.get(2)?,
            difficulty: row.get(3)?,
            due: row.get(4)?,
            last_review: row.get(5)?,
            reps: row.get(6)?,
        })
    }
}

impl Db {
    pub fn open() -> Result<Self> {
        let path = crate::paths::home_dir().join(".local/share/study-engine/study-engine.db");
        Self::open_at(&path)
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        migrate_schema(&conn)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Db(conn))
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        migrate_schema(&conn)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Db(conn))
    }

    /// Open at a specific path, or fall back to the default user path.
    pub fn open_or_at(path: Option<&Path>) -> Result<Self> {
        match path {
            Some(p) => Self::open_at(p),
            None => Self::open(),
        }
    }

    pub fn get_card(&self, cert: &str, id: &str) -> Result<CardState> {
        tracing::trace!(card_id = %id, %cert, "get_card");
        let res = self.0.query_row(
            "SELECT id, cert, stability, difficulty, due, last_review, reps
             FROM cards WHERE cert = ?1 AND id = ?2",
            params![cert, id],
            CardState::from_row,
        );
        match res {
            Ok(c) => Ok(c),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(CardState {
                id: id.to_string(),
                cert: cert.to_string(),
                ..Default::default()
            }),
            Err(e) => Err(e.into()),
        }
    }

    pub fn insert_session(&self, cert: &str, total: u32, correct: u32) -> Result<()> {
        let date = Local::now().format("%Y-%m-%d").to_string();
        tracing::debug!(%cert, total, correct, "insert_session");
        self.0.execute(
            "INSERT INTO sessions (cert, date, total, correct) VALUES (?1,?2,?3,?4)",
            params![cert, date, total, correct],
        )?;
        Ok(())
    }

    pub fn record_review(
        &self,
        card: &CardState,
        card_id: &str,
        cert: &str,
        correct: bool,
        rating: u32,
        selected_letter: Option<&str>,
    ) -> Result<()> {
        tracing::debug!(%card_id, %cert, rating, correct = correct as u8, reps = card.reps, due = ?card.due, "record_review");
        let tx = self.0.unchecked_transaction()?;
        let ts = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        tx.execute(
            "INSERT OR REPLACE INTO cards
             (id, cert, stability, difficulty, due, last_review, reps)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                card.id,
                card.cert,
                card.stability,
                card.difficulty,
                card.due,
                card.last_review,
                card.reps,
            ],
        )?;
        tx.execute(
            "INSERT INTO reviews (card_id, cert, ts, correct, rating, selected_letter)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![card_id, cert, ts, correct as i32, rating, selected_letter],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn save_pending_session(
        &self,
        cert: &str,
        card_ids_json: &str,
        control_mode: &str,
        control_domain: Option<i32>,
    ) -> Result<()> {
        let started_at = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        self.0.execute(
            "INSERT OR REPLACE INTO pending_sessions
             (cert, card_ids, control_mode, control_domain, started_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![cert, card_ids_json, control_mode, control_domain, started_at],
        )?;
        Ok(())
    }

    pub fn get_pending_session(
        &self,
        cert: &str,
    ) -> Result<Option<(String, String, Option<i32>, String)>> {
        let res = self.0.query_row(
            "SELECT card_ids, control_mode, control_domain, started_at
             FROM pending_sessions WHERE cert = ?1",
            params![cert],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i32>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        );
        match res {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Reviews for the given card IDs recorded at or after `since` (ISO timestamp).
    pub fn reviews_since(
        &self,
        cert: &str,
        card_ids: &[&str],
        since: &str,
    ) -> Result<Vec<(String, bool, u32, Option<String>)>> {
        if card_ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = card_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 3))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT card_id, correct, rating, selected_letter
             FROM reviews
             WHERE cert = ?1 AND ts >= ?2 AND card_id IN ({})
             ORDER BY ts",
            placeholders
        );
        let mut stmt = self.0.prepare(&sql)?;
        let mut raw_params: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(cert.to_string()),
            Box::new(since.to_string()),
        ];
        for id in card_ids {
            raw_params.push(Box::new(id.to_string()));
        }
        let param_refs: Vec<&dyn rusqlite::ToSql> =
            raw_params.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(1)? != 0,
                row.get::<_, u32>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn clear_pending_session(&self, cert: &str) -> Result<()> {
        self.0.execute(
            "DELETE FROM pending_sessions WHERE cert = ?1",
            params![cert],
        )?;
        Ok(())
    }

    /// Per-card review history, used by tests to verify `record_review` writes
    /// and cert-scoping. Production stats read the whole cert via `all_reviews`.
    #[cfg(test)]
    pub fn reviews_for(&self, cert: &str, card_id: &str) -> Result<Vec<(bool, u32)>> {
        let mut stmt = self.0.prepare(
            "SELECT correct, rating FROM reviews WHERE cert=?1 AND card_id=?2 ORDER BY ts",
        )?;
        let rows = stmt.query_map(params![cert, card_id], |row| {
            Ok((row.get::<_, i32>(0)? != 0, row.get::<_, u32>(1)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn all_cards(&self, cert: &str) -> Result<Vec<CardState>> {
        let mut stmt = self.0.prepare(
            "SELECT id, cert, stability, difficulty, due, last_review, reps
             FROM cards WHERE cert = ?1",
        )?;
        let rows = stmt.query_map(params![cert], CardState::from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn all_reviews(&self, cert: &str) -> Result<Vec<(String, bool, u32)>> {
        let mut stmt = self
            .0
            .prepare("SELECT card_id, correct, rating FROM reviews WHERE cert = ?1")?;
        let rows = stmt.query_map(params![cert], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(1)? != 0,
                row.get::<_, u32>(2)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn recent_sessions(&self, cert: &str, n: usize) -> Result<Vec<(String, u32, u32)>> {
        let mut stmt = self.0.prepare(
            "SELECT date, total, correct FROM sessions
             WHERE cert=?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![cert, n as u32], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, u32>(2)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn migrate_schema(conn: &Connection) -> Result<()> {
    let has_cards: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='cards')",
        [],
        |row| row.get(0),
    )?;
    if !has_cards {
        return Ok(());
    }

    // Migration 1: fix cards primary key (cert, id) from legacy (id only)
    let mut stmt = conn.prepare("PRAGMA table_info(cards)")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(1)?, row.get::<_, u32>(5)?))
    })?;
    let columns = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let id_pk = columns
        .iter()
        .find_map(|(name, pk)| (name == "id").then_some(*pk))
        .unwrap_or(0);
    let cert_pk = columns
        .iter()
        .find_map(|(name, pk)| (name == "cert").then_some(*pk))
        .unwrap_or(0);

    if id_pk == 1 && cert_pk == 0 {
        conn.execute_batch(
            "
            ALTER TABLE cards RENAME TO cards_legacy;
            CREATE TABLE cards (
                id          TEXT NOT NULL,
                cert        TEXT NOT NULL,
                stability   REAL,
                difficulty  REAL,
                due         TEXT,
                last_review TEXT,
                reps        INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (cert, id)
            );
            INSERT OR REPLACE INTO cards
                (id, cert, stability, difficulty, due, last_review, reps)
            SELECT id, cert, stability, difficulty, due, last_review, reps
            FROM cards_legacy;
            DROP TABLE cards_legacy;
            ",
        )?;
    }

    // Migration 2: add selected_letter to reviews if missing
    let has_reviews: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='reviews')",
        [],
        |row| row.get(0),
    )?;
    if has_reviews {
        let has_selected_letter: bool = conn
            .prepare("PRAGMA table_info(reviews)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|name| name == "selected_letter");
        if !has_selected_letter {
            conn.execute_batch("ALTER TABLE reviews ADD COLUMN selected_letter TEXT")?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn new_db() -> Db {
        Db::open_in_memory().unwrap()
    }

    fn sample_card(id: &str, cert: &str) -> CardState {
        CardState {
            id: id.to_string(),
            cert: cert.to_string(),
            stability: Some(1.5),
            difficulty: Some(4.8),
            due: Some("2026-06-10".to_string()),
            last_review: Some("2026-06-04T10:00:00".to_string()),
            reps: 1,
        }
    }

    // ── CardState unit tests ──────────────────────────────────────────────────

    #[test]
    fn new_card_is_new_and_not_due() {
        let card = CardState::default();
        assert!(card.is_new());
        assert!(!card.is_due_on("2026-06-05"));
    }

    #[test]
    fn card_with_past_due_is_due() {
        let card = CardState {
            due: Some("2020-01-01".to_string()),
            ..Default::default()
        };
        assert!(!card.is_new());
        assert!(card.is_due_on("2026-06-05"));
    }

    #[test]
    fn card_with_future_due_is_not_due() {
        let card = CardState {
            due: Some("2099-12-31".to_string()),
            ..Default::default()
        };
        assert!(!card.is_new());
        assert!(!card.is_due_on("2026-06-05"));
    }

    #[test]
    fn card_due_today_is_due() {
        let card = CardState {
            due: Some("2026-06-05".to_string()),
            ..Default::default()
        };
        assert!(card.is_due_on("2026-06-05"));
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn days_since_review_no_review_is_zero() {
        assert_eq!(CardState::default().days_since_review(date(2026, 6, 5)), 0);
    }

    #[test]
    fn days_since_review_counts_elapsed_days() {
        let card = CardState {
            last_review: Some("2026-06-01T12:00:00".to_string()),
            ..Default::default()
        };
        assert_eq!(card.days_since_review(date(2026, 6, 5)), 4);
    }

    #[test]
    fn days_since_review_today_is_zero() {
        let card = CardState {
            last_review: Some("2026-06-05T08:00:00".to_string()),
            ..Default::default()
        };
        assert_eq!(card.days_since_review(date(2026, 6, 5)), 0);
    }

    #[test]
    fn days_since_review_future_review_clamps_to_zero() {
        let card = CardState {
            last_review: Some("2026-06-10T08:00:00".to_string()),
            ..Default::default()
        };
        assert_eq!(card.days_since_review(date(2026, 6, 5)), 0);
    }

    #[test]
    fn days_since_review_short_date_is_zero() {
        let card = CardState {
            last_review: Some("bad".to_string()),
            ..Default::default()
        };
        assert_eq!(card.days_since_review(date(2026, 6, 5)), 0);
    }

    #[test]
    fn days_since_review_invalid_date_is_zero() {
        let card = CardState {
            last_review: Some("not-a-dateT12:00:00".to_string()),
            ..Default::default()
        };
        assert_eq!(card.days_since_review(date(2026, 6, 5)), 0);
    }

    // ── Db integration tests (in-memory) ────────────────────────────────────

    #[test]
    fn get_card_unknown_returns_default() {
        let db = new_db();
        let card = db.get_card("cert-a", "nonexistent").unwrap();
        assert_eq!(card.id, "nonexistent");
        assert_eq!(card.cert, "cert-a");
        assert!(card.is_new());
        assert_eq!(card.reps, 0);
    }

    #[test]
    fn get_card_propagates_row_errors() {
        let db = new_db();
        db.0.execute(
            "INSERT INTO cards (id, cert, reps) VALUES (?1, ?2, ?3)",
            params!["bad", "cert", -1],
        )
        .unwrap();

        assert!(db.get_card("cert", "bad").is_err());
    }

    #[test]
    fn record_review_and_get_card_round_trip() {
        let db = new_db();
        let card = sample_card("q1", "cert-a");
        db.record_review(&card, "q1", "cert-a", true, 4, None).unwrap();

        let retrieved = db.get_card("cert-a", "q1").unwrap();
        assert_eq!(retrieved.id, "q1");
        assert_eq!(retrieved.cert, "cert-a");
        assert!((retrieved.stability.unwrap() - 1.5).abs() < 0.001);
        assert!((retrieved.difficulty.unwrap() - 4.8).abs() < 0.001);
        assert_eq!(retrieved.due, Some("2026-06-10".to_string()));
        assert_eq!(retrieved.reps, 1);
    }

    #[test]
    fn record_review_stores_review_history() {
        let db = new_db();
        let card = sample_card("q-rev", "cert-a");
        db.record_review(&card, "q-rev", "cert-a", true, 4, None).unwrap();
        db.record_review(&card, "q-rev", "cert-a", false, 1, None).unwrap();

        let reviews = db.reviews_for("cert-a", "q-rev").unwrap();
        assert_eq!(reviews.len(), 2);
        assert_eq!(reviews[0], (true, 4));
        assert_eq!(reviews[1], (false, 1));
    }

    #[test]
    fn reviews_for_unknown_card_is_empty() {
        let db = new_db();
        let reviews = db.reviews_for("cert-a", "no-such-card").unwrap();
        assert!(reviews.is_empty());
    }

    #[test]
    fn insert_session_and_retrieve() {
        let db = new_db();
        db.insert_session("cert-x", 10, 7).unwrap();

        let sessions = db.recent_sessions("cert-x", 5).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].1, 10);
        assert_eq!(sessions[0].2, 7);
    }

    #[test]
    fn recent_sessions_respects_limit() {
        let db = new_db();
        for i in 0..5u32 {
            db.insert_session("cert-lim", 10, i).unwrap();
        }
        let sessions = db.recent_sessions("cert-lim", 3).unwrap();
        assert_eq!(sessions.len(), 3);
    }

    #[test]
    fn all_cards_returns_only_matching_cert() {
        let db = new_db();
        let a = sample_card("c1", "certA");
        let b = sample_card("c2", "certB");
        db.record_review(&a, "c1", "certA", true, 4, None).unwrap();
        db.record_review(&b, "c2", "certB", true, 4, None).unwrap();

        let cards_a = db.all_cards("certA").unwrap();
        assert_eq!(cards_a.len(), 1);
        assert_eq!(cards_a[0].id, "c1");

        let cards_b = db.all_cards("certB").unwrap();
        assert_eq!(cards_b.len(), 1);
        assert_eq!(cards_b[0].id, "c2");
    }

    #[test]
    fn all_reviews_returns_only_matching_cert() {
        let db = new_db();
        let card_a = sample_card("r1", "certA");
        let card_b = sample_card("r2", "certB");
        db.record_review(&card_a, "r1", "certA", true, 4, None).unwrap();
        db.record_review(&card_b, "r2", "certB", false, 1, None).unwrap();

        let revs_a = db.all_reviews("certA").unwrap();
        assert_eq!(revs_a.len(), 1);
        assert_eq!(revs_a[0].0, "r1");
        assert!(revs_a[0].1); // correct=true

        let revs_b = db.all_reviews("certB").unwrap();
        assert_eq!(revs_b.len(), 1);
        assert_eq!(revs_b[0].0, "r2");
        assert!(!revs_b[0].1); // correct=false
    }

    #[test]
    fn record_review_updates_existing_card() {
        let db = new_db();
        let v1 = sample_card("upd", "c");
        db.record_review(&v1, "upd", "c", true, 4, None).unwrap();

        let v2 = CardState {
            id: "upd".to_string(),
            cert: "c".to_string(),
            stability: Some(3.0),
            difficulty: Some(6.0),
            due: Some("2026-07-01".to_string()),
            last_review: Some("2026-06-05T10:00:00".to_string()),
            reps: 2,
        };
        db.record_review(&v2, "upd", "c", true, 4, None).unwrap();

        let retrieved = db.get_card("c", "upd").unwrap();
        assert_eq!(retrieved.reps, 2);
        assert_eq!(retrieved.due, Some("2026-07-01".to_string()));
        // Two review rows recorded
        assert_eq!(db.reviews_for("c", "upd").unwrap().len(), 2);
    }

    #[test]
    fn duplicate_card_ids_are_scoped_by_cert() {
        let db = new_db();
        let cert_a = sample_card("shared", "cert-a");
        let mut cert_b = sample_card("shared", "cert-b");
        cert_b.reps = 4;
        cert_b.due = Some("2026-08-01".to_string());

        db.record_review(&cert_a, "shared", "cert-a", true, 4, None).unwrap();
        db.record_review(&cert_b, "shared", "cert-b", true, 4, None).unwrap();

        let card_a = db.get_card("cert-a", "shared").unwrap();
        let card_b = db.get_card("cert-b", "shared").unwrap();

        assert_eq!(card_a.reps, 1);
        assert_eq!(card_a.due, Some("2026-06-10".to_string()));
        assert_eq!(card_b.reps, 4);
        assert_eq!(card_b.due, Some("2026-08-01".to_string()));
    }

    #[test]
    fn reviews_for_are_scoped_by_cert() {
        let db = new_db();
        let cert_a = sample_card("shared-review", "cert-a");
        let cert_b = sample_card("shared-review", "cert-b");

        db.record_review(&cert_a, "shared-review", "cert-a", true, 4, None).unwrap();
        db.record_review(&cert_b, "shared-review", "cert-b", false, 1, None).unwrap();

        assert_eq!(
            db.reviews_for("cert-a", "shared-review").unwrap(),
            vec![(true, 4)]
        );
        assert_eq!(
            db.reviews_for("cert-b", "shared-review").unwrap(),
            vec![(false, 1)]
        );
    }

    #[test]
    fn open_at_migrates_legacy_card_primary_key() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("legacy.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE cards (
                    id          TEXT PRIMARY KEY,
                    cert        TEXT NOT NULL,
                    stability   REAL,
                    difficulty  REAL,
                    due         TEXT,
                    last_review TEXT,
                    reps        INTEGER NOT NULL DEFAULT 0
                );
                INSERT INTO cards
                    (id, cert, stability, difficulty, due, last_review, reps)
                VALUES
                    ('shared', 'cert-a', 1.5, 4.8, '2026-06-10', '2026-06-04T10:00:00', 1);
                ",
            )
            .unwrap();
        }

        let db = Db::open_at(&path).unwrap();
        let cert_b = sample_card("shared", "cert-b");
        db.record_review(&cert_b, "shared", "cert-b", true, 4, None).unwrap();

        assert_eq!(db.get_card("cert-a", "shared").unwrap().reps, 1);
        assert_eq!(db.get_card("cert-b", "shared").unwrap().cert, "cert-b");
    }

    #[test]
    fn open_uses_default_home_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let previous_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", dir.path()) };

        let db = Db::open_or_at(None).unwrap();
        db.insert_session("cert-default", 1, 1).unwrap();

        unsafe {
            match previous_home {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
        }
        assert!(
            dir.path()
                .join(".local/share/study-engine/study-engine.db")
                .exists()
        );
    }
}
