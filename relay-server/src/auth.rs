//! Authentication backend for axum-login.
//!
//! Implements [`AuthUser`] on [`db::User`] and provides [`AuthBackend`] which
//! validates credentials against the database using Argon2 password hashing.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::password_hash::rand_core::OsRng;
use argon2::Argon2;
use axum_login::{AuthUser, AuthnBackend, UserId};
use sqlx::SqlitePool;

use crate::db;

// ── AuthUser ─────────────────────────────────────────────────────────────────

impl AuthUser for db::User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    /// Changing the password invalidates all existing sessions for this user.
    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}

// ── Credentials ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("password hashing error")]
    PasswordHash,
}

// ── Backend ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AuthBackend {
    pool: SqlitePool,
}

impl AuthBackend {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl AuthnBackend for AuthBackend {
    type User = db::User;
    type Credentials = Credentials;
    type Error = AuthError;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let Some(user) = db::get_user_by_username(&self.pool, &creds.username).await? else {
            return Ok(None);
        };

        let parsed = PasswordHash::new(&user.password_hash).map_err(|_| AuthError::PasswordHash)?;
        let valid = Argon2::default()
            .verify_password(creds.password.as_bytes(), &parsed)
            .is_ok();

        Ok(valid.then_some(user))
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        Ok(db::get_user_by_id(&self.pool, *user_id).await?)
    }
}

// ── Password hashing helper ───────────────────────────────────────────────────

/// Hashes a plaintext password with Argon2id. Used by the registration endpoint.
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| AuthError::PasswordHash)
}
