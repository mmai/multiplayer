use leptos::prelude::*;
use leptos_router::{components::{Route, Router, Routes, A}, path};

use crate::api::{self, MeResponse};
use crate::pages::{home::HomePage, profile::ProfilePage, game::GamePage};

#[derive(Clone, Debug)]
pub struct AuthState {
    pub user: RwSignal<Option<MeResponse>>,
}

#[component]
pub fn App() -> impl IntoView {
    let user = RwSignal::new(None::<MeResponse>);
    provide_context(AuthState { user });

    // Probe session on load.
    let auth = use_context::<AuthState>().unwrap();
    let _ = LocalResource::new(move || async move {
        if let Ok(me) = api::get_me().await {
            auth.user.set(Some(me));
        }
    });

    view! {
        <Router>
            <Nav />
            <main>
                <Routes fallback=|| view! { <p class="empty">"Page not found."</p> }>
                    <Route path=path!("/") view=HomePage />
                    <Route path=path!("/profile/:username") view=ProfilePage />
                    <Route path=path!("/games/:id") view=GamePage />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn Nav() -> impl IntoView {
    let auth = use_context::<AuthState>().unwrap();

    let logout = move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            let _ = api::post_logout().await;
            auth.user.set(None);
        });
    };

    view! {
        <nav>
            <A href="/" attr:class="brand">"Player Portal"</A>
            <span class="spacer" />
            {move || match auth.user.get() {
                Some(u) => view! {
                    <A href=format!("/profile/{}", u.username)>
                        { u.username.clone() }
                    </A>
                    <button class="btn" on:click=logout style="padding:0.25rem 0.75rem">
                        "Logout"
                    </button>
                }.into_any(),
                None => view! { <A href="/">"Login"</A> }.into_any(),
            }}
        </nav>
    }
}
