//! HTTP endpoints for user management (Phases 2 & 4).
//!
//! Routes:
//!   POST /auth/register
//!   POST /auth/login
//!   POST /auth/logout
//!   GET  /auth/me
//!   GET  /users/:username
//!   GET  /users/:username/games?page=0&per_page=20
//!   POST /games/result

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_login::AuthSession;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

use crate::auth::{AuthBackend, Credentials, hash_password};
use crate::db;
use crate::lobby::AppState;

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/users/{username}", get(user_profile))
        .route("/users/{username}/games", get(user_games))
        .route("/games/result", post(game_result))
        .route("/games/{id}", get(game_detail))
}

// ── Error type ────────────────────────────────────────────────────────────────

enum AppError {
    Database(sqlx::Error),
    NotFound,
    Conflict(&'static str),
    BadRequest(&'static str),
    Unauthorized,
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Database(e) => {
                tracing::error!("database error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
            }
            AppError::NotFound => StatusCode::NOT_FOUND.into_response(),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg).into_response(),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            AppError::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
            AppError::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Database(e)
    }
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db_err) if db_err.message().contains("UNIQUE constraint failed"))
}

// ── Request / response bodies ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct RegisterBody {
    username: String,
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct MeResponse {
    id: i64,
    username: String,
}

#[derive(Serialize)]
struct UserProfileResponse {
    id: i64,
    username: String,
    created_at: i64,
    total_games: i64,
    wins: i64,
    losses: i64,
    draws: i64,
}

#[derive(Deserialize)]
struct GamesQuery {
    #[serde(default)]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
}

fn default_per_page() -> i64 {
    20
}

#[derive(Serialize)]
struct GamesResponse {
    games: Vec<GameSummaryResponse>,
}

#[derive(Serialize)]
struct GameSummaryResponse {
    id: i64,
    game_id: String,
    room_code: String,
    started_at: i64,
    ended_at: Option<i64>,
    result: Option<String>,
    outcome: Option<String>,
}

impl From<db::GameSummary> for GameSummaryResponse {
    fn from(g: db::GameSummary) -> Self {
        Self {
            id: g.id,
            game_id: g.game_id,
            room_code: g.room_code,
            started_at: g.started_at,
            ended_at: g.ended_at,
            result: g.result,
            outcome: g.outcome,
        }
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn register(
    mut auth_session: AuthSession<AuthBackend>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterBody>,
) -> Result<impl IntoResponse, AppError> {
    if body.username.len() < 3 || body.username.len() > 30 {
        return Err(AppError::BadRequest("username must be 3–30 characters"));
    }
    if body.password.len() < 8 {
        return Err(AppError::BadRequest("password must be at least 8 characters"));
    }
    if !body.email.contains('@') {
        return Err(AppError::BadRequest("invalid email address"));
    }

    let hash = hash_password(&body.password).map_err(|_| AppError::Internal)?;

    let user_id = db::create_user(&state.db, &body.username, &body.email, &hash)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AppError::Conflict("username or email already taken")
            } else {
                AppError::Database(e)
            }
        })?;

    let user = db::get_user_by_id(&state.db, user_id)
        .await?
        .ok_or(AppError::Internal)?;

    auth_session.login(&user).await.map_err(|_| AppError::Internal)?;

    Ok((
        StatusCode::CREATED,
        Json(MeResponse {
            id: user.id,
            username: user.username,
        }),
    ))
}

async fn login(
    mut auth_session: AuthSession<AuthBackend>,
    Json(body): Json<LoginBody>,
) -> Result<impl IntoResponse, AppError> {
    let creds = Credentials {
        username: body.username,
        password: body.password,
    };

    let user = match auth_session.authenticate(creds).await {
        Ok(Some(u)) => u,
        Ok(None) => return Err(AppError::Unauthorized),
        Err(_) => return Err(AppError::Internal),
    };

    auth_session.login(&user).await.map_err(|_| AppError::Internal)?;

    Ok(Json(MeResponse {
        id: user.id,
        username: user.username,
    }))
}

