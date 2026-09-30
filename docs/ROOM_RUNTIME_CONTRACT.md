# Room runtime contract

The desktop owns delivery scheduling and local atomic claims. GitHub is the canonical task
record. Each Codex or Claude participant uses an independent local session actor and run ID.
Room messages form one continuous feed. No relay or remote collaborator service is required.

## Delivery and recovery

Automatic task continuation requires an active room and participant, a fresh readable board,
an eligible unclaimed or explicitly self-owned task, no pending turn or interaction, and a
remaining turn budget and a room concurrency slot (default 3, configurable from 1 to 5). The host offers work without appointing a manager. Peers choose work
and claim it before changing the remote task or beginning the task.

Human broadcasts and directed human/agent messages can wake idle peers. Agent broadcasts
are visible without waking everyone. Timers have a minimum interval and a finite delivery limit;
overdue messages coalesce while busy. Pauses, pending interactions and exhausted budgets block
delivery. Failed/quota/blocked participants do not automatically retry. Non-idle-only timers
persist one queued wake while busy; idle-only timers wait without queuing. Editing, disabling or
deleting a timer cancels its old queued wake. A long scheduling gap coalesces missed intervals
into one delivery rather than replaying a burst.

Three completed automatic task turns with no change in task status, ownership, claims or
completion evidence pause the peer with a visible reason. Chat chatter and timestamp changes
do not count as task progress. Explicit Resume resets this counter and the turn budget.
Permissions and clarifications expose a waiting reason; timed messages do not bypass them.

SQLite commits delivery intent and reserves budget before the provider write. An ambiguous
or interrupted delivery pauses its participant on restart until explicit Resume; it is never
blindly replayed. Provider history imports are idempotent. No wakeups run after the desktop quits.

## Task ownership

A participant owns at most one unfinished claim. Atomic reservations prevent two peers owning
the same task. GitHub write uncertainty retains ownership and pauses the participant. Completion
requires a summary and evidence, written to GitHub before Done. Task creation persists its exact
intent so uncertain network outcomes require reconciliation rather than duplicate creation.

Room-scoped stdio MCP tools are snapshot, post_message, read_task, create_task, claim_task,
finish_task, block_task, release_task, request_agent, request_decision, request_review,
respond_request and propose_completion. Each subprocess binds one room and participant; writes
check pauses and ownership. Codex and Claude use the same thirteen-tool per-process allowlist;
other provider permissions retain normal approval behavior.

## Requests and acceptance

Requests are atomically saved in the room database with a visible message. Review requests wake
only their appointed peer through the existing directed-message delivery path. Human responses
reach the requester through that same bounded path. Creating the same kind/title/content again
is idempotent; reusing a title with changed content fails. Rejected or corrected requests remain
in history, including the reviewer response even after human acceptance.

An additional-agent request contains a proposed provider/model/effort/budget/worktree and brief.
Only the desktop human command creates the peer, always paused. A durable creating state and
request-derived peer/run/worktree identity make interrupted approval retryable without duplicates.
Cancellation before the member is saved prevents creation; after it is saved the approval must
be reconciled. Agents cannot approve proposals, answer human decisions or accept completion.

Only a named different peer can approve a review or verify completion. Completion captures a
stable board/claim/roster signature. Verification and final acceptance require the same signature,
a fresh readable Project board (when attached), all tasks Done, no unfinished claims and no
other unresolved requests. Final human acceptance additionally requires the room paused with no
running/waiting/pending deliveries and archives it permanently. A changed snapshot requires
cancelling the old proposal and submitting a new one.

## Workspaces

Optional worktrees start from committed HEAD and preserve the primary checkout. Worktrees
persist after peer removal and room archive. Merge is user initiated and requires clean checkouts;
conflicts abort the attempted merge and are reported. There are no automatic force operations or
worktree deletions.

## Acceptance

The acceptance target includes a room with three peers using both actual providers, independent
worktrees, code changes and test/commit evidence, peer review messages, automatic continuation,
GitHub ownership and completion, bounded timers, hidden-window continuation and relaunch
persistence. Keep observed native results separate from deterministic policy tests. The original
Project #9 arithmetic fixture alone does not close this target. See [the acceptance record](LOCAL_AGENT_ROOM_LIVE_ACCEPTANCE.md)
for the actual results and remaining boundaries.
