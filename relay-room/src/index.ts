import { Env, bearer, equal, error, htmlEscape, json, limited, now, object, page, random, redirectOk, resource, sha256 } from "./common";
import { guardRpc } from "./guard";

type Client = { id: string; name: string; redirects: string; created: number };
type Pending = { id: string; client: string; redirect: string; challenge: string; state: string; scope: string; resource: string; expires: number; attempts: number; approved: number; status: string; code_hash: string };
type Grant = { hash: string; family: string; client: string; scope: string; resource: string; expires: number; consumed: number; kind: string; redirect: string; challenge: string };
type Work = { key: string; owner: string; payload_hash: string; rpc: string; state: string; result: string | null; result_hash: string | null; created: number; updated: number };
const SCOPES = ["room.read", "room.send", "room.events"];
const ACCESS_AGE = 3600, REFRESH_AGE = 30 * 86400, CODE_AGE = 120, CONSENT_AGE = 600;
const jsonRpcError = (id: unknown, message: string, data?: unknown) => ({ jsonrpc: "2.0", id, error: { code: -32001, message, ...(data ? { data } : {}) } });
const validScope = (s: string) => s.length <= 80 && s.split(" ").every(v => SCOPES.includes(v)) && new Set(s.split(" ")).size === s.split(" ").length;

function oauthError(status: number, code: string, description: string): Response {
  return error(status, code, description);
}
function form(raw: string): URLSearchParams { return new URLSearchParams(raw); }
function redirectWith(uri: string, values: Record<string, string>): Response {
  const u = new URL(uri);
  for (const [k, v] of Object.entries(values)) if (v) u.searchParams.set(k, v);
  return new Response(null, { status: 302, headers: { location: u.toString(), "cache-control": "no-store", "referrer-policy": "no-referrer" } });
}

export class Broker implements DurableObject {
  private sql: SqlStorage;
  constructor(private ctx: DurableObjectState, private env: Env) {
    this.sql = ctx.storage.sql;
    this.sql.exec(`CREATE TABLE IF NOT EXISTS clients(id TEXT PRIMARY KEY,name TEXT NOT NULL,redirects TEXT NOT NULL,created INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS pending(id TEXT PRIMARY KEY,client TEXT NOT NULL,redirect TEXT NOT NULL,challenge TEXT NOT NULL,state TEXT NOT NULL,scope TEXT NOT NULL,resource TEXT NOT NULL,expires INTEGER NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,approved INTEGER NOT NULL DEFAULT 0,status TEXT NOT NULL,code_hash TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS grants(hash TEXT PRIMARY KEY,family TEXT NOT NULL,client TEXT NOT NULL,scope TEXT NOT NULL,resource TEXT NOT NULL,expires INTEGER NOT NULL,consumed INTEGER NOT NULL DEFAULT 0,kind TEXT NOT NULL,redirect TEXT NOT NULL,challenge TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS grants_family ON grants(family);
      CREATE TABLE IF NOT EXISTS families(id TEXT PRIMARY KEY,revoked INTEGER NOT NULL DEFAULT 0);
      CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY,family TEXT NOT NULL,expires INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS work(key TEXT PRIMARY KEY,owner TEXT NOT NULL,payload_hash TEXT NOT NULL,rpc TEXT NOT NULL,state TEXT NOT NULL,result TEXT,result_hash TEXT,created INTEGER NOT NULL,updated INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS work_queue ON work(state,created);`);
  }

