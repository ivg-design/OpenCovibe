<script lang="ts">
  import { untrack } from "svelte";
  import { getTransport } from "$lib/transport";
  import { t } from "$lib/i18n/index.svelte";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { modalFocus } from "$lib/utils/modal-focus";
  import { fmtRelative } from "$lib/i18n/format";
  import { cwdDisplayLabel } from "$lib/utils/format";
  import {
    sessionTitle,
    sessionProject,
    sessionPreview,
    isBackgroundSession,
    filterSessions,
  } from "$lib/utils/session-browser";
  import type { CliSessionSummary, DiscoverResult, ImportResult, SyncResult } from "$lib/types";

  function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    return getTransport().invoke<T>(cmd, args);
  }

  let {
    cwd,
    onclose,
    onimported,
  }: {
    cwd: string;
    onclose: () => void;
    onimported: (runId: string) => void;
  } = $props();

  let sessions: CliSessionSummary[] = $state([]);
  let totalSessions = $state(0);
  let truncated = $state(false);
  let loading = $state(true);
  let discoverySequence = 0;
  let searchQuery = $state("");
  let scopeCwd = $state(untrack(() => cwd));
  let includeSubagents = $state(false);
  let includeBackground = $state(false);
  let includeArchived = $state(false);
  let projectChoices = $state<{ path: string; label: string; count: number }[]>([]);
  let choosingFolder = $state(false);
  let visibleLimit = $state(25);
  let importingId = $state<string | null>(null);
  let error = $state<string | null>(null);
  let warning = $state<string | null>(null);
  let importingAll = $state(false);
  // Agent toggle: localStorage-backed, defaults to "claude" for new users.
  let agent = $state<"claude" | "codex">(
    (typeof localStorage !== "undefined" &&
      (localStorage.getItem("cliImport_agent") as "claude" | "codex")) ||
      "claude",
  );

  function setAgent(next: "claude" | "codex") {
    if (next === agent || importingId || importingAll) return;
    agent = next;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("cliImport_agent", next);
    }
  }

  // ── Project filter ──
  const isShowAll = $derived(!scopeCwd || scopeCwd === "/");

  const filtered = $derived(
    filterSessions(
      sessions,
      agent === "codex" ? "" : searchQuery,
      includeSubagents,
      includeArchived,
      includeBackground,
    ),
  );

  async function chooseFolder() {
    choosingFolder = true;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({
        directory: true,
        title: t("layout_selectProjectFolder"),
        defaultPath: scopeCwd && scopeCwd !== "/" ? scopeCwd : undefined,
      });
      if (typeof path === "string") {
        scopeCwd = path;
      }
    } catch {
      error = t("room_folderPickerError");
    } finally {
      choosingFolder = false;
    }
  }

  const newCount = $derived(filtered.filter((s) => !s.alreadyImported).length);

  /** Effective cwd for import — use the session's own cwd */
  function importCwd(session: CliSessionSummary): string {
    return session.cwd || cwd;
  }

  // ── Load sessions on mount ──

  $effect(() => {
    const sourceCwd = scopeCwd;
    const sourceAgent = agent;
    const subagents = includeSubagents,
      archived = includeArchived;
    const query = sourceAgent === "codex" ? searchQuery : "";
    const timer = window.setTimeout(
      () =>
        untrack(() => void discoverSessions(sourceCwd, sourceAgent, subagents, archived, query)),
      query ? 250 : 0,
    );
    return () => {
      window.clearTimeout(timer);
      discoverySequence++;
    };
  });

  async function discoverSessions(
    sourceCwd = scopeCwd,
    sourceAgent = agent,
    subagents = includeSubagents,
    archived = includeArchived,
    query = searchQuery,
  ) {
    const sequence = ++discoverySequence;
    loading = true;
    visibleLimit = 25;
    sessions = [];
    totalSessions = 0;
    truncated = false;
    error = null;
    dbg("cli-browser", "discovering sessions", { cwd });
    try {
      const result = await invoke<DiscoverResult>("discover_cli_sessions", {
        cwd: sourceCwd,
        agent: sourceAgent,
        includeSubagents: subagents,
        includeArchived: archived,
        query: sourceAgent === "codex" ? query : "",
      });
      if (sequence !== discoverySequence) return;
      sessions = result.sessions;
      totalSessions = result.total;
      truncated = result.truncated;
      if (!sourceCwd || sourceCwd === "/") {
        const counts = new Map<string, number>();
        for (const session of result.sessions) {
          const path = sessionProject(session);
          if (path) counts.set(path, (counts.get(path) ?? 0) + 1);
        }
        projectChoices = [...counts]
          .sort((a, b) => a[0].localeCompare(b[0]))
          .map(([path, count]) => ({ path, label: cwdDisplayLabel(path), count }));
      }
      dbg("cli-browser", "discovered", {
        count: sessions.length,
        total: totalSessions,
        truncated,
      });
    } catch (e) {
      const msg = String(e);
      dbgWarn("cli-browser", "discover failed", msg);
      if (sequence === discoverySequence) error = msg;
    } finally {
      if (sequence === discoverySequence) loading = false;
    }
  }

  async function importSession(session: CliSessionSummary) {
    if (importingId) return;
    importingId = session.sessionId;
    error = null;
    warning = null;
    const sessionCwd = importCwd(session);
    dbg("cli-browser", "importing session", { sessionId: session.sessionId, cwd: sessionCwd });
    try {
      const result = await invoke<ImportResult>("import_cli_session", {
        sessionId: session.sessionId,
        cwd: sessionCwd,
        agent,
      });
      dbg("cli-browser", "import success", { runId: result.runId, events: result.eventsImported });
      if (result.usageIncomplete) {
        warning = t("cliSync_usageIncomplete");
      }
      await discoverSessions();
      onimported(result.runId);
    } catch (e) {
      const msg = String(e);
      dbgWarn("cli-browser", "import failed", msg);
      error = msg;
    } finally {
      importingId = null;
    }
  }

  async function syncSession(runId: string) {
    importingId = runId;
    error = null;
    warning = null;
    dbg("cli-browser", "syncing session", { runId });
    try {
      const result = await invoke<SyncResult>("sync_cli_session", { runId });
      dbg("cli-browser", "sync success", {
        newEvents: result.newEvents,
        newRollouts: result.newRollouts?.length ?? 0,
      });
      if (result.usageIncomplete) {
        warning = t("cliSync_usageIncomplete");
      } else if (result.newRollouts && result.newRollouts.length > 0) {
        warning = t("cliSync_newRollouts", { count: String(result.newRollouts.length) });
      }
      await discoverSessions();
    } catch (e) {
      const msg = String(e);
      dbgWarn("cli-browser", "sync failed", msg);
      error = msg;
    } finally {
      importingId = null;
    }
  }

  async function importAllNew() {
    const newSessions = filtered.filter((s) => !s.alreadyImported);
    if (newSessions.length === 0) return;
    importingAll = true;
    error = null;
    warning = null;
    dbg("cli-browser", "importing all new", { count: newSessions.length });
    let lastRunId: string | null = null;
    let importedCount = 0;
    try {
      for (const s of newSessions) {
        importingId = s.sessionId;
        const sessionCwd = importCwd(s);
        const result = await invoke<ImportResult>("import_cli_session", {
          sessionId: s.sessionId,
          cwd: sessionCwd,
          agent,
        });
        dbg("cli-browser", "imported", { sessionId: s.sessionId, runId: result.runId });
        lastRunId = result.runId;
        importedCount++;
        if (result.usageIncomplete) {
          warning = t("cliSync_usageIncomplete");
        }
      }
      await discoverSessions();
      if (lastRunId) {
        dbg("cli-browser", "import-all done, navigating", { importedCount, lastRunId });
        onimported(lastRunId);
      }
    } catch (e) {
      const msg = String(e);
      dbgWarn("cli-browser", "import-all failed", msg);
      error = msg;
      await discoverSessions().catch(() => {});
    } finally {
      importingId = null;
      importingAll = false;
    }
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onclose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in"
  role="dialog"
  aria-modal="true"
  aria-label={agent === "codex" ? t("cliSync_title_codex") : t("cliSync_title_claude")}
  tabindex="-1"
  use:modalFocus
  onclick={handleBackdropClick}
  onkeydown={handleKeydown}
>
  <div
    class="relative flex max-h-[85dvh] min-h-0 min-w-0 w-[calc(100%-2rem)] max-w-4xl flex-col rounded-xl border border-border bg-background shadow-2xl animate-slide-up"
  >
    <!-- Header -->
    <div class="border-b border-border px-4 py-4 sm:px-6">
      <div class="flex min-w-0 items-start justify-between gap-2">
        <div class="min-w-0">
          <h2 class="text-base font-semibold text-foreground">
            {agent === "codex" ? t("cliSync_title_codex") : t("cliSync_title_claude")}
          </h2>
          <p class="mt-0.5 break-words text-xs text-muted-foreground">
            {#if isShowAll}
              {t("cliSync_allProjects")} &middot;
              {#if truncated}
                {#if agent === "codex"}
                  {t("cliSync_foundTruncated_codex", {
                    threads: String(sessions.length),
                    files: String(totalSessions),
                  })}
                {:else}
                  {t("cliSync_foundTruncated", {
                    shown: String(sessions.length),
                    total: String(totalSessions),
                  })}
                {/if}
              {:else}
                {t("cliSync_found", { count: String(sessions.length) })}
              {/if}
            {:else}
              {scopeCwd} &middot;
              {#if truncated && agent === "codex"}
                {t("cliSync_foundTruncated_codex_filtered", {
                  threads: String(sessions.length),
                  files: String(totalSessions),
                })}
              {:else if truncated}
                {t("cliSync_foundTruncated", {
                  shown: String(sessions.length),
                  total: String(totalSessions),
                })}
              {:else}
                {t("cliSync_found", { count: String(sessions.length) })}
              {/if}
            {/if}
          </p>
        </div>
        <button
          class="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          onclick={onclose}
          aria-label="Close"
        >
          <svg
            class="h-5 w-5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"><path d="M18 6 6 18M6 6l12 12" /></svg
          >
        </button>
      </div>

      <!-- Agent toggle -->
      <div class="mt-3 inline-flex rounded-md border border-border p-0.5">
        <button
          class="rounded px-3 py-1 text-xs font-medium transition-colors
            {agent === 'claude'
            ? 'bg-accent text-accent-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => setAgent("claude")}
        >
          {t("cliImport_agent_claude")}
        </button>
        <button
          class="rounded px-3 py-1 text-xs font-medium transition-colors
            {agent === 'codex'
            ? 'bg-accent text-accent-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => setAgent("codex")}
        >
          {t("cliImport_agent_codex")}
        </button>
      </div>

      <div class="mt-3 flex flex-wrap items-end gap-2">
        <label class="min-w-0 flex-1 space-y-1 text-xs text-muted-foreground">
          <span>{t("cliSync_filterProject")}</span>
          <select
            class="min-h-9 w-full min-w-0 rounded-md border border-border bg-background px-2 text-sm"
            value={scopeCwd === "/" ? "" : scopeCwd}
            disabled={!!importingId || importingAll}
            onchange={(event) => {
              scopeCwd = event.currentTarget.value;
            }}
          >
            <option value="">{t("cliSync_allProjects")}</option>
            {#if scopeCwd && scopeCwd !== "/" && !projectChoices.some((p) => p.path === scopeCwd)}<option
                value={scopeCwd}>{cwdDisplayLabel(scopeCwd)}</option
              >{/if}
            {#each projectChoices as proj (proj.path)}<option value={proj.path}>{proj.label}</option
              >{/each}
          </select>
        </label>
        <button
          class="min-h-9 rounded-md border border-border px-3 text-xs"
          disabled={choosingFolder || !!importingId || importingAll}
          onclick={chooseFolder}>{t("room_chooseFolder")}</button
        >
      </div>
      {#if agent === "codex"}<div class="mt-2 flex flex-wrap gap-3 text-xs text-muted-foreground">
          <label class="flex flex-wrap items-center gap-2"
            ><input
              type="checkbox"
              bind:checked={includeSubagents}
              disabled={!!importingId || importingAll}
            />{t("cliSync_showSubagents")}</label
          >
          <label class="flex flex-wrap items-center gap-2"
            ><input
              type="checkbox"
              bind:checked={includeArchived}
              disabled={!!importingId || importingAll}
            />{t("cliSync_showArchived")}</label
          >
        </div>{/if}
      {#if agent === "claude"}
        <label class="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <input
            type="checkbox"
            bind:checked={includeBackground}
            disabled={!!importingId || importingAll}
          />
          {t("cliSync_includeBackground")}
        </label>
      {/if}
      <p class="mt-2 text-xs text-muted-foreground">{t("cliSync_mainChatsHelp")}</p>
    </div>

    <!-- Search -->
    <div class="border-b border-border px-4 py-3 sm:px-6">
      <div class="relative">
        <svg
          class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
        </svg>
        <input
          type="text"
          bind:value={searchQuery}
          placeholder={t("cliSync_searchPlaceholder")}
          class="w-full rounded-lg border border-border bg-muted/50 py-2 pl-10 pr-3 text-sm text-foreground placeholder:text-muted-foreground focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary"
        />
      </div>
    </div>

    <!-- Session list -->
    <div class="min-h-0 min-w-0 flex-1 overflow-y-auto px-4 py-3 sm:px-6">
      {#if loading}
        <div class="flex items-center justify-center py-12">
          <div
            class="h-5 w-5 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
          ></div>
        </div>
      {:else if error && sessions.length === 0}
        <div class="flex flex-col items-center gap-2 py-12 text-center">
          <svg
            class="h-8 w-8 text-destructive/60"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
          >
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          <p class="text-sm text-destructive">{error}</p>
        </div>
      {:else if filtered.length === 0}
        <div class="flex flex-col items-center gap-2 py-12 text-center">
          <svg
            class="h-8 w-8 text-muted-foreground/40"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
          >
            <path
              d="M3 7v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-6l-2-2H5a2 2 0 0 0-2 2z"
            />
          </svg>
          <p class="text-sm text-muted-foreground">
            {agent === "codex" ? t("cliSync_noSessions_codex") : t("cliSync_noSessions_claude")}
          </p>
        </div>
      {:else}
        <div class="space-y-2">
          {#each filtered.slice(0, visibleLimit) as session (session.sessionId)}
            {@const isImporting = importingId === session.sessionId}
            {@const isImported = session.alreadyImported}
            <div
              class="group rounded-lg border border-border p-3 transition-colors hover:bg-muted/30"
            >
              <div class="flex flex-wrap items-start justify-between gap-3">
                <!-- Left: status dot + time + prompt -->
                <div class="min-w-0 flex-[1_1_14rem]">
                  <div class="flex flex-wrap items-center gap-2">
                    <span
                      class="inline-block h-2 w-2 shrink-0 rounded-full {isImported
                        ? 'bg-emerald-500'
                        : 'bg-blue-500'}"
                    ></span>
                    <span class="text-xs text-muted-foreground shrink-0">
                      {fmtRelative(session.lastActivityAt)}
                    </span>
                    {#if isShowAll && session.cwd}
                      <span
                        class="shrink-0 truncate max-w-[140px] rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground"
                        title={session.cwd}
                      >
                        {cwdDisplayLabel(session.cwd)}
                      </span>
                    {/if}
                    {#if session.model}
                      <span
                        class="ml-auto shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground"
                      >
                        {session.model}
                      </span>
                    {/if}
                  </div>
                  <p class="mt-1 break-words text-sm font-medium text-foreground">
                    {sessionTitle(session, (project) =>
                      t("cliSync_sessionFromProject", { project }),
                    )}
                  </p>
                  {#if sessionPreview(session)}<p
                      class="mt-1 line-clamp-2 break-words text-xs text-muted-foreground"
                    >
                      {sessionPreview(session)}
                    </p>{/if}
                  <p
                    class="mt-1 break-words text-xs text-muted-foreground [overflow-wrap:anywhere]"
                  >
                    {t("room_sourceFolder")}: {session.cwd}
                  </p>
                  <div class="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    {#if session.isAutomated}<span>{t("cliSync_automatedChat")}</span>{/if}
                    {#if session.isSubagent}<span>{t("cliSync_subagentChat")}</span>{/if}
                    {#if isBackgroundSession(session) && !session.isAutomated && !session.isSubagent}<span
                        >{t("cliSync_backgroundChat")}</span
                      >{/if}
                    {#if session.archived}<span>{t("cliSync_archived")}</span>{/if}
                    {#if session.countsExact !== false}
                      {#if session.agent === "codex"}
                        <span>{t("cliSync_turns", { count: String(session.messageCount) })}</span>
                        {#if session.rolloutPaths && session.rolloutPaths.length > 1}
                          <span>&middot;</span>
                          <span
                            >{t("cliSync_rollouts", {
                              count: String(session.rolloutPaths.length),
                            })}</span
                          >
                        {/if}
                      {:else}
                        <span>{t("cliSync_messages", { count: String(session.messageCount) })}</span
                        >
                      {/if}
                    {/if}
                    <span>{formatSize(session.fileSize)}</span>
                    {#if session.hasSubagents}
                      <span>&middot;</span>
                      <span>{t("cliSync_subagents")}</span>
                    {/if}
                    {#if isImported && session.existingRunId}
                      <span>&middot;</span>
                      <span class="text-emerald-600 dark:text-emerald-400">
                        {t("cliSync_alreadyImported")}
                      </span>
                    {/if}
                  </div>
                </div>

                <!-- Right: action buttons -->
                <div class="flex items-center gap-1.5 shrink-0 pt-0.5">
                  {#if isImported && session.existingRunId}
                    <button
                      class="rounded-md border border-border px-2.5 py-1 text-xs font-medium text-foreground hover:bg-accent transition-colors disabled:opacity-50"
                      onclick={() => syncSession(session.existingRunId!)}
                      disabled={!!importingId}
                    >
                      {#if importingId === session.existingRunId}
                        <span
                          class="inline-block h-3 w-3 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
                        ></span>
                      {:else}
                        {t("cliSync_sync")}
                      {/if}
                    </button>
                    <button
                      class="rounded-md border border-border px-2.5 py-1 text-xs font-medium text-foreground hover:bg-accent transition-colors"
                      onclick={() => onimported(session.existingRunId!)}
                    >
                      {t("cliSync_open")}
                    </button>
                  {:else}
                    <button
                      class="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
                      onclick={() => importSession(session)}
                      disabled={!!importingId}
                    >
                      {#if isImporting}
                        <span
                          class="inline-block h-3 w-3 border-2 border-primary-foreground/30 border-t-primary-foreground rounded-full animate-spin"
                        ></span>
                      {:else}
                        {t("cliSync_import")}
                      {/if}
                    </button>
                  {/if}
                </div>
              </div>
            </div>
          {/each}
          {#if filtered.length > visibleLimit}<button
              class="w-full rounded-md border border-border px-3 py-2 text-sm"
              onclick={() => (visibleLimit += 25)}>{t("common_showMore")}</button
            >{/if}
        </div>
      {/if}
    </div>

    <!-- Footer -->
    {#if !loading && filtered.length > 0}
      <div
        class="flex flex-wrap items-center justify-between gap-2 border-t border-border px-4 py-3 sm:px-6"
      >
        {#if error}
          <p class="min-w-0 flex-1 truncate text-xs text-destructive">{error}</p>
        {:else if warning}
          <p class="min-w-0 flex-1 truncate text-xs text-yellow-600 dark:text-yellow-400">
            {warning}
          </p>
        {:else}
          <div></div>
        {/if}
        {#if newCount > 0}
          <button
            class="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
            onclick={importAllNew}
            disabled={!!importingId || importingAll}
          >
            {#if importingAll}
              <span class="flex flex-wrap items-center gap-2">
                <span
                  class="inline-block h-3.5 w-3.5 border-2 border-primary-foreground/30 border-t-primary-foreground rounded-full animate-spin"
                ></span>
                {t("cliSync_importing")}
              </span>
            {:else}
              {t("cliSync_importAll", { count: String(newCount) })}
            {/if}
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>
