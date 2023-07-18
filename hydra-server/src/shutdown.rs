use crate::AppState;
use axum::{http::Request, middleware::Next, response::Response};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::UNIX_EPOCH,
};
use tokio::{signal::unix::SignalKind, time};

// puts the server to sleep (shuts down) after 5 minutes of inactivity.
const TIME: u64 = 5 * 60; // 5 minutes

const POLL: u32 = 30; // check every 30 seconds

static LAST_REQUEST_TIME: AtomicU64 = AtomicU64::new(0);

pub async fn update_last_activity_middleware<B>(req: Request<B>, next: Next<B>) -> Response {
    update_last_activity();
    next.run(req).await
}

pub fn update_last_activity() {
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs();

    LAST_REQUEST_TIME.store(now, Ordering::Relaxed);
}

pub async fn run_check_test(state: AppState) {
    loop {
        time::sleep(tokio::time::Duration::from_secs(POLL.into())).await;

        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();

        let last = LAST_REQUEST_TIME.load(Ordering::Relaxed);

        if now - last > TIME.into() {
            log::info!("Shutting down due to inactivity.");
            cleanup(state).await;
            log::info!("Done. Exiting.");
            std::process::exit(0);
        }
    }
}

async fn cleanup(state: AppState) {
    // removing all containers
    if let Err(e) = state.read().await.container_pool.shutdown().await {
        log::error!("Failed to shutdown container pool: {}", e);
    }
}

pub async fn signal_handler(state: AppState) {
    let mut term_signal = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("failed to install SIGTERM signal handler");

    // listen to the stop signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = term_signal.recv() => {}
    }

    // print a newline after the ^C
    println!();

    log::info!("Received signal. Shutting down..");
    cleanup(state.clone()).await;

    log::info!("Done. Exiting.");
    std::process::exit(0);
}