  private first<T>(query: string, ...params: (string | number)[]): T | null {
    return (this.sql.exec(query, ...params).toArray()[0] as T | undefined) ?? null;
  }
  private configured(): boolean {
    return /^[0-9a-f]{64}$/.test(this.env.MAC_TOKEN_SHA256 ?? "") && (this.env.ROOM_ID === "*" || /^[0-9a-fA-F-]{36}$/.test(this.env.ROOM_ID ?? "")) && !!this.env.CONVERSATION_REF;
  }
  private async mac(req: Request): Promise<boolean> {
    const token = bearer(req);
    return !!token && equal(await sha256(token), this.env.MAC_TOKEN_SHA256);
  }
  private async consentCode(id: string): Promise<string> {
    const key = await crypto.subtle.importKey("raw", new TextEncoder().encode(this.env.MAC_TOKEN_SHA256), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
    const bytes = new Uint8Array(await crypto.subtle.sign("HMAC", key, new TextEncoder().encode("consent:" + id)));
    const n = ((bytes[0] * 2 ** 24 + bytes[1] * 2 ** 16 + bytes[2] * 2 ** 8 + bytes[3]) >>> 0) % 100_000_000;
    return n.toString().padStart(8, "0");
  }
  private async parseBody(req: Request, max: number): Promise<string | Response> {
    const raw = await limited(req, max);
    return raw === null ? error(413, "too_large", "Request body exceeds limit") : raw;
  }
  private requiredResource(req: Request): string { return req.headers.get("x-ocv-resource") ?? ""; }

  async fetch(req: Request): Promise<Response> {
    if (!this.configured()) return error(503, "misconfigured", "Owner relay configuration is incomplete");
    const u = new URL(req.url), p = u.pathname;
    if (p.startsWith("/bridge/")) {
      if (!(await this.mac(req))) return error(401, "unauthorized", "Mac bearer required");
      if (p === "/bridge/pending" && req.method === "GET") return this.pending();
      if (p === "/bridge/approve" && req.method === "POST") return this.approve(req);
      if (p === "/bridge/next" && req.method === "GET") return this.next();
      if (p === "/bridge/leased" && req.method === "GET") return this.leased();
      if (p === "/bridge/respond" && req.method === "POST") return this.respond(req);
      return error(404, "not_found", "Unknown bridge route");
    }
    if (p === "/register" && req.method === "POST") return this.register(req);
    if (p === "/authorize" && req.method === "GET") return this.authorize(u, req);
    if (p === "/consent" && req.method === "POST") return this.consent(req);
    if (p === "/token" && req.method === "POST") return this.token(req);
    if (p === "/mcp" && req.method === "POST") return this.mcp(req);
    return error(404, "not_found", "Unknown route");
  }

  private async register(req: Request): Promise<Response> {
    const raw = await this.parseBody(req, 8192);
    if (raw instanceof Response) return raw;
    let input: unknown;
    try { input = JSON.parse(raw); } catch { return oauthError(400, "invalid_client_metadata", "Invalid JSON"); }
    if (!object(input) || typeof input.client_name !== "string" || input.client_name.length < 1 || input.client_name.length > 120 || /[\u0000-\u001f\u007f-\u009f]/.test(input.client_name) || !Array.isArray(input.redirect_uris) || input.redirect_uris.length < 1 || input.redirect_uris.length > 5 || !input.redirect_uris.every(x => typeof x === "string" && redirectOk(x)) || new Set(input.redirect_uris).size !== input.redirect_uris.length || (input.token_endpoint_auth_method !== undefined && input.token_endpoint_auth_method !== "none") || (input.grant_types !== undefined && (!Array.isArray(input.grant_types) || input.grant_types.some(x => !["authorization_code", "refresh_token"].includes(x)))) || (input.response_types !== undefined && (!Array.isArray(input.response_types) || input.response_types.some(x => x !== "code")))) return oauthError(400, "invalid_client_metadata", "Public authorization-code client with exact HTTPS redirect URIs required");
    const count = this.first<{ n: number }>("SELECT COUNT(*) n FROM clients")?.n ?? 0;
    if (count >= 32) return oauthError(429, "temporarily_unavailable", "Client registration limit reached");
    const id = "ocv_" + random(18);
    this.sql.exec("INSERT INTO clients VALUES (?,?,?,?)", id, input.client_name, JSON.stringify(input.redirect_uris), now());
    return json(201, { client_id: id, client_name: input.client_name, redirect_uris: input.redirect_uris, token_endpoint_auth_method: "none", grant_types: ["authorization_code", "refresh_token"], response_types: ["code"], client_id_issued_at: now() });
  }

  private async authorize(u: URL, req: Request): Promise<Response> {
    const q = u.searchParams, client = this.first<Client>("SELECT * FROM clients WHERE id=?", q.get("client_id") ?? "");
    const redirect = q.get("redirect_uri") ?? "";
    if (!client || !(JSON.parse(client.redirects) as string[]).includes(redirect)) return page("Connection refused", "<h1>Unknown client or redirect address</h1>", 400);
    const state = q.get("state") ?? "", challenge = q.get("code_challenge") ?? "", scope = q.get("scope") ?? SCOPES.join(" ");
    if (q.get("response_type") !== "code" || q.get("code_challenge_method") !== "S256" || !/^[A-Za-z0-9_-]{43}$/.test(challenge) || q.get("resource") !== this.requiredResource(req) || !validScope(scope) || state.length > 1024) return redirectWith(redirect, { error: "invalid_request", state });
    const id = "r_" + random(18), code = await this.consentCode(id), codeHash = await sha256(code);
    const pending = this.ctx.storage.transactionSync(() => {
      // Reloading this exact OAuth attempt must preserve its code and expiry.
      const existing = this.first<Pending>("SELECT * FROM pending WHERE client=? AND redirect=? AND challenge=? AND state=? AND scope=? AND resource=? AND status='pending' AND expires>? ORDER BY expires DESC,id DESC LIMIT 1", client.id, redirect, challenge, state, scope, this.requiredResource(req), now());
      if (existing) return existing;
      if ((this.first<{ n: number }>("SELECT COUNT(*) n FROM pending WHERE status='pending' AND expires>?", now())?.n ?? 0) >= 20 || (this.first<{ n: number }>("SELECT COUNT(*) n FROM pending WHERE client=? AND status='pending' AND expires>?", client.id, now())?.n ?? 0) >= 3) return null;
      this.sql.exec("INSERT INTO pending VALUES (?,?,?,?,?,?,?,?,0,0,'pending',?)", id, client.id, redirect, challenge, state, scope, this.requiredResource(req), now() + CONSENT_AGE, codeHash);
      return this.first<Pending>("SELECT * FROM pending WHERE id=?", id);
    });
    if (!pending) return page("Approvals busy", "<h1>Too many pending approvals</h1>", 429);
    if (pending.attempts >= 5) return page("Request locked", "<h1>Too many attempts</h1><p>Start a new connection from ChatGPT.</p>", 429);
    return this.approvalPage(client.name, pending);
  }

  private approvalPage(clientName: string, pending: Pending, incorrect = false): Response {
    const reference = pending.id.slice(-8).toUpperCase();
    return page("Approve room access", `<h1>${htmlEscape(clientName)} requests room access</h1><p>Check this request in OpenCovibe on your Mac. Use the code for request <strong>${reference}</strong>.</p><p><small>Access: ${htmlEscape(pending.scope)}<br>Redirect: ${htmlEscape(pending.redirect)}<br>Expires in ${Math.max(1, Math.ceil((pending.expires - now()) / 60))} minutes</small></p>${incorrect ? '<p role="alert">The code is incorrect. Check the request number above before trying again.</p>' : ''}<form action="/consent" method="post"><input type="hidden" name="id" value="${pending.id}"><label>Code from your Mac<input name="code" inputmode="numeric" pattern="[0-9]{8}" maxlength="8" required></label><button>Approve connection</button></form>`, incorrect ? 403 : 200, pending.redirect);
  }

  private async pending(): Promise<Response> {
    const rows = this.sql.exec("SELECT p.*,c.name client_name FROM pending p JOIN clients c ON c.id=p.client WHERE p.status='pending' AND p.expires>? ORDER BY p.expires LIMIT 20", now()).toArray() as (Pending & { client_name: string })[];
    return json(200, { pending: await Promise.all(rows.map(async p => ({ id: p.id, request_reference: p.id.slice(-8).toUpperCase(), client_name: p.client_name, redirect_uri: p.redirect, resource: p.resource, scope: p.scope, expires_at: p.expires, owner_approved: !!p.approved, code: await this.consentCode(p.id) }))) });
  }
  private async approve(req: Request): Promise<Response> {
    const raw = await this.parseBody(req, 1024);
    if (raw instanceof Response) return raw;
    let data: unknown; try { data = JSON.parse(raw); } catch { return error(400, "invalid_request", "Invalid JSON"); }
    const id = object(data) && typeof data.id === "string" ? data.id : "";
    const p = this.first<Pending>("SELECT * FROM pending WHERE id=?", id);
    if (!p || p.expires <= now() || p.status !== "pending") return error(404, "not_found", "Pending request unavailable");
    // This alternative is for a Mac UI that obtains an explicit owner decision.
    // The Mac must never call it just because a pending request exists.
    const redirect = await this.finishConsent(p);
    return json(200, { id, approved: true, redirect: redirect.headers.get("location") });
  }
  private async finishConsent(p: Pending): Promise<Response> {
    const authCode = "ocva_" + random(32), family = "f_" + random(18);
    const hash = await sha256(authCode);
    const inserted = this.ctx.storage.transactionSync(() => {
      this.sql.exec("UPDATE pending SET status='used' WHERE id=? AND status='pending' AND expires>?", p.id, now());
      if ((this.first<{ n: number }>("SELECT changes() n")?.n ?? 0) !== 1) return false;
      this.sql.exec("INSERT INTO families VALUES (?,0)", family);
      this.sql.exec("INSERT INTO grants VALUES (?,?,?,?,?,?,0,'code',?,?)", hash, family, p.client, p.scope, p.resource, now() + CODE_AGE, p.redirect, p.challenge);
      return true;
    });
    if (!inserted) return page("Expired request", "<h1>This request has expired</h1>", 410);
    return redirectWith(p.redirect, { code: authCode, state: p.state });
  }
  private async consent(req: Request): Promise<Response> {
    const raw = await this.parseBody(req, 2048);
    if (raw instanceof Response) return raw;
    const f = form(raw), id = f.get("id") ?? "", code = f.get("code") ?? "";
    const p = this.first<Pending>("SELECT * FROM pending WHERE id=?", id);
    if (!p || p.status !== "pending" || p.expires <= now()) return page("Expired request", "<h1>This request has expired</h1>", 410);
    if (p.attempts >= 5) return page("Request locked", "<h1>Too many attempts</h1>", 429);
    this.sql.exec("UPDATE pending SET attempts=attempts+1 WHERE id=? AND attempts<5 AND status='pending' AND expires>?", id, now());
    if ((this.first<{ n: number }>("SELECT changes() n")?.n ?? 0) !== 1) return page("Request locked", "<h1>Too many attempts</h1>", 429);
    if (!/^[0-9]{8}$/.test(code) || !equal(await sha256(code), p.code_hash)) return this.approvalPage(this.first<Client>("SELECT * FROM clients WHERE id=?", p.client)!.name, p, true);
    return this.finishConsent(p);
  }

  private async token(req: Request): Promise<Response> {
    const raw = await this.parseBody(req, 4096);
    if (raw instanceof Response) return raw;
    const f = form(raw), type = f.get("grant_type"), client = f.get("client_id") ?? "", secret = type === "authorization_code" ? f.get("code") : f.get("refresh_token");
    if (!this.first<Client>("SELECT * FROM clients WHERE id=?", client)) return oauthError(400, "invalid_client", "Unknown client");
    if ((type !== "authorization_code" && type !== "refresh_token") || !secret || secret.length > 256) return oauthError(400, "invalid_grant", "Unsupported or missing grant");
    const hash = await sha256(secret), g = this.first<Grant>("SELECT * FROM grants WHERE hash=?", hash);
    if (!g || g.client !== client || g.kind !== (type === "authorization_code" ? "code" : "refresh")) return oauthError(400, "invalid_grant", "Grant invalid or expired");
    if (g.kind === "refresh" && g.consumed) {
      this.sql.exec("UPDATE families SET revoked=1 WHERE id=?", g.family);
      return oauthError(400, "invalid_grant", "Refresh replay revoked this grant family");
    }
    if (g.consumed || g.expires <= now() || this.first<{ revoked: number }>("SELECT revoked FROM families WHERE id=?", g.family)?.revoked) return oauthError(400, "invalid_grant", "Grant used or revoked");
    if (f.get("resource") !== g.resource || g.resource !== this.requiredResource(req)) return oauthError(400, "invalid_target", "Resource audience mismatch");
    if (g.kind === "code") {
      const verifier = f.get("code_verifier") ?? "";
      if (f.get("redirect_uri") !== g.redirect || !/^[A-Za-z0-9._~-]{43,128}$/.test(verifier)) return oauthError(400, "invalid_grant", "Redirect or PKCE verifier invalid");
      const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
      const challenge = btoa(String.fromCharCode(...new Uint8Array(digest))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
      if (!equal(challenge, g.challenge)) return oauthError(400, "invalid_grant", "PKCE verification failed");
    }
    const access = "ocvt_" + random(32), refresh = "ocvr_" + random(32);
    const accessHash = await sha256(access), refreshHash = await sha256(refresh);
    const rotated = this.ctx.storage.transactionSync(() => {
      this.sql.exec("UPDATE grants SET consumed=1 WHERE hash=? AND consumed=0", hash);
      if ((this.first<{ n: number }>("SELECT changes() n")?.n ?? 0) !== 1) return false;
      this.sql.exec("INSERT INTO grants VALUES (?,?,?,?,?,?,0,'access','','')", accessHash, g.family, client, g.scope, g.resource, now() + ACCESS_AGE);
      this.sql.exec("INSERT INTO grants VALUES (?,?,?,?,?,?,0,'refresh','','')", refreshHash, g.family, client, g.scope, g.resource, now() + REFRESH_AGE);
      return true;
    });
    if (!rotated) {
      if (g.kind === "refresh") this.sql.exec("UPDATE families SET revoked=1 WHERE id=?", g.family);
      return oauthError(400, "invalid_grant", "Grant already used");
    }
    return json(200, { access_token: access, token_type: "Bearer", expires_in: ACCESS_AGE, refresh_token: refresh, scope: g.scope });
  }

  private async access(req: Request): Promise<Grant | null> {
    const token = bearer(req);
    if (!token || !token.startsWith("ocvt_")) return null;
    const g = this.first<Grant>("SELECT * FROM grants WHERE hash=?", await sha256(token));
    return g && g.kind === "access" && g.expires > now() && !g.consumed && !this.first<{ revoked: number }>("SELECT revoked FROM families WHERE id=?", g.family)?.revoked && g.resource === this.requiredResource(req) ? g : null;
  }
  private async mcp(req: Request): Promise<Response> {
    const g = await this.access(req);
    if (!g) return error(401, "invalid_token", "OAuth bearer for this MCP resource required");
    const raw = await this.parseBody(req, 48 * 1024);
    if (raw instanceof Response) return raw;
    let input: unknown; try { input = JSON.parse(raw); } catch { return json(400, jsonRpcError(null, "Invalid JSON")); }
    const checked = guardRpc(input, this.env.ROOM_ID, this.env.CONVERSATION_REF);
    if ("error" in checked) return json(400, jsonRpcError(object(input) ? input.id ?? null : null, checked.error));
    const isInitialize = "request" in checked && checked.request.method === "initialize";
    const session = isInitialize ? "s_" + random(24) : req.headers.get("mcp-session-id") ?? "";
    if (isInitialize) this.sql.exec("INSERT INTO sessions VALUES (?,?,?)", session, g.family, now() + 86400);
    else if (!/^s_[0-9a-f]{48}$/.test(session) || !this.first<{ id: string }>("SELECT id FROM sessions WHERE id=? AND family=? AND expires>?", session, g.family, now())) return json(400, jsonRpcError(object(input) ? input.id ?? null : null, "MCP session missing or expired; initialize a new session"));
    if ("notification" in checked) return new Response(null, { status: 202 });
    const rpc = checked.request;
    const needed = rpc.method.startsWith("events/") ? "room.events" : rpc.method === "tools/call" && rpc.params?.name === "ocv.send_message" ? "room.send" : "room.read";
    if (!g.scope.split(" ").includes(needed)) return json(403, jsonRpcError(rpc.id, "Scope denied"));
    const payload = JSON.stringify(rpc), payloadHash = await sha256(payload);
    // A client may reuse JSON-RPC IDs after 24h. Within that window an exact retry
    // observes the same durable request; a changed payload is a conflict.
    const key = "q_" + (await sha256(session + ":" + String(rpc.id)));
    const reply = (status: number, body: unknown) => json(status, body, isInitialize ? { "mcp-session-id": session } : {});
    let work = this.first<Work>("SELECT * FROM work WHERE key=?", key);
    if (work && work.payload_hash !== payloadHash) return reply(409, jsonRpcError(rpc.id, "Request id reused with changed payload"));
    if (!work) {
      const count = this.first<{ n: number }>("SELECT COUNT(*) n FROM work WHERE state='queued'")?.n ?? 0;
      if (count >= 100) return reply(503, jsonRpcError(rpc.id, "Relay queue full"));
      this.sql.exec("INSERT INTO work VALUES (?,?,?,?,'queued',NULL,NULL,?,?)", key, g.family, payloadHash, payload, now(), now());
      work = this.first<Work>("SELECT * FROM work WHERE key=?", key)!;
    }
    for (let i = 0; i < 15; i++) {
      const current = this.first<Work>("SELECT * FROM work WHERE key=?", key)!;
      if (current.state === "done" && current.result) return reply(200, JSON.parse(current.result));
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    const state = this.first<Work>("SELECT * FROM work WHERE key=?", key)?.state ?? "unknown";
    return reply(504, jsonRpcError(rpc.id, "Mac response unavailable; delivery outcome may be uncertain", { request_key: key, state, retry: "Repeat the exact JSON-RPC id, payload, and MCP session to observe the existing request. It will not be resent to the Mac." }));
  }

  private next(): Response {
    const row = this.first<Work>("SELECT * FROM work WHERE state='queued' ORDER BY created,key LIMIT 1");
    if (!row) return json(200, { request: null });
    this.sql.exec("UPDATE work SET state='leased',updated=? WHERE key=? AND state='queued'", now(), row.key);
    return json(200, { request: { key: row.key, rpc: JSON.parse(row.rpc), created_at: row.created } });
  }
  private leased(): Response {
    const rows = this.sql.exec("SELECT * FROM work WHERE state='leased' ORDER BY created,key LIMIT 100").toArray() as Work[];
    return json(200, { requests: rows.map(row => ({ key: row.key, rpc: JSON.parse(row.rpc), created_at: row.created, leased_at: row.updated })) });
  }
  private async respond(req: Request): Promise<Response> {
    const raw = await this.parseBody(req, 64 * 1024);
    if (raw instanceof Response) return raw;
    let data: unknown; try { data = JSON.parse(raw); } catch { return error(400, "invalid_request", "Invalid JSON"); }
    if (!object(data) || typeof data.key !== "string" || !/^q_[a-f0-9]{64}$/.test(data.key) || !object(data.response)) return error(400, "invalid_request", "Expected key and JSON-RPC response");
    const row = this.first<Work>("SELECT * FROM work WHERE key=?", data.key);
    if (!row || row.state === "queued") return error(404, "not_found", "Leased request unavailable");
    const requested = JSON.parse(row.rpc) as { id: unknown };
    if (data.response.jsonrpc !== "2.0" || data.response.id !== requested.id || (!object(data.response.result) && !object(data.response.error)) || (data.response.result !== undefined && data.response.error !== undefined)) return error(400, "invalid_response", "JSON-RPC response does not match request");
    const response = JSON.stringify(data.response), hash = await sha256(response);
    if (row.state === "done") return row.result_hash === hash ? json(200, { ok: true, duplicate: true }) : error(409, "conflict", "Response differs from recorded response");
    this.sql.exec("UPDATE work SET state='done',result=?,result_hash=?,updated=? WHERE key=? AND state='leased'", response, hash, now(), row.key);
    if ((this.first<{ n: number }>("SELECT changes() n")?.n ?? 0) === 1) return json(200, { ok: true, duplicate: false });
    const winner = this.first<Work>("SELECT * FROM work WHERE key=?", row.key);
    return winner?.result_hash === hash ? json(200, { ok: true, duplicate: true }) : error(409, "conflict", "Response differs from recorded response");
  }
}

function metadata(origin: string) {
  return { issuer: origin, authorization_endpoint: origin + "/authorize", token_endpoint: origin + "/token", registration_endpoint: origin + "/register", response_types_supported: ["code"], grant_types_supported: ["authorization_code", "refresh_token"], token_endpoint_auth_methods_supported: ["none"], code_challenge_methods_supported: ["S256"], scopes_supported: SCOPES, resource_indicators_supported: true };
}
export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const u = new URL(req.url), path = u.pathname;
    const origin = u.origin, audience = resource(u);
    if (u.protocol !== "https:" && !["localhost", "127.0.0.1"].includes(u.hostname)) return error(400, "https_required", "HTTPS required");
    const cors = { "access-control-allow-origin": "*", "access-control-allow-methods": "GET, POST, OPTIONS", "access-control-allow-headers": "Authorization, Content-Type, MCP-Protocol-Version, MCP-Session-Id", "access-control-expose-headers": "MCP-Session-Id", "access-control-max-age": "600" };
    if (req.method === "OPTIONS" && ["/register", "/token", "/mcp", "/.well-known/oauth-authorization-server", "/.well-known/oauth-protected-resource", "/.well-known/oauth-protected-resource/mcp"].includes(path)) return new Response(null, { status: 204, headers: cors });
    if (!/^[0-9a-f]{64}$/.test(env.MAC_TOKEN_SHA256 ?? "") || !env.ROOM_ID || !env.CONVERSATION_REF) return error(503, "misconfigured", "Owner relay configuration is incomplete");
    if (path === "/" && req.method === "GET") return page("OpenCovibe private room relay", `<h1>OpenCovibe private room relay</h1><p>This private connector queues OpenCovibe room and session requests for the owner's Mac. The Mac must be online and the owner must approve each OAuth connection.</p><p><small>Agent endpoint: ${htmlEscape(audience)}<br>Room access remains controlled in OpenCovibe.</small></p>`);
    if (path === "/health" && req.method === "GET") return json(200, { service: "opencovibe-private-room-relay", configured: true });
    if (path === "/.well-known/oauth-authorization-server" && req.method === "GET") return json(200, metadata(origin), cors);
    if ((path === "/.well-known/oauth-protected-resource" || path === "/.well-known/oauth-protected-resource/mcp") && req.method === "GET") return json(200, { resource: audience, authorization_servers: [origin], scopes_supported: SCOPES, bearer_methods_supported: ["header"] }, cors);
    if (path === "/mcp" && req.method !== "POST") return error(405, "method_not_allowed", "POST only");
    if (!["/register", "/authorize", "/consent", "/token", "/mcp", "/bridge/pending", "/bridge/approve", "/bridge/next", "/bridge/leased", "/bridge/respond"].includes(path)) return error(404, "not_found", "Unknown route");
    if (path.startsWith("/bridge/") && !bearer(req)) return error(401, "unauthorized", "Mac bearer required");
    const stub = env.BROKER.get(env.BROKER.idFromName("single-owner-room"));
    const h = new Headers(req.headers);
    h.set("x-ocv-resource", audience);
    const response = await stub.fetch(new Request(req, { headers: h }));
    if (path === "/mcp" && response.status === 401) {
      const headers = new Headers(response.headers);
      headers.set("www-authenticate", `Bearer resource_metadata="${origin}/.well-known/oauth-protected-resource"`);
      for (const [k, v] of Object.entries(cors)) headers.set(k, v);
      return new Response(response.body, { status: 401, headers });
    }
    if (["/register", "/token", "/mcp"].includes(path)) {
      const headers = new Headers(response.headers);
      for (const [k, v] of Object.entries(cors)) headers.set(k, v);
      return new Response(response.body, { status: response.status, headers });
    }
    return response;
  },
} satisfies ExportedHandler<Env>;
