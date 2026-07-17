//! The Vidmesh conformance suite (build plan §11): a shared vector
//! format plus per-target execution logic, used by both the vector
//! generator (`src/bin/generate.rs`) and the runner (`src/main.rs`).
//!
//! This crate is deliberately thin: all protocol behavior comes from
//! `vidmesh-kernel`. What lives here is the vector format itself
//! ([`vectors`]) and the three ways to replay a vector set against a
//! runtime ([`kernel_target`], [`node_target`], [`relay_target`]).

pub mod kernel_target;
pub mod node_target;
pub mod relay_target;
pub mod vectors;

/// Load every `*.json` vector file under `dir`, recursively, sorted by
/// path for deterministic iteration order.
pub fn load_vectors(dir: &std::path::Path) -> std::io::Result<Vec<vectors::Vector>> {
    let mut paths = Vec::new();
    collect_json_paths(dir, &mut paths)?;
    paths.sort();
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let text = std::fs::read_to_string(&path)?;
        match serde_json::from_str::<vectors::Vector>(&text) {
            Ok(v) => out.push(v),
            Err(e) => {
                eprintln!("warning: failed to parse vector {}: {e}", path.display());
            }
        }
    }
    Ok(out)
}

fn collect_json_paths(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_paths(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(path);
        }
    }
    Ok(())
}
