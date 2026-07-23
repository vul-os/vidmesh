//! Fuzzes `codec::from_json` on arbitrary bytes interpreted as UTF-8.
//!
//! `from_json` is a hand-rolled parser (build plan §6: no panics on
//! untrusted input) so it must reject malformed JSON with `Err`, never
//! panic — including on truncated escapes, lone surrogates, oversized
//! integers, and excessive nesting depth. Any value it does accept must
//! round-trip through `to_json` -> `from_json` (spec 001 §11).

#![no_main]

use libfuzzer_sys::fuzz_target;
use evermesh_kernel::codec;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(value) = codec::from_json(s) else {
        return;
    };
    let json = codec::to_json(&value);
    let back = codec::from_json(&json)
        .unwrap_or_else(|e| panic!("to_json produced input from_json cannot parse: {e}\n{json}"));
    assert_eq!(back, value, "to_json -> from_json did not round-trip");
});
