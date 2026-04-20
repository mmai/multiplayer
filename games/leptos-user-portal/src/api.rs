use serde::{Deserialize, Serialize};

// In debug builds trunk serves on 9092 while the relay is on 8080 — use full URL.
// In release builds the portal is served by the relay itself — use relative paths.
#[cfg(debug_assertions)]
const BASE: &str = "http://localhost:8080";
#[cfg(not(debug_assertions))]
const BASE: &str = "";

fn url(path: &str) -> String {
    format!("{BASE}{path}")
}

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct MeResponse {
    pub id: i64,
    pub username: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UserProfile {
    pub id: i64,
    pub username: String,
    pub created_at: i64,
    pub total_games: i64,
    pub wins: i64,
    pub losses: i64,
    pub draws: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GameSummary {
    pub id: i64,
    pub game_id: String,
    pub room_code: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub result: Option<String>,
    pub outcome: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GamesResponse {
    pub games: Vec<GameSummary>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Participant {
    pub player_id: i64,
    pub outcome: Option<String>,
    pub username: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GameDetail {
    pub id: i64,
    pub game_id: String,
    pub room_code: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub result: Option<String>,
    pub participants: Vec<Participant>,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RegisterBody<'a> {
    pub username: &'a str,
    pub email: &'a str,
    pub password: &'a str,
}

#[derive(Serialize)]
pub struct LoginBody<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

// ── Fetch helpers ─────────────────────────────────────────────────────────────

pub async fn get_me() -> Result<MeResponse, String> {
    let resp = gloo_net::http::Request::get(&url("/auth/me"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 200 {
        resp.json::<MeResponse>().await.map_err(|e| e.to_string())
    } else {
        Err(format!("status {}", resp.status()))
    }
}

pub async fn post_login(username: &str, password: &str) -> Result<MeResponse, String> {
    let body = LoginBody { username, password };
    let resp = gloo_net::http::Request::post(&url("/auth/login"))
        .credentials(web_sys::RequestCredentials::Include)
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 200 {
        resp.json::<MeResponse>().await.map_err(|e| e.to_string())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(text)
    }
}

pub async fn post_register(username: &str, email: &str, password: &str) -> Result<MeResponse, String> {
    let body = RegisterBody { username, email, password };
    let resp = gloo_net::http::Request::post(&url("/auth/register"))
        .credentials(web_sys::RequestCredentials::Include)
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 201 {
        resp.json::<MeResponse>().await.map_err(|e| e.to_string())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(text)
    }
}

pub async fn post_logout() -> Result<(), String> {
    let resp = gloo_net::http::Request::post(&url("/auth/logout"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 204 {
        Ok(())
    } else {
        Err(format!("status {}", resp.status()))
    }
}

pub async fn get_user_profile(username: &str) -> Result<UserProfile, String> {
    let resp = gloo_net::http::Request::get(&url(&format!("/users/{username}")))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 200 {
        resp.json::<UserProfile>().await.map_err(|e| e.to_string())
    } else {
        Err(format!("status {}", resp.status()))
    }
}

pub async fn get_user_games(username: &str, page: i64) -> Result<GamesResponse, String> {
    let resp = gloo_net::http::Request::get(&url(&format!("/users/{username}/games?page={page}&per_page=20")))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 200 {
        resp.json::<GamesResponse>().await.map_err(|e| e.to_string())
    } else {
        Err(format!("status {}", resp.status()))
    }
}

pub async fn get_game_detail(id: i64) -> Result<GameDetail, String> {
    let resp = gloo_net::http::Request::get(&url(&format!("/games/{id}")))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 200 {
        resp.json::<GameDetail>().await.map_err(|e| e.to_string())
    } else {
        Err(format!("status {}", resp.status()))
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

pub fn format_ts(ts: i64) -> String {
    let ms = (ts * 1000) as f64;
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
    date.to_locale_string("en-GB", &wasm_bindgen::JsValue::UNDEFINED)
        .as_string()
        .unwrap_or_default()
}
