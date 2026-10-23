// Milestone 1 demo console. Follows frontend-spec 2.1/2.2 where it matters: access token in
// memory only, silent refresh via the HttpOnly cookie (once, then retry), If-Match on edits,
// Idempotency-Key on creates, errors read from {"error":{code,message,details}}.
"use strict";

const API = "/api/v1";
const $ = (sel) => document.querySelector(sel);
const state = { token: null, me: null, settings: null };

// ---------- API ----------
function ulidish() {
  // Good enough for an Idempotency-Key from a demo page: time + randomness.
  const r = crypto.getRandomValues(new Uint8Array(10));
  return Date.now().toString(36).toUpperCase() + Array.from(r, (b) => (b % 36).toString(36)).join("").toUpperCase();
}

let refreshing = null;
async function refresh() {
  // One refresh at a time: concurrent refreshes would present a rotated token (OPEN-QUESTIONS Q-auth-6).
  refreshing ??= raw("POST", "/auth/refresh").finally(() => { refreshing = null; });
  const r = await refreshing;
  if (r.ok) { state.token = r.body.accessToken; state.me = r.body.user; return true; }
  state.token = null; state.me = null;
  return false;
}

async function raw(method, path, body, headers = {}) {
  const h = { ...headers };
  if (body !== undefined) h["content-type"] = "application/json";
  if (state.token) h.authorization = `Bearer ${state.token}`;
  const res = await fetch(API + path, { method, headers: h, body: body === undefined ? undefined : JSON.stringify(body), credentials: "same-origin" });
  const text = await res.text();
  let parsed = null;
  try { parsed = text ? JSON.parse(text) : null; } catch { parsed = text; }
  log(method, path, res.status, parsed, res.headers.get("x-request-id"));
  return { ok: res.ok, status: res.status, body: parsed };
}

async function api(method, path, body, headers) {
  let r = await raw(method, path, body, headers);
  if (r.status === 401 && state.token && path !== "/auth/refresh") {
    if (await refresh()) r = await raw(method, path, body, headers);
  }
  return r;
}

function errorText(r) {
  const e = r.body && r.body.error;
  if (!e) return `HTTP ${r.status}`;
  const fields = e.details && e.details.fields ? " " + e.details.fields.map((f) => `${f.field}: ${f.message}`).join(" · ") : "";
  const reason = e.details && e.details.reason ? ` (${e.details.reason})` : "";
  return `${e.message}${reason}${fields}`;
}

// ---------- UI helpers ----------
function log(method, path, status, body, rid) {
  const li = document.createElement("li");
  const cls = status < 300 ? "ok" : status < 500 ? "warn" : "bad";
  li.innerHTML = `<div class="line"><span class="status ${cls}"></span><code></code><small></small></div><pre></pre>`;
  li.querySelector(".status").textContent = status;
  li.querySelector("code").textContent = `${method} ${API}${path}`;
  li.querySelector("small").textContent = `${new Date().toLocaleTimeString()}${rid ? " · request " + rid : ""}`;
  li.querySelector("pre").textContent = body == null ? "(no body)" : JSON.stringify(body, null, 2);
  $("#log").prepend(li);
}

function banner(text, isError = false) {
  const b = $("#banner");
  b.textContent = text;
  b.classList.toggle("error", isError);
  b.classList.toggle("hidden", !text);
  if (text) b.scrollIntoView({ behavior: "smooth", block: "nearest" });
}

function formData(form) {
  return Object.fromEntries(new FormData(form).entries());
}

function show(id, on) { $(id).classList.toggle("hidden", !on); }

function cell(row, content) {
  const td = document.createElement("td");
  if (content instanceof Node) td.append(content); else td.textContent = content ?? "";
  row.append(td);
  return td;
}

function header(table, cols) {
  table.innerHTML = "";
  const tr = document.createElement("tr");
  cols.forEach((c) => { const th = document.createElement("th"); th.textContent = c; tr.append(th); });
  table.append(tr);
}