async fn logout(mut auth_session: AuthSession<AuthBackend>) -> Result<StatusCode, AppError> {
    auth_session.logout().await.map_err(|_| AppError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(auth_session: AuthSession<AuthBackend>) -> Result<impl IntoResponse, AppError> {
    match auth_session.user {
        Some(user) => Ok(Json(MeResponse {
            id: user.id,
            username: user.username,
        })
        .into_response()),
        None => Ok(StatusCode::UNAUTHORIZED.into_response()),
    }
}

async fn user_profile(
    Path(username): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let user = db::get_user_by_username(&state.db, &username)
        .await?
        .ok_or(AppError::NotFound)?;

    let stats = db::get_user_stats(&state.db, user.id).await?;

    Ok(Json(UserProfileResponse {
        id: user.id,
        username: user.username,
        created_at: user.created_at,
        total_games: stats.total,
        wins: stats.wins,
        losses: stats.losses,
        draws: stats.draws,
    }))
}

async fn user_games(
    Path(username): Path<String>,
    Query(query): Query<GamesQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let per_page = query.per_page.clamp(1, 100);
    let page = query.page.max(0);

    let user = db::get_user_by_username(&state.db, &username)
        .await?
        .ok_or(AppError::NotFound)?;

    let summaries = db::get_user_games(&state.db, user.id, page, per_page).await?;

    Ok(Json(GamesResponse {
        games: summaries.into_iter().map(Into::into).collect(),
    }))
}

// ── Game detail (Phase 5) ─────────────────────────────────────────────────────

#[derive(sqlx::FromRow, Serialize)]
struct GameRecordRow {
    id: i64,
    game_id: String,
    room_code: String,
    started_at: i64,
    ended_at: Option<i64>,
    result: Option<String>,
}

#[derive(sqlx::FromRow, Serialize)]
struct ParticipantWithUsername {
    player_id: i64,
    outcome: Option<String>,
    username: Option<String>,
}

#[derive(Serialize)]
struct GameDetailResponse {
    id: i64,
    game_id: String,
    room_code: String,
    started_at: i64,
    ended_at: Option<i64>,
    result: Option<String>,
    participants: Vec<ParticipantWithUsername>,
}

async fn game_detail(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let record = sqlx::query_as::<_, GameRecordRow>(
        "SELECT id, game_id, room_code, started_at, ended_at, result
         FROM game_records WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let participants = sqlx::query_as::<_, ParticipantWithUsername>(
        "SELECT gp.player_id, gp.outcome, u.username
         FROM game_participants gp
         LEFT JOIN users u ON u.id = gp.user_id
         WHERE gp.game_record_id = ?
         ORDER BY gp.player_id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(GameDetailResponse {
        id: record.id,
        game_id: record.game_id,
        room_code: record.room_code,
        started_at: record.started_at,
        ended_at: record.ended_at,
        result: record.result,
        participants,
    }))
}

// ── Game result recording (Phase 4) ──────────────────────────────────────────

#[derive(Deserialize)]
struct GameResultBody {
    room_code: String,
    game_id: String,
    /// Opaque game-specific result, stored verbatim as JSON.
    result: JsonValue,
    /// Per-player outcomes keyed by player_id as a string ("0", "1", …).
    /// Accepted values: "win", "loss", "draw". Missing keys → NULL outcome.
    #[serde(default)]
    outcomes: HashMap<String, String>,
}

#[derive(Serialize)]
struct GameResultResponse {
    game_record_id: i64,
}

/// Called by the WASM host when a game ends.
///
/// The room code + game ID act as the shared secret (same trust level as WS join).
/// `close_game_record` is idempotent (no-op if already closed), and participant
/// inserts use `INSERT OR IGNORE`, so safe retries are supported.
async fn game_result(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GameResultBody>,
) -> Result<impl IntoResponse, AppError> {
    let compound_id = format!("{}#{}", body.room_code, body.game_id);

    // Snapshot the fields we need while holding the lock, then release immediately.
    let (game_record_id, user_ids) = {
        let rooms = state.rooms.lock().await;
        let room = rooms.get(&compound_id).ok_or(AppError::NotFound)?;
        let record_id = room
            .game_record_id
            .ok_or(AppError::NotFound)?;
        (record_id, room.user_ids.clone())
    };

    let result_json = serde_json::to_string(&body.result)
        .map_err(|_| AppError::BadRequest("could not serialise result"))?;

    db::close_game_record(&state.db, game_record_id, Some(&result_json)).await?;

    for (player_id, user_id) in &user_ids {
        let outcome = body.outcomes.get(&player_id.to_string()).map(String::as_str);
        db::insert_participant(&state.db, game_record_id, *user_id, *player_id, outcome).await?;
    }

    tracing::info!(
        game_record_id,
        room = body.room_code,
        "Game result recorded"
    );

    Ok(Json(GameResultResponse { game_record_id }))
}
