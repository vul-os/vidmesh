//! Vidmesh node app (Tauri 2 scaffold, Phase 8).
//!
//! Per build plan §2/§4, the node is **scaffold only** in v1: this binary
//! boots a plain Tauri shell around the static `ui/` directory (see
//! `tauri.conf.json`: `frontendDist: "./ui"`, no `devUrl`). There is no
//! frontend build step and no dev-server dependency — `cargo check -p
//! vidmesh-node` must succeed on a bare toolchain, without the `tauri` CLI
//! installed.
//!
//! Real pinning/seeding/budget enforcement does not exist yet. See
//! `src/pinning.rs` for the documented v1 design surface; the commands
//! below only read its scaffold (always-empty) state.

#![forbid(unsafe_code)]

mod pinning;

use std::path::PathBuf;

use pinning::PinStore;
use serde::Serialize;

/// Response shape for the `node_status` command.
#[derive(Serialize)]
struct NodeStatus {
    version: &'static str,
    pinned_count: u64,
    seeding: bool,
}

/// Response shape for the `budgets` command.
#[derive(Serialize)]
struct BudgetsResponse {
    disk_gb: u64,
    bandwidth_mbps: u64,
}

/// SCAFFOLD(phase-8): placeholder app-data directory. The real
/// implementation will resolve this via Tauri's path resolver
/// (`app.path().app_data_dir()`) from a running `AppHandle` once the app
/// has a real init flow. This never needs to exist on disk: `PinStore::open`
/// only computes a path, it never opens or creates the file.
fn scaffold_data_dir() -> PathBuf {
    PathBuf::from(".vidmesh-node")
}

/// Tauri command: node status summary shown in the shell UI header.
///
/// SCAFFOLD(phase-8): `pinned_count` comes from the scaffold `PinStore`,
/// which is always empty; `seeding` is hardcoded false until a seeding
/// session exists.
#[tauri::command]
fn node_status() -> NodeStatus {
    let store = PinStore::open(scaffold_data_dir());
    NodeStatus {
        version: env!("CARGO_PKG_VERSION"),
        pinned_count: store.pinned_count(),
        seeding: false,
    }
}

/// Tauri command: this node's own disk/bandwidth budgets (spec 000 §4 —
/// nodes "honor their own budgets").
///
/// SCAFFOLD(phase-8): reads the scaffold `PinStore`, which always reports
/// the zero/unconfigured budget until a settings UI exists.
#[tauri::command]
fn budgets() -> BudgetsResponse {
    let store = PinStore::open(scaffold_data_dir());
    let budget = store.budget();
    BudgetsResponse {
        disk_gb: budget.disk_gb,
        bandwidth_mbps: budget.bandwidth_mbps,
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![node_status, budgets])
        .run(tauri::generate_context!())
        .expect("error while running vidmesh-node scaffold");
}
