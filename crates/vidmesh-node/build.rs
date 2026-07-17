//! Tauri build script.
//!
//! SCAFFOLD(phase-8): this generates the platform bundle glue (Info.plist /
//! Windows resources / embedded `tauri.conf.json`) from the config file in
//! this crate's root. It carries no app logic of its own — it must exist
//! for `tauri::generate_context!()` in `src/main.rs` to compile, on every
//! platform, without a frontend dev server running.

fn main() {
    tauri_build::build()
}
