# Dotcliffe room controls — October 3, 2026

## Delivered behavior

OpenCovibe Local 0.3.6 exposes owner-granted `ocv.resume_room` and `ocv.wake_agent`. Room resume preserves individually paused participants. Wake atomically unpauses one existing participant and queues a message; it does not resume its room. The owning room scheduler starts/resumes Codex or Claude without introducing another provider writer. Ordinary sends preserve pauses. Permission/quota waits, unsettled paused turns, archives and existing turn limits remain enforced. An explicit `reset_turn_budget=true` grants a fresh counter without changing the configured limit.

Stable action IDs make exact retries durable. A wake retry returns the original receipt and never starts again or undoes a later owner pause. Room-action retries return the original action and current room pause. Changed payloads conflict. Native control access and OAuth room.send are both required; read-only OAuth was tested against the real Worker request handler.

The existing exact Dotcliffe conversation received the native control grant, preserving its bearer and all-room/session authorization. OAuth identities, grant storage and Durable Object name were retained. Worker deployment: `02424bd9-a070-49bf-a46c-3639e29deaaa`. Private plugin 0.1.2 release: `pluginrel_6ac17e4c00d081919e9f2ce5e97bf2bf`; backend source readback confirmed the app mapping and control instructions. ChatGPT's registered app tool catalog was refreshed.

## Source checks

- Rust library: **1,006 passed, 0 failed, 5 ignored**. Six new control tests cover exact scope/binding, individual pause preservation, atomic wake, provider/budget guards, concurrent exact retries, persistence, reply correlation and strict discovery/arguments.
- Required Rust formatting and Clippy passed.
- Frontend: **1,594 passed** across 61 files; ESLint and formatting passed. Svelte checks: 0 errors, 65 existing warnings.
- Relay: **7 passed**, plus TypeScript check and production deployment. Python client/relay syntax checks passed.
- Optional `npm run doc:check` is unavailable because the repository already lacks `scripts/doc-check.mjs`; documentation links were reviewed directly. This is not counted as a passing check.
- Final native frontend/Rust packaging passed. The participant disclosure now preserves manual open state during typing, model selection and room refresh; native editing and adding the second participant passed after the fix.

## Direct cloud/native acceptance

An isolated room was created through native UI with no GitHub Project, timers or automatic continuation. Repository `/private/tmp/ocv-control-acceptance`; room `267ab8ef-e0a4-44a8-a7ff-2450f52c76c5`. Test participants were created paused with a one-turn cap. No manual native resume or local message helper was used to execute the cloud tests.

Dotcliffe independently discovered and invoked both new controls through the connected cloud app. The room was resumed, its paused cold Codex participant was started, and its actual correlated visible answer was `OCV_DOT_WAKE_CODEX_PASS` at 18:20:52 EDT.

- Codex participant: `5a44f973-a034-4fdd-a4f9-888ca4418e80`; model GPT-6 Luna.
- Resume action: `dotcliffe-resume-acceptance-20261003-222009`.
- Wake action: `dotcliffe-wake-acceptance-20261003-222025`.
- Receipt: `c42a1386-3c3d-45e4-8a9b-baf9362193ca`; reply: `5b3b1a81-12a9-4da8-bfdc-a57ad23c152b`.
- Terminal receipt completed without error. Exact retries reused the original action timestamp, receipt and provider turn; wake_count stayed 1. The participant stopped with its original one-turn cap exhausted.

Dotcliffe independently verified the second cold-start test with Claude Haiku 4.5:

- Participant: `24402013-1e66-4cea-8b26-60e9d24a2247`.
- Wake action: `dotcliffe-wake-claude-acceptance-20261003-222442`. It queued with Room paused; independent discovery still reported the room paused and wake_count 0.
- Separate resume action: `dotcliffe-resume-claude-acceptance-20261003-222517`.
- Actual visible reply: `OCV_DOT_WAKE_CLAUDE_PASS` at 18:25:51 EDT; reply `a6aec0a6-8163-4251-b84b-a9b51a65d7ce`.
- Receipt `5940ae6a-e238-40db-812a-37cce52e9f85` completed without error at 18:25:54 EDT.
- Same-ID wake retry returned the original receipt/delivery. Wake count stayed 1, the agent remained individually paused at its original cap, and no extra reply appeared. Codex remained paused.

Both tokens were also visible in native room chat. The isolated room was archived through native UI after both tests settled; its history and receipts remain available. Dotcliffe was informed that the controls cover every authorized room and are available for normal owner-authorized coordination. No other room was resumed during acceptance. The unsent BidBot completion note was restored after the app restart, without clicking Approve.

The native app and installer are Developer ID signed, notarized and stapled. Gatekeeper accepted the installed final 0.3.6 bundle. Installer SHA-256: `cb8fc3eab03a021152ff099c6b52c6009ad104f0edf6550435895ba1d37f45b2`. Test history is retained for evidence; no live completion proposal was approved and no PR was merged.

## Acceptance limits

Explicit Dotcliffe-to-room controls are separate from unsolicited agent-to-Dotcliffe wake. The actual Dot host still exposes no OCV event subscription action/callback, so automatic cloud wake remains unverified. Standalone session start/resume is outside these new room controls. Provider permission, quota and unsettled-turn recovery remains in OpenCovibe.
