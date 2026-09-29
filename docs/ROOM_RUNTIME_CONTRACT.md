# Room runtime contract

The desktop owns delivery scheduling and local atomic claims. GitHub is the canonical task
record. Each Codex or Claude participant uses an independent local session actor and run ID.
Room messages form one continuous feed. No relay or remote collaborator service is required.

## Delivery and recovery

Automatic task continuation requires an active room and participant, a fresh readable board,
an eligible unclaimed or explicitly self-owned task, no pending turn or interaction, and a
remaining turn budget. The host offers work without appointing a manager. Peers choose work
and claim it before changing the remote task or beginning the task.

Human broadcasts and directed human/agent messages can wake idle peers. Agent broadcasts
are visible without waking everyone. Timers have a minimum interval and a finite delivery limit;
overdue messages coalesce while busy. Pauses, pending interactions and exhausted budgets block
delivery. Failed/quota/blocked participants do not automatically retry.

SQLite commits delivery intent and reserves budget before the provider write. An ambiguous
or interrupted delivery pauses its participant on restart until explicit Resume; it is never
blindly replayed. Provider history imports are idempotent. No wakeups run after the desktop quits.

## Task ownership

A participant owns at most one unfinished claim. Atomic reservations prevent two peers owning
the same task. GitHub write uncertainty retains ownership and pauses the participant. Completion
requires a summary and evidence, written to GitHub before Done. Task creation persists its exact
intent so uncertain network outcomes require reconciliation rather than duplicate creation.

Room-scoped stdio MCP tools are snapshot, post_message, read_task, create_task, claim_task,
finish_task and block_task. Each subprocess binds one room and participant; writes check pauses
and ownership. Codex's per-process approval override applies only to those seven tools.

## Workspaces

Optional worktrees start from committed HEAD and preserve the primary checkout. Worktrees
persist after peer removal and room archive. Merge is user initiated and requires clean checkouts;
conflicts abort the attempted merge and are reported. There are no automatic force operations or
worktree deletions.

## Acceptance

The native acceptance scenario uses two actual providers in one room, automatic continuation
to another task, ownership and GitHub completion proof, shared/directed messages, a timer,
pause/budget stops, relaunch persistence, and an isolated Git worktree. Test-only GitHub fixtures
belong in Project #9. Automated tests additionally cover interrupted delivery recovery and merge
failure behavior. See [the acceptance record](LOCAL_AGENT_ROOM_LIVE_ACCEPTANCE.md).
