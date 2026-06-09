use anyhow::Context;
use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post},
};
use chrono::Local;
use fsrs::FSRS;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use ts_rs::TS;

use crate::db::{CardState, Db};
use crate::progress::summarize_progress;
use crate::questions::{Bank, Question};
use crate::session::{ScheduledReview, apply_review, fsrs_next};
use crate::study_plan::plan_study_session;

// ─── Shared state ─────────────────────────────────────────────────────────────

pub struct AppState {
    pub questions_dir: PathBuf,
    pub default_cert: String,
    /// Override the DB path (used in tests; None = use the default user path).
    pub db_path: Option<PathBuf>,
}

impl AppState {
    fn cert_or_default(&self, param: Option<String>) -> String {
        param.unwrap_or_else(|| self.default_cert.clone())
    }
}

// ─── Error handling ───────────────────────────────────────────────────────────

struct ApiError {
    status: StatusCode,
    error: anyhow::Error,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: anyhow::anyhow!(message.into()),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            error: anyhow::anyhow!(message.into()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.error.to_string() })),
        )
            .into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(err: E) -> Self {
        ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: err.into(),
        }
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

// ─── Query / body types ───────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct CertParam {
    cert: Option<String>,
}

#[derive(Deserialize)]
struct SessionsParam {
    cert: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct DueParams {
    cert: Option<String>,
    #[serde(rename = "new")]
    max_new: Option<usize>,
    domain: Option<u32>,
    tag: Option<String>,
    ids: Option<String>,
    all: Option<bool>,
}

#[derive(Deserialize)]
struct QuestionsParams {
    cert: Option<String>,
    domain: Option<u32>,
    tag: Option<String>,
    search: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewBody {
    card_id: String,
    cert: Option<String>,
    rating: u32,
    is_correct: bool,
}

#[derive(Deserialize)]
struct SessionBody {
    cert: Option<String>,
    total: u32,
    correct: u32,
}

#[derive(Deserialize)]
struct UploadBankBody {
    /// Cert id, which becomes the `<name>.json` filename. Sanitized server-side.
    name: String,
    /// Raw JSON text of the bank, validated with the same rules as `Bank::load`.
    content: String,
    /// Replace an existing bank of the same name. Defaults to false so a
    /// collision returns 409 and the UI can confirm first.
    #[serde(default)]
    overwrite: bool,
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct StatsResponse {
    cert: String,
    cert_name: String,
    total: u32,
    introduced: u32,
    due_today: u32,
    new_available: u32,
    mastered: u32,
    domains: Vec<DomainStat>,
    tags: Vec<TagStat>,
    sessions: Vec<SessionItem>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct DomainStat {
    id: u32,
    name: String,
    total: u32,
    mastered: u32,
    review_total: u32,
    review_correct: u32,
    accuracy: u32,
}

#[derive(Serialize, TS)]
struct TagStat {
    tag: String,
    correct: u32,
    total: u32,
    accuracy: u32,
}

#[derive(Serialize, TS)]
struct SessionItem {
    date: String,
    total: u32,
    correct: u32,
    accuracy: u32,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct CardWithQuestion {
    question: Question,
    card_state: Option<CardState>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct DueResponse {
    cards: Vec<CardWithQuestion>,
    due_count: usize,
    new_count: usize,
    new_remaining: usize,
    mode: String,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct QuestionsResponse {
    cert: String,
    cert_name: String,
    domains: HashMap<String, String>,
    questions: Vec<CardWithQuestion>,
}

#[derive(Serialize, TS)]
struct SessionsResponse {
    sessions: Vec<SessionItem>,
}

#[derive(Serialize, TS)]
struct CertsResponse {
    certs: Vec<String>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct BankInfo {
    name: String,
    question_count: usize,
}

#[derive(Serialize, TS)]
struct BanksResponse {
    banks: Vec<BankInfo>,
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

fn accuracy(correct: u32, total: u32) -> u32 {
    if total > 0 { correct * 100 / total } else { 0 }
}

fn card_map_from(cards: &[CardState]) -> HashMap<&str, &CardState> {
    cards.iter().map(|c| (c.id.as_str(), c)).collect()
}

fn card_with_q(q: &Question, card_map: &HashMap<&str, &CardState>) -> CardWithQuestion {
    CardWithQuestion {
        question: q.clone(),
        card_state: card_map.get(q.id.as_str()).map(|c| (*c).clone()),
    }
}

fn session_item(date: String, total: u32, correct: u32) -> SessionItem {
    SessionItem {
        accuracy: accuracy(correct, total),
        date,
        total,
        correct,
    }
}

/// A bank name becomes a filename in `questions_dir`, so it must be a plain
/// slug. Rejecting anything outside `[A-Za-z0-9_-]` blocks path traversal
/// (`..`, `/`) and odd filesystem characters in one check.
fn sanitize_cert_name(name: &str) -> Result<String, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("Bank name cannot be empty"));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::bad_request(
            "Bank name may only contain letters, numbers, hyphens, and underscores",
        ));
    }
    Ok(trimmed.to_string())
}

/// List the `.json` bank stems in `dir`, sorted. Shared by `get_certs` and the
/// upload/delete handlers so a refreshed cert list always uses the same logic.
fn list_cert_names(dir: &std::path::Path) -> anyhow::Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if p.extension()?.to_str()? == "json" {
                p.file_stem()?.to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    Ok(names)
}

fn validate_review_rating(is_correct: bool, rating: u32) -> Result<(), ApiError> {
    match (is_correct, rating) {
        (true, 3 | 4) | (false, 1) => Ok(()),
        (true, _) => Err(ApiError::bad_request(
            "Correct reviews must use rating 3 (Good) or 4 (Easy)",
        )),
        (false, _) => Err(ApiError::bad_request(
            "Incorrect reviews must use rating 1 (Again)",
        )),
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

async fn get_stats(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CertParam>,
) -> ApiResult<StatsResponse> {
    let dir = state.questions_dir.clone();
    let cert = state.cert_or_default(params.cert);
    let db_path = state.db_path.clone();

    tracing::debug!(%cert, "get_stats");

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<StatsResponse> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let bank = Bank::load(&dir, &cert)?;
        let today = Local::now().format("%Y-%m-%d").to_string();

        let cards = db.all_cards(&cert)?;
        let reviews = db.all_reviews(&cert)?;
        let recent_sessions = db.recent_sessions(&cert, 5)?;
        let questions: Vec<&Question> = bank.questions.iter().collect();
        let summary = summarize_progress(
            &bank,
            &questions,
            &cards,
            &reviews,
            &recent_sessions,
            &today,
        );

        Ok(StatsResponse {
            cert_name: bank.name,
            cert,
            total: summary.total,
            introduced: summary.introduced,
            due_today: summary.due_today,
            new_available: summary.new_available,
            mastered: summary.mastered,
            domains: summary
                .domains
                .into_iter()
                .map(|domain| DomainStat {
                    id: domain.id,
                    name: domain.name,
                    total: domain.total,
                    mastered: domain.mastered,
                    review_total: domain.review_total,
                    review_correct: domain.review_correct,
                    accuracy: domain.accuracy,
                })
                .collect(),
            tags: summary
                .tags
                .into_iter()
                .map(|tag| TagStat {
                    tag: tag.tag,
                    correct: tag.correct,
                    total: tag.total,
                    accuracy: tag.accuracy,
                })
                .collect(),
            sessions: summary
                .sessions
                .into_iter()
                .map(|session| SessionItem {
                    date: session.date,
                    total: session.total,
                    correct: session.correct,
                    accuracy: session.accuracy,
                })
                .collect(),
        })
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

async fn get_due(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DueParams>,
) -> ApiResult<DueResponse> {
    let dir = state.questions_dir.clone();
    let cert = state.cert_or_default(params.cert);
    let max_new = params.max_new.unwrap_or(5);
    let db_path = state.db_path.clone();

    tracing::debug!(%cert, max_new, "get_due");

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<DueResponse> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let bank = Bank::load(&dir, &cert)?;
        let today = Local::now().format("%Y-%m-%d").to_string();

        let cards = db.all_cards(&cert)?;
        let card_map = card_map_from(&cards);

        // Quiz-all mode: return every matching question, shuffled
        if params.all.unwrap_or(false) {
            let mut qs = bank.filter(params.domain, params.tag.as_deref());
            qs.shuffle(&mut rand::rng());
            let n = qs.len();
            return Ok(DueResponse {
                cards: qs.iter().map(|q| card_with_q(q, &card_map)).collect(),
                due_count: 0,
                new_count: n,
                new_remaining: 0,
                mode: "all".into(),
            });
        }

        // Browse-quiz mode: return a specific subset by ID
        if let Some(ids_str) = params.ids {
            let id_set: std::collections::HashSet<&str> = ids_str.split(',').collect();
            let qs: Vec<_> = bank
                .questions
                .iter()
                .filter(|q| id_set.contains(q.id.as_str()))
                .collect();
            let n = qs.len();
            return Ok(DueResponse {
                cards: qs.iter().map(|q| card_with_q(q, &card_map)).collect(),
                due_count: 0,
                new_count: n,
                new_remaining: 0,
                mode: "quiz".into(),
            });
        }

        let filtered = bank.filter(params.domain, params.tag.as_deref());
        let plan = plan_study_session(
            &filtered,
            |q| card_map.get(q.id.as_str()).map(|card| (*card).clone()),
            &today,
            max_new,
        );

        Ok(DueResponse {
            due_count: plan.due.len(),
            new_count: plan.new.len() - plan.new_remaining,
            new_remaining: plan.new_remaining,
            mode: "study".into(),
            cards: plan
                .session
                .iter()
                .map(|q| card_with_q(q, &card_map))
                .collect(),
        })
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

async fn get_questions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<QuestionsParams>,
) -> ApiResult<QuestionsResponse> {
    let dir = state.questions_dir.clone();
    let cert = state.cert_or_default(params.cert);
    let db_path = state.db_path.clone();

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<QuestionsResponse> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let bank = Bank::load(&dir, &cert)?;

        let cards = db.all_cards(&cert)?;
        let card_map = card_map_from(&cards);

        let mut filtered = bank.filter(params.domain, params.tag.as_deref());

        if let Some(search) = params.search {
            let s = search.to_lowercase();
            filtered.retain(|q| {
                q.question.to_lowercase().contains(&s)
                    || q.scenario.to_lowercase().contains(&s)
                    || q.explanation.to_lowercase().contains(&s)
            });
        }

        Ok(QuestionsResponse {
            cert_name: bank.name.clone(),
            domains: bank.domains.clone(),
            cert,
            questions: filtered
                .into_iter()
                .map(|q| card_with_q(q, &card_map))
                .collect(),
        })
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

async fn post_review(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ReviewBody>,
) -> ApiResult<serde_json::Value> {
    validate_review_rating(body.is_correct, body.rating)?;
    let cert = state.cert_or_default(body.cert);
    let dir = state.questions_dir.clone();
    let db_path = state.db_path.clone();

    tracing::debug!(card_id = %body.card_id, rating = body.rating, is_correct = body.is_correct, "post_review");

    let result = tokio::task::spawn_blocking(move || -> Result<(), ApiError> {
        let bank = Bank::load(&dir, &cert)?;
        if !bank.questions.iter().any(|q| q.id == body.card_id) {
            return Err(ApiError::bad_request(format!(
                "Unknown question ID for cert {cert}: {}",
                body.card_id
            )));
        }
        let db = Db::open_or_at(db_path.as_deref())?;
        let card = db.get_card(&cert, &body.card_id)?;
        let fsrs = FSRS::new(&[]).context("FSRS init")?;
        let today = Local::now().date_naive();
        let (stability, difficulty, due) = fsrs_next(&card, &fsrs, body.rating, today)?;
        let updated_card = apply_review(
            &card,
            ScheduledReview {
                stability,
                difficulty,
                due,
            },
            Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
            body.is_correct,
        );

        db.record_review(
            &updated_card,
            &body.card_id,
            &cert,
            body.is_correct,
            body.rating,
        )?;
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?;
    result?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn post_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SessionBody>,
) -> ApiResult<serde_json::Value> {
    if body.correct > body.total {
        return Err(ApiError::bad_request(
            "Session correct count cannot exceed total",
        ));
    }
    let cert = state.cert_or_default(body.cert);
    let db_path = state.db_path.clone();

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        Db::open_or_at(db_path.as_deref())?.insert_session(&cert, body.total, body.correct)
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SessionsParam>,
) -> ApiResult<SessionsResponse> {
    let cert = state.cert_or_default(params.cert);
    let limit = params.limit.unwrap_or(30);
    let db_path = state.db_path.clone();

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<SessionsResponse> {
        let sessions = Db::open_or_at(db_path.as_deref())?
            .recent_sessions(&cert, limit)?
            .into_iter()
            .map(|(date, total, correct)| session_item(date, total, correct))
            .collect();
        Ok(SessionsResponse { sessions })
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

async fn get_certs(State(state): State<Arc<AppState>>) -> ApiResult<CertsResponse> {
    let dir = state.questions_dir.clone();
    let certs = tokio::task::spawn_blocking(move || list_cert_names(&dir))
        .await
        .context("spawn_blocking panicked")??;
    Ok(Json(CertsResponse { certs }))
}

async fn get_banks(State(state): State<Arc<AppState>>) -> ApiResult<BanksResponse> {
    let dir = state.questions_dir.clone();
    let banks = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<BankInfo>> {
        list_cert_names(&dir)?
            .into_iter()
            .map(|name| {
                let bank = Bank::load(&dir, &name)?;
                Ok(BankInfo {
                    name,
                    question_count: bank.questions.len(),
                })
            })
            .collect()
    })
    .await
    .context("spawn_blocking panicked")??;
    Ok(Json(BanksResponse { banks }))
}

async fn post_upload_bank(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UploadBankBody>,
) -> ApiResult<CertsResponse> {
    let name = sanitize_cert_name(&body.name)?;
    let dir = state.questions_dir.clone();

    tracing::debug!(%name, overwrite = body.overwrite, "post_upload_bank");

    let certs = tokio::task::spawn_blocking(move || -> Result<Vec<String>, ApiError> {
        // Validate the uploaded text with the exact rules `Bank::load` uses.
        Bank::parse(&body.content).map_err(|e| ApiError::bad_request(e.to_string()))?;

        let path = dir.join(format!("{name}.json"));
        if path.exists() && !body.overwrite {
            return Err(ApiError::conflict(format!(
                "A bank named '{name}' already exists. Replacing it may orphan saved progress if its question IDs changed."
            )));
        }
        std::fs::write(&path, &body.content)?;
        Ok(list_cert_names(&dir)?)
    })
    .await
    .context("spawn_blocking panicked")?;

    Ok(Json(CertsResponse { certs: certs? }))
}

async fn delete_bank(
    State(state): State<Arc<AppState>>,
    Path(cert): Path<String>,
) -> ApiResult<CertsResponse> {
    let name = sanitize_cert_name(&cert)?;
    let dir = state.questions_dir.clone();

    tracing::debug!(%name, "delete_bank");

    let certs = tokio::task::spawn_blocking(move || -> Result<Vec<String>, ApiError> {
        let path = dir.join(format!("{name}.json"));
        if !path.exists() {
            return Err(ApiError::bad_request(format!("No bank named '{name}'")));
        }
        // Removes the bank file only; FSRS review history in the DB is left
        // intact, so re-uploading the same cert resumes its schedule.
        std::fs::remove_file(&path)?;
        Ok(list_cert_names(&dir)?)
    })
    .await
    .context("spawn_blocking panicked")?;

    Ok(Json(CertsResponse { certs: certs? }))
}

// ─── Server entry point ───────────────────────────────────────────────────────

#[cfg(not(tarpaulin_include))]
pub async fn run(questions_dir: PathBuf, cert: String, port: u16) -> anyhow::Result<()> {
    let state = Arc::new(AppState {
        questions_dir,
        default_cert: cert,
        db_path: None,
    });

    let app = Router::new()
        .route("/api/certs", get(get_certs))
        .route("/api/banks", get(get_banks).post(post_upload_bank))
        .route("/api/banks/{cert}", delete(delete_bank))
        .route("/api/stats", get(get_stats))
        .route("/api/due", get(get_due))
        .route("/api/questions", get(get_questions))
        .route("/api/review", post(post_review))
        .route("/api/session", post(post_session))
        .route("/api/sessions", get(get_sessions))
        .layer(CorsLayer::permissive())
        .with_state(state);

    println!("study-engine serve  →  http://localhost:{port}");
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ─── TypeScript binding export ──────────────────────────────────────────────

#[cfg(test)]
mod ts_bindings {
    use super::*;
    use ts_rs::{Config, TS};

    /// Regenerates the TypeScript wire-contract bindings the web UI consumes.
    /// Rust is the single source of truth: change a wire struct, run `cargo test`,
    /// and these files are rewritten. CI fails if the committed output drifts
    /// (`git diff --exit-code` on the generated dir), so the contract cannot
    /// silently diverge between the backend and the frontend.
    #[test]
    fn export_typescript_bindings() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../study-engine-ui/src/lib/generated");
        std::fs::create_dir_all(&dir).unwrap();

        let cfg = Config::default();
        // The filename comes from each type's own `output_path` (the same name
        // its siblings import it by), so adding a wire type here is a one-line
        // change with no separate filename to keep in sync.
        macro_rules! export {
            ($($t:ty),* $(,)?) => {{
                $(
                    let file = <$t as TS>::output_path().expect("type has an output path");
                    std::fs::write(dir.join(file), <$t>::export_to_string(&cfg).unwrap()).unwrap();
                )*
            }};
        }
        export!(
            Question,
            CardState,
            CardWithQuestion,
            DueResponse,
            StatsResponse,
            DomainStat,
            TagStat,
            SessionItem,
            QuestionsResponse,
            SessionsResponse,
            CertsResponse,
            BankInfo,
            BanksResponse,
        );
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn accuracy_zero_total_returns_zero() {
        assert_eq!(accuracy(0, 0), 0);
        assert_eq!(accuracy(5, 0), 0);
    }

    #[test]
    fn accuracy_full_correct() {
        assert_eq!(accuracy(10, 10), 100);
    }

    #[test]
    fn accuracy_partial() {
        assert_eq!(accuracy(7, 10), 70);
    }

    #[test]
    fn accuracy_rounds_down() {
        assert_eq!(accuracy(1, 3), 33);
    }

    #[test]
    fn card_map_from_indexes_by_id() {
        let cards = vec![
            CardState {
                id: "a".to_string(),
                ..Default::default()
            },
            CardState {
                id: "b".to_string(),
                ..Default::default()
            },
        ];
        let map = card_map_from(&cards);
        assert!(map.contains_key("a"));
        assert!(map.contains_key("b"));
        assert!(!map.contains_key("c"));
    }

    #[test]
    fn session_item_computes_accuracy() {
        let item = session_item("2026-06-04".to_string(), 10, 8);
        assert_eq!(item.accuracy, 80);
        assert_eq!(item.total, 10);
        assert_eq!(item.correct, 8);
    }

    #[test]
    fn session_item_zero_total_accuracy_is_zero() {
        let item = session_item("2026-06-04".to_string(), 0, 0);
        assert_eq!(item.accuracy, 0);
    }
}

// ─── Handler integration tests ────────────────────────────────────────────────

#[cfg(test)]
mod handler_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::fs;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use crate::questions::tests::TEST_BANK_JSON;

    struct TestApp {
        _dir: TempDir,
        db_path: PathBuf,
        router: Router,
    }

    fn build_test_app() -> TestApp {
        let dir = TempDir::new().unwrap();
        let questions_dir = dir.path().join("questions");
        fs::create_dir_all(&questions_dir).unwrap();
        fs::write(questions_dir.join("test.json"), TEST_BANK_JSON).unwrap();

        let db_path = dir.path().join("test.db");
        let state = Arc::new(AppState {
            questions_dir,
            default_cert: "test".to_string(),
            db_path: Some(db_path.clone()),
        });

        let router = Router::new()
            .route("/api/certs", get(get_certs))
            .route("/api/banks", get(get_banks).post(post_upload_bank))
            .route("/api/banks/{cert}", delete(delete_bank))
            .route("/api/stats", get(get_stats))
            .route("/api/due", get(get_due))
            .route("/api/questions", get(get_questions))
            .route("/api/review", post(post_review))
            .route("/api/session", post(post_session))
            .route("/api/sessions", get(get_sessions))
            .with_state(state);

        TestApp {
            _dir: dir,
            db_path,
            router,
        }
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn get_stats_returns_ok_with_expected_fields() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(
                Request::get("/api/stats?cert=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["cert"], "test");
        assert_eq!(json["total"], 3);
        assert_eq!(json["introduced"], 0);
    }

    #[tokio::test]
    async fn get_due_study_mode_returns_new_cards() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(
                Request::get("/api/due?cert=test&new=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["mode"], "study");
        assert_eq!(json["dueCount"], 0);
        assert_eq!(json["newCount"], 3);
    }

    #[tokio::test]
    async fn get_due_study_mode_interleaves_new_cards_by_domain() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(
                Request::get("/api/due?cert=test&new=2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let ids: Vec<_> = json["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|card| card["question"]["id"].as_str().unwrap())
            .collect();

        assert_eq!(ids, vec!["q1", "q3"]);
    }

    #[tokio::test]
    async fn get_due_all_mode_returns_all_cards() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(
                Request::get("/api/due?cert=test&all=true")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["mode"], "all");
        assert_eq!(json["cards"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn get_due_ids_mode_returns_subset() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(
                Request::get("/api/due?cert=test&ids=q1,q3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["mode"], "quiz");
        assert_eq!(json["cards"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn get_questions_returns_all() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(
                Request::get("/api/questions?cert=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["questions"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn get_questions_search_filters_results() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(
                Request::get("/api/questions?cert=test&search=primary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        // Only q1 mentions "primary benefit"
        assert_eq!(json["questions"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn post_review_returns_ok() {
        let app = build_test_app();
        let body = serde_json::json!({
            "cardId": "q1",
            "cert": "test",
            "rating": 4,
            "isCorrect": true
        });
        let resp = app
            .router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/review")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["ok"], true);
    }

    #[tokio::test]
    async fn post_review_rejects_invalid_rating_for_correctness() {
        let app = build_test_app();
        let body = serde_json::json!({
            "cardId": "q1",
            "cert": "test",
            "rating": 4,
            "isCorrect": false
        });
        let resp = app
            .router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/review")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json = body_json(resp).await;
        assert!(
            json["error"]
                .as_str()
                .unwrap()
                .contains("Incorrect reviews")
        );
    }

    #[tokio::test]
    async fn post_review_rejects_unknown_card_id() {
        let app = build_test_app();
        let body = serde_json::json!({
            "cardId": "not-in-bank",
            "cert": "test",
            "rating": 4,
            "isCorrect": true
        });
        let resp = app
            .router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/review")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json = body_json(resp).await;
        assert!(
            json["error"]
                .as_str()
                .unwrap()
                .contains("Unknown question ID")
        );
    }

    #[tokio::test]
    async fn post_review_updates_card_state() {
        let app = build_test_app();
        // Submit a review for q1
        let review = serde_json::json!({
            "cardId": "q1",
            "cert": "test",
            "rating": 4,
            "isCorrect": true
        });
        app.router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/review")
                    .header("content-type", "application/json")
                    .body(Body::from(review.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Now stats should show 1 introduced card
        let resp = app
            .router
            .oneshot(
                Request::get("/api/stats?cert=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let json = body_json(resp).await;
        assert_eq!(json["introduced"], 1);
    }

    #[tokio::test]
    async fn get_stats_returns_populated_mastery_tags_and_sessions() {
        let app = build_test_app();
        let db = Db::open_at(&app.db_path).unwrap();
        let due_card = CardState {
            id: "q1".to_string(),
            cert: "test".to_string(),
            stability: Some(2.0),
            difficulty: Some(4.0),
            due: Some("2020-01-01".to_string()),
            last_review: Some("2026-06-04T12:00:00".to_string()),
            reps: 3,
        };
        let future_card = CardState {
            id: "q2".to_string(),
            cert: "test".to_string(),
            stability: Some(1.0),
            difficulty: Some(5.0),
            due: Some("2099-01-01".to_string()),
            last_review: Some("2026-06-04T12:00:00".to_string()),
            reps: 1,
        };
        db.record_review(&due_card, "q1", "test", true, 4).unwrap();
        db.record_review(&due_card, "q1", "test", true, 4).unwrap();
        db.record_review(&future_card, "q2", "test", false, 1)
            .unwrap();
        db.insert_session("test", 3, 2).unwrap();

        let resp = app
            .router
            .oneshot(
                Request::get("/api/stats?cert=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["introduced"], 2);
        assert_eq!(json["dueToday"], 1);
        assert_eq!(json["mastered"], 1);
        assert_eq!(json["sessions"].as_array().unwrap().len(), 1);
        assert!(!json["tags"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_due_study_mode_includes_due_and_skips_future_cards() {
        let app = build_test_app();
        let db = Db::open_at(&app.db_path).unwrap();
        let due_card = CardState {
            id: "q1".to_string(),
            cert: "test".to_string(),
            due: Some("2020-01-01".to_string()),
            reps: 1,
            ..Default::default()
        };
        let future_card = CardState {
            id: "q2".to_string(),
            cert: "test".to_string(),
            due: Some("2099-01-01".to_string()),
            reps: 1,
            ..Default::default()
        };
        db.record_review(&due_card, "q1", "test", false, 1).unwrap();
        db.record_review(&future_card, "q2", "test", true, 4)
            .unwrap();

        let resp = app
            .router
            .oneshot(
                Request::get("/api/due?cert=test&new=1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["dueCount"], 1);
        assert_eq!(json["newCount"], 1);
        let ids: Vec<_> = json["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|card| card["question"]["id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&"q1"));
        assert!(ids.contains(&"q3"));
        assert!(!ids.contains(&"q2"));
    }

    #[tokio::test]
    async fn post_review_incorrect_resets_reps() {
        let app = build_test_app();
        let body = serde_json::json!({
            "cardId": "q2",
            "cert": "test",
            "rating": 1,
            "isCorrect": false
        });
        let resp = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/review")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let card = Db::open_at(&app.db_path)
            .unwrap()
            .get_card("test", "q2")
            .unwrap();
        assert_eq!(card.reps, 0);
    }

    #[tokio::test]
    async fn post_session_and_get_sessions() {
        let app = build_test_app();
        let session = serde_json::json!({ "cert": "test", "total": 5, "correct": 4 });
        let post_resp = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header("content-type", "application/json")
                    .body(Body::from(session.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(post_resp.status(), StatusCode::OK);

        let get_resp = app
            .router
            .oneshot(
                Request::get("/api/sessions?cert=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_resp.status(), StatusCode::OK);
        let json = body_json(get_resp).await;
        let sessions = json["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["total"], 5);
        assert_eq!(sessions[0]["correct"], 4);
        assert_eq!(sessions[0]["accuracy"], 80);
    }

    #[tokio::test]
    async fn post_session_rejects_correct_count_above_total() {
        let app = build_test_app();
        let session = serde_json::json!({ "cert": "test", "total": 2, "correct": 3 });
        let resp = app
            .router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header("content-type", "application/json")
                    .body(Body::from(session.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn get_banks_returns_names_and_counts() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(Request::get("/api/banks").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let banks = json["banks"].as_array().unwrap();
        assert_eq!(banks.len(), 1);
        assert_eq!(banks[0]["name"], "test");
        assert_eq!(banks[0]["questionCount"], 3);
    }

    #[tokio::test]
    async fn post_upload_bank_writes_file_and_returns_certs() {
        let app = build_test_app();
        let body = serde_json::json!({ "name": "newbank", "content": TEST_BANK_JSON });
        let resp = app
            .router
            .oneshot(post_json("/api/banks", body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let certs: Vec<&str> = json["certs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        assert_eq!(certs, vec!["newbank", "test"]);
    }

    #[tokio::test]
    async fn post_upload_bank_rejects_malformed_json() {
        let app = build_test_app();
        let body = serde_json::json!({ "name": "broken", "content": "{not json" });
        let resp = app
            .router
            .oneshot(post_json("/api/banks", body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json = body_json(resp).await;
        assert!(
            json["error"]
                .as_str()
                .unwrap()
                .contains("parse question JSON")
        );
    }

    #[tokio::test]
    async fn post_upload_bank_rejects_invalid_bank_content() {
        let app = build_test_app();
        let bad = TEST_BANK_JSON.replace(r#""answer": "C""#, r#""answer": "Z""#);
        let body = serde_json::json!({ "name": "badbank", "content": bad });
        let resp = app
            .router
            .oneshot(post_json("/api/banks", body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json = body_json(resp).await;
        assert!(json["error"].as_str().unwrap().contains("is not in options"));
    }

    #[tokio::test]
    async fn post_upload_bank_rejects_unsafe_name() {
        let app = build_test_app();
        let body = serde_json::json!({ "name": "../escape", "content": TEST_BANK_JSON });
        let resp = app
            .router
            .oneshot(post_json("/api/banks", body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn post_upload_bank_collision_returns_conflict() {
        let app = build_test_app();
        let body = serde_json::json!({ "name": "test", "content": TEST_BANK_JSON });
        let resp = app
            .router
            .oneshot(post_json("/api/banks", body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn post_upload_bank_overwrite_replaces_existing() {
        let app = build_test_app();
        let body = serde_json::json!({
            "name": "test",
            "content": TEST_BANK_JSON,
            "overwrite": true
        });
        let resp = app
            .router
            .oneshot(post_json("/api/banks", body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn delete_bank_removes_file_and_returns_remaining() {
        let app = build_test_app();
        // Add a second bank so deleting one leaves a non-empty list.
        app.router
            .clone()
            .oneshot(post_json(
                "/api/banks",
                serde_json::json!({ "name": "extra", "content": TEST_BANK_JSON }),
            ))
            .await
            .unwrap();

        let resp = app
            .router
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/banks/extra")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let certs: Vec<&str> = json["certs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        assert_eq!(certs, vec!["test"]);
    }

    #[tokio::test]
    async fn delete_bank_unknown_errors() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/banks/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_stats_missing_cert_errors() {
        let app = build_test_app();
        // "badcert" doesn't exist in the questions dir
        let resp = app
            .router
            .oneshot(
                Request::get("/api/stats?cert=badcert")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
