# Private room relay

This Worker is a single-owner HTTPS/OAuth broker for OpenCovibe's existing loopback MCP endpoint. The Mac polls outward; the Worker never connects to the Mac and never owns a room scheduler or provider writer. Its SQLite Durable Object keeps OAuth grants and request states. The relay forwards only the native bridge's allowed JSON-RPC methods, bound to the exact Dotcliffe conversation in `wrangler.toml`. `ROOM_ID="*"` explicitly enables all-room and standalone-session routing; an exact room UUID retains the narrow mode.

No credential or access grant is provided in this directory. `MAC_TOKEN_SHA256` is a **required Wrangler secret** containing the lowercase SHA-256 hex digest of a high-entropy Mac bearer. The bearer itself belongs only in the Mac service's private configuration. The Mac must separately have a local `bridge-access.json` principal for this exact conversation and `read`, `send`, `subscribe` scopes. For events, the native principal's `callback_hosts` must separately include the actual platform callback host. The relay cannot authorize callback hosts and does not supply or guess a callback URL.

## Routes and behavior

| Route | Caller | Behavior |
| --- | --- | --- |
| `GET /` | Public | Human-readable relay status. No room data or schemas. |
| `GET /health` | Public | Configuration presence, not Mac reachability. |
| `GET /.well-known/oauth-authorization-server` | Public | OAuth server metadata. |
| `GET /.well-known/oauth-protected-resource[/mcp]` | Public | Resource metadata for the exact `https://<relay>/mcp` audience. |
| `POST /register` | OAuth client | Dynamic public-client registration. JSON: `client_name`, one to five exact HTTPS `redirect_uris`; optional `grant_types`, `response_types`, `token_endpoint_auth_method=none`. |
| `GET /authorize` | OAuth client/browser | `response_type=code`, registered `client_id` and exact `redirect_uri`, `resource=https://<relay>/mcp`, `code_challenge` and `code_challenge_method=S256`; optional `scope` and `state`. Displays owner consent page. |
| `POST /consent` | Owner browser | Form `id`, `code`; one-time eight-digit code is visible only through the Mac bearer route. Five tries, ten-minute expiry. Successful consent redirects to the registered URI with short-lived `code` and saved `state`. |
| `POST /token` | OAuth client | Form authorization-code exchange: `grant_type=authorization_code`, `client_id`, `code`, exact `redirect_uri`, `code_verifier`, exact `resource`. Refresh: `grant_type=refresh_token`, `client_id`, `refresh_token`, exact `resource`. |
| `POST /mcp` | OAuth bearer | One JSON-RPC request, at most 48 KiB. `initialize` creates a relay MCP session and returns `MCP-Session-Id`; later requests must send that header. Returns native JSON-RPC response or a clear timeout with durable request key. |
| `GET /bridge/pending` | Mac bearer | Pending OAuth requests and one-time codes to display on the Mac. |
| `POST /bridge/approve` | Mac bearer | Optional alternative for an explicit owner approval in the Mac UI. JSON `{ "id": "r_..." }`; returns `{ "id", "approved": true, "redirect": "https://..." }`. Never invoke automatically from a polling service. Manual code entry through `/consent` needs no separate Mac approval. |
| `GET /bridge/next` | Mac bearer | Atomically leases one queued RPC. Returns `{ "request": null }` or `{ "request": { "key": "q_<64 hex>", "rpc": { ... }, "created_at": 123 } }`. |
| `GET /bridge/leased` | Mac bearer | Lists leased requests for owner-assisted recovery after a crash. Reading never requeues or resends them. |
| `POST /bridge/respond` | Mac bearer | JSON `{ "key": "q_<64 hex>", "response": { "jsonrpc": "2.0", "id": ..., "result": ... } }` (or matching `error`). Returns `{ "ok": true, "duplicate": false }`; exact response retry returns `duplicate: true`; changed response is HTTP 409. |

Mac routes require `Authorization: Bearer <MAC_TOKEN>`. MCP requires a valid OAuth access bearer for this exact resource. All request bodies are bounded and Mac/MCP authentication happens before body parsing. Access tokens expire after one hour, refresh tokens after 30 days, authorization codes after two minutes. Only credential hashes are stored. A consumed refresh token used again revokes its entire grant family. Requested scopes are `room.read`, `room.send`, and `room.events` (all three by default).

