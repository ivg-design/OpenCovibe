# Private room messaging and approval controls — 0.3.6

## Completion approval

The verified completion action is now **Approve**. The response footer explains why approval is unavailable: another room action is in progress, the request is archived, a participant still has an active turn or pending delivery, the room has not been paused, or the approval note is empty. Cmd/Ctrl+Enter uses the same checks. The existing backend still validates the independent review, work signature, fresh completed board, settled claims and other requests before archiving a room. This change does not approve any live proposal.

At the initial BidBot inspection, the verified proposal was in an unpaused room and Codexitron still had a pending turn. Those conditions prevented approval; typing a note alone would not make it safe to archive that room.

## Owned room transport

The existing web listener has an additional private, scoped MCP endpoint at `/mcp/ocv`. It only accepts requests when the listener is bound to a loopback IP. An absent owner access file disables authentication. Ordinary app cookies and the unrestricted app access token do not authorize this endpoint.

The bridge exposes four messaging tools, plus two explicit controls for owner-granted principals:

| Tool | Behavior |
| --- | --- |
| `ocv.list_agents` | Lists authorized room participants and standalone Codex sessions with stable IDs, provider session metadata, actual scheduler state and available actions. |
| `ocv.send_message` | Atomically saves a queued message and its receipt in the room database. Exact retries return that receipt; a changed payload with the same client ID is rejected. |
| `ocv.get_message` | Returns the durable delivery state, timestamps, owning delivery/run, provider turn ID when available, and blocked/error information. |
| `ocv.read_replies` | Replays authorized, correlated visible replies after a durable cursor. |
| `ocv.resume_room` | Explicitly unpauses a room; preserves individual pauses, budgets and provider waits. Exact control retries do not undo a later pause. |
| `ocv.wake_agent` | Atomically unpauses an existing room agent and queues one instruction through its owning scheduler. Starts cold sessions, preserves the room pause, and returns the normal correlated receipt. |

Messages are routed through the room's existing scheduler and provider session actor. Ordinary `ocv.send_message` never starts a competing provider writer, forks, resumes or unpauses a participant. Explicit owner-granted controls can resume the existing scheduler and start its owning actor. It advertises queue mode only and explicitly rejects steering. Busy participants receive queued messages on subsequent eligible turns. Each external request is selected separately; later queued external payloads are excluded from the earlier turn's prompt.

Manual pauses, provider permission waits, turn budgets, concurrency and claim/review rules remain in force. A receipt can remain queued with a blocked reason. A delivery whose outcome cannot be established is marked ambiguous rather than replayed or declared successful. Attachments are references to files already attached to that room, never arbitrary local filesystem paths.

Only visible messages projected into the room are written to the reply outbox. Raw provider events, tool output and hidden reasoning are not copied. Receipt changes and visible reply projections are committed in the same room database transaction as the corresponding room update.

## Access configuration

Activation requires an owner-provisioned `bridge-access.json` in the selected app data directory. The owner authorized a single persistent connection covering all OCV rooms and standalone Codex sessions, bound to the exact named Dotcliffe conversation, with read/send scopes. The 0.3.5 source supports explicit `all_rooms` and `all_sessions` flags plus nonempty exact `conversations`; omitted flags retain narrow legacy grants. Callback subscription access remains disabled until the actual platform callback host is approved.

Each authenticated principal has an owner-defined ID, a SHA-256 digest of a bearer token, explicit room/conversation binding pairs, callback DNS hosts and scopes (`read`, `send`, `subscribe`, optional `control`). The bearer must contain at least 32 characters; use a high-entropy randomly generated token when provisioning. The configuration rejects duplicate identities/digests and invalid hashes. On macOS/Unix it must be owned by the app's user and have no group/world permissions. Authentication and delivery re-read grants so removal revokes access. Callers cannot supply a principal or pair a granted room with another room's conversation.

The SQLite database contains signing secrets for authorized subscriptions and is private to the local user. Protect its backups as credentials. Removing a principal stops its event delivery. Removing a room/conversation binding prevents reads and event delivery for that binding.

## Optional cloud events

Native modern discovery advertises protocol version 2026-07-28 and the event methods. Legacy `initialize` negotiates 2025-06-18 or 2025-11-25; it no longer returns a modern revision to a legacy client. The hosted relay currently requires a legacy initialized MCP session. `ocv.reply.created` supports conversation filters, persistent subscriptions and replay cursors. Subscription creation verifies a fresh challenge before activation, caches successful verification for a short period, and requires an owner-allowed HTTPS callback host. Addresses are resolved and validated for every outbound connection, then pinned while retaining hostname verification for TLS. Private/special destinations and redirects are blocked.

Callbacks use Standard Webhooks HMAC-SHA256 over the event ID, signing time and exact body. Event IDs remain stable across retries. Signing-key rotation uses both keys during a short overlap. Delivery retries are bounded, preserve the undelivered cursor and stop on 410/413. Subscriptions expire, survive database reopening, and cannot refresh past an event awaiting delivery. Unsubscribe is idempotent and uses the original event/filter/callback identity.

