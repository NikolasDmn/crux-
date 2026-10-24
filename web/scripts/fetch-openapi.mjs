// Saves the gateway's merged OpenAPI document to openapi.json (the client is generated from it with
// `npm run api`). Needs the stack running with every service, ai included:
//   CRUX_GATEWAY=http://localhost:8000 npm run api:fetch && npm run api
import { writeFile } from "node:fs/promises";

const base = process.env.CRUX_GATEWAY ?? "http://localhost:8000";
const r = await fetch(`${base}/api/v1/openapi.json`);
if (!r.ok) throw new Error(`${base}: ${r.status}`);
const doc = await r.json();
const prefixes = ["/files/", "/notify/", "/ai/"];
const missing = prefixes.filter((p) => !Object.keys(doc.paths).some((k) => k.startsWith(p)));
if (missing.length) throw new Error(`no paths under ${missing.join(", ")}: is every service up (profile ai)?`);
await writeFile(new URL("../openapi.json", import.meta.url), JSON.stringify(doc, null, 2) + "\n");
console.log(`openapi.json: ${Object.keys(doc.paths).length} paths`);