function button(text, cls, onClick) {
  const b = document.createElement("button");
  b.textContent = text; b.className = cls; b.type = "button";
  b.addEventListener("click", onClick);
  return b;
}

function when(iso) { return iso ? new Date(iso).toLocaleString() : "–"; }

// ---------- rendering ----------
function renderWho() {
  const who = $("#who");
  who.innerHTML = "";
  if (!state.me) { who.textContent = "Not signed in"; return; }
  const me = state.me;
  const name = document.createElement("span");
  name.textContent = `${me.firstName} ${me.lastName} · @${me.username} · ${me.role} · tier ${me.tier}`;
  const verified = document.createElement("span");
  verified.className = "pill " + (me.emailVerified ? "ok" : "warn");
  verified.textContent = me.emailVerified ? "✓ email verified" : "! email not verified";
  who.append(name, verified);
  if (!me.emailVerified) {
    who.append(button("Resend verification", "secondary", async () => {
      const r = await api("POST", "/auth/verify-email");
      banner(r.ok ? `Verification link sent to ${me.email}. Open Mailpit (localhost:8025).` : errorText(r), !r.ok);
    }));
  }
  who.append(button("Log out", "secondary", async () => {
    await raw("POST", "/auth/logout");
    state.token = null; state.me = null;
    render();
  }));
}

async function render() {
  renderWho();
  const signedIn = !!state.me;
  show("#signed-out", !signedIn && !location.pathname.startsWith("/reset"));
  for (const id of ["#profile", "#settings", "#invites"]) show(id, signedIn);
  show("#admin", signedIn && state.me.role === "Admin");
  if (!signedIn) return;

  const f = $("#f-profile");
  for (const k of ["username", "firstName", "lastName", "timeZone"]) f.elements[k].value = state.me[k];
  $("#profile-version").textContent = `version ${state.me.version}`;

  const [tier, settings] = await Promise.all([api("GET", "/me/tier"), api("GET", "/me/settings")]);
  if (tier.ok) {
    const t = tier.body;
    $("#tier").innerHTML = "";
    const add = (k, v) => { const a = document.createElement("span"); a.className = "k"; a.textContent = k;
      const b = document.createElement("span"); b.textContent = v; $("#tier").append(a, b); };
    add("Tier", `${t.name} (${t.code})`);
    add("Storage", `${(t.storageQuotaBytes / 1073741824).toFixed(1)} GiB`);
    add("Max courses", t.maxCourses ?? "unlimited");
  }
  if (settings.ok) {
    state.settings = settings.body;
    const s = settings.body, sf = $("#f-settings");
    sf.elements.flashNewPerDay.value = s.flashNewPerDay;
    sf.elements.flashReviewLimit.value = s.flashReviewLimit;
    sf.elements.flashRetention.value = s.flashRetention;
    sf.elements.dailyHours.value = s.extra.dailyHours ?? 4;
    sf.elements.mockGoEasy.checked = s.mockGoEasy;
    sf.elements.mockRealLayout.checked = s.mockRealLayout;
    $("#settings-version").textContent = `version ${s.version}`;
  }
  await loadInvites();
  if (state.me.role === "Admin") await Promise.all([loadUsers(""), loadTiers()]);
}

async function loadInvites() {
  const r = await api("GET", "/invites");
  const t = $("#invite-list");
  header(t, ["Created", "For", "Uses", "Expires", ""]);
  if (!r.ok) return;
  if (!r.body.items.length) { const tr = document.createElement("tr"); cell(tr, "No invites yet.").colSpan = 5; t.append(tr); }
  for (const i of r.body.items) {
    const tr = document.createElement("tr");
    cell(tr, when(i.createdAt)); cell(tr, i.email || "anyone"); cell(tr, `${i.uses}/${i.maxUses}`);
    cell(tr, i.revokedAt ? "revoked" : when(i.expiresAt));
    cell(tr, i.revokedAt ? "" : button("Revoke", "danger", async () => { await api("DELETE", `/invites/${i.id}`); loadInvites(); }));
    t.append(tr);
  }
}

