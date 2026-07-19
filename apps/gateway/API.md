# Gateway REST API (v1 contract)

The contract between `apps/gateway/server` and `apps/gateway/web`. Both
sides implement exactly this; changes happen here first. All responses
are JSON unless noted; hashes are lowercase hex; record JSON uses the
kernel's JSON interchange form (spec 001 §11) under a `record` key, with
derived fields beside it.

Errors: `{ "error": { "code": string, "message": string } }` with an
appropriate HTTP status. Common codes: `not_found`, `invalid`,
`policy_denied`, `rate_limited`, `upload_failed`.

## Public read API

| Endpoint | Response |
|----------|----------|
| `GET /api/videos?limit&cursor&channel&author` | `{ items: VideoSummary[], next: string\|null }` newest-first selected manifests |
| `GET /api/videos/{manifestId}` | `Video` (full manifest record + derived) |
| `GET /api/videos/{manifestId}/comments` | `{ items: Comment[] }` threaded via `parent` |
| `GET /api/videos/{manifestId}/claims` | `{ items: ClaimView[] }` chain per spec 005, presented as assertions |
| `GET /api/videos/{manifestId}/receipts` | `{ items: ReceiptView[] }` |
| `GET /api/channels/{identityId}` | `Channel` (profile + channels + videos) |
| `GET /api/channels/{identityId}/videos?limit&cursor` | `{ items: VideoSummary[], next }` |
| `GET /api/records/{recordId}` | `{ record, id, kind }` raw fetch of any indexed record |
| `GET /api/records/{recordId}/cbor` | canonical CBOR bytes, `application/cbor` — what the browser verifies client-side |
| `GET /api/search?q&limit` | `{ items: VideoSummary[] }` title/description/tags |
| `GET /api/policy` | `{ name, description, moderationPolicyHtml, feeds: [{feed, publisher}], stats }` — the visible moderation-policy page data |
| `GET /api/info` | `{ gateway, version, relays: string[], uploadEnabled: bool }` |

Types (shapes, not exhaustive):

```ts
VideoSummary = { id: string; title: string; author: { identityId, name, avatarUrl? };
                 thumbnailUrl: string|null; durationMs: number; createdAt: number;
                 channelId?: string }
Video = VideoSummary & {
  description: string; tags: string[]; language?: string;
  record: object;                 // manifest JSON interchange form
  recordCborUrl: string;          // for client-side verification
  playback: { hlsUrl: string|null; mp4Url: string|null;
              renditions: { height: number; hlsUrl: string }[] };
  captions: { language: string; url: string }[];
  license: string; payment: [number, string][];
  sponsorship: { startMs, endMs, label }[];
  counts: { comments: number; reactions: Record<string, number> };  // per-gateway claims
}
Comment = { id, author: {identityId, name}, text, createdAt, parent: string|null,
            record: object }
```

## Blob/media serving

| Endpoint | Behavior |
|----------|----------|
| `GET /media/hls/{manifestId}/master.m3u8` | HLS master playlist for the manifest's renditions |
| `GET /media/hls/{manifestId}/{rendition}/index.m3u8` | Media playlist |
| `GET /media/hls/{manifestId}/{rendition}/{segment}.m4s` | fMP4 segments (blobs) |
| `GET /media/blob/{blobId}` | Raw blob, Range supported |
| `GET /media/thumb/{blobId}` | Thumbnail (image content-type) |

## Authenticated API (v1: cookie session; server-custodied keys)

| Endpoint | Behavior |
|----------|----------|
| `POST /api/auth/register` `{handle, password}` | Creates account + custodied identity (genesis published); sets session |
| `POST /api/auth/login` / `POST /api/auth/logout` | Session management |
| `GET /api/me` | `{ handle, identityId, profile, exportAvailable: true }` |
| `POST /api/me/export` | `{ identity: {...}, secretKeys: [...] }` — the guaranteed identity-export (spec 009 §5); rate-limited, password re-confirmed |
| `PUT /api/me/profile` `{name, about, avatarBlobId?}` | Publishes profile record |
| `POST /api/upload` multipart `{file, title, description?, tags?, channelId?, license}` | → `{ uploadId }`; async pipeline |
| `GET /api/upload/{uploadId}` | `{ status: "processing"\|"published"\|"failed", manifestId?, progress?, error? }` |
| `POST /api/videos/{manifestId}/comments` `{text, parent?}` | Signs+publishes comment as user's identity |
| `POST /api/videos/{manifestId}/reactions` `{reaction}` | Signs+publishes reaction |
| `POST /api/follow` `{identityId}` / `DELETE /api/follow/{identityId}` | Follow records |

## Compliance API

| Endpoint | Behavior |
|----------|----------|
| `POST /api/compliance/notice` (structured form per spec 003 §6.5) | Emits signed `notice.takedown`, applies local policy, returns `{ noticeId }` |
| `POST /api/compliance/counter` | Emits `notice.counter` |
| `GET /api/compliance/notices/{id}` | Public notice record |

## Share cards

`GET /watch/{manifestId}` (the web app route) must be served with
OpenGraph meta tags server-side rendered (og:title, og:image →
thumbnail, og:video where applicable) for link unfurling; the server
provides `GET /api/videos/{id}/og` returning the fields if the web
layer needs them.

## Conventions

- `cursor` pagination: opaque string, `next: null` at the end.
- All identity-attributed content served only if the gateway's policy
  engine currently selects it; de-indexed content returns `not_found`
  with `policy_denied` code where disclosure is lawful.
- Counts are this gateway's claims and are labeled as such in the UI.
