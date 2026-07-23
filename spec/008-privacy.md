# 008: Privacy

**Status:** Draft 0.2
**Depends on:** [001-kernel.md](001-kernel.md), [002-identity.md](002-identity.md), [003-kinds-registry.md](003-kinds-registry.md), [004-manifest.md](004-manifest.md)
**Depended on by:** [009-gateway.md](009-gateway.md)

Privacy in Evermesh is key distribution, not server permission. The
substrate never inspects blobs; encrypted content is ordinary blobs plus
a manifest `encryption` field plus `keygrant` records. This file
specifies the four privacy modes, the content encryption scheme, the
key-wrap registry, and the metadata rules. Encryption is in the manifest
format from v1: retrofitting privacy onto a plaintext-assuming format is
a known failure mode and is rejected.

## 1. Modes

| Mode | Mechanism |
|------|-----------|
| Public | Plain blobs; manifest published to relays |
| Unlisted | Encrypted blobs; manifest **unpublished** (shared directly); content key in the share-URL fragment |
| Private | Encrypted blobs; content key wrapped per recipient via `keygrant` |
| Gated | Private, with a gateway as key vendor: pay ([010](010-economics.md)), receive a keygrant |

Modes are conventions over the same three primitives; nothing else in
the protocol changes.

## 2. Content encryption

`encryption` field of the manifest ([004](004-manifest.md) §1):

```
Enc = { scheme: uint, key_hint: text? }
```

**Content-encryption schemes**

| Id | Scheme |
|---:|--------|
| 0 | reserved |
| 1 | `xchacha20poly1305-chunked` (§2.1) |

### 2.1 Scheme 1: chunked XChaCha20-Poly1305

Two key levels: one random 32-byte **content key** per manifest, and
one random 32-byte **blob key** per encrypted blob. The content key
encrypts no media directly; it wraps blob keys. This eliminates
cross-blob nonce reuse by construction: no key ever encrypts two
plaintexts under the same nonce.

**Blob encryption.** Plaintext is split into segments of exactly
`1 MiB − 16` bytes (1,048,560), so each encrypted chunk is exactly
1 MiB and aligns with the chunk tree of [001](001-kernel.md) §8.
Chunk `i` (0-based) is encrypted with XChaCha20-Poly1305: key = the
blob key; nonce = 24 bytes: ASCII `vmenc:v1` (8 bytes) || `i` as 8
little-endian bytes || 8 zero bytes; AAD = empty; the 16-byte tag is
appended. The blob — and its id, `size`, and `chunk_root` — is the
**ciphertext**. Range reads therefore verify and decrypt per chunk
without the rest of the blob.

**Blob-key wrap.** In an encrypted manifest, every Media and Caption
entry carries one additional required field:

| Field | Type | Meaning |
|-------|------|---------|
| `wrapped_blob_key` | bytes(64) | Blob key encrypted with XChaCha20-Poly1305 under the content key; nonce = ASCII `vmenc:kw` || 16 zero bytes; wire = ciphertext (48) ‖ tag (16) |

The fixed wrap nonce is safe because each content key wraps each blob
key exactly once and every plaintext is a fresh random key. Possession
of the content key unlocks the whole rendition family.

### 2.2 Key hint

`key_hint` is a human/machine hint about how to obtain the content key
(`"url-fragment"`, `"keygrant"`, `"vendor:https://…"`). It carries no
key material.

## 3. Key distribution

* **Unlisted:** the share URL carries
  `#k=<base64url content key>`; fragments never reach servers.
* **Private:** a `keygrant` record ([003](003-kinds-registry.md) §8.2)
  per recipient wraps the content key.

**Key-wrap algorithms (`wrap_alg`)**

| Id | Algorithm |
|---:|-----------|
| 0 | reserved |
| 1 | `x25519-sealed`: ephemeral X25519 → HKDF-BLAKE3 → XChaCha20-Poly1305; wire = ephemeral pubkey (32) ‖ ciphertext ‖ tag (16) |

Recipients SHOULD publish a dedicated X25519 public key in their
profile (`enc_key`, [003](003-kinds-registry.md) §3.2). Wrapping to a
converted Ed25519 signing key is NOT RECOMMENDED — signing and
encryption keys should not be the same object, and rotation of one
should not force the other.

## 4. Gated access

A gateway acting as key vendor holds the content key by arrangement
with the creator, sells access, and issues `keygrant` records to
payers. This is a business built *on* the primitives: the protocol sees
ordinary keygrants; exit remains open (the creator holds the key and
can appoint other vendors).

## 5. Metadata rules (normative)

* Private/unlisted manifests MUST NOT be published to public relays —
  encryption hides content, not existence, titles, or social graphs.
  They travel directly, via private relays, or in bundles.
* Keygrants for private content SHOULD travel the same private paths; a
  public keygrant leaks recipient identity and grant existence.
* Encrypted blobs on public infrastructure leak size and timing. That
  is the accepted residual ([011](011-threat-model.md) §4).
* Nodes MAY pin ciphertext they cannot read, **by explicit subscription
  only** — the intended and most legally defensible posture.

## Decisions

* Per-blob random keys with a manifest-family content key wrapping
  them (§2.1) — eliminates cross-blob nonce reuse by construction
  rather than by convention.
* Encrypted chunk size is exactly 1 MiB *including* tag, keeping
  ciphertext chunk-tree-aligned so encrypted range reads verify
  identically to plaintext ones.
* Dedicated `enc_key` in profiles rather than Ed25519→X25519
  conversion.

## Test vectors

* `privacy/enc-*` — scheme 1 fixtures: known keys, 1-chunk and 3-chunk
  blobs, blob-key wrap/unwrap, decrypt-verify of a middle chunk via
  range proof; invalid: tag tamper, wrong chunk index nonce.
* `kinds/keygrant/` — wrap_alg 1 round-trip with fixed ephemeral key;
  per [003](003-kinds-registry.md) invalid mutations.