The relay forwards `initialize`, `ping`, `server/discover`, `tools/list`, `tools/call` for the four `ocv.*` tools, `events/list`, `events/subscribe`, and `events/unsubscribe`. It accepts `notifications/initialized` without forwarding it, matching the native no-content behavior. The relay creates a persistent, 24-hour session for each `initialize`; subsequent requests, including the initialized notification, must use the returned `MCP-Session-Id` header. A reconnect initializes a new session, so a client can start JSON-RPC IDs again without collision. In narrow mode, sends require the configured room UUID. In all-session mode, IDs must be `<room UUID>/<participant UUID>` or `session/<run UUID>`. The relay inserts the configured `conversation_ref` and queue mode. It rejects an explicit different room or conversation before queuing. The native scoped principal remains the final access check.

The Mac service should journal each leased `key` before calling `http://127.0.0.1:9476/mcp/ocv` with its **separate local scoped bearer**. It should post the full native JSON-RPC response to `/bridge/respond`; if that POST's result is uncertain, retry the **same** key and **same** response. It must not automatically replay a leased RPC after restart. `ocv.send_message` carries the caller's stable `client_message_id`, so a deliberate local retry uses the same native idempotency key. The cloud request key is stable for the MCP session and JSON-RPC ID; within a session clients must use unique IDs for distinct requests. Exact cloud retries with the same session header observe the existing request; a changed payload with the same ID gets HTTP 409. A request is never requeued after being leased.

The Worker waits about 15 seconds for the Mac response. If it does not arrive, it returns HTTP 504 with a JSON-RPC error and `data: { request_key, state, retry }`. `queued` means the Mac has not taken it; `leased` means delivery/execution may be uncertain. Repeating the **exact** JSON-RPC request with the **same session header** can retrieve a later recorded response and will never resend it. A leased request remains visible on `/bridge/leased` until its response arrives. This is a durable uncertainty record, not proof of execution.

For `events/subscribe` and `events/unsubscribe`, the relay passes the native callback protocol through. Native OpenCovibe validates the callback URL and host allowlist, performs challenge verification, and signs callbacks. A successful callback establishes platform receipt only; it does not prove the remote agent woke.

## Local verification

`npm ci`, `npm run typecheck`, `npm test`, and `npm run build` operate locally. `npm run build` is a Wrangler dry run. The owner-authorized rollout deployed this dedicated Worker at `https://opencovibe-private-room-relay.ivg-design.workers.dev`. Live health/OAuth discovery and rejection of unauthenticated MCP requests passed. OAuth consent completed after fixing the registered callback CSP. Direct cloud tool discovery and correlated delivery remain separate acceptance checks. Tests use disposable Miniflare SQLite state and dummy credentials, with no live room, Mac, OAuth client, Cloudflare account, or callback.

The implementation follows the structure of the local Herald relay as a reference. No Herald source was copied or modified.

## Installed Mac service and private plugin

`scripts/room-bridge-relay.py` is installed as the user LaunchAgent `design.ivg.opencovibe.room-relay`. Private configuration and its response journal live in `~/.opencovibe-local/`; no credentials are included in the repository. Remote calls use a dedicated Mac bearer and native calls use a separate room-scoped loopback bearer. No redirects or proxy forwarding are allowed. A crash after execution begins produces an ambiguous result instead of replaying native work. A lost response acknowledgement retries the saved response unchanged. A lease lost before journaling remains owner-assisted; `--pending` surfaces the number of untracked leased requests.

Run `python3 scripts/room-bridge-relay.py --pending` locally to inspect the real pending client's name, registered redirect, requested scopes and one-time owner code. The polling service never approves connections automatically. The owner enters that code on the exact OAuth consent page. Do not share it in chat or commit it.

Private plugin: [OpenCovibe Connection](https://chatgpt.com/plugins/plugins_6ac1338b4cb48191b104e6eac880c32b), version 0.1.1, release `pluginrel_6ac166306ea88191a8cfac7057d50591`. The wrapper references the existing connected registered app; its duplicate bundled OAuth route is disabled. Authentication is verified, while callable tools and direct delivery require host acceptance. The currently provisioned native principal has read/send scopes only. A real event connection requires the actual platform callback host to be separately owner-approved; automatic cloud wake remains unverified.

The Mac must stay awake with OpenCovibe open. Stop the service with `launchctl bootout gui/$(id -u)/design.ivg.opencovibe.room-relay`. Secrets remain local for reversible recovery. The deployment is independent of Herald's existing relay.
