// Performance budget (frontend-spec 6): the JS a first visit to Home loads (the entry, everything it
// imports statically, and the shell and Home route chunks with their static imports) stays at or
// under 250 KB gzipped. Run after `npm run build`; it reads the Vite manifest, then removes it so
// the gateway doesn't serve it.
import { readFileSync, rmSync } from "node:fs";
import { gzipSync } from "node:zlib";

const BUDGET_KB = 250;
const dist = new URL("../dist/", import.meta.url);
const manifest = JSON.parse(readFileSync(new URL(".vite/manifest.json", dist), "utf8"));
const START = [
  "index.html",
  "src/routes/_app.tsx?tsr-split=component",
  "src/routes/_app/index.tsx?tsr-split=component",
];

const seen = new Set();
const visit = (key) => {
  if (seen.has(key) || !manifest[key]) return;
  seen.add(key);
  for (const k of manifest[key].imports ?? []) visit(k);
};
for (const k of START) {
  if (!manifest[k]) throw new Error(`${k} isn't in the manifest: did the route files move?`);
  visit(k);
}

const rows = [...seen]
  .map((k) => manifest[k].file)
  .filter((f) => f.endsWith(".js"))
  .map((f) => ({ f, kb: gzipSync(readFileSync(new URL(f, dist))).length / 1024 }))
  .sort((a, b) => b.kb - a.kb);
const total = rows.reduce((n, r) => n + r.kb, 0);
for (const r of rows) console.log(`${r.kb.toFixed(1).padStart(7)} KB  ${r.f}`);
console.log(`${total.toFixed(1).padStart(7)} KB  shell + Home, gzipped (budget ${BUDGET_KB} KB)`);
rmSync(new URL(".vite/", dist), { recursive: true, force: true });
if (total > BUDGET_KB) {
  console.error(`Over the budget by ${(total - BUDGET_KB).toFixed(1)} KB`);
  process.exit(1);
}
