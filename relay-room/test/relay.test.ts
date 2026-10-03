import { afterEach, expect, it } from "vitest";
import { Miniflare } from "miniflare";
import { createHash } from "node:crypto";
import { guardRpc } from "../src/guard";

const room = "6159022b-e83d-4dcf-b80c-cdb788b0008c";
const conversation = "dotcliffe:01a0f942-ad0f-7640-bd9b-e12d096430eb";
const origin = "https://room.example.test";
const macToken = "test-mac-token-abcdefghijklmnopqrstuvwxyz-0123456789";
const digest = (s: string) => createHash("sha256").update(s).digest("hex");
const verifier = "a".repeat(43);
const challenge = createHash("sha256").update(verifier).digest("base64url");
const running: Miniflare[] = [];

function testRelay() {
  const mf = new Miniflare({
    modules: true,
    scriptPath: "dist/index.js",
    compatibilityDate: "2026-09-01",
    durableObjects: { BROKER: { className: "Broker", useSQLite: true } },
    bindings: { MAC_TOKEN_SHA256: digest(macToken), ROOM_ID: room, CONVERSATION_REF: conversation },
  });
  running.push(mf);
  const fetch = (path: string, init: RequestInit = {}) => mf.dispatchFetch(origin + path, init as Parameters<typeof mf.dispatchFetch>[1]);
  const mac = (path: string, init: RequestInit = {}) => fetch(path, { ...init, headers: { authorization: "Bearer " + macToken, ...(init.headers as Record<string, string> ?? {}) } });
  return { fetch, mac };
}
afterEach(async () => { await Promise.all(running.splice(0).map(x => x.dispose())); });

it("rejects cross-room and cross-conversation requests before forwarding", () => {
  const rpc = (name: string, args: object) => ({ jsonrpc: "2.0", id: 1, method: "tools/call", params: { name, arguments: args } });
  expect(guardRpc(rpc("ocv.list_agents", { room_id: "other" }), room, conversation)).toHaveProperty("error", "Forbidden room");
  expect(guardRpc(rpc("ocv.send_message", { agent_id: "other/peer", client_message_id: "x", text: "hello" }), room, conversation)).toHaveProperty("error", "Forbidden agent");
  expect(guardRpc(rpc("ocv.read_replies", { conversation_ref: "other" }), room, conversation)).toHaveProperty("error", "Forbidden conversation");
  expect(guardRpc({ jsonrpc: "2.0", id: 1, method: "events/subscribe", params: { name: "ocv.reply.created", arguments: { conversation_ref: "other" }, delivery: { mode: "webhook", url: "https://example.org" } } }, room, conversation)).toHaveProperty("error", "Forbidden conversation");
});

it("allows session discovery and routing only with explicit all-session owner scope", () => {
  const rpc = (name: string, args: object) => ({ jsonrpc: "2.0", id: 1, method: "tools/call", params: { name, arguments: args } });
  expect(guardRpc(rpc("ocv.list_agents", {}), "*", conversation)).toHaveProperty("request.params.arguments", {});
  expect(guardRpc(rpc("ocv.list_agents", { room_id: room }), "*", conversation)).toHaveProperty("request.params.arguments.room_id", room);
  expect(guardRpc(rpc("ocv.list_agents", { room_id: "../any" }), "*", conversation)).toHaveProperty("error", "Forbidden room");
  const send = { agent_id: "session/" + room, client_message_id: "one", text: "hello" };
  expect(guardRpc(rpc("ocv.send_message", send), "*", conversation)).toHaveProperty("request.params.arguments.conversation_ref", conversation);
  expect(guardRpc(rpc("ocv.send_message", send), room, conversation)).toHaveProperty("error", "Forbidden agent");
  expect(guardRpc(rpc("ocv.send_message", { ...send, conversation_ref: "other" }), "*", conversation)).toHaveProperty("error", "Forbidden conversation");
  expect(guardRpc(rpc("ocv.send_message", { ...send, agent_id: "session/../../system" }), "*", conversation)).toHaveProperty("error", "Forbidden agent");
});

