# assets

The Vidmesh brand: a hand-written SVG mark and wordmark, the design tokens
every surface reads from, the vendored type, an architecture diagram, and a
social share card.

## Inventory

| File | Purpose |
|---|---|
| `logo.svg` | Full lockup (mark + "vidmesh" wordmark) for light backgrounds. `viewBox="0 0 960 256"`. |
| `logo-dark.svg` | Same geometry, recolored for dark backgrounds. |
| `mark.svg` | Mark only, structure drawn in `currentColor` so it can be inlined into a UI and inherit the text color. |
| `favicon.svg` | Mark only, thickened to stay legible at 16px; structure follows `prefers-color-scheme`. `viewBox="0 0 256 256"`. |
| `tokens.css` | **The design tokens.** Colour ramps, semantic colours, type stack, radius, elevation — light and dark. Everything else reads from here. |
| `fonts/` | Vendored woff2 (latin subset) + `fonts.css` + the SIL OFL licenses. No CDN. |
| `architecture.svg` | Creators → substrate (records + blobs) → gateways → viewers/nodes. Ships its own neutral card background so it reads on any page. |
| `og-image.png` | 1200×630 social share card, rendered from `tools/brand/og-card.html` by `node tools/brand/render.mjs`. |

The card is HTML-sourced rather than SVG-sourced on purpose: an
`<img>`-embedded SVG cannot load the vendored faces, so an SVG card would
always export in a fallback system font.

## The mark

A play triangle whose three vertices each throw an edge out to a mesh
node — *the video is one point in a lattice that keeps going*. It is the
whole thesis in one glyph: a record is playable on its own, and it is never
only in one place.

Everything is drawn by hand as `<path>`/`<circle>`: no raster effects, no
gradients, no filters. The "vidmesh" wordmark is built the same way, on a
shared grid (baseline 180, x-height 100, ascender 30, stroke 22, round
caps), with circular bowls drawn as béziers. There is no `<text>` and no
`font-family` anywhere in `logo.svg` or `logo-dark.svg` — the lockup renders
identically on every system with nothing to load and nothing to fall back
on. The dot on the `i` is the one piece of signal colour in the wordmark.

## Palette — "signal on carbon"

Carbon is a near-black with a green cast. Signal is an acid lime that only
appears where something is **live, verified, or actionable**. Mesh blue is
structural — identity, verification, the substrate. Live red is reserved for
record/live state and errors. Nothing else in the interface gets to be
bright; that restraint is what makes the lime mean something.

| Role | Token | Light surfaces | Dark surfaces |
|---|---|---|---|
| Signal (brand) | `--vm-brand-*` | `#4F6E09` (700) for marks, `#3B5208` (800) for text | `#A8E01F` (400) for marks, `#BCEC4D` (300) for text |
| Mesh (accent) | `--vm-accent-*` | `#126595` (600) | `#93D2F0` (200) |
| Surface | `--vm-bg` / `--vm-surface` | `#FAFBF7` / `#F0F2EC` | `#0A0C0B` / `#121614` |
| Text | `--vm-fg` / `--vm-muted` | `#10140F` / `#4C554D` | `#E9F0EA` / `#9FB0A6` |
| Live / record | `--vm-live` | `#C42A1C` | `#FF4D3D` |

Measured contrast (WCAG relative luminance) for every pairing that carries
meaning:

| Pairing | Ratio | Floor |
|---|---:|---|
| `--vm-fg` on `--vm-bg`, light | 17.9:1 | 4.5 |
| `--vm-fg` on `--vm-bg`, dark | 16.9:1 | 4.5 |
| `--vm-muted` on `--vm-bg`, light / dark | 7.5:1 / 8.6:1 | 4.5 |
| link `brand-800` on light `--vm-surface` | 7.0:1 | 4.5 |
| link `brand-300` on dark `--vm-surface` | 13.3:1 | 4.5 |
| verified `accent-600` on light `--vm-surface` | 5.6:1 | 4.5 |
| verified `accent-200` on dark `--vm-surface` | 11.1:1 | 4.5 |
| live red on `--vm-bg`, light / dark | 5.5:1 / 6.0:1 | 4.5 |
| ink on a `brand-400` button fill | 12.5:1 | 4.5 |

Acid lime `#A8E01F` on paper is **1.5:1** — under even the 3:1 non-text
floor. That is why the light-background lockup uses `#4F6E09` and why the
light theme never puts lime on a pale surface.

## Type

Vendored, latin-subset woff2 in `fonts/` — nothing is fetched from a CDN at
runtime, in the site or in the app.

| Role | Family | Weights | Why |
|---|---|---|---|
| Display | **Syne** | 700, 800 | Wide, slightly odd geometry; reads as broadcast/editorial rather than SaaS. Headlines only. |
| UI / body | **Hanken Grotesk** | 400–700 | A quiet grotesque with real character in the terminals; carries long spec prose without fatigue. |
| Data | **JetBrains Mono** | 400, 500 | Content addresses, record ids, code, tables. Distinct `0`/`O` matters when a hash is the point. |

All three are SIL OFL 1.1 (`fonts/LICENSE-*.txt`).

## Usage rules

- Never recolor the mark outside the palette above; never add a gradient,
  drop shadow, or blur.
- Keep the triangle-and-satellites geometry intact when resizing — it is
  designed to read at 16px and at 512px.
- Pick the light or dark lockup by the background it sits on, via
  `<picture>` + `prefers-color-scheme` (see `apps/site/index.html`).
- Spend the signal colour sparingly. If two things on a screen are lime,
  at least one of them is wrong.
- `apps/site/assets/` holds copies of these files so the site directory is
  deployable on its own; if you edit one here, copy it there too.
  `tools/brand/render.mjs` regenerates the raster exports in both places.
