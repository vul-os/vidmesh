# @evermesh/ui

Shared React components for every Evermesh gateway frontend. This is the
component layer of the **uniform reference UI** (spec
[009-gateway.md](../../spec/009-gateway.md) §7): gateways differ by
domain, catalog, and branding accents — never by reimplementing the
player, the verification badge, or record display. Consumed today by
`apps/gateway/web`; any future gateway frontend should import from here
rather than re-building these pieces.

## What's here

| Export | File | Purpose |
|---|---|---|
| `Player` | `src/player/Player.tsx` | The video player: hls.js when MSE + an HLS URL are available, native `<video src>` otherwise (Safari HLS or mp4 fallback). Full custom control bar, keyboard-operable, captions via `<track>`, sponsorship markers on the scrubber. |
| `keyToAction`, `playerReducer`, `bufferedRanges`, `sponsorSegmentStyle`, `formatClockTime` | `src/player/playerLogic.ts` | The player's logic factored out as pure functions — no DOM, no React — so the keyboard map and state transitions are unit-testable without mounting a `<video>`. |
| `VerifiedBadge` | `src/VerifiedBadge.tsx` | Three-state (verifying/verified/failed) client-side verification badge with an expandable popover explaining *what* was checked. Never removed by a compliant gateway (spec 009 §7). |
| `RecordCard` | `src/RecordCard.tsx` | Generic author/time/kind/body layout used for comments, claims, receipts, notices. |
| `Avatar`, `TimeAgo`, `cn` | `src/*.ts(x)` | Small shared primitives (initials-fallback avatar, relative time, class-name join helper). |

## Player API

```tsx
import { Player } from "@evermesh/ui";

<Player
  hls="/media/hls/abc123/master.m3u8"
  mp4={null}
  poster="/media/thumb/def456"
  captions={[{ language: "en", url: "/media/blob/...vtt" }]}
  sponsorSegments={[{ startMs: 30_000, endMs: 45_000, label: "Sponsor read" }]}
/>
```

Keyboard map (active whenever the player container has focus):

| Key | Action |
|---|---|
| Space / `k` | Play / pause |
| `←` / `→` | Seek −5s / +5s |
| `↑` / `↓` | Volume +10% / −10% |
| `f` | Toggle fullscreen |
| `m` | Toggle mute |
| `c` | Toggle captions (last-selected track, or first available) |

All controls are real `<button>`/`<input type="range">` elements with
`aria-label`s; the scrubber and volume slider get native slider
semantics for free. Sponsor segments are additionally exposed as a
visually-hidden list for screen-reader users, since the on-timeline
marks are presentational.

## Verified badge

`VerifiedBadge` is presentational only — it does not itself call the
kernel. The consuming app performs the actual verification (fetch
`/api/records/{id}/cbor`, run `verifyRecord` + `deriveId` from
`@evermesh/kernel`, compare the derived id to the requested id) and maps
the result to `state: "verifying" | "verified" | "failed"`. See
`apps/gateway/web/src/lib/verify.ts` for the reference implementation.

## Using this package in a gateway frontend

1. Add `"@evermesh/ui": "workspace:*"` (or a published version) and
   `react`/`react-dom` as dependencies.
2. **Tailwind:** these components are styled with Tailwind utility
   classes and ship no CSS of their own. Your app's `tailwind.config`
   **must** include this package's sources in `content`, e.g.:

   ```ts
   content: [
     "./index.html",
     "./src/**/*.{ts,tsx}",
     "../../../packages/ui/src/**/*.{ts,tsx}", // adjust the relative path to your app
   ],
   ```

   Without this, Tailwind's JIT scanner never sees these components'
   class names and ships an empty stylesheet for them.
3. The components reference two color scales that must exist in your
   Tailwind theme: `brand` (identity/structure) and `accent`
   (verification, sponsorship, focus rings) — see
   `apps/gateway/web/tailwind.config.ts` for the reference mapping onto
   CSS custom properties, which is also the gateway branding-accent
   extension point.
4. Dark mode uses Tailwind's `class` strategy (`dark:` variants); the
   consuming app owns toggling the `dark` class on `<html>`.

## Development

No build step for consumers using the monorepo directly (`main`/`types`
point straight at `src/index.ts`; the consuming Vite app compiles
these `.tsx` files itself). `pnpm --filter @evermesh/ui lint` runs
`tsc --noEmit` in strict mode.
