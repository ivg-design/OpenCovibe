# Local Agent Room

The ivg-design/OpenCovibe fork retains OpenCovibe's Tauri/Svelte desktop UI and its local
Codex app-server and Claude Code session actors. A room holds one global objective, one
GitHub Project, independent peers, and a continuous shared conversation.

## Use

1. Open Rooms for the group chat, Room settings for configuration, and the sidebar Project board for progress. Create a room with an existing local Git repository and owner/repository.
   Project creation is enabled by default. A saved room always reuses its Project. If a network
   write is uncertain, use Retry Project setup to reconcile it, or attach its existing number.
2. Add named Codex or Claude peers with individual model, effort and optional turn limit and
   worktree. A newly added peer is paused and does not start a provider turn. Model and effort dropdowns use the provider catalog. Choose **Edit agent** to rename an existing agent without stopping its current activity. Pause it to change model, effort or the optional turn limit, then Save; the next resumed turn uses the saved settings and retains the same provider conversation, worktree, claims and turn count.
3. Resume the peers and the room. Give the objective or further instructions in the shared
   conversation. Human messages can address everyone or one peer; agent tools can post shared
   updates and directed questions. Agent broadcasts are visible to all and do not automatically
   wake everyone, avoiding response loops. Directed questions wake their recipient when idle.
4. Peers choose work and roles themselves. The host offers fresh eligible work after an idle
   turn, with a cooldown; atomic claims prevent two peers owning the same task. A peer owns
   at most one unfinished task. Blocked or uncertain claims require explicit resolution.
5. In Room settings, add custom timed messages with an interval and participant. Choose either a maximum
   delivery count or an end date/time in your local timezone. Expired queued messages are
   discarded before dispatch. Due messages
   coalesce while busy and deliver after the current turn; approval waits never get interrupted.
6. Open a peer's session to inspect tools, answer approval requests or send provider controls.
   The room board is read-only: inspect task bodies/evidence, filter titles/priority/agents, and
   follow GitHub links. Peers change the canonical task through room tools.
7. The persistent **Requests** panel beside the room chat records agent proposals, questions for the human, independent peer reviews and
   final completion. Approving an agent creates exactly one paused peer with its approved brief;
   explicitly resume it when ready. Search by text and request type, or filter Open, Your answer needed, Waiting on agents, Resolved, Archived or All requests. Select an entry to read its full context and retained responses. Close obsolete requests with a reason; Archive resolved clears finished entries from the active inbox while preserving their history. Archived entries can be restored. Active room peers can also use close_request and archive_requests; closing a request never grants approval. The Requests toolbar button and panel close button hide or show the panel without discarding drafts. A new actionable request opens a hidden panel and selects the new entry. Drag the divider to resize, or use its arrow keys; width and visibility are saved per room. Answer in the full-width multiline field beneath the context; buttons stay below and only the context scrolls. Room settings contains configuration, not requests or task claims. Reviews go to a named different
   peer and retain their evidence. Resolve requests for changes by closing the old request with an explanation and
   creating a new one after the correction. Completion needs all board work Done, no unfinished
   claims or other open requests, fresh GitHub data, and independent peer verification. Pause the
   room, refresh the board, and accept with a note to archive the completed room.
8. Pause the room to stop its provider processes. Resume preserves claims and provider context.
   The total turn limit is optional and disabled for newly added or imported agents. Existing explicit limits are retained. An enabled limit counts all room starts for that agent (messages, automatic tasks and timers together); it does not count each tool call or subagent separately. It pauses only that agent at the limit. Resume resets the count; editing settings does not. Each timer has its own separate delivery limit or end date. Archive
   keeps the room, Project, conversation and worktrees. Removing a peer requires releasing its
   unfinished claims. Worktrees are retained; Merge is explicit and requires clean checkouts.
9. Settings → Appearance includes persistent custom colors with reset and contrast handling.
10. To continue an existing conversation as a room, import its CLI session if needed, open
    the chat, and choose **Create room from session**. The original agent retains its provider
    session identity. Recent history is shared as context, with a link to the full transcript.
    Setup shows the source title and saved working folder before creating anything. The
    primary Git repository is detected from that folder; if it is General or another folder
    outside Git, choose the intended repository using the folder picker. The chosen folder
    becomes the original agent's working folder without changing its provider session or
    transcript. After creation, its sidebar entry moves under the new room. A single linked open
    GitHub Project is selected automatically; multiple candidates require a choice. Existing
    board fields are preserved. If this exact saved session already has a room, setup names
    that room and offers an explicit Open action or restoration of the detached original
    agent, paused. Setup errors stay in setup; Cancel returns to the source chat. Add new
    agents, then explicitly resume when ready.
11. The chat sidebar groups **project → room chat → named participants**. Select the room row for group chat or a colored participant row for its direct session. Standalone agent chats remain directly under their project. Room rows show a badge for requests needing your answer.
12. **View source session** opens the session used to start the room, including its full preserved history. **Room settings** is a button in the room header.
13. The sidebar **Project board** opens the selected room's board directly. Room briefings,
    automation envelopes, and generic protocol outputs render as readable summaries.

Room settings → **Attach existing agent to room** → **Choose existing agent** reconnects an agent with a saved local Codex or Claude conversation from the same repository, including linked worktrees. Former room participants appear first and can be renamed before attachment. They return under the room sidebar row, paused, with the same provider conversation and working folder; resume explicitly when ready. An agent already owned by another room or currently running cannot be attached. Future removals retain the participant identity, model, effort and worktree settings; older removals recover the participant id/name from retained message provenance when available. No old transcript is replayed into the room, and removed timers are not recreated.

