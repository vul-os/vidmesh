//! Fuzzes `Record::from_cbor` on arbitrary bytes.
//!
//! `from_cbor` must never panic on untrusted input (build plan §6),
//! including on records with a garbage signature or an unknown
//! `sig_alg` — `verify()` reports that as `Err`, not a panic. Any record
//! `from_cbor` accepts must also round-trip exactly through
//! `to_canonical_cbor` (the record envelope, like the codec underneath
//! it, is supposed to be canonical-in canonical-out).

#![no_main]

use libfuzzer_sys::fuzz_target;
use evermesh_kernel::record::Record;

fuzz_target!(|data: &[u8]| {
    let Ok(record) = Record::from_cbor(data) else {
        return;
    };
    // Signature verification on attacker-controlled key/signature bytes
    // must never panic; the result itself is uninteresting here.
    let _ = record.verify();
    let _ = record.id();

    let reencoded = record.to_canonical_cbor();
    assert_eq!(
        reencoded, data,
        "Record::from_cbor accepted bytes that to_canonical_cbor does not reproduce"
    );
});
