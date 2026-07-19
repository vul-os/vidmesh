//! Canonical CBOR codec and JSON interchange (spec 001 §2, §11).
//!
//! This module is self-contained (std only, no external crates) and
//! consensus-critical: [`encode_canonical`] must be byte-identical across
//! independent implementations, and [`decode_canonical`] must reject any
//! non-canonical encoding rather than normalize it (spec 001 §2: "any
//! encoder divergence is a consensus-integrity bug, not a formatting
//! preference").
//!
//! # Canonical CBOR rules enforced by [`decode_canonical`]
//!
//! 1. All lengths are definite; indefinite-length items (CBOR's
//!    "streaming" forms, additional info 31 on major types 2–5) are
//!    rejected.
//! 2. Integer and length arguments use the shortest possible encoding:
//!    values 0–23 are inlined in the initial byte; larger values use the
//!    smallest of the 1/2/4/8-byte argument forms that can hold them.
//!    Widened encodings (e.g. 24 spelled out with a 2-byte argument) are
//!    rejected.
//! 3. Map keys are pairwise-distinct and appear in strictly ascending
//!    bytewise order of their own canonical encodings. This single check
//!    also catches duplicate keys (a duplicate cannot be strictly greater
//!    than itself).
//! 4. No floating-point values (major 7, additional info 25/26/27) and no
//!    simple values other than `false`/`true`/`null` (additional info
//!    20/21/22); `undefined` (23), one-byte simple values (24), and any
//!    other major-7 form are rejected.
//! 5. No tags (major 6).
//!
//! Additionally, as a resource-exhaustion guard rather than a canonical
//! encoding rule, nesting depth is capped at 64 and every length header is
//! checked against the number of bytes actually remaining in the input
//! before anything is allocated, so a crafted length field cannot force a
//! huge allocation from a small input.
//!
//! # JSON interchange (spec 001 §11)
//!
//! `Uint`/`Nint` values become plain JSON integers. Byte strings become
//! the quoted string `"hex:<lowercase hex>"`. Arrays and maps map
//! naturally, except that JSON object keys must be strings, so map keys
//! are rendered as: `Uint`/`Nint` as a decimal string; `Bytes` as
//! `"hex:<hex>"`; `Text` as itself.
//!
//! This creates two ambiguities that this module resolves by escaping,
//! documented here because the mapping does not round-trip without it:
//!
//! * A `Text` value (key or plain value) whose content itself starts with
//!   `"hex:"` or `"txt:"` would otherwise be indistinguishable from an
//!   escaped byte string or a plain string that merely happens to start
//!   that way. Resolution: such text is rendered with an extra `"txt:"`
//!   prefix. On decode, a string starting `"hex:"` decodes to `Bytes`, a
//!   string starting `"txt:"` has that one prefix stripped and decodes to
//!   `Text` (whatever remains, verbatim), and any other string decodes to
//!   `Text` as-is.
//! * A `Text` **map key** whose content is itself a bare canonical decimal
//!   integer (e.g. the key `"5"` or `"-3"`) would otherwise be
//!   indistinguishable, after decoding, from an integer-valued map key
//!   that happens to render to the same digits. This is a second
//!   disambiguation this module adds beyond the plain hex:/txt: rule
//!   (the spec text describes the hex:/txt: escape but does not spell out
//!   this case): resolution is to apply the same `"txt:"` escape whenever
//!   a text key's raw content would otherwise be re-parsed by
//!   [`from_json`] as a decimal integer key. This makes the mapping a true
//!   bijection between the value space and its JSON rendering. Plain
//!   (non-key) text values never need this extra escape, because integers
//!   never appear as bare JSON strings in value position — only as JSON
//!   numbers.
//!
//! Integer map keys support the full `Uint`/`Nint` range (up to the
//! magnitude representable by a canonical CBOR negative integer). Bare
//! JSON *number* tokens used as ordinary values are narrower: positive
//! values fit `u64`, but negative values must fit `i64` (spec text:
//! "Numbers must fit u64 (positive) or i64 (negative)"); this asymmetry
//! is intentional and only affects the rarely used extreme end of the
//! negative range when a plain (non-key) numeric value is involved.

use crate::error::{Error, Result};

/// Maximum nesting depth accepted by the CBOR decoder and the JSON
/// parser. A resource-exhaustion guard, not a canonical-encoding rule.
const MAX_DEPTH: u32 = 64;

/// CBOR value subset used by Vidmesh (no floats, no tags).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Major type 0: an unsigned integer.
    Uint(u64),
    /// Major type 1: a negative integer, representing `-1 - n`.
    Nint(u64),
    /// Major type 2: a byte string.
    Bytes(Vec<u8>),
    /// Major type 3: a UTF-8 text string.
    Text(String),
    /// Major type 4: an array.
    Array(Vec<Value>),
    /// Major type 5: a map. Entries in canonical (sorted, unique-key)
    /// order once produced by [`decode_canonical`]; [`encode_canonical`]
    /// sorts and validates on the way out regardless of input order.
    Map(Vec<(Value, Value)>),
    /// Major type 7, additional info 20/21: a boolean.
    Bool(bool),
    /// Major type 7, additional info 22: null.
    Null,
}

impl Value {
    /// Build a signed integer value: non-negative values use `Uint`,
    /// negative values use `Nint` (RFC 8949 §3.1).
    pub fn from_i64(v: i64) -> Value {
        if v >= 0 {
            Value::Uint(v as u64)
        } else {
            // i128 avoids overflow computing `-1 - v` at `v == i64::MIN`.
            let n = (-1i128 - v as i128) as u64;
            Value::Nint(n)
        }
    }