async function loadUsers(q) {
  const r = await api("GET", "/admin/users" + (q ? `?q=${encodeURIComponent(q)}` : ""));
  const t = $("#user-list");
  header(t, ["User", "Email", "Tier", "Role", "Status"]);
  if (!r.ok) return;
  const tiers = await api("GET", "/admin/tiers");
  const codes = tiers.ok ? tiers.body.map((x) => x.code) : [];
  for (const u of r.body.items) {
    const tr = document.createElement("tr");
    cell(tr, `@${u.username}`); cell(tr, u.email + (u.emailVerified ? " ✓" : ""));
    const pick = (value, options, field) => {
      const s = document.createElement("select");
      s.setAttribute("aria-label", `${field} for ${u.username}`);
      options.forEach((o) => { const opt = document.createElement("option"); opt.value = opt.textContent = o; opt.selected = o === value; s.append(opt); });
      s.addEventListener("change", async () => {
        const r2 = await api("PATCH", `/admin/users/${u.id}`, { [field]: s.value });
        banner(r2.ok ? `@${u.username}: ${field} → ${s.value}` : errorText(r2), !r2.ok);
        loadUsers(q);
      });
      return s;
    };
    cell(tr, pick(u.tier, codes, "tierCode")); cell(tr, pick(u.role, ["User", "Admin"], "role"));
    cell(tr, pick(u.status, ["Active", "Disabled"], "status"));
    t.append(tr);
  }
}

async function loadTiers() {
  const r = await api("GET", "/admin/tiers");
  const t = $("#tier-list");
  header(t, ["Code", "Name", "Storage", "Courses", "Default"]);
  if (!r.ok) return;
  for (const x of r.body) {
    const tr = document.createElement("tr");
    cell(tr, x.code); cell(tr, x.name); cell(tr, `${(x.storageQuotaBytes / 1073741824).toFixed(1)} GiB`);
    cell(tr, x.maxCourses ?? "∞"); cell(tr, x.isDefault ? "✓" : "");
    t.append(tr);
  }
}

function showCreatedInvite(target, body) {
  target.innerHTML = "";
  const div = document.createElement("div");
  div.className = "created";
  div.innerHTML = `Invite code (shown once): <code></code><br>Signup link: <a target="_blank" rel="noopener"></a>`;
  div.querySelector("code").textContent = body.code;
  const a = div.querySelector("a");
  a.textContent = a.href = body.signupUrl;
  target.append(div);
}

// ---------- forms ----------
function onSubmit(id, handler) {
  $(id).addEventListener("submit", async (e) => {
    e.preventDefault();
    const btn = e.submitter; if (btn) btn.disabled = true;
    try { await handler(e.target); } finally { if (btn) btn.disabled = false; }
  });
}

onSubmit("#f-signup", async (f) => {
  const r = await raw("POST", "/auth/signup", formData(f), { "idempotency-key": ulidish() });
  if (!r.ok) return banner(errorText(r), true);
  state.token = r.body.accessToken; state.me = r.body.user;
  history.replaceState(null, "", "/");
  banner(`Welcome, ${state.me.firstName}. A verification link is in Mailpit (localhost:8025).`);
  render();
});

onSubmit("#f-login", async (f) => {
  const r = await raw("POST", "/auth/login", formData(f));
  if (!r.ok) return banner(errorText(r), true);
  state.token = r.body.accessToken; state.me = r.body.user;
  banner("");
  render();
});

onSubmit("#f-forgot", async (f) => {
  const r = await raw("POST", "/auth/password/forgot", formData(f));
  banner(r.ok ? "If that address has an account, a reset link is on its way (Mailpit)." : errorText(r), !r.ok);
});

