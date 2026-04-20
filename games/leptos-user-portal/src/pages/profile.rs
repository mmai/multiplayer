use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};

use crate::api::{self, GameSummary, UserProfile};

#[component]
pub fn ProfilePage() -> impl IntoView {
    let params = use_params_map();
    let username = move || params.read().get("username").unwrap_or_default();

    let profile = LocalResource::new(move || {
        let u = username();
        async move { api::get_user_profile(&u).await }
    });

    view! {
        <div>
            {move || match profile.get().map(|sw| sw.take()) {
                None => view! { <p class="loading">"Loading…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{ e }</p> }.into_any(),
                Some(Ok(p)) => view! { <ProfileContent profile=p username=username() /> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn ProfileContent(profile: UserProfile, username: String) -> impl IntoView {
    let page    = RwSignal::new(0i64);
    let games   = LocalResource::new(move || {
        let u = username.clone();
        let p = page.get();
        async move { api::get_user_games(&u, p).await }
    });

    let joined = crate::api::format_ts(profile.created_at);

    view! {
        <h1>{ profile.username.clone() }</h1>
        <p style="color:#777;margin-bottom:1.5rem">"Joined: " { joined }</p>

        <div class="stats-grid">
            <div class="stat-box">
                <div class="value">{ profile.total_games }</div>
                <div class="label">"Games"</div>
            </div>
            <div class="stat-box">
                <div class="value outcome-win">{ profile.wins }</div>
                <div class="label">"Wins"</div>
            </div>
            <div class="stat-box">
                <div class="value outcome-loss">{ profile.losses }</div>
                <div class="label">"Losses"</div>
            </div>
            <div class="stat-box">
                <div class="value outcome-draw">{ profile.draws }</div>
                <div class="label">"Draws"</div>
            </div>
        </div>

        <div class="card">
            <h2>"Game History"</h2>
            {move || match games.get().map(|sw| sw.take()) {
                None => view! { <p class="loading">"Loading…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{ e }</p> }.into_any(),
                Some(Ok(r)) => {
                    if r.games.is_empty() {
                        view! { <p class="empty">"No games recorded yet."</p> }.into_any()
                    } else {
                        view! { <GamesTable games=r.games page=page /> }.into_any()
                    }
                }
            }}
        </div>
    }
}

#[component]
fn GamesTable(games: Vec<GameSummary>, page: RwSignal<i64>) -> impl IntoView {
    let rows = games.clone();
    let has_next = games.len() == 20;

    view! {
        <table>
            <thead>
                <tr>
                    <th>"Room"</th>
                    <th>"Started"</th>
                    <th>"Ended"</th>
                    <th>"Outcome"</th>
                    <th>"Detail"</th>
                </tr>
            </thead>
            <tbody>
                {rows.into_iter().map(|g| {
                    let started = crate::api::format_ts(g.started_at);
                    let ended   = g.ended_at.map(crate::api::format_ts).unwrap_or_else(|| "—".into());
                    let outcome_class = match g.outcome.as_deref() {
                        Some("win")  => "outcome-win",
                        Some("loss") => "outcome-loss",
                        Some("draw") => "outcome-draw",
                        _            => "",
                    };
                    let outcome_text = g.outcome.clone().unwrap_or_else(|| "—".into());
                    view! {
                        <tr>
                            <td>{ g.room_code.clone() }</td>
                            <td>{ started }</td>
                            <td>{ ended }</td>
                            <td class=outcome_class>{ outcome_text }</td>
                            <td>
                                <A href=format!("/games/{}", g.id)>"View"</A>
                            </td>
                        </tr>
                    }
                }).collect_view()}
            </tbody>
        </table>
        <div style="display:flex;gap:0.75rem;margin-top:1rem;align-items:center">
            {move || if page.get() > 0 {
                view! {
                    <button class="btn" on:click=move |_| page.update(|p| *p -= 1)>"← Prev"</button>
                }.into_any()
            } else {
                view! { <span /> }.into_any()
            }}
            <span style="color:#777">"Page " { move || page.get() + 1 }</span>
            {if has_next {
                view! {
                    <button class="btn" on:click=move |_| page.update(|p| *p += 1)>"Next →"</button>
                }.into_any()
            } else {
                view! { <span /> }.into_any()
            }}
        </div>
    }
}