    /// This value as `i64`, if it fits: `Uint` up to `i64::MAX`, or
    /// `Nint(n)` whose represented value `-1 - n` is `>= i64::MIN`
    /// (i.e. `n <= i64::MAX as u64`).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Uint(v) if *v <= i64::MAX as u64 => Some(*v as i64),
            Value::Nint(n) if *n <= i64::MAX as u64 => Some((-1i128 - *n as i128) as i64),
            _ => None,
        }
    }

    /// This value as `u64`, if it is a `Uint`.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Uint(v) => Some(*v),
            _ => None,
        }
    }

    /// This value's bytes, if it is a `Bytes`.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// This value's text, if it is a `Text`.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// This value's elements, if it is an `Array`.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    /// This value's entries, if it is a `Map`.
    pub fn as_map(&self) -> Option<&[(Value, Value)]> {
        match self {
            Value::Map(m) => Some(m.as_slice()),
            _ => None,
        }
    }

    /// This value as `bool`, if it is a `Bool`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Look up a text key in a map. `None` if this is not a map or the
    /// key is absent.
    pub fn map_get(&self, key: &str) -> Option<&Value> {
        self.as_map()?
            .iter()
            .find(|(k, _)| k.as_text() == Some(key))
            .map(|(_, v)| v)
    }

    /// Look up an unsigned-integer key in a map. `None` if this is not a
    /// map or the key is absent.
    pub fn map_get_int(&self, key: u64) -> Option<&Value> {
        self.as_map()?
            .iter()
            .find(|(k, _)| k.as_u64() == Some(key))
            .map(|(_, v)| v)
    }

    /// Return this value with every nested map's entries reordered into
    /// canonical order — sorted by the bytewise order of their canonically
    /// encoded keys, the exact key [`encode_map`] uses on the way out.
    ///
    /// This changes only ordering; the set of entries, keys, and values is
    /// untouched, so it never alters the canonical encoding (which sorts
    /// regardless of input order). Its purpose is to make an in-memory
    /// value match the shape [`decode_canonical`] produces from the same
    /// bytes, so a hand-built [`Record`](crate::record::Record) and a
    /// decoded one compare equal (the `Map` invariant documented above).
    pub fn into_canonical(self) -> Value {
        match self {
            Value::Array(items) => {
                Value::Array(items.into_iter().map(Value::into_canonical).collect())
            }
            Value::Map(entries) => {
                let mut canon: Vec<(Vec<u8>, (Value, Value))> = entries
                    .into_iter()
                    .map(|(k, v)| {
                        let k = k.into_canonical();
                        let v = v.into_canonical();
                        let mut kbuf = Vec::new();
                        // Key encoding cannot fail for well-formed scalar
                        // keys; on the off chance it does, an empty buffer
                        // keeps the sort total and `encode_canonical` still
                        // surfaces the real error at signing time.
                        let _ = encode_value(&k, &mut kbuf);
                        (kbuf, (k, v))
                    })
                    .collect();
                canon.sort_by(|a, b| a.0.cmp(&b.0));
                Value::Map(canon.into_iter().map(|(_, kv)| kv).collect())
            }
            other => other,
        }
    }
}

// ---------------------------------------------------------------------
// Canonical encoding
// ---------------------------------------------------------------------

/// Write a CBOR head (major type + argument) in shortest form.
fn write_head(out: &mut Vec<u8>, major: u8, arg: u64) {
    let top = major << 5;
    if arg < 24 {
        out.push(top | arg as u8);
    } else if arg <= 0xff {
        out.push(top | 24);
        out.push(arg as u8);
    } else if arg <= 0xffff {
        out.push(top | 25);
        out.extend_from_slice(&(arg as u16).to_be_bytes());
    } else if arg <= 0xffff_ffff {
        out.push(top | 26);
        out.extend_from_slice(&(arg as u32).to_be_bytes());
    } else {
        out.push(top | 27);
        out.extend_from_slice(&arg.to_be_bytes());
    }
}

fn encode_value(v: &Value, out: &mut Vec<u8>) -> Result<()> {
    match v {
        Value::Uint(n) => {
            write_head(out, 0, *n);
            Ok(())
        }
        Value::Nint(n) => {
            write_head(out, 1, *n);
            Ok(())
        }
        Value::Bytes(b) => {
            write_head(out, 2, b.len() as u64);
            out.extend_from_slice(b);
            Ok(())
        }
        Value::Text(s) => {
            let bytes = s.as_bytes();
            write_head(out, 3, bytes.len() as u64);
            out.extend_from_slice(bytes);
            Ok(())
        }
        Value::Array(items) => {
            write_head(out, 4, items.len() as u64);
            for item in items {
                encode_value(item, out)?;
            }
            Ok(())
        }
        Value::Map(entries) => encode_map(entries, out),
        Value::Bool(b) => {
            out.push(if *b { 0xf5 } else { 0xf4 });
            Ok(())
        }
        Value::Null => {
            out.push(0xf6);
            Ok(())
        }
    }
}

/// Encode a map's entries, sorting by the bytewise order of their
/// canonically encoded keys and rejecting duplicates (spec 001 §2 rule
/// 3).
fn encode_map(entries: &[(Value, Value)], out: &mut Vec<u8>) -> Result<()> {
    let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(entries.len());
    for (k, v) in entries {
        let mut kbuf = Vec::new();
        encode_value(k, &mut kbuf)?;
        let mut vbuf = Vec::new();
        encode_value(v, &mut vbuf)?;
        pairs.push((kbuf, vbuf));
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    for w in pairs.windows(2) {
        if w[0].0 == w[1].0 {
            return Err(Error::Cbor("duplicate map key"));
        }
    }
    write_head(out, 5, pairs.len() as u64);
    for (k, v) in &pairs {
        out.extend_from_slice(k);
        out.extend_from_slice(v);
    }
    Ok(())
}

/// Encode canonically (RFC 8949 §4.2.1 + spec 001 §2). Sorts map entries
/// by the bytewise order of their canonically encoded keys. Errors on
/// duplicate map keys (`Error::Cbor`).
pub fn encode_canonical(v: &Value) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    encode_value(v, &mut out)?;
    Ok(out)
}

// ---------------------------------------------------------------------
// Canonical decoding
// ---------------------------------------------------------------------

fn read_u8(bytes: &[u8], pos: &mut usize) -> Result<u8> {
    let b = *bytes.get(*pos).ok_or(Error::Cbor("truncated input"))?;
    *pos += 1;
    Ok(b)
}

/// Read exactly `n` bytes starting at `*pos`, advancing `*pos`. Never
/// panics: bounds are checked with `get` rather than indexing.
fn read_exact<'a>(bytes: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = pos.checked_add(n).ok_or(Error::Cbor("length overflow"))?;
    let slice = bytes.get(*pos..end).ok_or(Error::Cbor("truncated input"))?;
    *pos = end;
    Ok(slice)
}

