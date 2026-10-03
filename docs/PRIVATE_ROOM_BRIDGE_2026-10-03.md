# Private room messaging and approval controls — 0.3.4

## Completion approval

The verified completion action is now **Approve**. The response footer explains why approval is unavailable: another room action is in progress, the request is archived, a participant still has an active turn or pending delivery, the room has not been paused, or the approval note is empty. Cmd/Ctrl+Enter uses the same checks. The existing backend still validates the independent review, work signature, fresh completed board, settled claims and other requests before archiving a room. This change does not approve any live proposal.

At the initial BidBot inspection, the verified proposal was in an unpaused room and Codexitron still had a pending turn. Those conditions prevented approval; typing a note alone would not make it safe to archive that room.

## Owned room transport

The existing web listener has an additional private, scoped MCP endpoint at `/mcp/ocv`. It only accepts requests when the listener is bound to a loopback IP. An absent owner access file disables authentication. Ordinary app cookies and the unrestricted app access token do not authorize this endpoint.

The bridge exposes four tools:

| Tool | Behavior |
| --- | --- |
| `ocv.list_agents` | Lists authorized room participants with stable IDs, provider session metadata, actual scheduler state and available actions. |
| `ocv.send_message` | Atomically saves a queued message and its receipt in the room database. Exact retries return that receipt; a changed payload with the same client ID is rejected. |
| `ocv.get_message` | Returns the durable delivery state, timestamps, owning delivery/run, provider turn ID when available, and blocked/error information. |
| `ocv.read_replies` | Replays authorized, correlated visible replies after a durable cursor. |

Messages are routed through the room's existing scheduler and provider session actor. The bridge neither starts a competing provider writer nor forks, resumes or unpauses a participant. It advertises queue mode only and explicitly rejects steering. Busy participants receive queued messages on subsequent eligible turns. Each external request is selected separately; later queued external payloads are excluded from the earlier turn's prompt.

Manual pauses, provider permission waits, turn budgets, concurrency and claim/review rules remain in force. A receipt can remain queued with a blocked reason. A delivery whose outcome cannot be established is marked ambiguous rather than replayed or declared successful. Attachments are references to files already attached to that room, never arbitrary local filesystem paths.

Only visible messages projected into the room are written to the reply outbox. Raw provider events, tool output and hidden reasoning are not copied. Receipt changes and visible reply projections are committed in the same room database transaction as the corresponding room update.

## Access configuration

Activation requires an owner-provisioned `bridge-access.json` in the selected app data directory. The local rollout provisioned a private principal bound only to BidBot and the named Dotcliffe conversation, with read/send scopes. Callback subscription access remains disabled until the actual platform callback host is approved.

Each authenticated principal has an owner-defined ID, a SHA-256 digest of a bearer token, explicit room/conversation binding pairs, callback DNS hosts and scopes (`read`, `send`, `subscribe`). The bearer must contain at least 32 characters; use a high-entropy randomly generated token when provisioning. The configuration rejects duplicate identities/digests and invalid hashes. On macOS/Unix it must be owned by the app's user and have no group/world permissions. Authentication and delivery re-read grants so removal revokes access. Callers cannot supply a principal or pair a granted room with another room's conversation.

The SQLite database contains signing secrets for authorized subscriptions and is private to the local user. Protect its backups as credentials. Removing a principal stops its event delivery. Removing a room/conversation binding prevents reads and event delivery for that binding.

## Optional cloud events

MCP discovery advertises protocol version 2026-07-28 and the event methods on the same authenticated endpoint. `ocv.reply.created` supports conversation filters, persistent subscriptions and replay cursors. Subscription creation verifies a fresh challenge before activation, caches successful verification for a short period, and requires an owner-allowed HTTPS callback host. Addresses are resolved and validated for every outbound connection, then pinned while retaining hostname verification for TLS. Private/special destinations and redirects are blocked.

Callbacks use Standard Webhooks HMAC-SHA256 over the event ID, signing time and exact body. Event IDs remain stable across retries. Signing-key rotation uses both keys during a short overlap. Delivery retries are bounded, preserve the undelivered cursor and stop on 410/413. Subscriptions expire, survive database reopening, and cannot refresh past an event awaiting delivery. Unsubscribe is idempotent and uses the original event/filter/callback identity.

A successful webhook response establishes receipt by the callback service; it does not prove that a cloud agent woke or acted. The protocol and callback requirements follow the [official MCP Events documentation](https://developers.openai.com/plugins/build/mcp-events).

## Owner-controlled connection and acceptance

The local endpoint is not a cloud-accessible server. A cloud connection still requires an owner-approved private HTTPS/OAuth transport or relay, a mapping from its authenticated identity to the local principal, a plugin connection in Work Cloud/Dot, and a platform-issued callback subscription. Preserve the loopback boundary and scoped authorization through that relay. Do not substitute an invented personal Dot callback URL or an unrestricted app token.

The signed, notarized 0.3.4 Local bundle was installed and launched. Dotcliffe used its existing Mac helper to send bounded checks through the private bridge and independently read correlated visible replies from both BidBot agents. This establishes the mediated round trip. A dedicated HTTPS/OAuth relay is deployed and the private [OpenCovibe BidBot plugin](https://chatgpt.com/plugins/plugins_6ac1338b4cb48191b104e6eac880c32b) is created; connecting that plugin, granting OAuth consent and observing an actual cloud event wake remain separate acceptance gates. BidBot was paused after the checks.

## Validation

Isolated tests use disposable room databases, fake principals and fixture replies. They cover atomic concurrent retries, changed-payload conflicts, cross-principal isolation, room/conversation binding, queued/busy/paused/budget/permission behavior, separate request turns, reply correlation and replay after database reopening, ambiguous delivery and removed participants, attachment retries after file removal, protocol tool names, signing against an independent HMAC fixture, forbidden callback addresses, subscription retry/refresh/expiration, key rotation and idempotent unsubscribe.

These tests exercise the scheduler boundary and durable projections; they do not claim real provider execution or cloud event wake acceptance. Final test/build counts and native deployment state are recorded in the accompanying acceptance report.
