//! Fuzzes `bundle::Bundle::import` on arbitrary bytes as the bundle
//! stream.
//!
//! `import` streams items and salvages what it can (spec 007 §2): it
//! must never panic and must never allocate unboundedly on adversarial
//! or truncated input. `read_item` already bounds a single item to 64
//! MiB and nesting depth to 64, so a fuzz corpus that grows the input
//! only grows memory linearly, not combinatorially — this target exists
//! to catch any place that bound is missed rather than to bound memory
//! itself. The result is intentionally ignored: every input, valid or
//! not, is expected to return `Ok` (with a possibly-empty salvage
//! report) or `Err` on a bad magic / unreadable stream, never panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use evermesh_kernel::bundle::Bundle;

fuzz_target!(|data: &[u8]| {
    let _ = Bundle::import(data);
});