fn read_be<const N: usize>(bytes: &[u8], pos: &mut usize) -> Result<[u8; N]> {
    let slice = read_exact(bytes, pos, N)?;
    let mut arr = [0u8; N];
    arr.copy_from_slice(slice);
    Ok(arr)
}

/// Read a CBOR integer/length argument, enforcing shortest-form encoding
/// (spec 001 §2 rule 2) and rejecting indefinite lengths, reserved
/// additional-info values, and floats/other major-7 forms are handled by
/// the caller for major type 7 (this function is only used for majors
/// 0–5).
fn read_arg(bytes: &[u8], pos: &mut usize, low: u8) -> Result<u64> {
    match low {
        0..=23 => Ok(low as u64),
        24 => {
            let v = read_u8(bytes, pos)? as u64;
            if v < 24 {
                return Err(Error::NonCanonical("non-shortest integer encoding"));
            }
            Ok(v)
        }
        25 => {
            let v = u16::from_be_bytes(read_be::<2>(bytes, pos)?) as u64;
            if v <= 0xff {
                return Err(Error::NonCanonical("non-shortest integer encoding"));
            }
            Ok(v)
        }
        26 => {
            let v = u32::from_be_bytes(read_be::<4>(bytes, pos)?) as u64;
            if v <= 0xffff {
                return Err(Error::NonCanonical("non-shortest integer encoding"));
            }
            Ok(v)
        }
        27 => {
            let v = u64::from_be_bytes(read_be::<8>(bytes, pos)?);
            if v <= 0xffff_ffff {
                return Err(Error::NonCanonical("non-shortest integer encoding"));
            }
            Ok(v)
        }
        28..=30 => Err(Error::Cbor("reserved additional info")),
        31 => Err(Error::NonCanonical("indefinite length not allowed")),
        _ => Err(Error::Cbor("invalid additional info")),
    }
}

/// Read a length argument (bytes/text/array/map) and check it against the
/// bytes actually remaining, so an attacker-controlled length cannot
/// force an oversized allocation.
fn read_length(bytes: &[u8], pos: &mut usize, low: u8) -> Result<usize> {
    let n = read_arg(bytes, pos, low)?;
    let n = usize::try_from(n).map_err(|_| Error::Cbor("length too large"))?;
    let remaining = bytes.len().saturating_sub(*pos);
    if n > remaining {
        return Err(Error::Cbor("length exceeds remaining input"));
    }
    Ok(n)
}

fn parse_value(bytes: &[u8], pos: &mut usize, depth: u32) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(Error::Cbor("nesting depth exceeds limit"));
    }
    let ib = read_u8(bytes, pos)?;
    let major = ib >> 5;
    let low = ib & 0x1f;
    match major {
        0 => Ok(Value::Uint(read_arg(bytes, pos, low)?)),
        1 => Ok(Value::Nint(read_arg(bytes, pos, low)?)),
        2 => {
            let len = read_length(bytes, pos, low)?;
            let data = read_exact(bytes, pos, len)?;
            Ok(Value::Bytes(data.to_vec()))
        }
        3 => {
            let len = read_length(bytes, pos, low)?;
            let data = read_exact(bytes, pos, len)?;
            let s = core::str::from_utf8(data).map_err(|_| Error::Cbor("invalid utf-8 in text"))?;
            Ok(Value::Text(s.to_string()))
        }
        4 => {
            let len = read_length(bytes, pos, low)?;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(parse_value(bytes, pos, depth + 1)?);
            }
            Ok(Value::Array(items))
        }
        5 => {
            let len = read_length(bytes, pos, low)?;
            let mut entries = Vec::with_capacity(len);
            let mut prev_key: Option<Vec<u8>> = None;
            for _ in 0..len {
                let key_start = *pos;
                let key = parse_value(bytes, pos, depth + 1)?;
                let key_end = *pos;
                let key_bytes = &bytes[key_start..key_end];
                if let Some(prev) = &prev_key {
                    if key_bytes <= prev.as_slice() {
                        return Err(Error::NonCanonical(
                            "map keys not in strictly ascending order",
                        ));
                    }
                }
                prev_key = Some(key_bytes.to_vec());
                let val = parse_value(bytes, pos, depth + 1)?;
                entries.push((key, val));
            }
            Ok(Value::Map(entries))
        }
        6 => Err(Error::NonCanonical("tags not allowed")),
        7 => match low {
            20 => Ok(Value::Bool(false)),
            21 => Ok(Value::Bool(true)),
            22 => Ok(Value::Null),
            23 => Err(Error::NonCanonical("undefined not allowed")),
            24 => Err(Error::NonCanonical("simple value not allowed")),
            25..=27 => Err(Error::NonCanonical("floating point not allowed")),
            28..=30 => Err(Error::Cbor("reserved additional info")),
            31 => Err(Error::Cbor("unexpected break code")),
            _ => Err(Error::Cbor("invalid simple value")),
        },
        _ => Err(Error::Cbor("invalid major type")),
    }
}

/// Strict canonical decode. Rejects trailing bytes, truncation,
/// indefinite lengths, floats/simple values other than bool/null, tags,
/// non-shortest integer/length heads, map keys not in strictly ascending
/// canonical byte order (which also catches duplicates), invalid UTF-8 in
/// text, nesting depth > 64, and any length header exceeding the
/// remaining input.
pub fn decode_canonical(bytes: &[u8]) -> Result<Value> {
    let mut pos = 0usize;
    let value = parse_value(bytes, &mut pos, 0)?;
    if pos != bytes.len() {
        return Err(Error::Cbor("trailing bytes after value"));
    }
    Ok(value)
}

// ---------------------------------------------------------------------
// JSON interchange
// ---------------------------------------------------------------------

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return Err(Error::Cbor("json: odd-length hex string"));
    }
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks_exact(2) {
        out.push((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?);
    }
    Ok(out)
}

fn hex_digit(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err(Error::Cbor("json: invalid hex digit")),
    }
}

fn uint_decimal(n: u64) -> String {
    n.to_string()
}

fn nint_decimal(n: u64) -> String {
    (-1i128 - n as i128).to_string()
}

fn bytes_json_string(b: &[u8]) -> String {
    format!("hex:{}", hex_lower(b))
}

