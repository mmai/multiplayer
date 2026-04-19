use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::api;
use crate::app::AuthState;

#[component]
pub fn HomePage() -> impl IntoView {
    let auth = use_context::<AuthState>().unwrap();
    let navigate = use_navigate();

    // Redirect to own profile when already logged in.
    Effect::new(move |_| {
        if let Some(u) = auth.user.get() {
            navigate(&format!("/profile/{}", u.username), Default::default());
        }
    });

    let tab = RwSignal::new("login");

    view! {
        <div class="card" style="max-width:420px;margin:3rem auto">
            <div class="tabs">
                <button
                    class=move || if tab.get() == "login" { "tab-btn active" } else { "tab-btn" }
                    on:click=move |_| tab.set("login")
                >"Login"</button>
                <button
                    class=move || if tab.get() == "register" { "tab-btn active" } else { "tab-btn" }
                    on:click=move |_| tab.set("register")
                >"Register"</button>
            </div>
            {move || if tab.get() == "login" {
                view! { <LoginForm /> }.into_any()
            } else {
                view! { <RegisterForm /> }.into_any()
            }}
        </div>
    }
}

#[component]
fn LoginForm() -> impl IntoView {
    let auth = use_context::<AuthState>().unwrap();
    let navigate = use_navigate();

    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error    = RwSignal::new(String::new());
    let pending  = RwSignal::new(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if pending.get() { return; }
        pending.set(true);
        error.set(String::new());
        let u = username.get();
        let p = password.get();
        let navigate = navigate.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match api::post_login(&u, &p).await {
                Ok(me) => {
                    let dest = format!("/profile/{}", me.username);
                    auth.user.set(Some(me));
                    navigate(&dest, Default::default());
                }
                Err(e) => {
                    error.set(e);
                    pending.set(false);
                }
            }
        });
    };

    view! {
        <form on:submit=submit>
            <label>"Username"</label>
            <input type="text" required
                prop:value=move || username.get()
                on:input=move |ev| username.set(event_target_value(&ev)) />
            <label>"Password"</label>
            <input type="password" required
                prop:value=move || password.get()
                on:input=move |ev| password.set(event_target_value(&ev)) />
            <button type="submit" disabled=move || pending.get()>"Login"</button>
            {move || if !error.get().is_empty() {
                view! { <p class="error">{ error.get() }</p> }.into_any()
            } else {
                view! { <span /> }.into_any()
            }}
        </form>
    }
}

#[component]
fn RegisterForm() -> impl IntoView {
    let auth = use_context::<AuthState>().unwrap();
    let navigate = use_navigate();

    let username = RwSignal::new(String::new());
    let email    = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error    = RwSignal::new(String::new());
    let pending  = RwSignal::new(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if pending.get() { return; }
        pending.set(true);
        error.set(String::new());
        let u = username.get();
        let e = email.get();
        let p = password.get();
        let navigate = navigate.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match api::post_register(&u, &e, &p).await {
                Ok(me) => {
                    let dest = format!("/profile/{}", me.username);
                    auth.user.set(Some(me));
                    navigate(&dest, Default::default());
                }
                Err(err) => {
                    error.set(err);
                    pending.set(false);
                }
            }
        });
    };

    view! {
        <form on:submit=submit>
            <label>"Username"</label>
            <input type="text" required
                prop:value=move || username.get()
                on:input=move |ev| username.set(event_target_value(&ev)) />
            <label>"Email"</label>
            <input type="email" required
                prop:value=move || email.get()
                on:input=move |ev| email.set(event_target_value(&ev)) />
            <label>"Password"</label>
            <input type="password" required
                prop:value=move || password.get()
                on:input=move |ev| password.set(event_target_value(&ev)) />
            <button type="submit" disabled=move || pending.get()>"Register"</button>
            {move || if !error.get().is_empty() {
                view! { <p class="error">{ error.get() }</p> }.into_any()
            } else {
                view! { <span /> }.into_any()
            }}
        </form>
    }
}
