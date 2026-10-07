# Reactive private relay — October 6, 2026

The owner reported approximately 75,000 daily Worker requests and required reactive delivery. The old KeepAlive LaunchAgent called /bridge/next every second even with OCV closed: approximately 86,400 requests per full day before useful RPC traffic. It was stopped immediately, removed from login startup, and its plist preserved privately in ~/.opencovibe-local/recovery/relay-polling-20261006/. Credentials, consent grants and the uncertain-execution journal were retained.

## Implementation

OCV Local 0.3.7 owns one outbound authenticated WSS connection while its private native listener runs. The Worker uses Durable Object WebSocket hibernation and pushes a queued lease when an actual cloud MCP request arrives. Work, saved responses and acknowledgements use WSS. Healthy idle has no HTTP poll or scheduled heartbeat. The Mac declares readiness only after a durable acknowledgement; only one lease is in flight. App/listener cancellation drops the connection. Network failures have bounded reconnect backoff; that timer never checks for work.

The existing native scoped principal is reauthenticated per request. The compatible private journal and exclusive lock prevent competing transports. Interrupted native execution becomes ambiguous; lost acknowledgements replay only the recorded response. Reconnection never silently requeues an untracked lease. Python unattended mode is retired; --once and --pending remain explicit operator utilities. Authenticated /bridge/status exposes transport counts and socket count, with no credentials or room contents. Native bridge-relay-status.json records human-readable last-known lifecycle state privately; the authenticated Worker socket count is authoritative after process exit.

An initial TLS startup failure was identified during installed validation, before acceptance. Native transport now explicitly uses the system TLS backend already used by OCV; a regression test initializes it. Certificate verification remains enabled. An early installer attempt also failed transiently during DMG generation; the final app and installer completed signing/notarization/stapling and Gatekeeper assessment.

## Source checks

- Rust: **1,010 passed, 0 failed, 5 ignored**. Four transport regressions cover secure TLS initialization, private configuration/listener binding, silent idle, disconnect, saved response replay and uncertain Python-journal migration.
- Required Rust formatting and Clippy passed.
- Relay: **8 tests passed**, plus TypeScript validation. The new real Miniflare WebSocket/OAuth test covers idle silence, exact authentication, queued push, ready/ack gating, socket replacement, lease preservation, duplicate response acknowledgement and malformed-frame rejection.
- Existing Python recovery tests: **3 passed**. Native/frontend app packaging passed. Existing frontend warning diagnostics remain.

## Installed and live acceptance

- Signed/notarized/stapled native **0.3.7** launched through the normal macOS UI and showed that version.
- Final Worker deployment: **6642e413-a9d3-428c-9be3-11a049154252**. Same Durable Object name and OAuth identities were retained.
- With OCV closed, durable Worker counters remained unchanged for **117 seconds**, excluding the explicit status snapshots. Socket count and HTTP poll count were zero.
- The installed app established one WSS connection and recovered the queued read-only initialization requests.
- The actual connected cloud ocv.list_agents call succeeded and returned **14 authorized room participants** through the new native transport. No local message helper or provider turn was used.
- Healthy open-app idle was measured for **43 seconds**: one connection remained open; all HTTP and socket-frame counters stayed identical except the two explicit status snapshots. HTTP /bridge/next calls remained **zero**.
- Quitting through Cmd+Q closed the native app and the Worker reported **zero connections**. The response journal was settled. The private native diagnostic retained its last Connected state after immediate process exit; it is a last-known debug record, not the authoritative online check. Closed-app follow-up is recorded below.

No room/participant was resumed, no agent work was assigned, and no live completion proposal was approved. These bounded checks prove startup, actual cloud/native RPC, idle silence and shutdown; extended network-outage/sleep/soak reliability is not claimed. Automatic agent-reply wake of Dotcliffe remains a separate platform-subscription gate.

Final installer SHA-256: 46e4370d1c6870602749bfa4013b8f06aa6bbdfcf2c013b625e8051034a65ec4.

Reference: [Cloudflare WebSocket hibernation](https://developers.cloudflare.com/durable-objects/best-practices/websockets/).

Final quit follow-up: **43 seconds** with zero sockets and unchanged HTTP/frame counters, excluding the two explicit diagnostic snapshots. No polling process or login LaunchAgent remained.
