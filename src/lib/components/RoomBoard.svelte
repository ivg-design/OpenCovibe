<script lang="ts">
  import { onMount } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import Card from "$lib/components/Card.svelte";
  import Input from "$lib/components/Input.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { groupBoardItems, visibleBoardItems } from "$lib/rooms/board";
  import type { RoomBoard } from "$lib/rooms/types";

  let {
    board,
    loading = false,
    canRefresh = true,
    onrefresh,
  }: {
    board: RoomBoard;
    loading?: boolean;
    canRefresh?: boolean;
    onrefresh: () => void;
  } = $props();

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
                    {#if item.url}
                      <a
                        class="text-sm font-medium text-primary hover:underline"
                        href={item.url}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {item.title}
                      </a>
                    {:else}
                      <p class="text-sm font-medium text-foreground">{item.title}</p>
                    {/if}
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
            </Card>
          {/each}
        </section>
      {/each}
    </div>
  {/if}
</section>
