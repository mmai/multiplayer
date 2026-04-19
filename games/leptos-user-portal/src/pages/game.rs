use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};

use crate::api::{self, GameDetail, Participant};

#[component]
pub fn GamePage() -> impl IntoView {
    let params = use_params_map();
    let id_str = move || params.read().get("id").unwrap_or_default();

    let detail = LocalResource::new(move || {
        let s = id_str();
        async move {
            let id: i64 = s.parse().map_err(|_| "invalid game id".to_string())?;
            api::get_game_detail(id).await
        }
    });

    view! {
        <div>
            {move || match detail.get().map(|sw| sw.take()) {
                None => view! { <p class="loading">"Loading…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{ e }</p> }.into_any(),
                Some(Ok(g)) => view! { <GameDetailView game=g /> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn GameDetailView(game: GameDetail) -> impl IntoView {
    let started = api::format_ts(game.started_at);
    let ended   = game.ended_at.map(api::format_ts).unwrap_or_else(|| "ongoing".into());

    view! {
        <div class="card">
            <h1 style="margin-bottom:0.25rem">"Game " { game.room_code.clone() }</h1>
            <p style="color:#777;margin-bottom:1.5rem">
                "Started: " { started.clone() } " · Ended: " { ended }
            </p>

            <h2>"Players"</h2>
            <table>
                <thead>
                    <tr>
                        <th>"Player"</th>
                        <th>"Username"</th>
                        <th>"Outcome"</th>
                    </tr>
                </thead>
                <tbody>
                    {game.participants.iter().map(|p| {
                        view! { <ParticipantRow participant=p.clone() /> }
                    }).collect_view()}
                </tbody>
            </table>

            {game.result.as_ref().map(|r| view! {
                <div style="margin-top:1.5rem">
                    <h2>"Result data"</h2>
                    <pre style="background:#f5f5f5;padding:0.75rem;border-radius:5px;overflow:auto;font-size:0.85rem">
                        { r.clone() }
                    </pre>
                </div>
            })}
        </div>
    }
}

#[component]
fn ParticipantRow(participant: Participant) -> impl IntoView {
    let outcome_class = match participant.outcome.as_deref() {
        Some("win")  => "outcome-win",
        Some("loss") => "outcome-loss",
        Some("draw") => "outcome-draw",
        _            => "",
    };
    let outcome_text = participant.outcome.clone().unwrap_or_else(|| "—".into());
    let name = participant.username.clone();

    view! {
        <tr>
            <td>"Player " { participant.player_id }</td>
            <td>
                {match name {
                    Some(u) => view! {
                        <A href=format!("/profile/{u}")>{ u }</A>
                    }.into_any(),
                    None => view! { <span style="color:#aaa">"anonymous"</span> }.into_any(),
                }}
            </td>
            <td class=outcome_class>{ outcome_text }</td>
        </tr>
    }
}