/// Render a `Text` value's content for JSON, escaping the hex:/txt:
/// collision only (see module docs). Used for plain (non-key) values.
fn text_json_string(s: &str) -> String {
    if s.starts_with("hex:") || s.starts_with("txt:") {
        format!("txt:{s}")
    } else {
        s.to_string()
    }
}

/// Parse a decimal-integer map key (`"5"`, `"-3"`, but not `"-0"` or
/// anything with a leading zero) into `Uint`/`Nint`. Supports the full
/// `Uint`/`Nint` magnitude range (wider than bare JSON number literals),
/// so that any map key produced by [`key_json_string`] parses back
/// exactly. Returns `None` (not an error) for anything that is not this
/// exact grammar — the caller falls through to text/bytes handling.
fn try_parse_int_key(s: &str) -> Option<Value> {
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    if negative && digits == "0" {
        return None;
    }
    // Parse in u128 so the extreme Nint magnitude (2^64, one more than
    // u64::MAX) is representable during the subtraction below.
    let magnitude: u128 = digits.parse().ok()?;
    if negative {
        if magnitude > (u64::MAX as u128) + 1 {
            return None;
        }
        Some(Value::Nint((magnitude - 1) as u64))
    } else {
        if magnitude > u64::MAX as u128 {
            return None;
        }
        Some(Value::Uint(magnitude as u64))
    }
}

/// Render a map key's content for JSON (spec 001 §11 plus the two
/// disambiguations documented at module level).
fn key_json_string(k: &Value) -> String {
    match k {
        Value::Uint(n) => uint_decimal(*n),
        Value::Nint(n) => nint_decimal(*n),
        Value::Bytes(b) => bytes_json_string(b),
        Value::Text(s) => {
            if s.starts_with("hex:") || s.starts_with("txt:") || try_parse_int_key(s).is_some() {
                format!("txt:{s}")
            } else {
                s.clone()
            }
        }
        // Arrays/maps/bool/null as map keys are not part of this
        // protocol's normal usage (envelope and body maps use only
        // integer or text keys); render something panic-free and
        // non-colliding rather than fail, since `to_json` cannot return
        // an error. This does not round-trip through `from_json`.
        Value::Array(_) | Value::Map(_) | Value::Bool(_) | Value::Null => {
            format!("unsupported-key:{k:?}")
        }
    }
}

fn push_json_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                // `write!` to a `String` never fails.
                use core::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}

fn value_to_json(v: &Value, out: &mut String) {
    match v {
        Value::Uint(n) => out.push_str(&uint_decimal(*n)),
        Value::Nint(n) => out.push_str(&nint_decimal(*n)),
        Value::Bytes(b) => {
            out.push('"');
            push_json_escaped(out, &bytes_json_string(b));
            out.push('"');
        }
        Value::Text(s) => {
            out.push('"');
            push_json_escaped(out, &text_json_string(s));
            out.push('"');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                value_to_json(item, out);
            }
            out.push(']');
        }
        Value::Map(entries) => {
            out.push('{');
            for (i, (k, val)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('"');
                push_json_escaped(out, &key_json_string(k));
                out.push('"');
                out.push(':');
                value_to_json(val, out);
            }
            out.push('}');
        }
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
    }
}

/// JSON interchange form (spec 001 §11): integer map keys become decimal
/// strings, byte strings become `"hex:<lowercase hex>"`, `Uint`/`Nint`
/// values become JSON integers, text/array/map/bool/null map naturally.
/// See the module docs for the hex:/txt: and integer-lookalike-key
/// disambiguations this applies so the mapping round-trips.
pub fn to_json(v: &Value) -> String {
    let mut out = String::new();
    value_to_json(v, &mut out);
    out
}

/// A JSON string parsed as a plain (non-key) value: only the hex:/txt:
/// disambiguation applies (no integer-lookalike check — bare integers
/// never appear as JSON strings in value position, only as JSON numbers).
fn string_value_from_json(raw: &str) -> Result<Value> {
    if let Some(hex) = raw.strip_prefix("hex:") {
        Ok(Value::Bytes(decode_hex(hex)?))
    } else if let Some(rest) = raw.strip_prefix("txt:") {
        Ok(Value::Text(rest.to_string()))
    } else {
        Ok(Value::Text(raw.to_string()))
    }
}

/// A JSON object key string parsed back into a `Value`: integer-lookalike
/// strings become `Uint`/`Nint`, then the ordinary hex:/txt:/plain rule.
fn key_value_from_json(raw: &str) -> Result<Value> {
    if let Some(v) = try_parse_int_key(raw) {
        return Ok(v);
    }
    string_value_from_json(raw)
}

