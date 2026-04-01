use std::future::Future;

/// Spawns a background task.
/// - WASM: uses `wasm_bindgen_futures::spawn_local` (no Send required)
/// - Native: spawns an OS thread running `futures::executor::block_on`
#[cfg(target_arch = "wasm32")]
pub fn spawn_task<F>(fut: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(fut);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_task<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    std::thread::spawn(move || {
        futures::executor::block_on(fut);
    });
}

/// Yields for approximately `ms` milliseconds.
/// - WASM: non-blocking yield via browser timer
/// - Native: blocks the current thread (safe on a dedicated background thread)
#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: u32) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
}

/// Platform-agnostic bound for types that can be moved into a background task.
/// - WASM: only requires `'static` (single-threaded, no Send needed)
/// - Native: requires `Send + 'static`
#[cfg(target_arch = "wasm32")]
pub trait TaskBound: 'static {}
#[cfg(target_arch = "wasm32")]
impl<T: 'static> TaskBound for T {}

#[cfg(not(target_arch = "wasm32"))]
pub trait TaskBound: Send + 'static {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + 'static> TaskBound for T {}