it("requires Mac-only consent, exact PKCE and audience, rotates refresh and burns replay", async () => {
  const { fetch, mac } = testRelay();
  expect((await fetch("/.well-known/openid-configuration")).status).toBe(404);
  const registration = await fetch("/register", { method: "POST", body: JSON.stringify({ client_name: "BidBot", redirect_uris: ["https://client.example/callback"] }) });
  if (registration.status !== 201) throw new Error(await registration.text());
  expect(registration.status).toBe(201);
  const { client_id } = await registration.json() as { client_id: string };
  const q = new URLSearchParams({ client_id, redirect_uri: "https://client.example/callback", response_type: "code", code_challenge: challenge, code_challenge_method: "S256", resource: origin + "/mcp", state: "test" });
  const approval = await fetch("/authorize?" + q);
  expect(approval.status).toBe(200);
  expect(approval.headers.get("content-security-policy")).toContain("form-action 'self' https://client.example;");
  expect(approval.headers.get("content-security-policy")).not.toContain("form-action *");
  expect((await fetch("/bridge/pending")).status).toBe(401);
  expect((await fetch("/bridge/pending", { headers: { authorization: "Bearer " + "x".repeat(48) } })).status).toBe(401);
  const { pending } = await (await mac("/bridge/pending")).json() as { pending: { id: string; code: string }[] };
  expect(pending).toHaveLength(1);
  expect(pending[0].code).toMatch(/^[0-9]{8}$/);
  for (let i = 0; i < 6; i++) {
    const reload = await fetch("/authorize?" + q);
    expect(reload.status).toBe(200);
    expect(await reload.text()).toContain(pending[0].id.slice(-8).toUpperCase());
  }
  const reloaded = await (await mac("/bridge/pending")).json();
  expect(reloaded).toEqual({ pending });
  const wrongCode = pending[0].code === "00000000" ? "11111111" : "00000000";
  const wrong = await fetch("/consent", { method: "POST", body: new URLSearchParams({ id: pending[0].id, code: wrongCode }) });
  expect(wrong.status).toBe(403);
  const incorrectPage = await wrong.text();
  expect(wrong.headers.get("content-security-policy")).toContain("form-action 'self' https://client.example;");
  expect(incorrectPage).toContain(pending[0].id.slice(-8).toUpperCase());
  expect(incorrectPage).toContain('<form action="/consent"');
  expect((await fetch("/authorize?" + q)).status).toBe(200);
  const consent = await fetch("/consent", { method: "POST", body: new URLSearchParams({ id: pending[0].id, code: pending[0].code }), redirect: "manual" });
  expect(consent.status).toBe(302);
  const code = new URL(consent.headers.get("location")!).searchParams.get("code")!;
  const base = { grant_type: "authorization_code", code, client_id, redirect_uri: "https://client.example/callback", resource: origin + "/mcp" };
  expect((await fetch("/token", { method: "POST", body: new URLSearchParams({ ...base, code_verifier: "b".repeat(43) }) })).status).toBe(400);
  expect((await fetch("/token", { method: "POST", body: new URLSearchParams({ ...base, code_verifier: verifier, resource: "https://wrong.example/mcp" }) })).status).toBe(400);
  const tokenResponse = await fetch("/token", { method: "POST", body: new URLSearchParams({ ...base, code_verifier: verifier }) });
  expect(tokenResponse.status).toBe(200);
  const tokens = await tokenResponse.json() as { access_token: string; refresh_token: string };
  expect((await fetch("/token", { method: "POST", body: new URLSearchParams({ ...base, code_verifier: verifier }) })).status).toBe(400);
  expect((await fetch("/mcp", { method: "POST", body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "ping" }) })).status).toBe(401);
  const refresh = { grant_type: "refresh_token", refresh_token: tokens.refresh_token, client_id, resource: origin + "/mcp" };
  const rotated = await fetch("/token", { method: "POST", body: new URLSearchParams(refresh) });
  expect(rotated.status).toBe(200);
  expect((await fetch("/token", { method: "POST", body: new URLSearchParams(refresh) })).status).toBe(400);
  expect((await fetch("/mcp", { method: "POST", headers: { authorization: "Bearer " + tokens.access_token }, body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "ping" }) })).status).toBe(401);
});

