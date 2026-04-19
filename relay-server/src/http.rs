//! HTTP endpoints for user management (Phase 2).
//!
//! Routes:
//!   POST /auth/register
//!   POST /auth/login
//!   POST /auth/logout
//!   GET  /auth/me
//!   GET  /users/:username
//!   GET  /users/:username/games?page=0&per_page=20

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_login::AuthSession;
use serde::{Deserialize, Serialize};
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