A successful webhook response establishes receipt by the callback service; it does not prove that a cloud agent woke or acted. The protocol and callback requirements follow the [official MCP Events documentation](https://developers.openai.com/plugins/build/mcp-events).

## Owner-controlled connection and acceptance

The local endpoint is not a cloud-accessible server. A cloud connection still requires an owner-approved private HTTPS/OAuth transport or relay, a mapping from its authenticated identity to the local principal, a plugin connection in Work Cloud/Dot, and a platform-issued callback subscription. Preserve the loopback boundary and scoped authorization through that relay. Do not substitute an invented personal Dot callback URL or an unrestricted app token.

The signed, notarized 0.3.4 Local bundle was installed and launched. Dotcliffe used its existing Mac helper to send bounded checks through the private bridge and independently read correlated visible replies from both BidBot agents. This establishes the mediated round trip. A dedicated HTTPS/OAuth relay is deployed and the private [OpenCovibe Connection plugin](https://chatgpt.com/plugins/plugins_6ac1338b4cb48191b104e6eac880c32b) is connected through its registered app. The later 0.3.5 rollout verified OAuth consent, callable tools and direct cloud delivery separately, as recorded in the acceptance report. An actual autonomous cloud event wake remains unverified. BidBot was paused after the checks.

The bundled plugin instructions alone did not expose cloud tools. ChatGPT's custom MCP registration reached the actual owner consent page. Repeated page reloads initially created separate pending codes; the October 3 relay repair now reuses the same unexpired OAuth attempt, preserving expiry and failed-attempt lockout. The approval page and Mac helper display a matching request reference; incorrect-code responses retain the same form. Relay version `72b27eb2-0932-4b74-86f7-e3186e7c5d21` was deployed and the visible approval reference was matched to the Mac's pending request. A second defect was the approval page CSP: `form-action` blocked the registered cross-origin ChatGPT redirect after consuming consent. The repair permits only the exact registered callback origin. OAuth then completed, and ChatGPT visibly showed the connected account. The private wrapper was updated to 0.1.1 with the registered app ID; its duplicate bundled OAuth route was disabled. A page refresh cleared Dotcliffe's stale 0.1.0 package, but callable tools were still absent before the 0.3.5 native update. No codes or credentials are stored in this report.

## Validation

Isolated tests use disposable room databases, fake principals and fixture replies. They cover atomic concurrent retries, changed-payload conflicts, cross-principal isolation, room/conversation binding, queued/busy/paused/budget/permission behavior, separate request turns, reply correlation and replay after database reopening, ambiguous delivery and removed participants, attachment retries after file removal, protocol tool names, signing against an independent HMAC fixture, forbidden callback addresses, subscription retry/refresh/expiration, key rotation and idempotent unsubscribe.

These tests exercise the scheduler boundary and durable projections; they do not claim real provider execution or cloud event wake acceptance. Final test/build counts and native deployment state are recorded in the accompanying acceptance report.

## Standalone sessions in 0.3.5

The explicit all-session grant adds `session/<run_id>` targets. Only persisted Codex chats managed through OCV's SessionActor are eligible, including imported chats after OCV actor conversion. Room participants retain room IDs and are not duplicated as standalone targets. Queues and tagged visible replies survive database reopening. Dispatch uses an already connected idle actor, waits through busy or user-input states, rechecks authorization and ownership, and marks interrupted execution ambiguous instead of replaying it. It never launches or resumes a provider. Remote slash text remains ordinary message text. Standalone attachments are not yet supported; room attachment IDs remain supported.

The all-session relay deployment is `689cb206-a05e-48f9-8da2-f8b8d46d3dd2`. It retains the existing OAuth grants and service identity. Native 0.3.5 is installed, notarized and running. The live loopback bridge reports the version and lists five rooms plus a connected standalone test session. Direct cloud results are recorded separately in the acceptance report.

## Explicit room controls — 0.3.6

The owner requested delegated room and individual-agent resume/start controls. The exact existing Dotcliffe principal now has `control` in addition to read/send. The relay requires OAuth `room.send` for both controls and still enforces exact room/conversation binding; read-only accounts cannot use them. No extra OAuth flow or persistent bearer was created. Standalone-session control is outside this change.

Controls use stable `client_action_id` values. Room actions are saved durably; wake actions commit the participant state, queued message and receipt atomically. Same-ID retries reuse the original result and never resume a second time after a later human pause. Changed payloads conflict. Busy unpaused agents keep their active turn and writer. Permission, quota and unsettled paused turns require recovery in OCV. Turn counters reset only when the caller explicitly grants `reset_turn_budget=true`; existing limits stay unchanged. Archived rooms are rejected.

The resume audit is human-readable but does not broadcast a new work instruction. Wake messages identify the explicit resume instruction while retaining room rules and task scope. Native and direct cloud acceptance are recorded in [room-control acceptance](DOTCLIFFE_ROOM_CONTROLS_ACCEPTANCE_2026-10-03.md).
