# Local Agent Room

The ivg-design/OpenCovibe fork retains OpenCovibe's Tauri/Svelte desktop UI and its local
Codex app-server and Claude Code session actors. A room holds one global objective, one
GitHub Project, independent peers, and a continuous shared conversation.

## Use

1. Open Rooms and create a room with an existing local Git repository and owner/repository.
   Project creation is enabled by default. A saved room always reuses its Project. If a network
   write is uncertain, use Retry Project setup to reconcile it, or attach its existing number.
2. Add named Codex or Claude peers with individual model, effort, turn budget and optional
   worktree. A newly added peer is paused and does not start a provider turn.
3. Resume the peers and the room. Give the objective or further instructions in the shared
   conversation. Human messages can address everyone or one peer; agent tools can post shared
   updates and directed questions. Agent broadcasts are visible to all and do not automatically
   wake everyone, avoiding response loops. Directed questions wake their recipient when idle.
4. Peers choose work and roles themselves. The host offers fresh eligible work after an idle
   turn, with a cooldown; atomic claims prevent two peers owning the same task. A peer owns
   at most one unfinished task. Blocked or uncertain claims require explicit resolution.
5. Add custom timed messages with an interval, participant and delivery limit. Due messages
   coalesce while busy and deliver after the current turn; approval waits never get interrupted.
6. Open a peer's session to inspect tools, answer approval requests or send provider controls.
   The room board is read-only: inspect task bodies/evidence, filter titles/priority/agents, and
   follow GitHub links. Peers change the canonical task through room tools.
7. Pause the room to stop its provider processes. Resume preserves claims and provider context.
   A peer that reaches its turn limit needs explicit Resume to grant another budget. Archive
   keeps the room, Project, conversation and worktrees. Removing a peer requires releasing its
   unfinished claims. Worktrees are retained; Merge is explicit and requires clean checkouts.
8. Settings → Appearance includes persistent custom colors with reset and contrast handling.

## Runtime boundaries

The desktop host owns wakeups, not a model supervisor. It does not appoint a team lead or
create additional agents. Peers ask the human through the room conversation when they need
another agent, a decision or approval. The human adds peers through the room controls.

Every delivery reserves its budget and timer count in SQLite before sending. On an interrupted
or ambiguous delivery, startup pauses that participant and does not replay the message. Inspect
its session before explicitly resuming. Network failures preserve uncertain task claims; refresh
and reconcile or release them before continuing. A reserved count means an attempted delivery,
including an interrupted attempt, rather than proof of a provider response.

Pause, blockers, provider errors, quota rejection, pending interactions and exhausted budgets
prevent autonomous continuation. The board must be fresh and readable for automatic task
wakeups. Timers and human messages can wake an idle participant without a fresh board, but
claiming work still requires a fresh snapshot. The host runs while the desktop app is alive,
including when hidden in the tray; it does not schedule work after Quit.

Room-scoped stdio MCP tools provide snapshot, post_message, read_task, create_task, claim_task,
finish_task and block_task. They bind one room and participant and check ownership and pauses.
Finishing requires a summary and evidence; the proof is stored on GitHub before marking Done.
Exact task-creation intents are persisted to avoid blindly repeating uncertain writes. Creation
waits for the new task to appear in the claimable Project snapshot. In-flight board reads cannot
overwrite a task change or newer refresh committed while the read was pending.
Codex approvals are configured only for the seven app-owned room tools using per-process
[per-tool overrides](https://learn.chatgpt.com/docs/config-file/config-reference).
Other permission requests keep the provider's normal approval behavior.

Provider sessions use existing local CLI authentication. The room does not inject API keys,
change global CLI configuration, or provision credentials. Worktrees start from committed HEAD;
uncommitted primary-checkout changes are not copied. Failed merges abort their own merge and
report the conflict; there are no automatic force operations or worktree deletions.

## Build and data

Run `./scripts/local-room-dev.sh` for development. It uses the ignored `.local-data` profile.
Build a local macOS bundle with:

```sh
npm run tauri -- build --debug --config src-tauri/tauri.local.conf.json --bundles app
```

The local bundle uses identifier `design.ivg.opencovibe.local` and defaults to
`~/.opencovibe-local` when opened from Finder. An absolute `OPENCOVIBE_DATA_DIR` overrides
that profile for isolated acceptance. Provider login remains in the CLI's normal location.
The current artifact is an unsigned development build, not a notarized release.

Verification and real mixed-provider results are recorded in
[the live acceptance record](LOCAL_AGENT_ROOM_LIVE_ACCEPTANCE.md).
