//! Evermesh relay binary.
//!
//! A thin entry point: load config, open the store, wire up the axum
//! router and gossip clients, serve, and shut down gracefully on
//! Ctrl+C/SIGTERM. All protocol logic lives in the library crate
//! (`evermesh_relay` and its modules) so it can be unit-tested without
//! a running process.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use axum::routing::{get, put};
use axum::Router;

use evermesh_relay::{blobs, config::RelayConfig, gossip, info, store::Store, sync, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("EVERMESH_RELAY_CONFIG").ok())
        .map(PathBuf::from);

    let config = match config_path {
        Some(path) => {
            tracing::info!("loading config from {}", path.display());
            RelayConfig::load(&path)?
        }
        None => {
            tracing::warn!(
                "no config path given (argv[1] or EVERMESH_RELAY_CONFIG unset); using defaults"
            );
            RelayConfig::default()
        }
    };

    let listen_addr = config.listen_addr.clone();
    let db_path = config.db_path.clone();
    tracing::info!("opening store at {db_path}");
    let store = Store::open(&db_path)?;

    let state = AppState::new(config, store);

    if state.config.blob.enabled {
        tracing::info!("blob sidecar enabled under {}", state.config.blob.dir);
    }
    if !state.config.peers.is_empty() {
        tracing::info!(
            "gossiping with {} configured peer(s)",
            state.config.peers.len()
        );
        gossip::spawn_all(state.clone());
    }

    let app = Router::new()
        .route("/sync", get(sync::sync_handler))
        .route("/info", get(info::get_info))
        .route("/blob", put(blobs::put_blob))
        .route("/blob/:id", get(blobs::get_blob).head(blobs::head_blob))
        .route("/blob/:id/proof", get(blobs::get_blob_proof))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!("evermesh-relay listening on {listen_addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Waits for Ctrl+C (all platforms) or SIGTERM (Unix), whichever comes
/// first, so container orchestrators get a clean graceful shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl+C, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
