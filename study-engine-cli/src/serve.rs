use anyhow::Context;
use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
};
use chrono::{Duration, Local};
use fsrs::FSRS;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use ts_rs::TS;

use crate::db::{CardState, Db, GroupRoomRecord};
use crate::progress::summarize_progress;
use crate::questions::{Bank, GlossaryEntry, Question};
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

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            error: anyhow::anyhow!(message.into()),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
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

// ─── User key + access code helpers ──────────────────────────────────────────

fn extract_user_key(headers: &HeaderMap) -> String {
    headers
        .get("X-User")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "anonymous".to_string())
}

async fn access_code_middleware(
    headers: HeaderMap,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Response {
    if let Ok(required) = std::env::var("STUDY_ACCESS_CODE") {
        let provided = headers
            .get("X-Access-Code")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != required {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid access code" })),
            )
                .into_response();
        }
    }
    next.run(request).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigResponse {
    requires_code: bool,
}

#[derive(Deserialize)]
struct VerifyCodeBody {
    code: String,
}

async fn get_config() -> Json<ConfigResponse> {
    Json(ConfigResponse {
        requires_code: std::env::var("STUDY_ACCESS_CODE").is_ok(),
    })
}

async fn post_verify_code(
    Json(body): Json<VerifyCodeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match std::env::var("STUDY_ACCESS_CODE") {
        Ok(required) if body.code == required => Ok(Json(serde_json::json!({ "ok": true }))),
        Ok(_) => Err(ApiError {
            status: StatusCode::UNAUTHORIZED,
            error: anyhow::anyhow!("invalid access code"),
        }),
        Err(_) => Ok(Json(serde_json::json!({ "ok": true }))),
    }
}

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
    selected: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingSessionBody {
    cert: Option<String>,
    card_ids: Vec<String>,
    control_mode: String,
    control_domain: Option<i32>,
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

#[derive(Deserialize)]
struct CreateGroupRoomBody {
    cert: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupRoomParams {
    participant_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupVoteBody {
    participant_id: String,
    answer: String,
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
    next_due: Option<String>,
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
    glossary: Vec<GlossaryEntry>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct QuestionsResponse {
    cert: String,
    cert_name: String,
    domains: HashMap<String, String>,
    questions: Vec<CardWithQuestion>,
    glossary: Vec<GlossaryEntry>,
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

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct ReviewedCard {
    card_id: String,
    is_correct: bool,
    rating: u32,
    selected_letter: Option<String>,
    domain: u32,
    correct_answer: String,
    question_text: String,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct PendingSessionResponse {
    card_ids: Vec<String>,
    control_mode: String,
    control_domain: Option<i32>,
    reviewed_cards: Vec<ReviewedCard>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct GroupQuestion {
    id: String,
    domain: u32,
    scenario: String,
    question: String,
    options: HashMap<String, String>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct GroupVoteCount {
    answer: String,
    count: u32,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct GroupRoomState {
    code: String,
    cert: String,
    status: String,
    current_index: usize,
    total_questions: usize,
    current_question: Option<GroupQuestion>,
    vote_counts: Vec<GroupVoteCount>,
    total_votes: u32,
    selected_answer: Option<String>,
    correct_answer: Option<String>,
    explanation: Option<String>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct CreateGroupRoomResponse {
    code: String,
    host_token: String,
    join_url: String,
    state: GroupRoomState,
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

fn group_cleanup_cutoff() -> String {
    (Local::now() - Duration::hours(8))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

fn random_group_code() -> String {
    const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
    (0..6)
        .map(|_| ALPHABET[rand::random_range(0..ALPHABET.len())] as char)
        .collect()
}

fn random_group_token() -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    (0..32)
        .map(|_| ALPHABET[rand::random_range(0..ALPHABET.len())] as char)
        .collect()
}

fn join_url_from_headers(headers: &HeaderMap, code: &str) -> String {
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim_end_matches('/').to_string())
        .or_else(|| {
            headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .map(|host| format!("http://{host}"))
        })
        .unwrap_or_default();
    format!("{origin}/?room={code}")
}

fn normalize_group_code(code: &str) -> String {
    code.trim().to_ascii_uppercase()
}

fn host_token_from_headers(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-group-host-token")
        .and_then(|v| v.to_str().ok())
}

fn require_group_host(room: &GroupRoomRecord, token: Option<&str>) -> Result<(), ApiError> {
    if token.is_some_and(|token| token == room.host_token) {
        Ok(())
    } else {
        Err(ApiError::forbidden("Invalid group host token"))
    }
}

fn validate_group_answer(answer: &str, q: &Question) -> Result<String, ApiError> {
    let normalized = answer.trim().to_ascii_uppercase();
    if normalized.len() != 1 || !q.options.contains_key(&normalized) {
        return Err(ApiError::bad_request(
            "Answer is not valid for this question",
        ));
    }
    Ok(normalized)
}

fn group_question(q: &Question) -> GroupQuestion {
    GroupQuestion {
        id: q.id.clone(),
        domain: q.domain,
        scenario: q.scenario.clone(),
        question: q.question.clone(),
        options: q.options.clone(),
    }
}

fn build_group_room_state(
    db: &Db,
    room: &GroupRoomRecord,
    bank: &Bank,
    participant_id: Option<&str>,
    include_host_fields: bool,
) -> Result<GroupRoomState, ApiError> {
    let card_ids: Vec<String> =
        serde_json::from_str(&room.card_ids_json).context("deserialize group room card ids")?;
    let current_id = if room.status == "ended" {
        None
    } else {
        card_ids.get(room.current_index)
    };
    let question_map: HashMap<&str, &Question> =
        bank.questions.iter().map(|q| (q.id.as_str(), q)).collect();

    let current_question = current_id
        .and_then(|id| question_map.get(id.as_str()))
        .copied();

    let mut vote_counts = vec![];
    let mut total_votes = 0;
    let mut selected_answer = None;
    if let Some(q) = current_question {
        let raw_counts = db.group_vote_counts(&room.code, &q.id)?;
        let count_map: HashMap<String, u32> = raw_counts.into_iter().collect();
        let mut answers: Vec<String> = q.options.keys().cloned().collect();
        answers.sort();
        vote_counts = answers
            .into_iter()
            .map(|answer| {
                let count = count_map.get(&answer).copied().unwrap_or(0);
                total_votes += count;
                GroupVoteCount { answer, count }
            })
            .collect();

        if let Some(participant_id) = participant_id.filter(|id| !id.trim().is_empty()) {
            selected_answer =
                db.group_vote_for_participant(&room.code, &q.id, participant_id.trim())?;
        }
    }

    let revealed = room.status == "revealed";
    Ok(GroupRoomState {
        code: room.code.clone(),
        cert: room.cert.clone(),
        status: room.status.clone(),
        current_index: room.current_index,
        total_questions: card_ids.len(),
        current_question: current_question.map(group_question),
        vote_counts,
        total_votes,
        selected_answer,
        correct_answer: current_question
            .filter(|_| revealed)
            .map(|q| q.answer.clone()),
        explanation: current_question
            .filter(|_| revealed && include_host_fields)
            .map(|q| q.explanation.clone()),
    })
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
/// A missing dir means no banks yet (questions/ is gitignored, so fresh clones
/// don't have it) — that's an empty list, not an error.
fn list_cert_names(dir: &std::path::Path) -> anyhow::Result<Vec<String>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut names: Vec<String> = entries
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
    headers: HeaderMap,
) -> ApiResult<StatsResponse> {
    let dir = state.questions_dir.clone();
    let cert = state.cert_or_default(params.cert);
    let db_path = state.db_path.clone();
    let user_key = extract_user_key(&headers);

    tracing::debug!(%cert, %user_key, "get_stats");

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<StatsResponse> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let bank = Bank::load(&dir, &cert)?;
        let today = Local::now().format("%Y-%m-%d").to_string();

        let cards = db.all_cards(&user_key, &cert)?;
        let reviews = db.all_reviews(&user_key, &cert)?;
        let recent_sessions = db.recent_sessions(&user_key, &cert, 5)?;
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
            next_due: summary.next_due,
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
    headers: HeaderMap,
) -> ApiResult<DueResponse> {
    let dir = state.questions_dir.clone();
    let cert = state.cert_or_default(params.cert);
    let max_new = params.max_new.unwrap_or(5);
    let db_path = state.db_path.clone();
    let user_key = extract_user_key(&headers);

    tracing::debug!(%cert, max_new, "get_due");

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<DueResponse> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let bank = Bank::load(&dir, &cert)?;
        let today = Local::now().format("%Y-%m-%d").to_string();

        let cards = db.all_cards(&user_key, &cert)?;
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
                glossary: bank.glossary.clone(),
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
                glossary: bank.glossary.clone(),
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
            glossary: bank.glossary.clone(),
        })
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

async fn get_questions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<QuestionsParams>,
    headers: HeaderMap,
) -> ApiResult<QuestionsResponse> {
    let dir = state.questions_dir.clone();
    let cert = state.cert_or_default(params.cert);
    let db_path = state.db_path.clone();
    let user_key = extract_user_key(&headers);

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<QuestionsResponse> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let bank = Bank::load(&dir, &cert)?;

        let cards = db.all_cards(&user_key, &cert)?;
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
            glossary: bank.glossary.clone(),
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
    headers: HeaderMap,
    Json(body): Json<ReviewBody>,
) -> ApiResult<serde_json::Value> {
    validate_review_rating(body.is_correct, body.rating)?;
    let cert = state.cert_or_default(body.cert);
    let dir = state.questions_dir.clone();
    let db_path = state.db_path.clone();
    let user_key = extract_user_key(&headers);

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
        let card = db.get_card(&user_key, &cert, &body.card_id)?;
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
            &user_key,
            &updated_card,
            &body.card_id,
            &cert,
            body.is_correct,
            body.rating,
            body.selected.as_deref(),
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
    headers: HeaderMap,
    Json(body): Json<SessionBody>,
) -> ApiResult<serde_json::Value> {
    if body.correct > body.total {
        return Err(ApiError::bad_request(
            "Session correct count cannot exceed total",
        ));
    }
    let cert = state.cert_or_default(body.cert);
    let db_path = state.db_path.clone();
    let user_key = extract_user_key(&headers);

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        Db::open_or_at(db_path.as_deref())?.insert_session(&user_key, &cert, body.total, body.correct)
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SessionsParam>,
    headers: HeaderMap,
) -> ApiResult<SessionsResponse> {
    let cert = state.cert_or_default(params.cert);
    let limit = params.limit.unwrap_or(30);
    let db_path = state.db_path.clone();
    let user_key = extract_user_key(&headers);

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<SessionsResponse> {
        let sessions = Db::open_or_at(db_path.as_deref())?
            .recent_sessions(&user_key, &cert, limit)?
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
        std::fs::create_dir_all(&dir)?;
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

async fn post_pending_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PendingSessionBody>,
) -> ApiResult<serde_json::Value> {
    let cert = state.cert_or_default(body.cert);
    let db_path = state.db_path.clone();
    let user_key = extract_user_key(&headers);
    let card_ids_json = serde_json::to_string(&body.card_ids).context("serialize card_ids")?;

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        Db::open_or_at(db_path.as_deref())?.save_pending_session(
            &user_key,
            &cert,
            &card_ids_json,
            &body.control_mode,
            body.control_domain,
        )
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_pending_session(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CertParam>,
    headers: HeaderMap,
) -> Result<Json<PendingSessionResponse>, ApiError> {
    let cert = state.cert_or_default(params.cert);
    let dir = state.questions_dir.clone();
    let db_path = state.db_path.clone();
    let user_key = extract_user_key(&headers);

    let result = tokio::task::spawn_blocking(
        move || -> Result<Option<PendingSessionResponse>, ApiError> {
            let db = Db::open_or_at(db_path.as_deref())?;
            let Some((card_ids_json, control_mode, control_domain, started_at)) =
                db.get_pending_session(&user_key, &cert)?
            else {
                return Ok(None);
            };

            let card_ids: Vec<String> =
                serde_json::from_str(&card_ids_json).context("deserialize card_ids")?;

            let bank = Bank::load(&dir, &cert)?;
            let question_map: std::collections::HashMap<&str, &Question> =
                bank.questions.iter().map(|q| (q.id.as_str(), q)).collect();

            let id_refs: Vec<&str> = card_ids.iter().map(|s| s.as_str()).collect();
            let raw_reviews = db.reviews_since(&user_key, &cert, &id_refs, &started_at)?;

            let reviewed_cards: Vec<ReviewedCard> = raw_reviews
                .into_iter()
                .filter_map(|(card_id, is_correct, rating, selected_letter)| {
                    let q = question_map.get(card_id.as_str())?;
                    Some(ReviewedCard {
                        card_id,
                        is_correct,
                        rating,
                        selected_letter,
                        domain: q.domain,
                        correct_answer: q.answer.clone(),
                        question_text: q.question.clone(),
                    })
                })
                .collect();

            Ok(Some(PendingSessionResponse {
                card_ids,
                control_mode,
                control_domain,
                reviewed_cards,
            }))
        },
    )
    .await
    .context("spawn_blocking panicked")??;

    match result {
        Some(r) => Ok(Json(r)),
        None => Err(ApiError {
            status: StatusCode::NOT_FOUND,
            error: anyhow::anyhow!("no pending session for this cert"),
        }),
    }
}

async fn delete_pending_session(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CertParam>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    let cert = state.cert_or_default(params.cert);
    let db_path = state.db_path.clone();
    let user_key = extract_user_key(&headers);

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        Db::open_or_at(db_path.as_deref())?.clear_pending_session(&user_key, &cert)
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn post_group_room(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateGroupRoomBody>,
) -> ApiResult<CreateGroupRoomResponse> {
    let cert = state.cert_or_default(body.cert);
    let dir = state.questions_dir.clone();
    let db_path = state.db_path.clone();

    let (code, host_token, room_state) =
        tokio::task::spawn_blocking(move || -> Result<_, ApiError> {
            let db = Db::open_or_at(db_path.as_deref())?;
            db.cleanup_old_group_rooms(&group_cleanup_cutoff())?;

            let bank = Bank::load(&dir, &cert)?;
            if bank.questions.is_empty() {
                return Err(ApiError::bad_request("Question bank has no questions"));
            }

            let mut card_ids: Vec<String> = bank.questions.iter().map(|q| q.id.clone()).collect();
            card_ids.shuffle(&mut rand::rng());
            let card_ids_json = serde_json::to_string(&card_ids).context("serialize card ids")?;
            let host_token = random_group_token();
            let created_at = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();

            let mut inserted_code = None;
            for _ in 0..20 {
                let code = random_group_code();
                if db.get_group_room(&code)?.is_some() {
                    continue;
                }
                db.insert_group_room(&code, &host_token, &cert, &card_ids_json, &created_at)?;
                inserted_code = Some(code);
                break;
            }

            let code = inserted_code
                .ok_or_else(|| ApiError::bad_request("Could not allocate a group room code"))?;
            let room = db
                .get_group_room(&code)?
                .ok_or_else(|| ApiError::not_found("Group room was not created"))?;
            let state = build_group_room_state(&db, &room, &bank, None, true)?;
            Ok((code, host_token, state))
        })
        .await
        .context("spawn_blocking panicked")??;

    Ok(Json(CreateGroupRoomResponse {
        join_url: join_url_from_headers(&headers, &code),
        code,
        host_token,
        state: room_state,
    }))
}

async fn get_group_room(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Query(params): Query<GroupRoomParams>,
    headers: HeaderMap,
) -> ApiResult<GroupRoomState> {
    let dir = state.questions_dir.clone();
    let db_path = state.db_path.clone();
    let code = normalize_group_code(&code);
    let header_token = host_token_from_headers(&headers).map(str::to_string);

    let result = tokio::task::spawn_blocking(move || -> Result<GroupRoomState, ApiError> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let room = db
            .get_group_room(&code)?
            .ok_or_else(|| ApiError::not_found("Group room not found"))?;
        let bank = Bank::load(&dir, &room.cert)?;
        let include_host_fields = header_token
            .as_deref()
            .is_some_and(|token| token == room.host_token);
        build_group_room_state(
            &db,
            &room,
            &bank,
            params.participant_id.as_deref(),
            include_host_fields,
        )
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

async fn post_group_vote(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Json(body): Json<GroupVoteBody>,
) -> ApiResult<GroupRoomState> {
    let dir = state.questions_dir.clone();
    let db_path = state.db_path.clone();
    let code = normalize_group_code(&code);

    let result = tokio::task::spawn_blocking(move || -> Result<GroupRoomState, ApiError> {
        let participant_id = body.participant_id.trim();
        if participant_id.is_empty() {
            return Err(ApiError::bad_request("participantId is required"));
        }

        let db = Db::open_or_at(db_path.as_deref())?;
        let room = db
            .get_group_room(&code)?
            .ok_or_else(|| ApiError::not_found("Group room not found"))?;
        if room.status != "voting" {
            return Err(ApiError::bad_request("Voting is closed for this question"));
        }

        let bank = Bank::load(&dir, &room.cert)?;
        let card_ids: Vec<String> =
            serde_json::from_str(&room.card_ids_json).context("deserialize group room card ids")?;
        let card_id = card_ids
            .get(room.current_index)
            .ok_or_else(|| ApiError::bad_request("Group room has no current question"))?;
        let question = bank
            .questions
            .iter()
            .find(|q| q.id == *card_id)
            .ok_or_else(|| ApiError::bad_request("Current group question is unavailable"))?;
        let answer = validate_group_answer(&body.answer, question)?;

        db.save_group_vote(&room.code, card_id, participant_id, &answer)?;
        build_group_room_state(&db, &room, &bank, Some(participant_id), false)
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

async fn post_group_reveal(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> ApiResult<GroupRoomState> {
    let dir = state.questions_dir.clone();
    let db_path = state.db_path.clone();
    let code = normalize_group_code(&code);
    let header_token = host_token_from_headers(&headers).map(str::to_string);

    let result = tokio::task::spawn_blocking(move || -> Result<GroupRoomState, ApiError> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let room = db
            .get_group_room(&code)?
            .ok_or_else(|| ApiError::not_found("Group room not found"))?;
        require_group_host(&room, header_token.as_deref())?;
        if room.status == "ended" {
            return Err(ApiError::bad_request("Group room has ended"));
        }

        db.update_group_room_status(&room.code, "revealed")?;
        let room = db
            .get_group_room(&code)?
            .ok_or_else(|| ApiError::not_found("Group room not found"))?;
        let bank = Bank::load(&dir, &room.cert)?;
        build_group_room_state(&db, &room, &bank, None, true)
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

async fn post_group_next(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> ApiResult<GroupRoomState> {
    let dir = state.questions_dir.clone();
    let db_path = state.db_path.clone();
    let code = normalize_group_code(&code);
    let header_token = host_token_from_headers(&headers).map(str::to_string);

    let result = tokio::task::spawn_blocking(move || -> Result<GroupRoomState, ApiError> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let room = db
            .get_group_room(&code)?
            .ok_or_else(|| ApiError::not_found("Group room not found"))?;
        require_group_host(&room, header_token.as_deref())?;
        if room.status == "ended" {
            return Err(ApiError::bad_request("Group room has ended"));
        }

        let card_ids: Vec<String> =
            serde_json::from_str(&room.card_ids_json).context("deserialize group room card ids")?;
        let next_index = room.current_index + 1;
        let next_status = if next_index >= card_ids.len() {
            "ended"
        } else {
            "voting"
        };
        let next_index = next_index.min(card_ids.len().saturating_sub(1));
        db.advance_group_room(&room.code, next_index, next_status)?;

        let room = db
            .get_group_room(&code)?
            .ok_or_else(|| ApiError::not_found("Group room not found"))?;
        let bank = Bank::load(&dir, &room.cert)?;
        build_group_room_state(&db, &room, &bank, None, true)
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

async fn post_group_end(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> ApiResult<GroupRoomState> {
    let dir = state.questions_dir.clone();
    let db_path = state.db_path.clone();
    let code = normalize_group_code(&code);
    let header_token = host_token_from_headers(&headers).map(str::to_string);

    let result = tokio::task::spawn_blocking(move || -> Result<GroupRoomState, ApiError> {
        let db = Db::open_or_at(db_path.as_deref())?;
        let room = db
            .get_group_room(&code)?
            .ok_or_else(|| ApiError::not_found("Group room not found"))?;
        require_group_host(&room, header_token.as_deref())?;
        db.update_group_room_status(&room.code, "ended")?;

        let room = db
            .get_group_room(&code)?
            .ok_or_else(|| ApiError::not_found("Group room not found"))?;
        let bank = Bank::load(&dir, &room.cert)?;
        build_group_room_state(&db, &room, &bank, None, true)
    })
    .await
    .context("spawn_blocking panicked")??;

    Ok(Json(result))
}

// ─── Server entry point ───────────────────────────────────────────────────────

#[cfg(not(tarpaulin_include))]
pub async fn run(questions_dir: PathBuf, cert: String, port: u16) -> anyhow::Result<()> {
    // questions/ is gitignored, so fresh clones start without it; create it up
    // front so the first upload has somewhere to land.
    std::fs::create_dir_all(&questions_dir)?;
    let state = Arc::new(AppState {
        questions_dir,
        default_cert: cert,
        db_path: None,
    });

    let protected = Router::new()
        .route("/api/certs", get(get_certs))
        .route("/api/banks", get(get_banks).post(post_upload_bank))
        .route("/api/banks/{cert}", delete(delete_bank))
        .route("/api/stats", get(get_stats))
        .route("/api/due", get(get_due))
        .route("/api/questions", get(get_questions))
        .route("/api/review", post(post_review))
        .route("/api/session", post(post_session))
        .route("/api/sessions", get(get_sessions))
        .route(
            "/api/pending-session",
            get(get_pending_session)
                .post(post_pending_session)
                .delete(delete_pending_session),
        )
        .route("/api/group-rooms", post(post_group_room))
        .route("/api/group-rooms/{code}", get(get_group_room))
        .route("/api/group-rooms/{code}/vote", post(post_group_vote))
        .route("/api/group-rooms/{code}/reveal", post(post_group_reveal))
        .route("/api/group-rooms/{code}/next", post(post_group_next))
        .route("/api/group-rooms/{code}/end", post(post_group_end))
        .layer(middleware::from_fn(access_code_middleware))
        .with_state(state);

    let public = Router::new()
        .route("/api/config", get(get_config))
        .route("/api/verify-code", post(post_verify_code));

    let mut app = Router::new()
        .merge(public)
        .merge(protected)
        .layer(CorsLayer::permissive());

    // Serve built frontend assets when STUDY_ENGINE_STATIC_DIR is set.
    // SPA fallback: any unmatched path serves index.html.
    if let Ok(static_dir) = std::env::var("STUDY_ENGINE_STATIC_DIR") {
        let index = format!("{static_dir}/index.html");
        app = app.fallback_service(
            ServeDir::new(&static_dir).not_found_service(ServeFile::new(index)),
        );
    }

    println!("study-engine serve  →  http://0.0.0.0:{port}");
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
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
            GlossaryEntry,
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
            ReviewedCard,
            PendingSessionResponse,
            GroupQuestion,
            GroupVoteCount,
            GroupRoomState,
            CreateGroupRoomResponse,
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
        build_test_app_inner(true)
    }

    /// Like `build_test_app`, but the questions dir does not exist on disk —
    /// the state a fresh clone is in before any bank is uploaded.
    fn build_test_app_missing_questions_dir() -> TestApp {
        build_test_app_inner(false)
    }

    fn build_test_app_inner(create_questions_dir: bool) -> TestApp {
        let dir = TempDir::new().unwrap();
        let questions_dir = dir.path().join("questions");
        if create_questions_dir {
            fs::create_dir_all(&questions_dir).unwrap();
            fs::write(questions_dir.join("test.json"), TEST_BANK_JSON).unwrap();
        }

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
            .route(
                "/api/pending-session",
                get(get_pending_session)
                    .post(post_pending_session)
                    .delete(delete_pending_session),
            )
            .route("/api/group-rooms", post(post_group_room))
            .route("/api/group-rooms/{code}", get(get_group_room))
            .route("/api/group-rooms/{code}/vote", post(post_group_vote))
            .route("/api/group-rooms/{code}/reveal", post(post_group_reveal))
            .route("/api/group-rooms/{code}/next", post(post_group_next))
            .route("/api/group-rooms/{code}/end", post(post_group_end))
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
    async fn get_due_includes_bank_glossary() {
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
        let glossary = json["glossary"].as_array().unwrap();
        assert_eq!(glossary.len(), 1);
        assert_eq!(glossary[0]["term"], "widget");
        assert_eq!(glossary[0]["sourceUrl"], "https://example.com/widget");
        // q1 carries its exclusion through the wire.
        let q1 = json["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|card| &card["question"])
            .find(|q| q["id"] == "q1")
            .unwrap();
        assert_eq!(q1["glossaryExclude"][0], "widget");
    }

    #[tokio::test]
    async fn get_questions_includes_bank_glossary() {
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
        assert_eq!(json["glossary"].as_array().unwrap().len(), 1);
        assert_eq!(json["glossary"][0]["definition"], "A small reusable part.");
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
        db.record_review("anonymous", &due_card, "q1", "test", true, 4, None)
            .unwrap();
        db.record_review("anonymous", &due_card, "q1", "test", true, 4, None)
            .unwrap();
        db.record_review("anonymous", &future_card, "q2", "test", false, 1, None)
            .unwrap();
        db.insert_session("anonymous", "test", 3, 2).unwrap();

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
        let today = Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(json["introduced"], 2);
        assert_eq!(json["dueToday"], 1);
        assert_eq!(json["nextDue"], today);
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
        db.record_review("anonymous", &due_card, "q1", "test", false, 1, None)
            .unwrap();
        db.record_review("anonymous", &future_card, "q2", "test", true, 4, None)
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
            .get_card("anonymous", "test", "q2")
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

    fn post_empty_with_host(uri: &str, host_token: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("x-group-host-token", host_token)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn group_room_flow_votes_reveals_and_advances() {
        let app = build_test_app();
        let create_resp = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/group-rooms")
                    .header("content-type", "application/json")
                    .header("origin", "http://class.test")
                    .body(Body::from(
                        serde_json::json!({ "cert": "test" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::OK);
        let created = body_json(create_resp).await;
        let code = created["code"].as_str().unwrap().to_string();
        let host_token = created["hostToken"].as_str().unwrap().to_string();
        assert_eq!(code.len(), 6);
        assert_eq!(
            created["joinUrl"],
            format!("http://class.test/?room={code}")
        );
        assert_eq!(created["state"]["status"], "voting");
        assert_eq!(created["state"]["currentIndex"], 0);
        assert_eq!(created["state"]["totalQuestions"], 3);
        assert!(created["state"]["currentQuestion"]["answer"].is_null());
        assert!(created["state"]["correctAnswer"].is_null());

        let current_id = created["state"]["currentQuestion"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let p1_vote = app
            .router
            .clone()
            .oneshot(post_json(
                &format!("/api/group-rooms/{code}/vote"),
                serde_json::json!({ "participantId": "p1", "answer": "A" }),
            ))
            .await
            .unwrap();
        assert_eq!(p1_vote.status(), StatusCode::OK);
        let p1_json = body_json(p1_vote).await;
        assert_eq!(p1_json["selectedAnswer"], "A");
        assert_eq!(p1_json["totalVotes"], 1);

        let p2_vote = app
            .router
            .clone()
            .oneshot(post_json(
                &format!("/api/group-rooms/{code}/vote"),
                serde_json::json!({ "participantId": "p2", "answer": "B" }),
            ))
            .await
            .unwrap();
        assert_eq!(p2_vote.status(), StatusCode::OK);

        let p1_change = app
            .router
            .clone()
            .oneshot(post_json(
                &format!("/api/group-rooms/{code}/vote"),
                serde_json::json!({ "participantId": "p1", "answer": "C" }),
            ))
            .await
            .unwrap();
        let changed = body_json(p1_change).await;
        assert_eq!(changed["selectedAnswer"], "C");
        assert_eq!(changed["totalVotes"], 2);
        let counts = changed["voteCounts"].as_array().unwrap();
        assert_eq!(
            counts.iter().find(|c| c["answer"] == "A").unwrap()["count"],
            0
        );
        assert_eq!(
            counts.iter().find(|c| c["answer"] == "C").unwrap()["count"],
            1
        );

        let forbidden_reveal = app
            .router
            .clone()
            .oneshot(post_json(
                &format!("/api/group-rooms/{code}/reveal"),
                serde_json::json!({}),
            ))
            .await
            .unwrap();
        assert_eq!(forbidden_reveal.status(), StatusCode::FORBIDDEN);

        let reveal = app
            .router
            .clone()
            .oneshot(post_empty_with_host(
                &format!("/api/group-rooms/{code}/reveal"),
                &host_token,
            ))
            .await
            .unwrap();
        assert_eq!(reveal.status(), StatusCode::OK);
        let revealed = body_json(reveal).await;
        assert_eq!(revealed["status"], "revealed");
        assert!(revealed["correctAnswer"].as_str().is_some());
        assert!(revealed["explanation"].as_str().is_some());

        let late_vote = app
            .router
            .clone()
            .oneshot(post_json(
                &format!("/api/group-rooms/{code}/vote"),
                serde_json::json!({ "participantId": "p3", "answer": "D" }),
            ))
            .await
            .unwrap();
        assert_eq!(late_vote.status(), StatusCode::BAD_REQUEST);

        let public_get = app
            .router
            .clone()
            .oneshot(
                Request::get(format!("/api/group-rooms/{code}?participantId=p1"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let public_json = body_json(public_get).await;
        assert!(public_json["correctAnswer"].as_str().is_some());
        assert!(public_json["explanation"].is_null());
        assert_eq!(public_json["selectedAnswer"], "C");

        let host_get = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/group-rooms/{code}"))
                    .header("x-group-host-token", &host_token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let host_json = body_json(host_get).await;
        assert!(host_json["explanation"].as_str().is_some());

        let next = app
            .router
            .clone()
            .oneshot(post_empty_with_host(
                &format!("/api/group-rooms/{code}/next"),
                &host_token,
            ))
            .await
            .unwrap();
        let next_json = body_json(next).await;
        assert_eq!(next_json["status"], "voting");
        assert_eq!(next_json["currentIndex"], 1);
        assert_ne!(next_json["currentQuestion"]["id"], current_id);
        assert_eq!(next_json["totalVotes"], 0);

        let end = app
            .router
            .clone()
            .oneshot(post_empty_with_host(
                &format!("/api/group-rooms/{code}/end"),
                &host_token,
            ))
            .await
            .unwrap();
        let ended = body_json(end).await;
        assert_eq!(ended["status"], "ended");
    }

    #[tokio::test]
    async fn get_group_room_rejects_unknown_code() {
        let app = build_test_app();
        let resp = app
            .router
            .oneshot(
                Request::get("/api/group-rooms/NOPE99")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_certs_returns_empty_when_questions_dir_missing() {
        let app = build_test_app_missing_questions_dir();
        let resp = app
            .router
            .oneshot(Request::get("/api/certs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["certs"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_banks_returns_empty_when_questions_dir_missing() {
        let app = build_test_app_missing_questions_dir();
        let resp = app
            .router
            .oneshot(Request::get("/api/banks").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["banks"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn post_upload_bank_creates_missing_questions_dir() {
        let app = build_test_app_missing_questions_dir();
        let body = serde_json::json!({
            "name": "newbank",
            "content": TEST_BANK_JSON,
            "overwrite": false
        });
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
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(certs, vec!["newbank"]);
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
        assert!(
            json["error"]
                .as_str()
                .unwrap()
                .contains("is not in options")
        );
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
