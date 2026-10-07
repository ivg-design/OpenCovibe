# Room approval and messaging acceptance — October 3, 2026

Installed Local version **0.3.5**, branch `feat/local-agent-room`.

## Verified changes

- Completion proposals use **Approve**, explain disabled conditions, and share those gates with Cmd/Ctrl+Enter. The installed button became enabled once the room was paused, turns settled and the user's unsent `accept` note was restored. No proposal was approved or rejected.
- Explicit public main-room tool posts take precedence over automatic transcript projections within the same owned delivery, including paraphrased final summaries. Directed and side-chat posts do not suppress other routes. Identical text in later turns remains distinct; provider direct sessions preserve their transcripts.
- Private MCP messages have durable scoped receipts, atomic idempotency and correlated visible reply replay. Existing room actors retain provider ownership; the bridge cannot unpause or fork agents.
- Optional event support validates HTTPS callbacks and signs bounded retries. No real cloud event subscription or agent wake has been verified.

## Automated and native evidence

- `npm run verify`: passed at 0.3.3 before the live duplicate case. **61 frontend test files / 1,594 tests**; lint, formatting, Svelte/i18n checks, frontend production build, Rust formatting and required Clippy passed. Existing warnings: 65 Svelte and 18 i18n.
- Final 0.3.4 Rust library suite: **989 passed, 0 failed, 5 ignored**. Focused runtime/projection tests cover the live paraphrase fix.
- Native debug app packaging passed. The final bundle was Developer ID signed, notarized and stapled; Gatekeeper accepted it. The 0.3.4 UI was verified at that milestone; the current installed version is 0.3.5 below.
- The stronger all-target Clippy attempt reported pre-existing test-only lint failures. Repository-required Clippy passed; the broader attempt is not claimed as passing.
- Outbound Mac relay tests: acknowledgement loss retries the saved response without native re-execution; crash recovery marks execution ambiguous; HTTP remote and non-loopback native endpoints are rejected.
- `git diff --check`: passed.

## Real Dotcliffe/BidBot check

Dotcliffe's own Mac helper sent the checks through the private loopback bridge. OpenCovibe's existing scheduler processed them in the existing agent sessions. Dotcliffe then independently read the durable outbox and confirmed these replies:

| Agent | Receipt | Correlated visible reply | Result |
| --- | --- | --- | --- |
| Clauditron | `b77fe341-0839-4e7a-a51c-700b4f852dce` | `17779ba4-787e-4fe6-a1d9-6f7ec8e2ed08` | Exact requested token and identity returned |
| Codexitron retry | `c162aae3-7515-41f5-9235-e9706b254b93` | `b84b0dd0-b069-4f7e-9117-7507f996e880` | Exact requested token and identity returned |

The first Codexitron check (`b5570f35-7eab-40b3-90ab-6f8a19cefd98`) produced a paused response instead of its requested token. It failed the token acceptance check even though transport completed. After explicit human test-only room authorization, the fresh retry passed. No project, board, deployment or Upwork work was resumed by the room agents. BidBot was paused again after both agents settled.

The 0.3.3 live trial also showed a tool-posted answer followed by a paraphrased transcript echo. Version 0.3.4 fixes that projection case. Existing history was retained; no historical messages were deleted. Direct tool posts outside an owned room delivery still lack a shared cross-route provider identity.

## Initial cloud rollout