The Chats/Teams switch is inherited from upstream: **Teams** displays local Claude Code teams, their tasks and inboxes. Mixed Codex/Claude rooms are managed through Rooms and do not require Teams.

Room settings includes **Edit room instructions**. The saved instructions are included in new
wakeups for every peer. Running or already reserved turns keep their existing prompt. Saving an
outdated edit reports a conflict instead of overwriting a newer edit. **Maximum active room
agents** is the number of top-level peers allowed to work simultaneously; it does not count
provider-supported local subagents. Local delegation follows the room instructions, repository
rules and provider limits. Adding a persistent room participant still requires human approval.

Choose **Start side chat** on a message to name a focused conversation and choose existing peers.
The conversation selector switches between the main room and its side chats. Agents can use
create_sidechat/read_sidechat and post_message to do the same. Side chats retain the source message,
shared instructions, repository and project; they do not create new provider sessions or worktrees.
Peers keep their existing provider context. Messages and wakeups are routed to the selected
conversation, and creating a side chat alone never wakes a peer. Human messages appear on the
right and agent replies on the left. The full-width multiline composer stays visible, with controls on the row below, while history scrolls. It grows without collapsing the live field during typing. Reading older messages preserves your position; Jump to latest resumes following. Rooms initially render 50 recent messages and load older messages in batches on demand. Direct sessions open on the latest projected page before becoming visible, and load earlier history on upward scrolling.

## Runtime boundaries

The desktop host owns wakeups, not a model supervisor. It does not appoint a team lead or
create additional agents autonomously. Peers use persistent request cards to ask for another
agent or a human decision and appoint independent reviewers. The human approves the proposed
configuration and brief, answers decisions and accepts final completion. These actions are not
available through agent MCP tools. Completion approval preserves the peer review evidence and
checks that the board, claims and peer roster still match the reviewed snapshot.

Every delivery records its turn start in SQLite before sending and checks any enabled total turn limit. Timer delivery counts increase
only after the provider accepts dispatch. On an interrupted
or ambiguous delivery, startup pauses that participant and does not replay the message. Inspect
its session before explicitly resuming. Network failures preserve uncertain task claims; refresh
and reconcile or release them before continuing. An accepted delivery does not by itself prove
that the provider completed a response.

Pause, blockers, provider errors, quota rejection, pending interactions and exhausted budgets
prevent autonomous continuation. The board must be fresh and readable for automatic task
wakeups. Timers and human messages can wake an idle participant without a fresh board, but
claiming work still requires a fresh snapshot. The host runs while the desktop app is alive,
including when hidden in the tray; it does not schedule work after Quit.

Room-scoped stdio MCP tools provide snapshot, post_message, create_sidechat, read_sidechat, read_task, create_task, update_task, convert_draft_task, claim_task,
finish_task, block_task, release_task, request_agent, request_decision, request_review,
respond_request and propose_completion. They bind one room and participant and check ownership,
pauses and any enabled turn limits for governance writes. Repeated identical request creation returns
the existing record; conflicting reuse of a request title is rejected.
Finishing requires a summary and evidence; the proof is stored on GitHub before marking Done.
Exact task-creation intents are persisted to avoid blindly repeating uncertain writes. Creation
waits for the new task to appear in the claimable Project snapshot. In-flight board reads cannot
overwrite a task change or newer refresh committed while the read was pending.

New tasks are repository Issues linked to the room Project. Their descriptions require
Outcome, Work, Acceptance criteria, Progress and References sections. Every participant
wake includes the same built-in tracking policy; the room's editable instructions add
project-specific requirements. Claiming updates Status and Agent, progress updates retain
evidence and commit/PR references on the Issue and in the room's live claim, and finishing
records proof before marking Done. Participants enrich and convert older drafts when
taking them on; conversion preserves the Project item and ownership. Drafts cannot be
finished through the agent tool until converted. GitHub Issues must be enabled on the
repository; preflight reports this before recording a creation intent.

Fresh human broadcasts (including `@everyone` and the Everyone picker), directed peer
messages, explicit peer mentions, and assigned review requests revive agents stopped
automatically for a blocked task or lack of progress. Ordinary peer progress broadcasts
and imported transcript output do not revive dormant agents. Peer `@everyone` addresses
the other members of the current room or sidechat. Named recipients and sidechat membership
restrict who wakes. Unread addressed input received during a finishing turn is reconsidered
after completion, including after restart. Manual agent/room pauses, permission waits,
quota errors, uncertain deliveries and turn limits remain in force. Waking a blocked
participant lets it respond to new input; it does not resolve its blocked task or grant
an approval.
Codex approvals are configured only for the app-owned room tools using per-process
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

Session conversion connects the primary repository only; additional working folders and
repositories are not discovered from prose. The original app's external automation remains
separate from OpenCovibe scheduling. End-date timers are checked before dispatch; an already
started turn is not cancelled at expiry.

Verification and real mixed-provider results are recorded in
[the live acceptance record](LOCAL_AGENT_ROOM_LIVE_ACCEPTANCE.md) and
[session-room and side-chat acceptance](SESSION_ROOM_ACCEPTANCE_2026-09-30.md).
