# Room approval and messaging acceptance — October 3, 2026

Installed Local version **0.3.4**, branch `feat/local-agent-room`.

## Verified changes

- Completion proposals use **Approve**, explain disabled conditions, and share those gates with Cmd/Ctrl+Enter. The installed button became enabled once the room was paused, turns settled and the user's unsent `accept` note was restored. No proposal was approved or rejected.
- Explicit public main-room tool posts take precedence over automatic transcript projections within the same owned delivery, including paraphrased final summaries. Directed and side-chat posts do not suppress other routes. Identical text in later turns remains distinct; provider direct sessions preserve their transcripts.
- Private MCP messages have durable scoped receipts, atomic idempotency and correlated visible reply replay. Existing room actors retain provider ownership; the bridge cannot unpause or fork agents.
- Optional event support validates HTTPS callbacks and signs bounded retries. No real cloud event subscription or agent wake has been verified.

## Automated and native evidence

- `npm run verify`: passed at 0.3.3 before the live duplicate case. **61 frontend test files / 1,594 tests**; lint, formatting, Svelte/i18n checks, frontend production build, Rust formatting and required Clippy passed. Existing warnings: 65 Svelte and 18 i18n.
- Final 0.3.4 Rust library suite: **989 passed, 0 failed, 5 ignored**. Focused runtime/projection tests cover the live paraphrase fix.
- Native debug app packaging passed. The final bundle was Developer ID signed, notarized and stapled; Gatekeeper accepted it. The launched UI displays **v0.3.4**.
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

## Cloud rollout and remaining gate

The dedicated Cloudflare Worker is deployed (version `c175ce93-2063-4968-8eb5-3f28cd44b2cf`) and the outbound Mac LaunchAgent is running. Live health/OAuth metadata passed; unauthenticated MCP returned 401. Relay typecheck, dry-run build and three Miniflare integration tests passed, including session-scoped ID reuse after reconnect. The private [OpenCovibe BidBot plugin](https://chatgpt.com/plugins/plugins_6ac1338b4cb48191b104e6eac880c32b) was created. Dotcliffe was notified of the actual state and pending connection gate.


The mediated two-way route is confirmed. Direct cloud access needs the private plugin connected with owner OAuth consent; cloud wake additionally needs an actual platform-issued callback subscription and an observed autonomous Dotcliffe turn. HTTP callback success alone is not evidence of agent execution. General agent-to-Dot messages are not yet a named room-participant route: the current event outbox covers replies correlated to external requests.

See [implementation and connection requirements](PRIVATE_ROOM_BRIDGE_2026-10-03.md) and [relay source](../relay-room/README.md).

The created plugin appears installed in ChatGPT's plugin sidebar and Dotcliffe can read its coordination skill. It still reports no exposed `ocv` tools. The plugin's Manage action opens the ChatGPT desktop app, whose UI is disallowed to computer-use automation. The owner must finish the account connection/consent there; no connection success is claimed.