it("keeps separate OAuth attempts distinct and preserves lockout across reloads", async () => {
  const { fetch, mac } = testRelay();
  const registration = await fetch("/register", { method: "POST", body: JSON.stringify({ client_name: "BidBot", redirect_uris: ["https://client.example/callback"] }) });
  const { client_id } = await registration.json() as { client_id: string };
  const q = new URLSearchParams({ client_id, redirect_uri: "https://client.example/callback", response_type: "code", code_challenge: challenge, code_challenge_method: "S256", resource: origin + "/mcp", state: "first" });
  await fetch("/authorize?" + q);
  const { pending } = await (await mac("/bridge/pending")).json() as { pending: { id: string; code: string }[] };
  const wrongCode = pending[0].code === "00000000" ? "11111111" : "00000000";
  for (let i = 0; i < 5; i++) expect((await fetch("/consent", { method: "POST", body: new URLSearchParams({ id: pending[0].id, code: wrongCode }) })).status).toBe(403);
  expect((await fetch("/authorize?" + q)).status).toBe(429);
  expect((await fetch("/consent", { method: "POST", body: new URLSearchParams({ id: pending[0].id, code: pending[0].code }) })).status).toBe(429);
  q.set("state", "second");
  expect((await fetch("/authorize?" + q)).status).toBe(200);
  const remaining = await (await mac("/bridge/pending")).json() as { pending: { id: string }[] };
  expect(remaining.pending).toHaveLength(2);
  expect(new Set(remaining.pending.map(p => p.id)).size).toBe(2);
});