struct Parser<'a> {
    chars: core::iter::Peekable<core::str::Chars<'a>>,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Parser {
            chars: s.chars().peekable(),
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn next_char(&mut self) -> Option<char> {
        self.chars.next()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek_char(), Some(' ' | '\t' | '\n' | '\r')) {
            self.chars.next();
        }
    }

    fn expect(&mut self, c: char) -> Result<()> {
        match self.next_char() {
            Some(x) if x == c => Ok(()),
            _ => Err(Error::Cbor("json: unexpected character")),
        }
    }

    fn read_hex4(&mut self) -> Result<u16> {
        let mut v: u16 = 0;
        for _ in 0..4 {
            let c = self
                .next_char()
                .ok_or(Error::Cbor("json: truncated unicode escape"))?;
            let d = c
                .to_digit(16)
                .ok_or(Error::Cbor("json: invalid unicode escape"))?;
            v = v * 16 + d as u16;
        }
        Ok(v)
    }

    /// Parse a JSON string literal, assuming the opening `"` was already
    /// consumed. Handles all standard escapes including `\uXXXX` and
    /// surrogate pairs; rejects lone surrogates and raw control
    /// characters.
    fn parse_string_raw(&mut self) -> Result<String> {
        let mut s = String::new();
        loop {
            let c = self
                .next_char()
                .ok_or(Error::Cbor("json: unterminated string"))?;
            match c {
                '"' => return Ok(s),
                '\\' => {
                    let esc = self
                        .next_char()
                        .ok_or(Error::Cbor("json: unterminated escape"))?;
                    match esc {
                        '"' => s.push('"'),
                        '\\' => s.push('\\'),
                        '/' => s.push('/'),
                        'b' => s.push('\u{08}'),
                        'f' => s.push('\u{0c}'),
                        'n' => s.push('\n'),
                        'r' => s.push('\r'),
                        't' => s.push('\t'),
                        'u' => {
                            let cp = self.read_hex4()?;
                            if (0xD800..=0xDBFF).contains(&cp) {
                                if self.next_char() != Some('\\') || self.next_char() != Some('u') {
                                    return Err(Error::Cbor("json: lone surrogate"));
                                }
                                let low = self.read_hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(Error::Cbor("json: invalid surrogate pair"));
                                }
                                let combined = 0x10000u32
                                    + ((cp as u32 - 0xD800) << 10)
                                    + (low as u32 - 0xDC00);
                                let ch = char::from_u32(combined)
                                    .ok_or(Error::Cbor("json: invalid unicode scalar"))?;
                                s.push(ch);
                            } else if (0xDC00..=0xDFFF).contains(&cp) {
                                return Err(Error::Cbor("json: lone surrogate"));
                            } else {
                                let ch = char::from_u32(cp as u32)
                                    .ok_or(Error::Cbor("json: invalid unicode scalar"))?;
                                s.push(ch);
                            }
                        }
                        _ => return Err(Error::Cbor("json: invalid escape sequence")),
                    }
                }
                c if (c as u32) < 0x20 => {
                    return Err(Error::Cbor("json: control character in string"))
                }
                c => s.push(c),
            }
        }
    }

    fn parse_literal(&mut self, lit: &str, value: Value) -> Result<Value> {
        for expected in lit.chars() {
            match self.next_char() {
                Some(c) if c == expected => {}
                _ => return Err(Error::Cbor("json: invalid literal")),
            }
        }
        Ok(value)
    }

    /// Parse a JSON number. Only integers are supported: an optional
    /// leading `-`, then digits with no leading zero (except a bare `0`),
    /// and no fraction or exponent. Positive values must fit `u64`;
    /// negative values must additionally fit `i64` (spec: "Numbers must
    /// fit u64 (positive) or i64 (negative)").
    fn parse_number(&mut self) -> Result<Value> {
        let mut s = String::new();
        if self.peek_char() == Some('-') {
            s.push('-');
            self.next_char();
        }
        let mut any_digit = false;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                s.push(c);
                any_digit = true;
                self.next_char();
            } else {
                break;
            }
        }
        if !any_digit {
            return Err(Error::Cbor("json: invalid number"));
        }
        if matches!(self.peek_char(), Some('.') | Some('e') | Some('E')) {
            return Err(Error::Cbor(
                "json: fractional or exponential numbers are not supported",
            ));
        }
        let negative = s.starts_with('-');
        let digits = if negative { &s[1..] } else { s.as_str() };
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(Error::Cbor("json: leading zero in number"));
        }
        if negative {
            if digits == "0" {
                return Err(Error::Cbor("json: negative zero not allowed"));
            }
            let magnitude: u64 = digits
                .parse()
                .map_err(|_| Error::Cbor("json: integer out of range"))?;
            if magnitude > 9_223_372_036_854_775_808u64 {
                return Err(Error::Cbor("json: integer out of range"));
            }
            Ok(Value::Nint(magnitude - 1))
        } else {
            let magnitude: u64 = digits
                .parse()
                .map_err(|_| Error::Cbor("json: integer out of range"))?;
            Ok(Value::Uint(magnitude))
        }
    }

    fn parse_array(&mut self, depth: u32) -> Result<Value> {
        self.next_char(); // consume '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek_char() == Some(']') {
            self.next_char();
            return Ok(Value::Array(items));
        }
        loop {
            let val = self.parse_value(depth + 1)?;
            items.push(val);
            self.skip_ws();
            match self.next_char() {
                Some(',') => {
                    self.skip_ws();
                    continue;
                }
                Some(']') => break,
                _ => return Err(Error::Cbor("json: expected ',' or ']'")),
            }
        }
        Ok(Value::Array(items))
    }

    fn parse_object(&mut self, depth: u32) -> Result<Value> {
        self.next_char(); // consume '{'
        let mut entries: Vec<(Value, Value)> = Vec::new();
        self.skip_ws();
        if self.peek_char() == Some('}') {
            self.next_char();
            return Ok(Value::Map(entries));
        }
        loop {
            self.skip_ws();
            self.expect('"')?;
            let raw_key = self.parse_string_raw()?;
            let key = key_value_from_json(&raw_key)?;
            self.skip_ws();
            self.expect(':')?;
            let val = self.parse_value(depth + 1)?;
            if entries.iter().any(|(k, _)| *k == key) {
                return Err(Error::Cbor("json: duplicate object key"));
            }
            entries.push((key, val));
            self.skip_ws();
            match self.next_char() {
                Some(',') => continue,
                Some('}') => break,
                _ => return Err(Error::Cbor("json: expected ',' or '}'")),
            }
        }
        Ok(Value::Map(entries))
    }

    fn parse_value(&mut self, depth: u32) -> Result<Value> {
        if depth > MAX_DEPTH {
            return Err(Error::Cbor("json: nesting depth exceeds limit"));
        }
        self.skip_ws();
        match self.peek_char() {
            Some('{') => self.parse_object(depth),
            Some('[') => self.parse_array(depth),
            Some('"') => {
                self.next_char();
                let raw = self.parse_string_raw()?;
                string_value_from_json(&raw)
            }
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            Some('t') => self.parse_literal("true", Value::Bool(true)),
            Some('f') => self.parse_literal("false", Value::Bool(false)),
            Some('n') => self.parse_literal("null", Value::Null),
            _ => Err(Error::Cbor("json: unexpected character or end of input")),
        }
    }
}

