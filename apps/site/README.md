# apps/site

evermesh.org: a static landing page and a hash-routed docs viewer. No
framework, no build step, no CDN. Deployable as a directory.

```
index.html          landing page — hero, survival test, how it works,
                    roles, honest status, spec index, objections
docs.html           docs viewer (hash routes over ./docs/*.md)
docs/               copies of spec/ + DECISIONS.md + docs/, written by
                    tools/site/sync-docs.mjs — do not hand-edit
style.css           layout only; colour/type come from assets/tokens.css
assets/             brand: logos, favicon, tokens.css, vendored fonts,
                    architecture diagram, OG card, marked.js
screenshots/        refreshed by `just site-shots`
```

## Preview locally

```sh
just site-serve        # http://127.0.0.1:8080
```

Any static file server works (`npx serve`, `caddy file-server`, an nginx
`root`). The docs viewer fetches markdown, so it needs HTTP — opening
`docs.html` over `file://` will show load errors.

## Verify

```sh
just site-check        # sync check + real-browser check
```

`tools/site/check.mjs` drives Chromium over the landing page (dark **and**
light) and every docs route, and fails on: any console or page error, any
failed request or 4xx, any internal link that does not resolve, any docs
route that renders empty or errors, and the display font failing to load.
`just site-shots` does the same and refreshes `screenshots/`.

`tools/site/sync-docs.mjs --check` fails if a copy under `docs/` has
drifted from its source in `spec/`. The copies are byte-identical: the
viewer rewrites cross-references at render time rather than editing spec
text, because the spec is normative and this is only a rendering of it.

## Deploy

Copy `apps/site/*` to any static host with no build command. Everything is
same-origin: fonts, the markdown renderer (`assets/vendor/marked.umd.js`,
MIT), and the documents themselves.

Before going live, check `<link rel="canonical">`, the `og:` URLs,
`robots.txt` and `sitemap.xml` — they all assume the
`https://evermesh.org/` origin.

## Design

The brand is documented in [`assets/README.md`](../../assets/README.md):
"signal on carbon", Syne / Hanken Grotesk / JetBrains Mono, and the
`--bo-*` tokens in `assets/tokens.css` that this stylesheet and the
gateway's reference UI both read from.

- Both themes come from `prefers-color-scheme` — no toggle, no JavaScript,
  no flash of the wrong theme. Contrast is measured, not eyeballed; the
  table is in `assets/README.md`.
- One animation on the whole site (a packet crossing the hero lattice),
  and it is removed entirely under `prefers-reduced-motion`.
- The status section is deliberately unflattering and mirrors the
  repository README. If the README's truth changes, change it here in the
  same commit.
