# @evermesh/gateway-web

The reference gateway frontend: React 18 + TypeScript + Vite + Tailwind
CSS v3 + TanStack Query v5 + react-router-dom v6. This is **the**
uniform reference UI (spec [009-gateway.md](../../../spec/009-gateway.md)
§7) — every gateway that wants Evermesh trademark compliance deploys this
same product. Gateways differ by domain, catalog (what they choose to
index), and branding accents; they do not differ by interface.

## Uniform-UI doctrine

> A viewer moving between gateways changes URL and catalog, not
> interface. (spec 009 §1)

Concretely, that means:

- Every gateway ships the same pages, the same player, the same
  verification badge, the same interaction patterns.
- A gateway MAY extend the UI (add pages, add features). It MUST NOT
  remove:
  1. **The verification badge** (`@evermesh/ui`'s `VerifiedBadge`,
     wired up for real in `src/routes/Watch.tsx` via
     `src/hooks/useVerification.ts`) — the client-side proof that a
     manifest's signature and record id were checked in the viewer's
     own browser, not asserted by the server.
  2. **The moderation-policy page** (`/policy`, `src/routes/Policy.tsx`)
     — "what this gateway serves," including its subscribed takedown
     feeds and the "counts are this gateway's claims" explainer.
  3. **The identity-export flow** (`/me`, `src/routes/Me.tsx`) — a
     custodial gateway's demonstrable exit path: password re-confirm,
     then a downloaded JSON file with the genesis record, rotation
     chain, and held keys (spec 009 §5). Non-negotiable.
- Differentiation happens in **branding accents** (see below), catalog,
  and gateway-specific services — never in relearning the product.

## Pages

| Route | File | Notes |
|---|---|---|
| `/` | `src/routes/Home.tsx` | Latest videos (infinite-scroll "Load more"), or search results when `?q=` is present. |
| `/watch/:id` | `src/routes/Watch.tsx` | Player, **real** client-side verification badge, reactions, threaded comments, tip panel (payment pointers + receipts, display-only), provenance claims panel. |
| `/channel/:identityId` | `src/routes/Channel.tsx` | Profile, videos, follow button. |
| `/upload` | `src/routes/Upload.tsx` | Auth-gated. File picker + drag-drop, metadata form, async processing-status polling. |
| `/policy` | `src/routes/Policy.tsx` | The moderation-policy page (see above). |
| `/auth` | `src/routes/Auth.tsx` | Sign in / create account. |
| `/me` | `src/routes/Me.tsx` | Profile edit + the identity-export flow. |

`src/components/Layout.tsx` provides the header (search, nav, dark-mode
toggle), the skip link, focus management on route change, and the
footer ("powered by evermesh" + policy link) around every page.

## Client-side verification (the substance behind the badge)

`src/lib/verify.ts` exports `verifyRecordById`, a pure function that:

1. Fetches a record's canonical CBOR bytes from
   `GET /api/records/{id}/cbor`.
2. Runs `@evermesh/kernel`'s `verifyRecord` (Ed25519 signature + envelope
   shape) on those bytes, in the browser.
3. Runs `deriveId` on the same bytes and checks it equals the id the
   page requested.

Only if both checks pass does the badge say "Verified." No server
response is trusted for that word. `src/hooks/useVerification.ts` wires
this into TanStack Query so watch pages get loading/error states for
free, and the kernel (WASM) is only fetched on pages that actually
verify something (dynamic `import("@evermesh/kernel")`).

## Running against the server

```bash
pnpm --filter @evermesh/gateway-server dev   # apps/gateway/server, port 8600
pnpm --filter @evermesh/gateway-web dev      # this app, Vite dev server
```

`vite.config.ts` proxies `/api` and `/media` to
`http://localhost:8600` in dev, so every fetch in `src/api.ts` uses a
plain relative path — no CORS, no base-URL configuration. Production
builds are served from the same origin as the API (reverse-proxied
together).

## Branding-accent extension points

A gateway operator re-skins accents by overriding CSS custom properties
in `src/styles/index.css` (`--bo-brand-*`, `--bo-accent-*`, plus
`--bo-bg`/`--bo-surface`/`--bo-fg`/`--bo-muted`/`--bo-border`) — no
Tailwind rebuild, no component edits, no interface changes. Dark mode
uses Tailwind's `class` strategy; the toggle
(`src/hooks/useTheme.ts`) persists to `localStorage` with a
`prefers-color-scheme` default, applied synchronously in `index.html`
before first paint to avoid a flash.

