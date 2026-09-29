<script lang="ts">
  import { onMount } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import Card from "$lib/components/Card.svelte";
  import Input from "$lib/components/Input.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { groupBoardItems, visibleBoardItems } from "$lib/rooms/board";
  import { readRoomTask } from "$lib/rooms/api";
  import type { RoomBoard } from "$lib/rooms/types";

  let {
    board,
    roomId,
    loading = false,
    canRefresh = true,
    onrefresh,
  }: {
    board: RoomBoard;
    roomId: string;
    loading?: boolean;
    canRefresh?: boolean;
    onrefresh: () => void;
  } = $props();

  type TaskDetails = { id: string; title: string; body: string; url: string | null; kind: string };
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
  $effect(() => {
    const currentRoomId = roomId;
    if (previousRoomId !== null && previousRoomId !== currentRoomId) {
      generation++;
      inspection = null;
    }
    previousRoomId = currentRoomId;
  });
  let titleFilter = $state("");
  let priorityFilter = $state("");
  let agentFilter = $state("");
  const tr: typeof t = t;
  const staleAfterMs = 15 * 60 * 1000;

  let filteredGroups = $derived(
    groupBoardItems(
      visibleBoardItems(board.items, {
        title: titleFilter,
        priority: priorityFilter,
        agent: agentFilter,
      }),
    ),
  );
  let now = $state(Date.now());
  let stale = $derived(
    !!board.synced_at && now - new Date(board.synced_at).getTime() > staleAfterMs,
  );
  onMount(() => {
    const timer = window.setInterval(() => (now = Date.now()), 60_000);
    return () => window.clearInterval(timer);
  });
  async function inspectTask(itemId: string) {
    if (inspection?.roomId === roomId && inspection.itemId === itemId) {
      generation++;
      inspection = null;
      return;
    }
    const request = ++generation;
    const requestRoomId = roomId;
    inspection = { roomId: requestRoomId, itemId, loading: true, details: null, error: "" };
    try {
      const details = await readRoomTask(requestRoomId, itemId);
      if (request === generation && roomId === requestRoomId && inspection?.itemId === itemId) {
        inspection = { roomId: requestRoomId, itemId, loading: false, details, error: "" };
      }
    } catch (cause) {
      if (request === generation && roomId === requestRoomId && inspection?.itemId === itemId) {
        inspection = {
          roomId: requestRoomId,
          itemId,
          loading: false,
          details: null,
          error: String(cause),
        };
      }
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
</script>

<section class="space-y-3" aria-label={tr("room_boardTitle")}>
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div>
      <h2 class="text-base font-semibold text-foreground">{tr("room_boardTitle")}</h2>
      <p class="text-xs text-muted-foreground">
        {tr("room_lastSynced", { time: formatDate(board.synced_at) })}
      </p>
    </div>
    <Button variant="outline" size="sm" {loading} disabled={!canRefresh} onclick={onrefresh}>
      {tr("room_refreshBoard")}
    </Button>
  </div>

  {#if board.error}
    <div
      class="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive"
      role="status"
    >
      {tr("room_boardSyncError", { error: board.error })}
    </div>
  {:else if stale}
    <div
      class="rounded-md border border-amber-500/40 bg-amber-500/5 px-3 py-2 text-sm text-foreground"
      role="status"
    >
      {tr("room_boardStale")}
    </div>
  {/if}

  <div class="grid gap-2 sm:grid-cols-3">
    <label class="space-y-1 text-xs text-muted-foreground">
      <span>{tr("room_filterTitle")}</span>
      <Input bind:value={titleFilter} />
    </label>
    <label class="space-y-1 text-xs text-muted-foreground">
      <span>{tr("room_filterPriority")}</span>
      <Input bind:value={priorityFilter} />
    </label>
    <label class="space-y-1 text-xs text-muted-foreground">
      <span>{tr("room_filterAgent")}</span>
      <Input bind:value={agentFilter} />
    </label>
  </div>

  {#if filteredGroups.length === 0}
    <Card variant="subtle" class="p-5 text-sm text-muted-foreground">{tr("room_boardEmpty")}</Card>
  {:else}
    <div class="grid gap-3 xl:grid-cols-3">
      {#each filteredGroups as group (group.status)}
        {@const hidden = group.status === "__hidden__"}
        <section
          class="min-w-0 space-y-2"
          aria-label={hidden ? tr("room_hiddenItems") : group.status}
        >
          <h3
            class="flex items-center justify-between text-xs font-semibold uppercase tracking-wide text-muted-foreground"
          >
            <span>{hidden ? tr("room_hiddenItems") : group.status}</span>
            <span>{group.items.length}</span>
          </h3>
          {#each group.items as item (item.id)}
            <Card class="p-3">
              {#if item.kind === "redacted"}
                <p class="text-sm text-muted-foreground">{tr("room_hiddenItem")}</p>
              {:else}
                <div class="flex items-start justify-between gap-2">
                  <div class="min-w-0">
                    <button
                      type="button"
                      class="text-left text-sm font-medium text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      aria-expanded={visibleInspection?.itemId === item.id}
                      aria-label={`${tr("room_inspectTask")}: ${item.title}`}
                      onclick={() => void inspectTask(item.id)}
                    >
                      {item.title}
                    </button>
                    {#if item.url}<a
                        class="ml-2 text-xs text-muted-foreground hover:text-primary hover:underline"
                        href={item.url}
                        target="_blank"
                        rel="noreferrer">{tr("room_openGitHub")}</a
                      >{/if}
                    <p class="mt-1 text-xs text-muted-foreground">
                      {item.kind === "pull_request"
                        ? tr("room_pullRequest")
                        : item.kind === "issue"
                          ? tr("room_issue")
                          : tr("room_draft")}
                    </p>
                  </div>
                  {#if item.priority}<span class="shrink-0 rounded bg-muted px-2 py-0.5 text-xs"
                      >{item.priority}</span
                    >{/if}
                </div>
                {#if item.agent}<p class="mt-2 text-xs text-muted-foreground">
                    {tr("room_agent", { name: item.agent })}
                  </p>{/if}
              {/if}
              {#if visibleInspection?.itemId === item.id}
                <div class="mt-3 space-y-2 border-t pt-3" aria-live="polite">
                  <div class="flex items-start justify-between gap-3">
                    <div class="min-w-0">
                      <h4 class="text-sm font-semibold text-foreground">
                        {visibleInspection.details?.title ?? tr("room_taskDetailsTitle")}
                      </h4>
                      {#if visibleInspection.details}<p class="mt-1 text-xs text-muted-foreground">
                          {visibleInspection.details.kind}
                        </p>{/if}
                    </div>
                    <button
                      type="button"
                      class="shrink-0 rounded px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
                      aria-label={tr("room_closeTaskDetails")}
                      onclick={closeInspection}>{tr("common_cancel")}</button
                    >
                  </div>
                  {#if visibleInspection.loading}<p
                      class="text-sm text-muted-foreground"
                      role="status"
                    >
                      {tr("room_taskLoading")}
                    </p>
                  {:else if visibleInspection.error}<p
                      class="text-sm text-destructive"
                      role="alert"
                    >
                      {tr("room_taskLoadError", { error: visibleInspection.error })}
                    </p>
                  {:else if visibleInspection.details}<div
                      class="max-h-96 overflow-y-auto rounded-md bg-background/70 p-2"
                    >
                      <MarkdownContent
                        text={visibleInspection.details.body || tr("room_taskBodyEmpty")}
                        lazy={false}
                      />
                    </div>{/if}
                </div>
              {/if}
            </Card>
          {/each}
        </section>
      {/each}
    </div>
  {/if}
</section>
