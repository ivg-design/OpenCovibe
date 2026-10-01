<script lang="ts">
  import { onMount } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { boardTasks, groupBoardItems, visibleBoardItems, type BoardTask } from "$lib/rooms/board";
  import { readRoomTask } from "$lib/rooms/api";
  import { roomParticipantColor } from "$lib/utils/room-participant-colors";
  import type { RoomBoard, RoomClaim, RoomParticipant } from "$lib/rooms/types";

  let {
    board,
    roomId,
    claims = [],
    participants = [],
    loading = false,
    canRefresh = true,
    onrefresh,
  }: {
    board: RoomBoard;
    roomId: string;
    claims?: RoomClaim[];
    participants?: RoomParticipant[];
    loading?: boolean;
    canRefresh?: boolean;
    onrefresh: () => void;
  } = $props();
  type TaskDetails = {
    id: string;
    title: string;
    body: string;
    url: string | null;
    kind: string;
    progress_updates?: { body: string; url: string; created_at: string; author: string }[];
  };
  type Inspection = {
    roomId: string;
    itemId: string;
    loading: boolean;
    details: TaskDetails | null;
    error: string;
  };
  let inspection = $state<Inspection | null>(null);
  let generation = 0;
  let previousRoomId: string | null = null;
  let visibleInspection = $derived(inspection?.roomId === roomId ? inspection : null);
  let titleFilter = $state("");
  let priorityFilter = $state("");
  let agentFilter = $state("");
  let statusFilter = $state("");
  let view = $state<"list" | "board">("list");
  let page = $state(0);
  const pageSize = 40;
  const columnPageSize = 10;
  const tr: typeof t = t;
  let tasks = $derived(boardTasks(board.items, claims, participants));
  let groups = $derived(groupBoardItems(tasks));
  let filtered = $derived(
    visibleBoardItems(tasks, {
      title: titleFilter,
      priority: priorityFilter,
      agent: agentFilter,
      status: statusFilter,
    }) as BoardTask[],
  );
  let shown = $derived(filtered.slice(page * pageSize, (page + 1) * pageSize));
  let filteredGroups = $derived(groupBoardItems(filtered));
  let shownGroups = $derived(
    filteredGroups.map((group) => ({
      ...group,
      items: group.items.slice(page * columnPageSize, (page + 1) * columnPageSize),
    })),
  );
  let pages = $derived(
    Math.max(
      1,
      ...(view === "list"
        ? [Math.ceil(filtered.length / pageSize)]
        : filteredGroups.map((group) => Math.ceil(group.items.length / columnPageSize))),
    ),
  );
  let selectedTask = $derived(tasks.find((item) => item.id === visibleInspection?.itemId));
  let priorities = $derived(
    [
      ...new Set(
        tasks
          .filter((i) => i.kind !== "redacted")
          .map((i) => i.priority)
          .filter((s): s is string => !!s),
      ),
    ].sort(),
  );
  let owners = $derived(
    [
      ...new Set(
        tasks
          .filter((i) => i.kind !== "redacted")
          .flatMap((i) => [i.agent, ...(i.assignees ?? [])])
          .filter((s): s is string => !!s),
      ),
    ].sort(),
  );
  let done = $derived(tasks.filter((i) => /^(done|completed|closed)$/i.test(i.status)).length);
  let drafts = $derived(tasks.filter((i) => i.kind === "draft").length);
  let visibleCount = $derived(tasks.filter((i) => i.kind !== "redacted").length);
  let now = $state(Date.now());
  let stale = $derived(
    !!board.synced_at && now - new Date(board.synced_at).getTime() > 15 * 60 * 1000,
  );
  $effect(() => {
    const currentRoomId = roomId;
    if (previousRoomId !== null && previousRoomId !== currentRoomId) {
      generation++;
      inspection = null;
      titleFilter = priorityFilter = agentFilter = statusFilter = "";
    }
    previousRoomId = currentRoomId;
  });
  $effect(() => {
    void titleFilter;
    void priorityFilter;
    void agentFilter;
    void statusFilter;
    void view;
    void roomId;
    page = 0;
  });
  $effect(() => {
    if (page >= pages) page = pages - 1;
  });
  onMount(() => {
    const timer = window.setInterval(() => (now = Date.now()), 60_000);
    return () => window.clearInterval(timer);
  });
  async function inspectTask(itemId: string) {
    const request = ++generation;
    const requestRoomId = roomId;
    const cached = tasks.find((i) => i.id === itemId);
    inspection = {
      roomId: requestRoomId,
      itemId,
      loading: true,
      details: cached?.body ? { ...cached, body: cached.body } : null,
      error: "",
    };
    try {
      const details = await readRoomTask(requestRoomId, itemId);
      if (request === generation && roomId === requestRoomId && inspection?.itemId === itemId)
        inspection = { roomId: requestRoomId, itemId, loading: false, details, error: "" };
    } catch (cause) {
      if (request === generation && roomId === requestRoomId && inspection?.itemId === itemId)
        inspection = { ...inspection!, loading: false, error: String(cause) };
    }
  }
  function closeInspection() {
    generation++;
    inspection = null;
  }
  function formatDate(value: string | null): string {
    if (!value) return tr("room_boardNeverSynced");
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }
  function statusClass(status: string): string {
    if (/block|attention/i.test(status)) return "blocked";
    if (/progress|active/i.test(status)) return "active";
    if (/review/i.test(status)) return "review";
    if (/^(done|completed|closed)$/i.test(status)) return "done";
    return "pending";
  }
  function ownerColor(item: BoardTask): string {
    return item.ownerId
      ? roomParticipantColor(participants, item.ownerId)
      : "hsl(var(--muted-foreground))";
  }
