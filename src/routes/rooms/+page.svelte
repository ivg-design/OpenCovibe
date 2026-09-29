<script lang="ts">
  import { onMount } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import Card from "$lib/components/Card.svelte";
  import Input from "$lib/components/Input.svelte";
  import Textarea from "$lib/components/Textarea.svelte";
  import RoomBoard from "$lib/components/RoomBoard.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";
  import {
    createRoom,
    ensureRoomProject,
    listRooms,
    postRoomMessage,
    refreshRoomBoard,
    getRoom,
    setRoomPaused,
  } from "$lib/rooms/api";
  import type { CreateRoomInput, Room } from "$lib/rooms/types";

  const tr: typeof t = t;
  const selectedRoomStorageKey = "opencovibe:selected-room";
  function storedSelection(): string | null {
    try {
      return localStorage.getItem(selectedRoomStorageKey);
    } catch {
      return null;
    }
  }
  function saveSelection(id: string | null) {
    try {
      if (id) localStorage.setItem(selectedRoomStorageKey, id);
      else localStorage.removeItem(selectedRoomStorageKey);
    } catch {
      // Room persistence lives in SQLite; this optional navigation preference may fail.
    }
  }
  let desktop = $state(false);
  let ready = $state(false);
  let rooms = $state<Room[]>([]);
  let selected = $state<Room | null>(null);
  let loadingList = $state(false);
  let loadingRoom = $state(false);
  let busyAction = $state("");
  let error = $state("");
  let notice = $state("");
  let generation = 0;
  let listGeneration = 0;

  let showCreate = $state(false);
  let title = $state("");
  let objective = $state("");
  let repoPath = $state("");
  let repository = $state("");
  let createProject = $state(true);
  let creating = $state(false);
  let humanNote = $state("");
  let actionsDisabled = $derived(loadingRoom || !!busyAction || creating);

  onMount(() => {
    desktop = getTransport().isDesktop();
    ready = true;
    if (desktop) void loadRooms(storedSelection());
  });

  async function loadRooms(preferredRoomId: string | null = null) {
    if (!desktop) return;
    const request = ++listGeneration;
    loadingList = true;
    error = "";
    try {
      const result = await listRooms();
      if (request !== listGeneration) return;
      rooms = result;
      if (selected && !result.some((room) => room.id === selected?.id)) {
        selected = null;
        saveSelection(null);
      }
      const selectedId = selected?.id;
      const roomId =
        (preferredRoomId && result.some((room) => room.id === preferredRoomId)
          ? preferredRoomId
          : null) ??
        (selectedId && result.some((room) => room.id === selectedId) ? selectedId : null) ??
        result[0]?.id;
      if (roomId && roomId !== selected?.id) await selectRoom(roomId);
      else if (!roomId) selected = null;
    } catch (cause) {
      if (request === listGeneration) error = String(cause);
    } finally {
      if (request === listGeneration) loadingList = false;
    }
  }

  async function selectRoom(id: string) {
    const request = ++generation;
    loadingRoom = true;
    busyAction = "";
    error = "";
    try {
      const room = await getRoom(id);
      if (request !== generation) return;
      selected = room;
      saveSelection(room.id);
      if (room.project && !room.board.synced_at) await refreshInitialBoard(room, request);
    } catch (cause) {
      if (request === generation) error = String(cause);
    } finally {
      if (request === generation) loadingRoom = false;
    }
  }

  async function refreshInitialBoard(room: Room, request: number) {
    busyAction = "board";
    try {
      const updated = await refreshRoomBoard(room.id);
      if (request === generation && selected?.id === room.id) {
        selected = updated;
        rooms = rooms.map((entry) => (entry.id === updated.id ? updated : entry));
      }
    } catch (cause) {
      if (request === generation && selected?.id === room.id) error = String(cause);
    } finally {
      if (request === generation) busyAction = "";
    }
  }

  function applyRoom(room: Room, request: number) {
    if (request !== generation || selected?.id !== room.id) return;
    selected = room;
    rooms = rooms.map((entry) => (entry.id === room.id ? room : entry));
  }

  async function perform(action: string, operation: (id: string) => Promise<Room>) {
    if (!selected || !desktop || loadingRoom || busyAction) return;
    const roomId = selected.id;
    const request = ++generation;
    busyAction = action;
    error = "";
    notice = "";
    try {
      const updated = await operation(roomId);
      applyRoom(updated, request);
      if (action === "project" && updated.project && !updated.board.synced_at) {
        await refreshInitialBoard(updated, request);
      }
    } catch (cause) {
      if (request === generation && selected?.id === roomId) error = String(cause);
    } finally {
      if (request === generation) busyAction = "";
    }
  }

  async function submitCreate(event: SubmitEvent) {
    event.preventDefault();
    if (!desktop || creating || busyAction || loadingRoom) return;
    if (
      !title.trim() ||
      !objective.trim() ||
      !repoPath.trim() ||
      !/^\S+\/\S+$/.test(repository.trim())
    ) {
      error = tr("room_formValidation");
      return;
    }
    const input: CreateRoomInput = {
      title: title.trim(),
      objective: objective.trim(),
      repo_path: repoPath.trim(),
      repository: repository.trim(),
      create_project: createProject,
    };
    const request = ++generation;
    loadingRoom = false;
    busyAction = "";
    creating = true;
    error = "";
    try {
      const room = await createRoom(input);
      rooms = [room, ...rooms.filter((entry) => entry.id !== room.id)];
      if (request === generation) {
        loadingRoom = false;
        busyAction = "";
        selected = room;
        saveSelection(room.id);
        showCreate = false;
        title = objective = repoPath = repository = "";
        createProject = true;
        notice = room.project ? tr("room_created") : tr("room_projectRecoverable");
        if (room.project && !room.board.synced_at) void refreshInitialBoard(room, request);
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      creating = false;
    }
  }

  async function submitNote(event: SubmitEvent) {
    event.preventDefault();
    const body = humanNote.trim();
    if (!body || !selected || !desktop || loadingRoom || busyAction) return;
    const roomId = selected.id;
    const request = ++generation;
    busyAction = "note";
    error = "";
    try {
      const updated = await postRoomMessage(roomId, body);
      applyRoom(updated, request);
      if (request === generation && selected?.id === roomId) humanNote = "";
    } catch (cause) {
      if (request === generation && selected?.id === roomId) error = String(cause);
    } finally {
      if (request === generation) busyAction = "";
    }
  }
</script>

<svelte:head><title>{tr("room_pageTitle")}</title></svelte:head>

<main class="mx-auto max-w-7xl space-y-5 p-4 md:p-6">
  <header class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <h1 class="text-2xl font-semibold text-foreground">{tr("room_pageTitle")}</h1>
      <p class="mt-1 text-sm text-muted-foreground">{tr("room_pageDescription")}</p>
    </div>
    {#if desktop}
      <div class="flex gap-2">
        <Button variant="outline" onclick={() => void loadRooms()} loading={loadingList}
          >{tr("room_reloadRooms")}</Button
        >
        <Button onclick={() => (showCreate = !showCreate)}
          >{showCreate ? tr("common_cancel") : tr("room_newRoom")}</Button
        >
      </div>
    {/if}
  </header>

  {#if ready && !desktop}
    <Card class="p-5 text-sm text-muted-foreground">{tr("room_desktopOnly")}</Card>
  {:else if desktop}
    {#if error}<div
        class="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm text-destructive"
        role="alert"
      >
        {error}
      </div>{/if}
    {#if notice}<div
        class="rounded-md border border-primary/30 bg-primary/5 p-3 text-sm text-foreground"
        role="status"
      >
        {notice}
      </div>{/if}

    {#if showCreate}
      <Card class="p-4 md:p-5">
        <h2 class="mb-4 text-base font-semibold">{tr("room_createTitle")}</h2>
        <form class="grid gap-3 md:grid-cols-2" onsubmit={submitCreate}>
          <label class="space-y-1 text-xs text-muted-foreground"
            ><span>{tr("room_titleLabel")}</span><Input bind:value={title} /></label
          >
          <label class="space-y-1 text-xs text-muted-foreground"
            ><span>{tr("room_repositoryLabel")}</span><Input
              bind:value={repository}
              placeholder="OWNER/REPO"
            /></label
          >
          <label class="space-y-1 text-xs text-muted-foreground md:col-span-2"
            ><span>{tr("room_objectiveLabel")}</span><Textarea
              bind:value={objective}
              rows={3}
            /></label
          >
          <label class="space-y-1 text-xs text-muted-foreground md:col-span-2"
            ><span>{tr("room_repoPathLabel")}</span><Input bind:value={repoPath} /></label
          >
          <label class="flex items-center gap-2 text-sm text-foreground md:col-span-2"
            ><input type="checkbox" bind:checked={createProject} />{tr("room_createProject")}</label
          >
          <div class="md:col-span-2">
            <Button loading={creating}>{tr("room_createRoom")}</Button>
          </div>
        </form>
      </Card>
    {/if}

    <div class="grid gap-5 lg:grid-cols-[250px_minmax(0,1fr)]">
      <aside class="space-y-2">
        <h2 class="px-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          {tr("room_savedRooms")}
        </h2>
        {#if rooms.length === 0 && !loadingList}
          <Card variant="subtle" class="p-4 text-sm text-muted-foreground"
            >{tr("room_noRooms")}</Card
          >
        {/if}
        {#each rooms as room (room.id)}
          <button
            class="w-full rounded-lg border p-3 text-left transition-colors hover:bg-accent {selected?.id ===
            room.id
              ? 'border-primary/50 bg-accent'
              : 'bg-background'}"
            aria-pressed={selected?.id === room.id}
            disabled={!!busyAction || loadingRoom}
            onclick={() => void selectRoom(room.id)}
          >
            <span class="block truncate text-sm font-medium text-foreground">{room.title}</span>
            <span class="mt-1 block truncate text-xs text-muted-foreground">{room.repository}</span>
          </button>
        {/each}
      </aside>

      <div class="min-w-0 space-y-5">
        {#if selected}
          <Card class="p-4 md:p-5">
            <div class="flex flex-wrap items-start justify-between gap-3">
              <div class="min-w-0">
                <h2 class="text-xl font-semibold">{selected.title}</h2>
                <p class="mt-1 text-sm text-muted-foreground">{selected.objective}</p>
                <p class="mt-2 text-xs text-muted-foreground">
                  {selected.repository} · {selected.repo_path}
                </p>
              </div>
              <Button
                variant="outline"
                onclick={() => void perform("pause", (id) => setRoomPaused(id, !selected?.paused))}
                loading={busyAction === "pause"}
                disabled={actionsDisabled}
                >{selected.paused ? tr("room_resume") : tr("room_pause")}</Button
              >
            </div>
            <div class="mt-4 flex flex-wrap items-center gap-2 border-t pt-4">
              {#if selected.project}
                <a
                  href={selected.project.url}
                  target="_blank"
                  rel="noreferrer"
                  class="text-sm text-primary hover:underline"
                  >{tr("room_openProject", { number: String(selected.project.number) })}</a
                >
              {:else}
                <span class="text-sm text-muted-foreground">{tr("room_projectRecoverable")}</span>
                <Button
                  size="sm"
                  variant="outline"
                  onclick={() => void perform("project", ensureRoomProject)}
                  loading={busyAction === "project"}
                  disabled={actionsDisabled}>{tr("room_retryProject")}</Button
                >
              {/if}
              {#if selected.participants.length === 0}<span class="text-xs text-muted-foreground"
                  >{tr("room_noPeers")}</span
                >{/if}
            </div>
          </Card>

          {#if loadingRoom}<p class="text-sm text-muted-foreground" role="status">
              {tr("room_loading")}
            </p>{/if}
          <RoomBoard
            board={selected.board}
            loading={busyAction === "board"}
            canRefresh={!!selected.project && !actionsDisabled}
            onrefresh={() => void perform("board", refreshRoomBoard)}
          />

          <Card class="p-4">
            <h2 class="mb-3 text-base font-semibold">{tr("room_humanNotes")}</h2>
            <div class="mb-3 max-h-72 space-y-2 overflow-y-auto">
              {#if selected.messages.length === 0}<p class="text-sm text-muted-foreground">
                  {tr("room_noNotes")}
                </p>{/if}
              {#each selected.messages as message (message.id)}
                <article class="rounded-md bg-muted/50 p-3">
                  <div class="flex justify-between gap-3 text-xs text-muted-foreground">
                    <span>{message.sender}</span><time
                      >{new Date(message.created_at).toLocaleString()}</time
                    >
                  </div>
                  <p class="mt-1 whitespace-pre-wrap text-sm text-foreground">{message.body}</p>
                </article>
              {/each}
            </div>
            <form class="flex flex-col gap-2 sm:flex-row" onsubmit={submitNote}>
              <label class="flex-1">
                <span class="sr-only">{tr("room_notePlaceholder")}</span>
                <Input
                  class="w-full"
                  bind:value={humanNote}
                  placeholder={tr("room_notePlaceholder")}
                  disabled={actionsDisabled}
                /></label
              >
              <Button
                disabled={!humanNote.trim() || actionsDisabled}
                loading={busyAction === "note"}>{tr("room_postNote")}</Button
              >
            </form>
          </Card>
        {:else if !loadingList}
          <Card variant="subtle" class="p-6 text-sm text-muted-foreground"
            >{tr("room_selectOrCreate")}</Card
          >
        {/if}
      </div>
    </div>
  {/if}
</main>