onSubmit("#f-reset", async (f) => {
  const token = new URLSearchParams(location.search).get("token");
  const r = await raw("POST", "/auth/password/reset", { token, password: f.elements.password.value });
  if (!r.ok) return banner(errorText(r), true);
  history.replaceState(null, "", "/");
  show("#reset", false);
  banner("Password changed. Every session was signed out; log in with the new password.");
  render();
});

onSubmit("#f-profile", async (f) => {
  const r = await api("PATCH", "/me", formData(f), { "if-match": String(state.me.version) });
  if (r.status === 409) {
    state.me = r.body.error.details.current;
    banner("Changed somewhere else: reloaded the current version. Save again to overwrite.", true);
    return render();
  }
  if (!r.ok) return banner(errorText(r), true);
  state.me = r.body; banner("Profile saved."); render();
});

onSubmit("#f-settings", async (f) => {
  const v = f.elements;
  const body = {
    flashNewPerDay: Number(v.flashNewPerDay.value), flashReviewLimit: Number(v.flashReviewLimit.value),
    flashRetention: Number(v.flashRetention.value), mockGoEasy: v.mockGoEasy.checked, mockRealLayout: v.mockRealLayout.checked,
    extra: { ...(state.settings.extra || {}), dailyHours: Number(v.dailyHours.value) },
  };
  const r = await api("PATCH", "/me/settings", body, { "if-match": String(state.settings.version) });
  banner(r.ok ? "Settings saved." : errorText(r), !r.ok);
  if (r.ok || r.status === 409) render();
});

onSubmit("#f-invite", async (f) => {
  const d = formData(f);
  const body = { maxUses: Number(d.maxUses), expiresInDays: Number(d.expiresInDays) };
  if (d.email) body.email = d.email;
  const r = await api("POST", "/invites", body, { "idempotency-key": ulidish() });
  if (!r.ok) return banner(errorText(r), true);
  showCreatedInvite($("#invite-created"), r.body);
  loadInvites();
});

onSubmit("#f-user-search", (f) => loadUsers(f.elements.q.value.trim()));

onSubmit("#f-tier", async (f) => {
  const d = formData(f);
  const body = { code: d.code, name: d.name, storageQuotaBytes: Math.round(Number(d.gib) * 1073741824) };
  if (d.maxCourses !== "") body.maxCourses = Number(d.maxCourses);
  const r = await api("POST", "/admin/tiers", body);
  banner(r.ok ? `Tier ${d.code} added.` : errorText(r), !r.ok);
  if (r.ok) { f.reset(); loadTiers(); loadUsers(""); }
});

onSubmit("#f-admin-invite", async (f) => {
  const d = formData(f);
  const body = { maxUses: Number(d.maxUses), expiresInDays: Number(d.expiresInDays) };
  if (d.tierCode) body.tierCode = d.tierCode;
  const r = await api("POST", "/admin/invites", body, { "idempotency-key": ulidish() });
  if (!r.ok) return banner(errorText(r), true);
  showCreatedInvite($("#admin-invite-created"), r.body);
});

$("#clear-log").addEventListener("click", () => { $("#log").innerHTML = ""; });

// ---------- start: links from emails, then a silent refresh ----------
(async function start() {
  $("#f-signup").elements.timeZone.value = Intl.DateTimeFormat().resolvedOptions().timeZone || "Europe/Amsterdam";
  const params = new URLSearchParams(location.search);
  if (location.pathname.startsWith("/signup") && params.get("invite")) $("#f-signup").elements.inviteCode.value = params.get("invite");

  if (location.pathname.startsWith("/verify") && params.get("token")) {
    const r = await raw("POST", "/auth/verify-email/confirm", { token: params.get("token") });
    history.replaceState(null, "", "/");
    banner(r.ok ? `Email ${r.body.email} verified. You can create invites now.` : errorText(r), !r.ok);
  }
  if (location.pathname.startsWith("/reset") && params.get("token")) show("#reset", true);

  await refresh();
  render();
})();