## State management

TanStack Query owns all server data (queries + mutations); component
state is local `useState` for form/UI state only. No Redux, no other
global state library, per the locked stack.

## Typed API client

`src/api.ts` has one function per `apps/gateway/API.md` endpoint, all
routed through a single `request()`/`requestBuffer()` helper so the
`{error:{code,message}}` envelope is handled in exactly one place and
surfaced as `ApiError` (with `.code` and `.status`). Types are
hand-written in `src/lib/api-types.ts`, mirrored from the contract (and
cross-checked against `apps/gateway/server/src/types.ts`) — no zod, by
design, since both processes are built from the same locked contract.

### API.md gaps found while implementing (flagged for the lead to reconcile)

- **`createdAt` units are unspecified.** API.md's response shapes use
  an explicit `Ms` suffix for millisecond fields (`durationMs`,
  `startMs`/`endMs`) but plain `createdAt` for timestamps. We assumed
  Unix **seconds**, matching the kernel record's native `createdAt`
  unit (spec 001), and convert with `* 1000` at every render call site
  (`TimeAgo` takes `unixMs` explicitly to force this at the call site).
  If the server actually emits milliseconds, every `* 1000` in this app
  needs to come out.
- **Register/login response body is undocumented** beyond "sets
  session." Typed as `MeResponse`; the app re-fetches `/api/me`
  immediately after either call rather than trusting the response
  shape (`src/routes/Auth.tsx`).
- **`POST /api/me/export` request body is undocumented** beyond
  "password re-confirmed." Assumed `{ password }`.
- **No endpoint to read the caller's current follow state** for a
  given identity (no `following` field on `Channel`, nothing on
  `/api/me`). `src/routes/Channel.tsx` tracks it optimistically in
  local state; it resets on reload until the contract adds a source of
  truth.
- **No endpoint to list "my channels"** for the upload form's
  `channelId` field, or to upload an avatar image to get an
  `avatarBlobId`. Both are left as plain optional text inputs
  (`src/routes/Upload.tsx`, `src/routes/Me.tsx`) pending those
  endpoints.
- **Comment/reaction POST response bodies are undocumented.** Treated
  as fire-and-forget; the caller invalidates the relevant query
  (`["comments", id]`) instead of trusting a return value.

None of these are inventions of new endpoints — every request in
`src/api.ts` targets an endpoint API.md documents; the gaps above are
about *shapes* API.md left unstated.

## Tests

`vitest` + `@testing-library/react` + `jsdom` (added as devDependencies
beyond the locked stack — justified by the standing directive to test
extensively; not shipped to production, dev-only). Never run
automatically as part of `build`; run explicitly with `pnpm test`.

| File | Covers |
|---|---|
| `test/api.test.ts` | The fetch client: query-string building, the `{error}` envelope → `ApiError`, non-JSON error bodies, JSON vs. multipart request bodies. |
| `test/commentTree.test.ts` | Comment threading: nesting, sibling ordering, orphaned-parent promotion (never silently dropping a subtree), counting. |
| `test/verifiedBadge.test.tsx` | The verification state machine (`verifyRecordById`) with a mocked kernel — verified/failed/signature-throws/fetch-throws — plus rendering of `VerifiedBadge`'s three states and its expandable detail popover. |
| `test/playerLogic.test.ts` | The player's keyboard map and reducer, imported straight from `@evermesh/ui` — every key binding, clamping, buffered-range parsing, sponsor-segment percentage math, clock formatting. |
| `test/routes.smoke.test.tsx` | Every route renders without throwing against mocked queries, with correct empty/error copy for a brand-new, signed-out, empty gateway. |

## Development

```bash
pnpm --filter @evermesh/gateway-web lint   # tsc --noEmit, strict
pnpm --filter @evermesh/gateway-web dev    # Vite dev server
pnpm --filter @evermesh/gateway-web build  # tsc -b && vite build
pnpm --filter @evermesh/gateway-web test   # vitest run
```