</script>

<section class="project-workspace" aria-label={tr("room_boardTitle")}>
  <header class="board-heading">
    <div class="min-w-0">
      <h2 class="text-base font-semibold text-foreground">{tr("room_boardTitle")}</h2>
      <p class="text-xs text-muted-foreground">
        {tr("room_lastSynced", { time: formatDate(board.synced_at) })}
      </p>
    </div>
    <div class="view-actions">
      <div class="view-switch" role="group" aria-label="Project layout">
        <button type="button" aria-pressed={view === "list"} onclick={() => (view = "list")}
          >List</button
        ><button type="button" aria-pressed={view === "board"} onclick={() => (view = "board")}
          >Board</button
        >
      </div>
      <Button variant="outline" size="sm" {loading} disabled={!canRefresh} onclick={onrefresh}
        >{tr("room_refreshBoard")}</Button
      >
    </div>
  </header>
  {#if board.error}<p class="board-warning text-destructive" role="status">
      {tr("room_boardSyncError", { error: board.error })}
    </p>{:else if stale}<p class="board-warning" role="status">{tr("room_boardStale")}</p>{/if}
  <div class="project-progress" role="img" aria-label={`${done} of ${visibleCount} tasks complete`}>
    <div class="progress-track">
      {#each groups.filter((g) => g.status !== "__hidden__") as group (group.status)}<span
          class={`progress-segment ${statusClass(group.status)}`}
          style:flex-grow={group.items.length}
          title={`${group.status}: ${group.items.length}`}
        ></span>{/each}
    </div>
    <span class="text-xs text-muted-foreground"
      >{done}/{visibleCount} complete{#if drafts}
        · {drafts} draft{drafts === 1 ? "" : "s"}{/if}</span
    >
  </div>
  <div class="status-filters" aria-label="Filter by task status">
    <button type="button" aria-pressed={!statusFilter} onclick={() => (statusFilter = "")}
      >All <span>{tasks.length}</span></button
    >{#each groups as group (group.status)}<button
        type="button"
        aria-pressed={statusFilter === group.status}
        onclick={() => (statusFilter = statusFilter === group.status ? "" : group.status)}
        ><i class={`status-dot ${statusClass(group.status)}`} aria-hidden="true"
        ></i>{group.status === "__hidden__" ? tr("room_hiddenItems") : group.status}<span
          >{group.items.length}</span
        ></button
      >{/each}
  </div>
  <div class="board-filters">
    <label class="search-field"
      ><span class="sr-only">Search tasks and references</span><input
        type="search"
        bind:value={titleFilter}
        placeholder="Search tasks, issues, PRs, labels…"
      /></label
    ><label
      ><span class="sr-only">{tr("room_filterAgent")}</span><select bind:value={agentFilter}
        ><option value="">All owners</option>{#each owners as owner}<option value={owner}
            >{owner}</option
          >{/each}</select
      ></label
    ><label
      ><span class="sr-only">{tr("room_filterPriority")}</span><select bind:value={priorityFilter}
        ><option value="">All priorities</option>{#each priorities as priority}<option
            value={priority}>{priority}</option
          >{/each}</select
      ></label
    >
  </div>
  <details class="tracking-rules text-xs text-muted-foreground">
    <summary>Room-wide task tracking rules</summary>
    <p class="mt-2">
      Agents create repository issues linked to this project, with a clear outcome, work plan,
      acceptance criteria, progress, and references. They claim tasks before working, record
      meaningful progress and blockers, and attach commits, pull requests, and test evidence as
      these become available. Older drafts are enriched and converted as agents work on them. Room
      instructions can add project-specific requirements.
    </p>
  </details>
  <div class="board-content" class:has-detail={!!visibleInspection}>
    <div class="task-browser">
      {#if filtered.length === 0}<p class="py-6 text-sm text-muted-foreground">
          {tr("room_boardEmpty")}
        </p>
      {:else if view === "list"}<div class="task-list" aria-label="Project task list">
          {#each shown as item (item.id)}{#if item.kind === "redacted"}<p
                class="task-row text-sm text-muted-foreground"
              >
                {tr("room_hiddenItem")}
              </p>{:else}<button
                type="button"
                class="task-row"
                class:selected={visibleInspection?.itemId === item.id}
                aria-expanded={visibleInspection?.itemId === item.id}
                onclick={() => void inspectTask(item.id)}
                ><span class="task-main"
                  ><span class="task-title"
                    >{#if item.number}<span class="task-number">#{item.number}</span>
                    {/if}{item.title}</span
                  ><span class="task-context"
                    >{item.claim?.summary ||
                      item.labels?.join(" · ") ||
                      (item.kind === "draft"
                        ? "Draft — not linked to a repository issue"
                        : item.linked_prs?.length
                          ? `${item.linked_prs.length} linked pull request(s)`
                          : item.kind === "pull_request"
                            ? "Pull request"
                            : "Repository issue")}</span
                  ></span
                ><span class="task-owner"
                  ><i class="owner-dot" style:background={ownerColor(item)} aria-hidden="true"
                  ></i>{item.agent || item.assignees?.join(", ") || "Unassigned"}</span
                ><span class="task-state"
                  ><i class={`status-dot ${statusClass(item.status)}`} aria-hidden="true"
                  ></i>{item.status}</span
                >{#if item.priority}<span class="task-priority">{item.priority}</span>{/if}</button
              >{/if}{/each}
        </div>
      {:else}<div class="task-board">
          {#each shownGroups as group (group.status)}<section
              class="task-column"
              aria-label={group.status}
            >
              <h3 class="column-heading">
                <i class={`status-dot ${statusClass(group.status)}`} aria-hidden="true"
                ></i>{group.status === "__hidden__" ? tr("room_hiddenItems") : group.status}<span
                  >{filtered.filter((i) => i.status === group.status).length}</span
                >
              </h3>
              {#each group.items as raw (raw.id)}{@const item =
                  raw as BoardTask}{#if item.kind === "redacted"}<p
                    class="text-xs text-muted-foreground"
                  >
                    {tr("room_hiddenItem")}
                  </p>{:else}<button
                    type="button"
                    class="task-tile"
                    class:selected={visibleInspection?.itemId === item.id}
                    aria-expanded={visibleInspection?.itemId === item.id}
                    onclick={() => void inspectTask(item.id)}
                    ><span class="task-title"
                      >{#if item.number}<span class="task-number">#{item.number}</span>
                      {/if}{item.title}</span
                    ><span class="task-context"
                      >{item.claim?.summary ||
                        item.labels?.join(" · ") ||
                        (item.kind === "draft" ? "Draft" : "Repository issue")}</span
                    ><span class="tile-footer"
                      ><span class="task-owner"
                        ><i class="owner-dot" style:background={ownerColor(item)} aria-hidden="true"
                        ></i>{item.agent || item.assignees?.join(", ") || "Unassigned"}</span
                      >{#if item.priority}<span class="task-priority">{item.priority}</span
                        >{/if}</span
                    ></button
                  >{/if}{/each}
            </section>{/each}
        </div>{/if}
      <div class="board-pagination" aria-live="polite">
        <span
          >{#if view === "board"}{shownGroups.reduce(
              (count, group) => count + group.items.length,
              0,
            )} shown · {filtered.length} tasks{:else}{filtered.length
              ? page * pageSize + 1
              : 0}–{Math.min((page + 1) * pageSize, filtered.length)} of {filtered.length} tasks{/if}</span
        >
        <div class="view-actions">
          <Button variant="ghost" size="sm" disabled={page === 0} onclick={() => page--}
            >Previous</Button
          ><span>{page + 1}/{pages}</span><Button
            variant="ghost"
            size="sm"
            disabled={page + 1 >= pages}
            onclick={() => page++}>Next</Button
          >
        </div>
      </div>
    </div>
    {#if visibleInspection}<aside
        class="task-detail"
        aria-label={tr("room_taskDetailsTitle")}
        aria-live="polite"
      >
        <header class="detail-heading">
          <h3 class="text-sm font-semibold">
            {visibleInspection.details?.title ?? selectedTask?.title ?? tr("room_taskDetailsTitle")}
          </h3>
          <button
            type="button"
            class="close-detail"
            aria-label={tr("room_closeTaskDetails")}
            onclick={closeInspection}>Close</button
          >
        </header>
        {#if selectedTask}<div class="detail-meta">
            <span class="task-state"
              ><i class={`status-dot ${statusClass(selectedTask.status)}`} aria-hidden="true"
              ></i>{selectedTask.status}</span
            ><span class="task-owner"
              ><i class="owner-dot" style:background={ownerColor(selectedTask)} aria-hidden="true"
              ></i>{selectedTask.agent || selectedTask.assignees?.join(", ") || "Unassigned"}</span
            >{#if selectedTask.priority}<span>{selectedTask.priority}</span>{/if}
          </div>{/if}
        {#if selectedTask?.claim?.summary}<section class="detail-section">
            <h4>Latest room progress</h4>
            <MarkdownContent text={selectedTask.claim.summary} lazy={false} />
          </section>{/if}{#if selectedTask?.claim?.evidence}<section class="detail-section">
            <h4>Evidence</h4>
            <MarkdownContent text={selectedTask.claim.evidence} lazy={false} />
          </section>{/if}
        {#if visibleInspection.loading}<p class="text-sm text-muted-foreground" role="status">
            {tr("room_taskLoading")}
          </p>{/if}{#if visibleInspection.error}<p class="text-sm text-destructive" role="alert">
            {tr("room_taskLoadError", { error: visibleInspection.error })}
          </p>{/if}{#if visibleInspection.details}<section class="detail-section">
            <h4>Task description</h4>
            <MarkdownContent
              text={visibleInspection.details.body || tr("room_taskBodyEmpty")}
              lazy={false}
            />
          </section>{/if}
        {#if selectedTask?.kind === "draft"}<p class="text-xs text-muted-foreground">
            This older entry is a project draft. Ask a room agent to convert it to a repository
            issue and fill in its outcome, acceptance criteria, progress, and references.
          </p>{/if}{#if visibleInspection.details?.url || selectedTask?.url}<a
            class="text-sm text-primary hover:underline"
            href={visibleInspection.details?.url || selectedTask?.url || undefined}
            target="_blank"
            rel="noreferrer">{tr("room_openGitHub")}</a
          >{/if}{#if selectedTask?.linked_prs?.length}<section class="detail-section">
            <h4>Linked pull requests</h4>
            {#each selectedTask.linked_prs as pr}<a
                class="block text-sm text-primary hover:underline"
                href={pr}
                target="_blank"
                rel="noreferrer">{pr}</a
              >{/each}
          </section>{/if}
        {#if visibleInspection.details?.progress_updates?.length}<section class="detail-section">
            <h4>Progress history</h4>
            {#each visibleInspection.details.progress_updates as update}
              <details class="mb-2">
                <summary>{update.author} · {formatDate(update.created_at)}</summary><MarkdownContent
                  text={update.body}
                  lazy={false}
                /><a
                  href={update.url}
                  target="_blank"
                  rel="noreferrer"
                  class="text-xs text-primary hover:underline">Open update on GitHub</a
                >
              </details>
            {/each}
          </section>{/if}
      </aside>{/if}
  </div>
</section>

<style>
  .project-workspace {
    container-type: inline-size;
    min-width: 0;
    display: grid;
    gap: 0.65rem;
  }
  .board-heading,
  .view-actions,
  .detail-heading,
  .board-pagination {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    flex-wrap: wrap;
    min-width: 0;
  }
  .view-switch {
    display: flex;
    border: 1px solid hsl(var(--border));
    border-radius: 0.35rem;
    overflow: hidden;
  }
  .view-switch button,
  .status-filters button,
  .close-detail {
    padding: 0.35rem 0.6rem;
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
  }
  .view-switch button[aria-pressed="true"],
  .status-filters button[aria-pressed="true"] {
    color: hsl(var(--foreground));
    background: hsl(var(--accent));
  }
  .board-warning {
    padding: 0.5rem 0.65rem;
    font-size: 0.8rem;
    background: color-mix(in srgb, hsl(var(--destructive)) 7%, transparent);
    overflow-wrap: anywhere;
  }
  .project-progress {
    display: flex;
    gap: 0.65rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .progress-track {
    display: flex;
    height: 0.35rem;
    background: hsl(var(--muted));
    flex: 1;
    min-width: 5rem;
    gap: 2px;
    overflow: hidden;
    border-radius: 3px;
  }
  .progress-segment {
    min-width: 2px;
  }
  .blocked {
    background: hsl(var(--destructive));
  }
  .active {
    background: #5677c8;
  }
  .review {
    background: #bd8250;
  }
  .done {
    background: hsl(var(--primary));
  }
  .pending {
    background: hsl(var(--muted-foreground));
    opacity: 0.6;
  }
  .status-filters {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }
  .status-filters button {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
    border-radius: 0.3rem;
  }
  .status-filters button span {
    font-variant-numeric: tabular-nums;
    color: hsl(var(--muted-foreground));
  }
  .status-dot,
  .owner-dot {
    display: inline-block;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .board-filters {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .board-filters label {
    min-width: 0;
    flex: 1 1 9rem;
  }
  .board-filters .search-field {
    flex: 3 1 16rem;
  }
  .board-filters input,
  .board-filters select {
    width: 100%;
    min-width: 0;
    height: 2rem;
    padding: 0.3rem 0.5rem;
    background: hsl(var(--background));
    border: 1px solid hsl(var(--input));
    border-radius: 0.3rem;
    font-size: 0.8rem;
  }
  .board-content {
    min-width: 0;
    display: grid;
    gap: 0.8rem;
    align-items: start;
  }
  .task-browser {
    min-width: 0;
  }
  .task-list {
    display: grid;
  }
  .task-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(5rem, 0.25fr) minmax(5rem, 0.22fr) 3rem;
    align-items: center;
    gap: 0.65rem;
    padding: 0.55rem 0.5rem;
    text-align: left;
    width: 100%;
    min-width: 0;
    border-bottom: 1px solid hsl(var(--border));
  }
  .task-row:hover,
  .task-tile:hover,
  .selected {
    background: hsl(var(--accent));
  }
  .task-main {
    min-width: 0;
    display: grid;
    gap: 0.2rem;
  }
  .task-title {
    font-size: 0.85rem;
    color: hsl(var(--foreground));
    word-break: normal;
    overflow-wrap: anywhere;
  }
  .task-number {
    margin-right: 0.4rem;
    color: hsl(var(--muted-foreground));
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .task-context {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 0.72rem;
    color: hsl(var(--muted-foreground));
    word-break: normal;
    overflow-wrap: anywhere;
  }
  .task-owner,
  .task-state {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
    min-width: 0;
    font-size: 0.73rem;
    color: hsl(var(--muted-foreground));
    word-break: normal;
    overflow-wrap: anywhere;
  }
  .task-priority {
    font-size: 0.73rem;
    color: hsl(var(--muted-foreground));
    white-space: nowrap;
  }
  .task-board {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 15rem), 1fr));
    gap: 0.7rem;
    min-width: 0;
  }
  .task-column {
    min-width: 0;
    display: grid;
    align-content: start;
    gap: 0.35rem;
  }
  .column-heading {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
    padding: 0.4rem 0.2rem;
  }
  .column-heading span {
    margin-left: auto;
  }
  .task-tile {
    display: grid;
    gap: 0.4rem;
    padding: 0.65rem;
    text-align: left;
    border: 1px solid hsl(var(--border));
    border-radius: 0.35rem;
    min-width: 0;
  }
  .tile-footer {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .board-pagination {
    padding-top: 0.55rem;
    font-size: 0.73rem;
    color: hsl(var(--muted-foreground));
  }
  .task-detail {
    grid-row: 1;
    min-width: 0;
    display: grid;
    gap: 0.65rem;
    border-top: 1px solid hsl(var(--border));
    padding-top: 0.7rem;
    overflow-wrap: anywhere;
  }
  .detail-heading {
    align-items: start;
  }
  .detail-heading h3 {
    flex: 1;
    min-width: 8rem;
  }
  .detail-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
    font-size: 0.75rem;
  }
  .detail-section {
    min-width: 0;
    font-size: 0.8rem;
  }
  .detail-section h4 {
    color: hsl(var(--muted-foreground));
    font-size: 0.72rem;
    font-weight: 600;
    margin-bottom: 0.4rem;
  }
  @container (min-width: 65rem) {
    .board-content.has-detail {
      grid-template-columns: minmax(0, 1fr) minmax(19rem, 0.6fr);
    }
    .task-detail {
      grid-row: auto;
      position: sticky;
      top: 0;
      border-top: 0;
      border-left: 1px solid hsl(var(--border));
      padding: 0 0 0 0.8rem;
      max-height: 70vh;
      overflow-y: auto;
    }
  }
  @container (max-width: 40rem) {
    .task-row {
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 0.35rem;
    }
    .task-main {
      grid-column: 1 / -1;
    }
    .task-owner {
      grid-column: 1;
    }
    .task-priority {
      grid-column: 2;
      text-align: right;
    }
    .task-state {
      grid-column: 2;
      grid-row: 2;
    }
  }
</style>
