//! The engine's own thread: a small tokio runtime running `limits_core`'s engine,
//! plus a forwarder that wakes the UI thread on every state change.

use std::{sync::mpsc as std_mpsc, thread::JoinHandle};

use limits_core::{spawn_engine, AppState, Command, EngineDeps};
use tokio::sync::{mpsc, watch};

/// Starts the engine on a dedicated thread (tokio multi-thread runtime, 2 workers).
/// `wake` runs on that thread after every state change (and once at start).
pub fn start(
    deps: EngineDeps,
    wake: impl Fn() + Send + 'static,
) -> (
    mpsc::Sender<Command>,
    watch::Receiver<AppState>,
    JoinHandle<()>,
) {
    let (ready_tx, ready_rx) = std_mpsc::channel();
    let handle = std::thread::Builder::new()
        .name("engine".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("engine-worker")
                .enable_all()
                .build()
                .expect("tokio runtime");
            runtime.block_on(async move {
                let (cmd, state) = spawn_engine(deps);
                let _ = ready_tx.send((cmd, state.clone()));
                forward(state, wake).await;
            });
        })
        .expect("spawn the engine thread");
    let (cmd, state) = ready_rx.recv().expect("the engine thread started");
    (cmd, state, handle)
}

/// Logs each `conn` change and calls `wake` on every state change, until the
/// engine stops.
async fn forward(mut rx: watch::Receiver<AppState>, wake: impl Fn()) {
    let mut last_conn = None;
    loop {
        let conn = rx.borrow_and_update().conn.clone();
        if last_conn.as_ref() != Some(&conn) {
            tracing::info!(?conn, "state");
            last_conn = Some(conn);
        }
        wake();
        if rx.changed().await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;

    #[tokio::test(flavor = "current_thread")]
    async fn forwarder_wakes_on_every_change_and_ends_with_the_sender() {
        let (tx, rx) = watch::channel(AppState::default());
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let task = tokio::spawn(forward(rx, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(wakes.load(Ordering::SeqCst), 1, "once at start");
        tx.send_modify(|s| s.reset_seq += 1);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(wakes.load(Ordering::SeqCst), 2);
        drop(tx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("ends when the engine drops its sender")
            .unwrap();
    }
}