/// Strict inverse of [`to_json`]: a hand-rolled minimal JSON parser
/// (objects, arrays, strings with standard escapes including `\uXXXX`
/// surrogate pairs, integers only, `true`/`false`/`null`). Depth limit
/// 64. Numbers must fit `u64` (positive) or `i64` (negative) else error.
/// Map keys that are decimal strings map back to integer keys only when
/// they round-trip exactly (no leading zeros, no `-0`); see the module
/// docs for the full key-disambiguation rule.
pub fn from_json(s: &str) -> Result<Value> {
    let mut parser = Parser::new(s);
    let value = parser.parse_value(0)?;
    parser.skip_ws();
    if parser.peek_char().is_some() {
        return Err(Error::Cbor("json: trailing data after value"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Value helpers -----------------------------------------------

    #[test]
    fn from_i64_as_i64_round_trip() {
        for v in [
            0i64,
            1,
            -1,
            23,
            -24,
            255,
            -256,
            i64::MAX,
            i64::MIN,
            i64::MIN + 1,
        ] {
            assert_eq!(Value::from_i64(v).as_i64(), Some(v), "round trip for {v}");
        }
    }

    #[test]
    fn from_i64_matches_expected_variant() {
        assert_eq!(Value::from_i64(0), Value::Uint(0));
        assert_eq!(Value::from_i64(1), Value::Uint(1));
        assert_eq!(Value::from_i64(-1), Value::Nint(0));
        assert_eq!(Value::from_i64(-24), Value::Nint(23));
        assert_eq!(Value::from_i64(-25), Value::Nint(24));
        assert_eq!(Value::from_i64(i64::MIN), Value::Nint(i64::MAX as u64));
    }

    #[test]
    fn as_i64_rejects_out_of_range() {
        // Nint(n) with n > i64::MAX as u64 represents a value below
        // i64::MIN.
        assert_eq!(Value::Nint(i64::MAX as u64 + 1).as_i64(), None);
        assert_eq!(Value::Uint(u64::MAX).as_i64(), None);
    }

    #[test]
    fn accessors() {
        assert_eq!(Value::Uint(5).as_u64(), Some(5));
        assert_eq!(Value::Nint(5).as_u64(), None);
        assert_eq!(Value::Bytes(vec![1, 2]).as_bytes(), Some(&[1u8, 2][..]));
        assert_eq!(Value::Text("hi".into()).as_text(), Some("hi"));
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::Null.as_bool(), None);
        let arr = Value::Array(vec![Value::Uint(1)]);
        assert_eq!(arr.as_array(), Some(&[Value::Uint(1)][..]));
        let map = Value::Map(vec![(Value::Uint(1), Value::Uint(2))]);
        assert_eq!(map.as_map(), Some(&[(Value::Uint(1), Value::Uint(2))][..]));
    }

    #[test]
    fn map_get_helpers() {
        let map = Value::Map(vec![
            (Value::Text("a".into()), Value::Uint(1)),
            (Value::Uint(9), Value::Text("nine".into())),
        ]);
        assert_eq!(map.map_get("a"), Some(&Value::Uint(1)));
        assert_eq!(map.map_get("missing"), None);
        assert_eq!(map.map_get_int(9), Some(&Value::Text("nine".into())));
        assert_eq!(map.map_get_int(1), None);
        assert_eq!(Value::Uint(1).map_get("a"), None);
    }

    // -- Known-answer encodings (RFC 8949 examples) -------------------

    fn enc(v: &Value) -> Vec<u8> {
        encode_canonical(v).unwrap()
    }

    #[test]
    fn kat_small_uints() {
        assert_eq!(enc(&Value::Uint(0)), vec![0x00]);
        assert_eq!(enc(&Value::Uint(1)), vec![0x01]);
        assert_eq!(enc(&Value::Uint(23)), vec![0x17]);
        assert_eq!(enc(&Value::Uint(24)), vec![0x18, 0x18]);
        assert_eq!(enc(&Value::Uint(255)), vec![0x18, 0xff]);
        assert_eq!(enc(&Value::Uint(256)), vec![0x19, 0x01, 0x00]);
        assert_eq!(
            enc(&Value::Uint(u64::MAX)),
            vec![0x1b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    fn kat_negative_ints() {
        assert_eq!(enc(&Value::from_i64(-1)), vec![0x20]);
        assert_eq!(enc(&Value::from_i64(-24)), vec![0x37]);
        assert_eq!(enc(&Value::from_i64(-25)), vec![0x38, 0x18]);
    }

    #[test]
    fn kat_text() {
        assert_eq!(enc(&Value::Text(String::new())), vec![0x60]);
        assert_eq!(enc(&Value::Text("a".into())), vec![0x61, 0x61]);
        assert_eq!(
            enc(&Value::Text("IETF".into())),
            vec![0x64, b'I', b'E', b'T', b'F']
        );
    }

    #[test]
    fn kat_bytes() {
        assert_eq!(enc(&Value::Bytes(vec![1, 2, 3, 4])), vec![0x44, 1, 2, 3, 4]);
    }

    #[test]
    fn kat_nested_array() {
        let v = Value::Array(vec![
            Value::Uint(1),
            Value::Array(vec![Value::Uint(2), Value::Uint(3)]),
        ]);
        assert_eq!(enc(&v), vec![0x82, 0x01, 0x82, 0x02, 0x03]);
    }

    #[test]
    fn kat_map() {
        let v = Value::Map(vec![
            (Value::Text("a".into()), Value::Uint(1)),
            (
                Value::Text("b".into()),
                Value::Array(vec![Value::Uint(2), Value::Uint(3)]),
            ),
        ]);
        assert_eq!(
            enc(&v),
            vec![0xa2, 0x61, 0x61, 0x01, 0x61, 0x62, 0x82, 0x02, 0x03]
        );
    }

    #[test]
    fn bool_null() {
        assert_eq!(enc(&Value::Bool(false)), vec![0xf4]);
        assert_eq!(enc(&Value::Bool(true)), vec![0xf5]);
        assert_eq!(enc(&Value::Null), vec![0xf6]);
    }

    // -- Map key sorting -----------------------------------------------

    #[test]
    fn map_sorts_by_encoded_key_bytes_not_string_order() {
        // "b" encodes [0x61, 0x62]; "aa" encodes [0x62, 0x61, 0x61].
        // First byte 0x61 < 0x62, so "b" sorts before "aa" even though
        // "aa" < "b" lexicographically as a string.
        let v = Value::Map(vec![
            (Value::Text("aa".into()), Value::Uint(1)),
            (Value::Text("b".into()), Value::Uint(2)),
        ]);
        let bytes = enc(&v);
        // "b" key/value: 61 62 02 ; "aa" key/value: 62 61 61 01
        let expected = vec![0xa2, 0x61, 0x62, 0x02, 0x62, 0x61, 0x61, 0x01];
        assert_eq!(bytes, expected);
    }

    #[test]
    fn duplicate_map_key_rejected_on_encode() {
        let v = Value::Map(vec![
            (Value::Uint(1), Value::Uint(1)),
            (Value::Uint(1), Value::Uint(2)),
        ]);
        assert!(matches!(encode_canonical(&v), Err(Error::Cbor(_))));
    }

    // -- Round trips -----------------------------------------------------

    fn round_trip(v: &Value) {
        let bytes = encode_canonical(v).unwrap();
        let back = decode_canonical(&bytes).unwrap();
        assert_eq!(&back, v);
    }

    #[test]
    fn round_trip_every_variant() {
        round_trip(&Value::Uint(0));
        round_trip(&Value::Uint(u64::MAX));
        round_trip(&Value::Nint(0));
        round_trip(&Value::Nint(u64::MAX));
        round_trip(&Value::Bytes(vec![]));
        round_trip(&Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]));
        round_trip(&Value::Text(String::new()));
        round_trip(&Value::Text("hello, world".into()));
        round_trip(&Value::Array(vec![]));
        round_trip(&Value::Map(vec![]));
        round_trip(&Value::Bool(true));
        round_trip(&Value::Bool(false));
        round_trip(&Value::Null);
    }

    #[test]
    fn round_trip_nested_structure() {
        let v = Value::Map(vec![
            (
                Value::Uint(1),
                Value::Array(vec![Value::Uint(1), Value::Nint(0)]),
            ),
            (
                Value::Uint(2),
                Value::Map(vec![(Value::Text("x".into()), Value::Bool(true))]),
            ),
            (Value::Uint(3), Value::Bytes(vec![1, 2, 3])),
        ]);
        round_trip(&v);
    }

    // -- Rejections, one per rule -----------------------------------------

    fn decode_err(bytes: &[u8]) -> Error {
        decode_canonical(bytes).expect_err("expected rejection")
    }

    #[test]
    fn rejects_trailing_byte() {
        let mut bytes = encode_canonical(&Value::Uint(0)).unwrap();
        bytes.push(0x00);
        assert!(matches!(decode_err(&bytes), Error::Cbor(_)));
    }

    #[test]
    fn rejects_truncated_input() {
        assert!(matches!(decode_err(&[0x18]), Error::Cbor(_)));
        assert!(matches!(decode_err(&[]), Error::Cbor(_)));
    }

    #[test]
    fn rejects_indefinite_array() {
        assert!(matches!(decode_err(&[0x9f]), Error::NonCanonical(_)));
    }

    #[test]
    fn rejects_floats() {
        assert!(matches!(
            decode_err(&[0xf9, 0x00, 0x00]),
            Error::NonCanonical(_)
        ));
        assert!(matches!(
            decode_err(&[0xfa, 0, 0, 0, 0]),
            Error::NonCanonical(_)
        ));
        assert!(matches!(
            decode_err(&[0xfb, 0, 0, 0, 0, 0, 0, 0, 0]),
            Error::NonCanonical(_)
        ));
    }

    #[test]
    fn rejects_tag() {
        assert!(matches!(decode_err(&[0xc0, 0x00]), Error::NonCanonical(_)));
    }

    #[test]
    fn rejects_undefined() {
        assert!(matches!(decode_err(&[0xf7]), Error::NonCanonical(_)));
    }

    #[test]
    fn rejects_non_shortest_uint() {
        // 24 encoded via the 1-byte-argument form with argument 1 (should
        // have been the inline form 0x01).
        assert!(matches!(decode_err(&[0x18, 0x01]), Error::NonCanonical(_)));
        // 23 widened to the 1-byte-argument form (0x1817 in the task's
        // notation): argument 23 < 24, so the 1-byte form was unnecessary.
        assert!(matches!(decode_err(&[0x18, 0x17]), Error::NonCanonical(_)));
    }

    #[test]
    fn rejects_non_shortest_length() {
        // Byte string of length 5 spelled with a 2-byte length argument
        // instead of the inline form.
        let mut bytes = vec![0x59, 0x00, 0x05];
        bytes.extend_from_slice(&[0u8; 5]);
        assert!(matches!(decode_err(&bytes), Error::NonCanonical(_)));
    }

    #[test]
    fn rejects_unsorted_map() {
        // {"b": 2, "a": 1} in that (wrong) order.
        let bytes = vec![0xa2, 0x61, 0x62, 0x02, 0x61, 0x61, 0x01];
        assert!(matches!(decode_err(&bytes), Error::NonCanonical(_)));
    }

    #[test]
    fn rejects_duplicate_key() {
        // {1: 1, 1: 2}
        let bytes = vec![0xa2, 0x01, 0x01, 0x01, 0x02];
        assert!(matches!(decode_err(&bytes), Error::NonCanonical(_)));
    }

    #[test]
    fn rejects_bad_utf8_text() {
        // Text of length 1 containing an invalid UTF-8 byte.
        let bytes = vec![0x61, 0xff];
        assert!(matches!(decode_err(&bytes), Error::Cbor(_)));
    }

    #[test]
    fn rejects_excess_nesting_depth() {
        // 65 nested one-element arrays around a Uint(0): the innermost
        // scalar is parsed at depth 65, which exceeds the limit of 64.
        let mut bytes = vec![0x00u8];
        for _ in 0..65 {
            let mut wrapped = vec![0x81u8];
            wrapped.extend_from_slice(&bytes);
            bytes = wrapped;
        }
        assert!(matches!(decode_err(&bytes), Error::Cbor(_)));
    }

    #[test]
    fn accepts_exactly_64_nesting_depth() {
        let mut bytes = vec![0x00u8];
        for _ in 0..64 {
            let mut wrapped = vec![0x81u8];
            wrapped.extend_from_slice(&bytes);
            bytes = wrapped;
        }
        assert!(decode_canonical(&bytes).is_ok());
    }

    #[test]
    fn rejects_length_exceeding_remaining_input() {
        // Byte string head claiming an 8-byte length of u64::MAX, with no
        // data following at all.
        let bytes = vec![0x5b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
        assert!(matches!(decode_err(&bytes), Error::Cbor(_)));
    }

    // -- JSON ------------------------------------------------------------

    fn json_round_trip(v: &Value) {
        let json = to_json(v);
        let back = from_json(&json).unwrap();
        assert_eq!(&back, v, "json was: {json}");
    }

    #[test]
    fn json_round_trip_scalars() {
        json_round_trip(&Value::Uint(0));
        json_round_trip(&Value::Uint(u64::MAX));
        json_round_trip(&Value::from_i64(-1));
        json_round_trip(&Value::from_i64(i64::MIN));
        json_round_trip(&Value::Bool(true));
        json_round_trip(&Value::Bool(false));
        json_round_trip(&Value::Null);
        json_round_trip(&Value::Text("hello".into()));
        json_round_trip(&Value::Text(String::new()));
        json_round_trip(&Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]));
        json_round_trip(&Value::Bytes(vec![]));
    }

    #[test]
    fn json_hex_disambiguation_round_trips() {
        // A real byte string.
        let bytes = Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(to_json(&bytes), "\"hex:deadbeef\"");
        json_round_trip(&bytes);

        // Text that happens to start with "hex:" must not be confused
        // with a byte string.
        let text = Value::Text("hex:deadbeef".into());
        assert_eq!(to_json(&text), "\"txt:hex:deadbeef\"");
        json_round_trip(&text);

        // Text that happens to start with "txt:" likewise.
        let text2 = Value::Text("txt:something".into());
        assert_eq!(to_json(&text2), "\"txt:txt:something\"");
        json_round_trip(&text2);
    }

    #[test]
    fn json_integer_and_text_map_keys_disambiguated() {
        // A genuine integer key renders unescaped.
        let int_keyed = Value::Map(vec![(Value::Uint(5), Value::Bool(true))]);
        assert_eq!(to_json(&int_keyed), "{\"5\":true}");
        json_round_trip(&int_keyed);

        // A text key that looks like a decimal integer must be escaped so
        // it does not collide with a genuine integer key on decode.
        let text_keyed = Value::Map(vec![(Value::Text("5".into()), Value::Bool(true))]);
        assert_eq!(to_json(&text_keyed), "{\"txt:5\":true}");
        json_round_trip(&text_keyed);

        // Negative-integer key vs. lookalike text key.
        let neg_int_keyed = Value::Map(vec![(Value::from_i64(-3), Value::Null)]);
        json_round_trip(&neg_int_keyed);
        let neg_text_keyed = Value::Map(vec![(Value::Text("-3".into()), Value::Null)]);
        assert_eq!(to_json(&neg_text_keyed), "{\"txt:-3\":null}");
        json_round_trip(&neg_text_keyed);

        // Leading-zero text keys are never ambiguous (never produced by
        // the integer-key path), so they pass through unescaped.
        let leading_zero_text = Value::Map(vec![(Value::Text("007".into()), Value::Null)]);
        assert_eq!(to_json(&leading_zero_text), "{\"007\":null}");
        json_round_trip(&leading_zero_text);
    }

    #[test]
    fn json_nested_and_arrays() {
        let v = Value::Map(vec![
            (Value::Text("text".into()), Value::Text("hi".into())),
            (
                Value::Uint(4),
                Value::Array(vec![Value::Array(vec![
                    Value::Uint(0),
                    Value::Bytes(vec![0xaa; 32]),
                ])]),
            ),
        ]);
        json_round_trip(&v);
    }

    #[test]
    fn json_rejects_float() {
        assert!(matches!(from_json("1.5"), Err(Error::Cbor(_))));
    }

    #[test]
    fn json_rejects_exponent() {
        assert!(matches!(from_json("1e5"), Err(Error::Cbor(_))));
        assert!(matches!(from_json("1E5"), Err(Error::Cbor(_))));
    }

    #[test]
    fn json_rejects_leading_zero() {
        assert!(matches!(from_json("01"), Err(Error::Cbor(_))));
        assert!(matches!(from_json("-01"), Err(Error::Cbor(_))));
    }

    #[test]
    fn json_rejects_negative_zero() {
        assert!(matches!(from_json("-0"), Err(Error::Cbor(_))));
    }

    #[test]
    fn json_rejects_bad_escape() {
        assert!(matches!(from_json("\"\\q\""), Err(Error::Cbor(_))));
    }

    #[test]
    fn json_rejects_lone_surrogate() {
        assert!(matches!(from_json("\"\\uD800\""), Err(Error::Cbor(_))));
        assert!(matches!(from_json("\"\\uDC00\""), Err(Error::Cbor(_))));
    }

    #[test]
    fn json_accepts_surrogate_pair() {
        // U+1F600 GRINNING FACE as a surrogate pair.
        let v = from_json("\"\\uD83D\\uDE00\"").unwrap();
        assert_eq!(v, Value::Text("\u{1F600}".to_string()));
    }

    #[test]
    fn json_rejects_trailing_data() {
        assert!(matches!(from_json("1 2"), Err(Error::Cbor(_))));
    }

    #[test]
    fn json_rejects_out_of_range_numbers() {
        // u64::MAX + 1
        assert!(matches!(
            from_json("18446744073709551616"),
            Err(Error::Cbor(_))
        ));
        // Negative numbers are restricted to i64 range even though u64::MAX
        // magnitude would otherwise be representable as an Nint.
        assert!(matches!(
            from_json("-9223372036854775809"),
            Err(Error::Cbor(_))
        ));
        assert!(from_json("-9223372036854775808").is_ok());
    }

    #[test]
    fn json_rejects_duplicate_object_key() {
        assert!(matches!(
            from_json("{\"a\":1,\"a\":2}"),
            Err(Error::Cbor(_))
        ));
    }

    #[test]
    fn json_rejects_depth_exceeding_limit() {
        let mut s = String::new();
        for _ in 0..66 {
            s.push('[');
        }
        s.push('0');
        for _ in 0..66 {
            s.push(']');
        }
        assert!(matches!(from_json(&s), Err(Error::Cbor(_))));
    }

    #[test]
    fn json_non_round_trippable_record_example() {
        // Spec 001 §11 test-vector group `json/` requires a JSON fixture
        // that must fail to round-trip to canonical CBOR. A float value
        // anywhere in the document cannot be represented, so it is
        // rejected at parse time rather than silently coerced.
        assert!(from_json("{\"1\":1.0}").is_err());
    }
}