The dedicated Cloudflare Worker is deployed (version `c175ce93-2063-4968-8eb5-3f28cd44b2cf`) and the outbound Mac LaunchAgent is running. Live health/OAuth metadata passed; unauthenticated MCP returned 401. Relay typecheck, dry-run build and three Miniflare integration tests passed, including session-scoped ID reuse after reconnect. The private [OpenCovibe BidBot plugin](https://chatgpt.com/plugins/plugins_6ac1338b4cb48191b104e6eac880c32b) was created. Dotcliffe was notified of the actual state and pending connection gate.


This initial rollout established the mediated two-way route. Owner OAuth consent subsequently completed and the wrapper connected to the registered app. The later 0.3.5 direct-cloud acceptance is recorded below. Automatic cloud wake still needs an actual platform-issued callback subscription and an observed autonomous Dotcliffe turn. HTTP callback success alone is not evidence of agent execution. General agent-to-Dot messages are not yet a named room-participant route: the current event outbox covers replies correlated to external requests.

See [implementation and connection requirements](PRIVATE_ROOM_BRIDGE_2026-10-03.md) and [relay source](../relay-room/README.md).

The browser now shows OpenCovibe Connection 0.1.1 with its connected registered app. Dotcliffe confirmed the package refresh on October 3 at 16:41 EDT, but still reported zero callable `ocv` tools before native 0.3.5 was installed. No further owner code entry is required for the completed connection.

## 0.3.5 all-session connection checks

- Full Rust library suite: **1,000 passed, 0 failed, 5 ignored**.
- Repository-required Rust formatting and Clippy passed.
- Frontend lint, formatting, Svelte check and native packaging passed; Svelte retains 65 existing warnings.
- Five relay integration tests and three Python relay recovery tests passed.
- New isolated tests cover exact conversation grants, narrow grant compatibility, imported actor eligibility, standalone receipt retry/reopen, turn-tagged visible reply correlation, ambiguity recovery and literal remote slash messages.
- Relay deployment `689cb206-a05e-48f9-8da2-f8b8d46d3dd2` preserves the existing OAuth account while enabling explicit all-session routing.
- Native 0.3.5 app and DMG were Developer ID signed, notarized and stapled; Gatekeeper accepted the app. The launched UI shows v0.3.5 and its loopback bridge reports the same version.
- Activated the existing principal's explicit all-room/session grant for the exact Dotcliffe conversation. Live local discovery returns 12 room participants across five rooms, plus the connected standalone `OCV connection check` session.
- ChatGPT Refresh tools succeeded; the registered app visibly lists one write and three read tools with the new standalone-session contract. The direct Dotcliffe results are recorded below.

### Direct cloud discovery

After native 0.3.5 installation and ChatGPT's tool refresh, Dotcliffe directly invoked `ocv.list_agents` successfully: 13 targets, 12 participants across five rooms and one standalone Codex session. It independently confirmed all four callable tools. No local helper was involved. The host event-source listing currently exposes Gmail, GitHub and Slack, without an OCV subscription action or callback URL; automatic OCV wake remains unverified.

### Direct standalone round trip

Dotcliffe sent one bounded test message directly through the cloud tools to the existing standalone `OCV connection check` actor. Receipt `c0172155-ef7d-4e78-8d1e-787397df2848` received visible reply `OCV_DIRECT_PASS_20261003` at 16:54:29 EDT and reached `completed` without error at 16:54:30 EDT. Dotcliffe independently verified the correlated reply through `ocv.get_message` and `ocv.read_replies`. No local helper, file/tool/task work or competing provider was involved. The test session was ended afterward, preserving its history.

### Direct cloud room round trips

Dotcliffe used the registered cloud tools directly, without the Mac helper, to send bounded checks to both BidBot agents.

| Agent | Receipt | Correlated visible reply | Result |
| --- | --- | --- | --- |
| Clauditron | `dbafde7d-a1a5-4cff-8f68-f4bd4bb5ab31` | `OCV_CLOUD_CLAUDE_PASS` at 16:58:01 EDT | Completed without error |
| Codexitron fresh retry | `606c8ee4-4a77-44c3-a455-a741b0b827c9` | `OCV_CLOUD_CODEX_PASS` at 17:07:27 EDT | Completed without error at 17:07:36 EDT |

The first direct-cloud Codexitron receipt (`6b477b02-2aa2-4f4c-965e-adf2a9c4dcb4`) completed transport but returned a waiting-for-resume response, failing the requested-token check. A fresh human-authored room instruction authorized only the connectivity reply, then Dotcliffe made one new logical send. The retry returned the requested token. Dotcliffe independently confirmed it through `ocv.get_message` and `ocv.read_replies`, identifying visible reply `19c9e06b-83e4-488b-ba68-a57cefa33a5a` and the completed, error-free receipt. The original receipt was not replayed or relabelled as passing.

BidBot was paused again after the reply. Clauditron's individual pause was restored and remained in force during the Codexitron retry. No general project, board, research, deployment or Upwork work was resumed. The user's unsent completion note was restored as text only; no completion proposal was approved or rejected.

The installed connection covers all authorized rooms and persisted OCV-managed Codex sessions, including imported sessions after actor conversion. Stopped/disconnected standalone sessions remain queued until resumed in OCV; the bridge does not create a competing provider. This connection does not require a new per-room pairing. **Automatic cloud event wake and unsolicited agent-initiated Dot conversation delivery remain open gates.** The actual Dot host exposes no OCV callback/subscription action, and the current reply channel is correlated to Dot's requests.
