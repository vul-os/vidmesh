#!/usr/bin/env node
/**
 * Copies the documents the site renders into apps/site/docs/.
 *
 *   node tools/site/sync-docs.mjs          # copy
 *   node tools/site/sync-docs.mjs --check  # fail if a copy is stale
 *
 * The site is deployable as a plain directory (see apps/site/README.md), so
 * its docs viewer fetches same-origin `.md` files rather than reaching into
 * the repo. These copies are byte-identical to the sources — the viewer
 * does its own link rewriting at render time, so nothing here edits spec
 * text. `--check` is the guard that keeps the copies honest; run it in CI
 * or before a release.
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const dest = path.join(repo, "apps", "site", "docs");

/** source (repo-relative) -> slug the docs viewer routes on */
export const DOCS = [
  ["spec/README.md", "spec-index"],
  ["spec/000-overview.md", "000-overview"],
  ["spec/001-kernel.md", "001-kernel"],
  ["spec/002-identity.md", "002-identity"],
  ["spec/003-kinds-registry.md", "003-kinds-registry"],
  ["spec/004-manifest.md", "004-manifest"],
  ["spec/005-claims.md", "005-claims"],
  ["spec/006-relay.md", "006-relay"],
  ["spec/007-bundles.md", "007-bundles"],
  ["spec/008-privacy.md", "008-privacy"],
  ["spec/009-gateway.md", "009-gateway"],
  ["spec/010-economics.md", "010-economics"],
  ["spec/011-threat-model.md", "011-threat-model"],
  ["spec/CHANGELOG.md", "changelog"],
  ["spec/draft-evermesh-protocol-00.md", "draft-evermesh-protocol-00"],
  ["DECISIONS.md", "decisions"],
  ["docs/DMTAP-CONVERGENCE.md", "dmtap-convergence"],
];

const check = process.argv.includes("--check");
fs.mkdirSync(dest, { recursive: true });

let stale = 0;
for (const [src, slug] of DOCS) {
  const from = path.join(repo, src);
  const to = path.join(dest, `${slug}.md`);
  const body = fs.readFileSync(from);
  if (check) {
    const current = fs.existsSync(to) ? fs.readFileSync(to) : null;
    if (current === null || !current.equals(body)) {
      console.error(`stale: apps/site/docs/${slug}.md != ${src}`);
      stale += 1;
    }
  } else {
    fs.writeFileSync(to, body);
  }
}

if (check) {
  if (stale) {
    console.error(`\n${stale} stale copy/copies — run: node tools/site/sync-docs.mjs`);
    process.exit(1);
  }
  console.log(`site docs in sync (${DOCS.length} files)`);
} else {
  console.log(`synced ${DOCS.length} docs -> apps/site/docs/`);
}