it("leases once, accepts an exact response retry, and reuses the recorded RPC result", async () => {
  const { fetch, mac } = testRelay();
  const registration = await fetch("/register", { method: "POST", body: JSON.stringify({ client_name: "BidBot", redirect_uris: ["https://client.example/callback"] }) });
  if (registration.status !== 201) throw new Error(await registration.text());
  const { client_id } = await registration.json() as { client_id: string };
  const q = new URLSearchParams({ client_id, redirect_uri: "https://client.example/callback", response_type: "code", code_challenge: challenge, code_challenge_method: "S256", resource: origin + "/mcp" });
  await fetch("/authorize?" + q);
  const { pending } = await (await mac("/bridge/pending")).json() as { pending: { id: string; code: string }[] };
  const consent = await fetch("/consent", { method: "POST", body: new URLSearchParams(pending[0]), redirect: "manual" });
  const code = new URL(consent.headers.get("location")!).searchParams.get("code")!;
  const tokenResponse = await fetch("/token", { method: "POST", body: new URLSearchParams({ grant_type: "authorization_code", client_id, code, code_verifier: verifier, redirect_uri: "https://client.example/callback", resource: origin + "/mcp" }) });
  const { access_token } = await tokenResponse.json() as { access_token: string };
  const initialize = { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25" } };
  const initPending = fetch("/mcp", { method: "POST", headers: { authorization: "Bearer " + access_token }, body: JSON.stringify(initialize) });
  let initNext: { request: { key: string } | null } = { request: null };
  for (let i = 0; i < 20 && !initNext.request; i++) {
    initNext = await (await mac("/bridge/next")).json() as typeof initNext;
    if (!initNext.request) await new Promise(resolve => setTimeout(resolve, 25));
  }
  expect(initNext.request).not.toBeNull();
  expect((await mac("/bridge/respond", { method: "POST", body: JSON.stringify({ key: initNext.request!.key, response: { jsonrpc: "2.0", id: 1, result: { protocolVersion: "2025-11-25" } } }) })).status).toBe(200);
  const initialized = await initPending;
  expect(initialized.status).toBe(200);
  const session = initialized.headers.get("mcp-session-id")!;
  expect(session).toMatch(/^s_[a-f0-9]{48}$/);
  const rpc = { jsonrpc: "2.0", id: "message-1", method: "tools/call", params: { name: "ocv.send_message", arguments: { agent_id: room + "/" + "b".repeat(8) + "-" + "b".repeat(4) + "-" + "b".repeat(4) + "-" + "b".repeat(4) + "-" + "b".repeat(12), text: "hello", client_message_id: "message-1" } } };
  const init = { method: "POST", headers: { authorization: "Bearer " + access_token, "mcp-session-id": session }, body: JSON.stringify(rpc) };
  expect((await fetch("/mcp", { method: "POST", headers: { authorization: "Bearer " + access_token }, body: JSON.stringify(rpc) })).status).toBe(400);
  const pendingResult = fetch("/mcp", init);
  let next: { request: { key: string; rpc: typeof rpc } | null } = { request: null };
  for (let i = 0; i < 20 && !next.request; i++) {
    next = await (await mac("/bridge/next")).json() as typeof next;
    if (!next.request) await new Promise(resolve => setTimeout(resolve, 25));
  }
  expect(next.request).not.toBeNull();
  expect(next.request!.rpc.params.arguments).toMatchObject({ conversation_ref: conversation, mode: "queue" });
  expect((await (await mac("/bridge/next")).json() as { request: unknown }).request).toBeNull();
  const response = { jsonrpc: "2.0", id: "message-1", result: { content: [{ type: "text", text: "ok" }] } };
  const reply = () => mac("/bridge/respond", { method: "POST", body: JSON.stringify({ key: next.request!.key, response }) });
  expect(await (await reply()).json()).toMatchObject({ ok: true, duplicate: false });
  expect(await (await reply()).json()).toMatchObject({ ok: true, duplicate: true });
  expect((await mac("/bridge/respond", { method: "POST", body: JSON.stringify({ key: next.request!.key, response: { ...response, result: {} } }) })).status).toBe(409);
  expect((await pendingResult).status).toBe(200);
  expect(await (await fetch("/mcp", init)).json()).toEqual(response);
  const reconnect = fetch("/mcp", { method: "POST", headers: { authorization: "Bearer " + access_token }, body: JSON.stringify(initialize) });
  let reconnectNext: { request: { key: string } | null } = { request: null };
  for (let i = 0; i < 20 && !reconnectNext.request; i++) {
    reconnectNext = await (await mac("/bridge/next")).json() as typeof reconnectNext;
    if (!reconnectNext.request) await new Promise(resolve => setTimeout(resolve, 25));
  }
  expect(reconnectNext.request).not.toBeNull();
  await mac("/bridge/respond", { method: "POST", body: JSON.stringify({ key: reconnectNext.request!.key, response: { jsonrpc: "2.0", id: 1, result: { protocolVersion: "2025-11-25" } } }) });
  const secondSession = (await reconnect).headers.get("mcp-session-id")!;
  expect(secondSession).not.toBe(session);
  const changed = { ...rpc, params: { ...rpc.params, arguments: { ...rpc.params.arguments, text: "a new message", client_message_id: "message-2" } } };
  const newPending = fetch("/mcp", { method: "POST", headers: { authorization: "Bearer " + access_token, "mcp-session-id": secondSession }, body: JSON.stringify(changed) });
  let newNext: { request: { key: string } | null } = { request: null };
  for (let i = 0; i < 20 && !newNext.request; i++) {
    newNext = await (await mac("/bridge/next")).json() as typeof newNext;
    if (!newNext.request) await new Promise(resolve => setTimeout(resolve, 25));
  }
  expect(newNext.request?.key).not.toBe(next.request!.key);
  await mac("/bridge/respond", { method: "POST", body: JSON.stringify({ key: newNext.request!.key, response }) });
  expect((await newPending).status).toBe(200);
});
