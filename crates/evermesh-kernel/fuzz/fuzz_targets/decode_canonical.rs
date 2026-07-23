//! Fuzzes `codec::decode_canonical` on arbitrary bytes.
//!
//! The primary invariant (build plan §6: no panics on untrusted input)
//! is that `decode_canonical` never panics — it returns `Ok` or `Err`.
//! The secondary, more valuable invariant is canonicality itself: any
//! value `decode_canonical` accepts must re-encode, via
//! `encode_canonical`, to the exact same bytes it was decoded from. A
//! failure of that invariant means the decoder accepted a non-canonical
//! form (or the encoder disagrees with the decoder about what canonical
//! means) — a real canonicality bug, not a fuzzer false positive, so it
//! panics loudly with both sides in hex.

#![no_main]

use libfuzzer_sys::fuzz_target;
use evermesh_kernel::codec;

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fuzz_target!(|data: &[u8]| {
    let Ok(value) = codec::decode_canonical(data) else {
        return;
    };
    match codec::encode_canonical(&value) {
        Ok(reencoded) if reencoded == data => {}
        Ok(reencoded) => panic!(
            "canonicality violation: decode_canonical accepted bytes that \
             encode_canonical does not reproduce\n  input:  {}\n  output: {}",
            to_hex(data),
            to_hex(&reencoded),
        ),
        Err(e) => panic!(
            "canonicality violation: decode_canonical accepted a value that \
             encode_canonical then rejects ({e})\n  input: {}",
            to_hex(data),
        ),
    }
});
