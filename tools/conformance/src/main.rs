//! `vidmesh-conformance run` — the conformance suite runner (build plan
//! §11).
//!
//! ```text
//! vidmesh-conformance run --vectors <dir> [--target kernel|node|relay]
//!                          [--node-harness <path>] [--relay-url <ws url>]
//! ```
//!
//! Executes every vector under `<dir>` (default `tools/conformance/vectors`,
//! resolved relative to this crate's manifest directory when a relative
//! path is given) against the chosen target, prints a per-group
//! pass/fail/skip table, and exits nonzero if anything failed. This is
//! the "golden rule" instrument: the same vectors must pass identically
//! against the kernel crate, `@vidmesh/kernel` under Node, and a live
//! relay.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vidmesh_conformance::kernel_target::{self, Outcome};
use vidmesh_conformance::load_vectors;
use vidmesh_conformance::node_target::{self, NodeHarness};
use vidmesh_conformance::relay_target::{self, RelayConn};
use vidmesh_conformance::vectors::Vector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Kernel,
    Node,
    Relay,
}

struct Args {
    vectors_dir: PathBuf,
    target: Target,
    node_harness: PathBuf,
    relay_url: String,
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn parse_args() -> Result<Args, String> {
    let mut argv = std::env::args().skip(1);
    let sub = argv.next().unwrap_or_default();
    if sub != "run" {
        return Err(format!(
            "usage: vidmesh-conformance run --vectors <dir> [--target kernel|node|relay] \
             [--node-harness <path>] [--relay-url <ws url>]\n(got subcommand {sub:?})"
        ));
    }
    let mut vectors_dir = crate_dir().join("vectors");
    let mut target = Target::Kernel;
    let mut node_harness = crate_dir().join("node-harness.mjs");
    let mut relay_url = "ws://127.0.0.1:8787/sync".to_string();

    let rest: Vec<String> = argv.collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--vectors" => {
                i += 1;
                vectors_dir = PathBuf::from(rest.get(i).ok_or("--vectors needs a value")?);
            }
            "--target" => {
                i += 1;
                target = match rest.get(i).map(String::as_str) {
                    Some("kernel") => Target::Kernel,
                    Some("node") => Target::Node,
                    Some("relay") => Target::Relay,
                    other => {
                        return Err(format!("--target must be kernel|node|relay, got {other:?}"))
                    }
                };
            }
            "--node-harness" => {
                i += 1;
                node_harness = PathBuf::from(rest.get(i).ok_or("--node-harness needs a value")?);
            }
            "--relay-url" => {
                i += 1;
                relay_url = rest.get(i).ok_or("--relay-url needs a value")?.clone();
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
        i += 1;
    }
    Ok(Args {
        vectors_dir,
        target,
        node_harness,
        relay_url,
    })
}

struct GroupTally {
    pass: usize,
    fail: usize,
    skip: usize,
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    let vectors = match load_vectors(&args.vectors_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "failed to load vectors from {}: {e}",
                args.vectors_dir.display()
            );
            std::process::exit(2);
        }
    };
    if vectors.is_empty() {
        eprintln!(
            "no vectors found under {} — run `cargo run --bin generate` first",
            args.vectors_dir.display()
        );
        std::process::exit(2);
    }

    let results: Vec<(Vector, Outcome)> = match args.target {
        Target::Kernel => run_kernel(&vectors),
        Target::Node => run_node(&vectors, &args.node_harness),
        Target::Relay => run_relay(&vectors, &args.relay_url),
    };

    let any_fail = print_report(&results);
    std::process::exit(if any_fail { 1 } else { 0 });
}

fn run_kernel(vectors: &[Vector]) -> Vec<(Vector, Outcome)> {
    vectors
        .iter()
        .map(|v| (v.clone(), kernel_target::run(v)))
        .collect()
}

fn run_node(vectors: &[Vector], harness_path: &Path) -> Vec<(Vector, Outcome)> {
    let mut harness = match NodeHarness::spawn(harness_path) {
        Ok(h) => h,
        Err(e) => {
            eprintln!(
                "failed to spawn node harness at {} ({e}). Requires Node >= 22.6 and \
                 crates/vidmesh-wasm built into packages/kernel-ts/wasm/ (see README).",
                harness_path.display()
            );
            std::process::exit(2);
        }
    };
    vectors
        .iter()
        .map(|v| (v.clone(), node_target::run(&mut harness, v)))
        .collect()
}

fn run_relay(vectors: &[Vector], relay_url: &str) -> Vec<(Vector, Outcome)> {
    let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
    runtime.block_on(async move {
        let mut conn = match RelayConn::connect(relay_url).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("failed to connect to relay at {relay_url}: {e}");
                std::process::exit(2);
            }
        };
        if let Err(e) = conn.req_roundtrip("conformance-smoke-test").await {
            eprintln!("REQ/EOSE smoke test failed: {e}");
            std::process::exit(2);
        }
        let mut out = Vec::with_capacity(vectors.len());
        for v in vectors {
            let outcome = relay_target::run(&mut conn, v).await;
            out.push((v.clone(), outcome));
        }
        out
    })
}

/// Print the per-group table and the detail of every failure. Returns
/// whether any vector failed.
fn print_report(results: &[(Vector, Outcome)]) -> bool {
    let mut tally: BTreeMap<String, GroupTally> = BTreeMap::new();
    let mut failures: Vec<(&Vector, &Outcome)> = Vec::new();

    for (v, outcome) in results {
        let entry = tally.entry(v.group.clone()).or_insert(GroupTally {
            pass: 0,
            fail: 0,
            skip: 0,
        });
        match outcome {
            Outcome::Pass => entry.pass += 1,
            Outcome::Fail(_) => {
                entry.fail += 1;
                failures.push((v, outcome));
            }
            Outcome::Skip(_) => entry.skip += 1,
        }
    }

    let width = tally
        .keys()
        .map(String::len)
        .max()
        .unwrap_or(5)
        .max("GROUP".len());
    println!(
        "{:width$}  {:>6}  {:>6}  {:>6}",
        "GROUP",
        "PASS",
        "FAIL",
        "SKIP",
        width = width
    );
    let mut total_pass = 0;
    let mut total_fail = 0;
    let mut total_skip = 0;
    for (group, t) in &tally {
        println!(
            "{:width$}  {:>6}  {:>6}  {:>6}",
            group,
            t.pass,
            t.fail,
            t.skip,
            width = width
        );
        total_pass += t.pass;
        total_fail += t.fail;
        total_skip += t.skip;
    }
    println!(
        "{:width$}  {:>6}  {:>6}  {:>6}",
        "TOTAL",
        total_pass,
        total_fail,
        total_skip,
        width = width
    );

    if !failures.is_empty() {
        println!("\nFAILURES:");
        for (v, outcome) in &failures {
            let Outcome::Fail(msg) = outcome else {
                unreachable!()
            };
            println!("  {}/{}: {msg}", v.group, v.name);
        }
    }

    total_fail > 0
}
