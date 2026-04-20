//! Database access layer.
//!
//! All SQLite interaction is funnelled through this module. Functions return
//! `sqlx::Result` so callers can handle errors uniformly.

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{SqlitePool, pool::PoolOptions};
use std::time::{SystemTime, UNIX_EPOCH};

/// A registered user as stored in the database.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub created_at: i64,
}

/// Aggregated game statistics for a user's public profile.
#[derive(sqlx::FromRow)]
pub struct UserStats {
    pub total: i64,
    pub wins: i64,
    pub losses: i64,
    pub draws: i64,
}

/// A condensed game entry returned by [`get_user_games`].
#[derive(sqlx::FromRow)]
pub struct GameSummary {
    pub id: i64,
    pub game_id: String,
    pub room_code: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub result: Option<String>,
    pub outcome: Option<String>,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Opens (or creates) the SQLite database at `path` and runs all pending migrations.
pub async fn init_db(path: &str) -> SqlitePool {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .expect("Failed to create database directory");
        }
    }

    let pool = PoolOptions::<sqlx::Sqlite>::new()
        .max_connections(5)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await
        .expect("Failed to open SQLite database");

    sqlx::migrate::Migrator::new(
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")),
    )
    .await
    .expect("Failed to locate migrations directory")
    .run(&pool)
    .await
    .expect("Failed to run database migrations");

    pool
}

// ── Users ────────────────────────────────────────────────────────────────────

pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    email: &str,
    password_hash: &str,
) -> sqlx::Result<i64> {
    let id = sqlx::query(
        "INSERT INTO users (username, email, password_hash, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .bind(now_unix())
    .execute(pool)
    .await?
    .last_insert_rowid();
    Ok(id)
}

pub async fn get_user_by_id(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, email, password_hash, created_at FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_user_by_username(pool: &SqlitePool, username: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, email, password_hash, created_at FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

// ── Game records ─────────────────────────────────────────────────────────────

/// Creates a new game record when a room opens. Returns the record id.
pub async fn insert_game_record(
    pool: &SqlitePool,
    game_id: &str,
    room_code: &str,
) -> sqlx::Result<i64> {
    let id = sqlx::query(
        "INSERT INTO game_records (game_id, room_code, started_at) VALUES (?, ?, ?)",
    )
    .bind(game_id)
    .bind(room_code)
    .bind(now_unix())
    .execute(pool)
    .await?
    .last_insert_rowid();
    Ok(id)
}

/// Stamps `ended_at` and stores the opaque result JSON supplied by the game.
pub async fn close_game_record(
    pool: &SqlitePool,
    record_id: i64,
    result_json: Option<&str>,
) -> sqlx::Result<()> {
    // AND ended_at IS NULL prevents overwriting a result already set by POST /games/result
    sqlx::query(
        "UPDATE game_records SET ended_at = ?, result = ? WHERE id = ? AND ended_at IS NULL",
    )
    .bind(now_unix())
    .bind(result_json)
    .bind(record_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Records a player's participation in a game. `user_id` is `None` for anonymous players.
pub async fn insert_participant(
    pool: &SqlitePool,
    record_id: i64,
    user_id: Option<i64>,
    player_id: u16,
    outcome: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO game_participants (game_record_id, user_id, player_id, outcome)
         VALUES (?, ?, ?, ?)",
    )
    .bind(record_id)
    .bind(user_id)
    .bind(player_id as i64)
    .bind(outcome)
    .execute(pool)
    .await?;
    Ok(())
}

/// Returns win/loss/draw counts for a user. All values are 0 when the user has no games.
pub async fn get_user_stats(pool: &SqlitePool, user_id: i64) -> sqlx::Result<UserStats> {
    sqlx::query_as::<_, UserStats>(
        "SELECT
             COUNT(*) as total,
             COALESCE(SUM(CASE WHEN outcome = 'win'  THEN 1 ELSE 0 END), 0) as wins,
             COALESCE(SUM(CASE WHEN outcome = 'loss' THEN 1 ELSE 0 END), 0) as losses,
             COALESCE(SUM(CASE WHEN outcome = 'draw' THEN 1 ELSE 0 END), 0) as draws
         FROM game_participants
         WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

/// Returns a paginated list of games a user participated in, newest first.
pub async fn get_user_games(
    pool: &SqlitePool,
    user_id: i64,
    page: i64,
    per_page: i64,
) -> sqlx::Result<Vec<GameSummary>> {
    sqlx::query_as::<_, GameSummary>(
        "SELECT gr.id, gr.game_id, gr.room_code, gr.started_at, gr.ended_at, gr.result, gp.outcome
         FROM game_records gr
         JOIN game_participants gp ON gp.game_record_id = gr.id
         WHERE gp.user_id = ?
         ORDER BY gr.started_at DESC
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(per_page)
    .bind(page * per_page)
    .fetch_all(pool)
    .await
}
