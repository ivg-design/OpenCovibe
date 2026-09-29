<script lang="ts">
  import { onMount } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import Card from "$lib/components/Card.svelte";
  import Input from "$lib/components/Input.svelte";
  import Textarea from "$lib/components/Textarea.svelte";
  import RoomBoard from "$lib/components/RoomBoard.svelte";
  import RoomParticipants from "$lib/components/RoomParticipants.svelte";
  import RoomTimers from "$lib/components/RoomTimers.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";
  import {
    addRoomParticipant,
    archiveRoom,
    attachRoomProject,
    createRoom,
    getRoom,
    listRooms,
    mergeRoomWorktree,
    postRoomMessage,
    refreshRoomBoard,
    releaseRoomClaim,
    removeRoomParticipant,
    removeRoomTimer,
    saveRoomTimer,
    setRoomAutoContinue,
    setRoomPaused,
    setRoomParticipantPaused,
    wakeRoomParticipant,
  } from "$lib/rooms/api";
  import type {
    CreateRoomInput,
    Room,
    RoomParticipant,
    RoomTimer,
    SaveTimerInput,
  } from "$lib/rooms/types";

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
      /* Optional selection preference. */
    }
  }
  let desktop = $state(false),
    ready = $state(false),
    rooms = $state<Room[]>([]),
    selected = $state<Room | null>(null);
  let loadingList = $state(false),
    loadingRoom = $state(false),
    busyAction = $state(""),
    error = $state(""),
    notice = $state("");
  let generation = 0,
    pollGeneration = 0,
    listGeneration = 0;
  let showCreate = $state(false),
    title = $state(""),
    objective = $state(""),
    repoPath = $state(""),
    repository = $state(""),
    createProject = $state(true),
    creating = $state(false);
  let humanMessage = $state(""),
    targetParticipantId = $state("");
  let projectNumber = $state("");
  let actionsDisabled = $derived(
    loadingRoom || !!busyAction || creating || !selected || selected.archived,
  );

  onMount(() => {
    desktop = getTransport().isDesktop();
    ready = true;
    if (desktop) void loadRooms(storedSelection());
    const timer = window.setInterval(() => void pollSelected(), 2000);
    return () => {
      window.clearInterval(timer);
      generation++;
    };
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
      const roomId =
        (preferredRoomId && result.some((r) => r.id === preferredRoomId)
          ? preferredRoomId
          : null) ??
        (selected && result.some((r) => r.id === selected?.id) ? selected.id : null) ??
        result[0]?.id;
      if (roomId && roomId !== selected?.id) await selectRoom(roomId);
      else if (!roomId) {
        selected = null;
        saveSelection(null);
      }
    } catch (cause) {
      if (request === listGeneration) error = String(cause);
    } finally {
      if (request === listGeneration) loadingList = false;
    }
  }
  async function pollSelected() {
    const id = selected?.id;
    if (!desktop || !id || loadingRoom || busyAction || creating) return;
    const selectionGeneration = generation,
      request = ++pollGeneration;
    try {
      const room = await getRoom(id);
      if (
        request === pollGeneration &&
        selectionGeneration === generation &&
        !busyAction &&
        selected?.id === id
      )
        applyRoom(room);
    } catch (cause) {
      if (request === pollGeneration && selected?.id === id) error = String(cause);
    }
  }
  async function selectRoom(id: string) {
    const request = ++generation;
    pollGeneration++;
    loadingRoom = true;
    busyAction = "";
    error = "";
    try {
      const room = await getRoom(id);
      if (request !== generation) return;
      applyRoom(room);
      saveSelection(room.id);
      if (room.project && !room.board.synced_at) {
        busyAction = "board";
        try {
          const refreshed = await refreshRoomBoard(room.id);
          if (request === generation) applyRoom(refreshed);
        } catch (cause) {
          if (request === generation) error = String(cause);
        } finally {
          if (request === generation) busyAction = "";
        }
      }
    } catch (cause) {
      if (request === generation) error = String(cause);
    } finally {
      if (request === generation) loadingRoom = false;
    }
  }
  function applyRoom(room: Room) {
    selected = room;
    rooms = rooms.map((r) => (r.id === room.id ? room : r));
  }
  async function perform(action: string, operation: (id: string) => Promise<Room>) {
    if (!selected || !desktop || busyAction || loadingRoom) return;
    const roomId = selected.id,
      request = ++generation;
    pollGeneration++;
    busyAction = action;
    error = "";
    notice = "";
    try {
      const room = await operation(roomId);
      if (request === generation && selected?.id === roomId) applyRoom(room);
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
    creating = true;
    error = "";
    try {
      const room = await createRoom(input);
      if (request !== generation) return;
      rooms = [room, ...rooms.filter((r) => r.id !== room.id)];
      selected = room;
      saveSelection(room.id);
      showCreate = false;
      title = objective = repoPath = repository = "";
      createProject = true;
      notice = room.project ? tr("room_created") : tr("room_createdNoProject");
    } catch (cause) {
      error = String(cause);
    } finally {
      creating = false;
    }
  }
  async function submitMessage(event: SubmitEvent) {
    event.preventDefault();
    const body = humanMessage.trim();
    if (!body || !selected || actionsDisabled) return;
    const id = selected.id,
      target = targetParticipantId || null;
    const request = ++generation;
    pollGeneration++;
    busyAction = "message";
    error = "";
    try {
      const room = await postRoomMessage(id, body, target);
      if (request === generation && selected?.id === id) {
        applyRoom(room);
        humanMessage = "";
      }
    } catch (cause) {
      if (request === generation && selected?.id === id) error = String(cause);
    } finally {
      if (request === generation) busyAction = "";
    }
  }
  function participantAction(action: string, participant: RoomParticipant, message = "") {
    if (action.startsWith("release:")) {
      const taskId = action.slice("release:".length);
      void perform(`release-${taskId}`, (id) => releaseRoomClaim(id, taskId));
      return;
    }
    const ops: Record<string, () => Promise<Room>> = {
      merge: () => mergeRoomWorktree(selected!.id, participant.id),
      "pause-participant": () => setRoomParticipantPaused(selected!.id, participant.id, true),
      "resume-participant": () => setRoomParticipantPaused(selected!.id, participant.id, false),
      "remove-participant": () => removeRoomParticipant(selected!.id, participant.id),
      wake: () => wakeRoomParticipant(selected!.id, participant.id, message),
    };
    const op = ops[action];
    if (op) void perform(action, () => op());
  }
  function addParticipant(input: Parameters<typeof addRoomParticipant>[1]) {
    void perform("add-participant", (id) => addRoomParticipant(id, input));
  }
  function saveTimer(input: SaveTimerInput) {
    void perform("save-timer", (id) => saveRoomTimer(id, input));
  }
  function deleteTimer(timer: RoomTimer) {
    void perform(`delete-timer-${timer.id}`, (id) => removeRoomTimer(id, timer.id));
  }
  function attachProject(event: SubmitEvent) {
    event.preventDefault();
    const number = Number(projectNumber);
    if (Number.isInteger(number) && number > 0)
      void perform("attach-project", (id) => attachRoomProject(id, number));
  }
  async function doArchive() {
    if (!selected || !window.confirm(tr("room_confirmArchive"))) return;
    const id = selected.id;
    await perform("archive", archiveRoom);
    if (selected?.id === id && selected.archived) {
      selected = null;
      saveSelection(null);
      await loadRooms();
      notice = tr("room_archived");
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
    {#if desktop}<div class="flex gap-2">
        <Button variant="outline" onclick={() => void loadRooms()} loading={loadingList}
          >{tr("room_reloadRooms")}</Button
        ><Button onclick={() => (showCreate = !showCreate)}
          >{showCreate ? tr("common_cancel") : tr("room_newRoom")}</Button
        >
      </div>{/if}
  </header>
  {#if ready && !desktop}<Card class="p-5 text-sm text-muted-foreground"
      >{tr("room_desktopOnly")}</Card
    >{:else if desktop}
    {#if error}<div
        class="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm text-destructive"
        role="alert"
      >
        {error}
      </div>{/if}{#if notice}<div
        class="rounded-md border border-primary/30 bg-primary/5 p-3 text-sm"
        role="status"
      >
        {notice}
      </div>{/if}
    {#if showCreate}<Card class="p-4 md:p-5"
        ><h2 class="mb-4 text-base font-semibold">{tr("room_createTitle")}</h2>
        <form class="grid gap-3 md:grid-cols-2" onsubmit={submitCreate}>
          <label class="space-y-1 text-xs text-muted-foreground"
            ><span>{tr("room_titleLabel")}</span><Input bind:value={title} /></label
          ><label class="space-y-1 text-xs text-muted-foreground"
            ><span>{tr("room_repositoryLabel")}</span><Input
              bind:value={repository}
              placeholder="OWNER/REPO"
            /></label
          ><label class="space-y-1 text-xs text-muted-foreground md:col-span-2"
            ><span>{tr("room_objectiveLabel")}</span><Textarea
              bind:value={objective}
              rows={3}
            /></label
          ><label class="space-y-1 text-xs text-muted-foreground md:col-span-2"
            ><span>{tr("room_repoPathLabel")}</span><Input bind:value={repoPath} /></label
          ><label class="flex items-center gap-2 text-sm md:col-span-2"
            ><input type="checkbox" bind:checked={createProject} />{tr("room_createProject")}</label
          >
          <div class="md:col-span-2">
            <Button loading={creating}>{tr("room_createRoom")}</Button>
          </div>
        </form></Card
      >{/if}
    <div class="grid gap-5 lg:grid-cols-[250px_minmax(0,1fr)]">
      <aside class="space-y-2">
        <h2 class="px-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          {tr("room_savedRooms")}
        </h2>
        {#if rooms.length === 0 && !loadingList}<Card
            variant="subtle"
            class="p-4 text-sm text-muted-foreground">{tr("room_noRooms")}</Card
          >{/if}{#each rooms as room (room.id)}<button
            class="w-full rounded-lg border p-3 text-left transition-colors hover:bg-accent {selected?.id ===
            room.id
              ? 'border-primary/50 bg-accent'
              : 'bg-background'}"
            aria-pressed={selected?.id === room.id}
            disabled={!!busyAction || loadingRoom}
            onclick={() => void selectRoom(room.id)}
            ><span class="block truncate text-sm font-medium">{room.title}</span><span
              class="mt-1 block truncate text-xs text-muted-foreground">{room.repository}</span
            ></button
          >{/each}
      </aside>
      <div class="min-w-0 space-y-5">
        {#if selected}
          <Card class="p-4 md:p-5"
            ><div class="flex flex-wrap items-start justify-between gap-3">
              <div class="min-w-0">
                <h2 class="text-xl font-semibold">{selected.title}</h2>
                <p class="mt-1 text-sm text-muted-foreground">{selected.objective}</p>
                <p class="mt-2 text-xs text-muted-foreground">
                  {selected.repository} · {selected.repo_path}
                </p>
              </div>
              <div class="flex gap-2">
                <Button
                  variant="outline"
                  onclick={() =>
                    void perform("pause-room", (id) => setRoomPaused(id, !selected?.paused))}
                  loading={busyAction === "pause-room"}
                  disabled={actionsDisabled}
                  >{selected.paused ? tr("room_resume") : tr("room_pause")}</Button
                ><Button
                  variant="outline"
                  onclick={() =>
                    void perform("auto-continue", (id) =>
                      setRoomAutoContinue(id, !selected?.auto_continue),
                    )}
                  disabled={actionsDisabled}
                  >{selected.auto_continue
                    ? tr("room_autoContinueOn")
                    : tr("room_autoContinueOff")}</Button
                >
              </div>
            </div>
            {#if selected.runtime_error}<p
                class="mt-3 rounded border border-destructive/30 bg-destructive/5 p-2 text-sm text-destructive"
                role="status"
              >
                {tr("room_runtimeError", { error: selected.runtime_error })}
              </p>{/if}
            <div class="mt-4 flex flex-wrap items-center gap-3 border-t pt-4">
              {#if selected.project}<a
                  href={selected.project.url}
                  target="_blank"
                  rel="noreferrer"
                  class="text-sm text-primary hover:underline"
                  >{tr("room_openProject", { number: String(selected.project.number) })}</a
                >{:else}<span class="text-sm text-muted-foreground"
                  >{tr("room_noProjectAttached")}</span
                >
                <form class="flex flex-wrap items-end gap-2" onsubmit={attachProject}>
                  <label class="space-y-1 text-xs text-muted-foreground"
                    ><span>{tr("room_projectNumber")}</span><input
                      class="h-9 w-full rounded-md border bg-background px-3 text-sm"
                      type="number"
                      min="1"
                      bind:value={projectNumber}
                    /></label
                  ><Button
                    size="sm"
                    disabled={actionsDisabled || !projectNumber}
                    loading={busyAction === "attach-project"}>{tr("room_attachProject")}</Button
                  >
                </form>{/if}
              <label class="flex items-center gap-2 text-sm"
                ><input
                  type="checkbox"
                  checked={selected.auto_continue}
                  disabled={actionsDisabled}
                  onchange={() =>
                    void perform("auto-continue", (id) =>
                      setRoomAutoContinue(id, !selected?.auto_continue),
                    )}
                />{tr("room_autoContinue")}</label
              >
              {#if !selected.archived}<Button
                  size="sm"
                  variant="outline"
                  disabled={actionsDisabled}
                  loading={busyAction === "archive"}
                  onclick={() => void doArchive()}>{tr("room_archive")}</Button
                >{:else}<span class="text-xs text-muted-foreground">{tr("room_roomArchived")}</span
                >{/if}
            </div></Card
          >
          {#if loadingRoom}<p class="text-sm text-muted-foreground" role="status">
              {tr("room_loading")}
            </p>{/if}
          <RoomParticipants
            participants={selected.participants}
            claims={selected.claims}
            disabled={actionsDisabled}
            {busyAction}
            onAction={participantAction}
            onAdd={addParticipant}
          />
          <RoomTimers
            timers={selected.timers}
            participants={selected.participants}
            disabled={actionsDisabled}
            {busyAction}
            onSave={saveTimer}
            onDelete={deleteTimer}
          />
          {#if selected.project}<RoomBoard
              roomId={selected.id}
              board={selected.board}
              loading={busyAction === "board"}
              canRefresh={!actionsDisabled}
              onrefresh={() => void perform("board", refreshRoomBoard)}
            />{/if}
          <Card class="p-4"
            ><div class="mb-3 flex items-center justify-between">
              <h2 class="text-base font-semibold">{tr("room_conversation")}</h2>
              <span class="text-xs text-muted-foreground">{selected.messages.length}</span>
            </div>
            <div class="mb-3 max-h-96 space-y-2 overflow-y-auto">
              {#if selected.messages.length === 0}<p class="text-sm text-muted-foreground">
                  {tr("room_noMessages")}
                </p>{/if}{#each selected.messages as message (message.id)}<article
                  class="rounded-md border-l-2 bg-muted/40 p-3"
                  style={`border-left-color: ${
                    message.participant_id
                      ? ["#5677c8", "#b36a9a", "#538b70", "#bd8250", "#7481aa", "#ad665d"][
                          Math.max(
                            0,
                            selected.participants.findIndex((p) => p.id === message.participant_id),
                          ) % 6
                        ]
                      : "hsl(var(--primary))"
                  }`}
                >
                  <div class="flex justify-between gap-3 text-xs text-muted-foreground">
                    <span
                      >{message.sender}{#if message.target_participant_id}
                        · {tr("room_directedMessage", {
                          name:
                            selected.participants.find(
                              (p) => p.id === message.target_participant_id,
                            )?.name ?? message.target_participant_id,
                        })}{/if}</span
                    ><time>{new Date(message.created_at).toLocaleString()}</time>
                  </div>
                  <p class="mt-1 whitespace-pre-wrap text-sm">{message.body}</p>
                </article>{/each}
            </div>
            <form class="space-y-2" onsubmit={submitMessage}>
              <div class="flex flex-wrap items-end gap-2">
                <label class="min-w-48 space-y-1 text-xs text-muted-foreground"
                  ><span>{tr("room_target")}</span><select
                    class="h-9 w-full rounded-md border bg-background px-2 text-sm text-foreground"
                    bind:value={targetParticipantId}
                    ><option value="">{tr("room_everyone")}</option
                    >{#each selected.participants as p (p.id)}<option value={p.id}>{p.name}</option
                      >{/each}</select
                  ></label
                >
                <div class="flex-1">
                  <label
                    ><span class="sr-only">{tr("room_messagePlaceholder")}</span><Input
                      class="w-full"
                      bind:value={humanMessage}
                      placeholder={tr("room_messagePlaceholder")}
                      disabled={actionsDisabled}
                    /></label
                  >
                </div>
                <Button
                  disabled={!humanMessage.trim() || actionsDisabled}
                  loading={busyAction === "message"}>{tr("room_sendMessage")}</Button
                >
              </div>
            </form></Card
          >
          {#if selected.claims.length > 0}<Card class="p-4"
              ><h2 class="mb-3 text-base font-semibold">{tr("room_claims")}</h2>
              <div class="space-y-2">
                {#each selected.claims as claim (claim.task_id)}{@const owner =
                    selected.participants.find(
                      (p) => p.id === claim.participant_id,
                    )}{@const ownerActive =
                    owner &&
                    !owner.paused &&
                    ["running", "working", "active", "starting", "busy"].includes(
                      owner.state.toLowerCase(),
                    )}
                  <div
                    class="flex flex-wrap items-center justify-between gap-2 border-b pb-2 text-sm"
                  >
                    <span
                      >{selected.board.items.find((item) => item.id === claim.task_id)?.title ??
                        claim.task_id} · {claim.state} · {tr("room_claimOwner", {
                        name: owner?.name ?? claim.participant_id,
                      })}</span
                    ><Button
                      size="sm"
                      variant="outline"
                      disabled={actionsDisabled ||
                        !!ownerActive ||
                        (!selected.paused && !owner?.paused) ||
                        ["done", "released"].includes(claim.state)}
                      onclick={() =>
                        void perform(`release-${claim.task_id}`, (id) =>
                          releaseRoomClaim(id, claim.task_id),
                        )}>{tr("room_releaseClaim")}</Button
                    >
                  </div>{/each}
              </div></Card
            >{/if}
        {:else if !loadingList}<Card variant="subtle" class="p-6 text-sm text-muted-foreground"
            >{tr("room_selectOrCreate")}</Card
          >{/if}
      </div>
    </div>
  {/if}
</main>
