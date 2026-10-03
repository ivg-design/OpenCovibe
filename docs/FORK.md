# OpenCovibe Local

This is the independently maintained **ivg-design/OpenCovibe** fork. Version **0.3.0**
starts its own release history, based on upstream 0.2.9.

It adds local Codex/Claude rooms, shared GitHub Projects, persistent requests,
timed wakeups, session continuation, side chats, attachments and a Matrix palette.
Language and appearance preferences are available in **Settings**.

Automatic update checks and upstream download prompts are disabled. Local development
builds are installed manually; a fork update channel must be validated before it is enabled.

[Fork source and issues](https://github.com/ivg-design/OpenCovibe)

Based on [AnyiWang/OpenCovibe](https://github.com/AnyiWang/OpenCovibe), under Apache 2.0.
Original copyright and license notices are retained.

## Version 0.3.1

App-wide UI/UX remediation improves Matrix readability, responsive layouts, room controls,
request focus and keyboard replies, import clarity and identity settings. See the
[UI/UX audit](UI_UX_AUDIT_2026-10-03.md) and
[acceptance evidence](UI_UX_ACCEPTANCE_2026-10-03.md) for changes, validation and deployment status.

## Version 0.3.2

GitHub task-sync failures keep uncertain claims protected while leaving room messaging
and coordination available. Legacy task-sync waits recover without overriding manual
pauses or replaying interrupted deliveries. See
[BidBot recovery evidence](BIDBOT_WAIT_RECOVERY_2026-10-03.md).

## Versions 0.3.3–0.3.4

Completion approval explains its disabled state and uses the shorter **Approve** label.
Room delivery identities suppress duplicate tool/transcript replies while preserving
recipient routing, later turns and dormant-agent wakeups. An owner-configured private
MCP bridge adds durable queued messages, scoped receipts and visible reply replay;
optional cloud event connection remains separate and disabled until configured.
See [implementation](PRIVATE_ROOM_BRIDGE_2026-10-03.md) and
[acceptance evidence](ROOM_APPROVAL_MESSAGING_ACCEPTANCE_2026-10-03.md).

Version 0.3.4 extends duplicate suppression to paraphrased automatic summaries after an explicit main-room post. It is installed and notarized. Dotcliffe independently verified BidBot replies through the existing Mac helper; direct cloud OAuth/event wake acceptance remains open.

## Version 0.3.5

The private Dotcliffe connection expands from one room to all owner-authorized OCV rooms and standalone Codex sessions. Imported sessions become eligible after OCV actor conversion. Messages use persistent queues and visible correlated replies, preserve provider ownership and pauses, and never implicitly start or resume a provider. Remote slash text is treated literally.

The cloud OAuth callback policy is fixed, the account connection is complete, and the private coordination plugin is 0.1.1 with a registered-app mapping. Legacy MCP initialization now negotiates a compatible 2025 revision. Native source checks passed: 1,000 Rust tests, required Clippy, formatting, Svelte checks and frontend packaging; relay integration tests passed. Native 0.3.5 is installed and notarized. Dotcliffe directly discovered 13 targets and verified correlated cloud replies from a standalone Codex session and both BidBot room agents. Automatic cloud event wake remains unverified; see the October 3 acceptance report for exact receipts and limits.
