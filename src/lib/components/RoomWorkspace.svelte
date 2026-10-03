<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    hasNewRequest,
    requestInboxKeys,
    requestPanelWidth,
    isOpenRoomRequest,
  } from "$lib/rooms/requests";
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { visibleRooms, revealRoom, roomVisibilityEvent } from "$lib/rooms/visibility";
  import Button from "$lib/components/Button.svelte";
  import Card from "$lib/components/Card.svelte";
  import Input from "$lib/components/Input.svelte";
  import Textarea from "$lib/components/Textarea.svelte";
  import RoomBoard from "$lib/components/RoomBoard.svelte";
  import RoomParticipants from "$lib/components/RoomParticipants.svelte";
  import RoomAttachChat from "$lib/components/RoomAttachChat.svelte";
  import RoomRequests from "$lib/components/RoomRequests.svelte";
  import RoomTimers from "$lib/components/RoomTimers.svelte";
  import RoomConversation from "$lib/components/RoomConversation.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";
  import {
    addRoomParticipant,
    attachRoomChat,
    approveRoomAgent,
    closeRoomRequest,
    archiveRoomRequests,
    archiveRoom,
    attachRoomProject,
    createRoom,
    createRoomFromSession,
    createRoomSidechat,
    getRoomSessionSeed,
    getRoom,
    listRooms,
    mergeRoomWorktree,
    postRoomMessage,
    refreshRoomBoard,
    resolveRoomRequest,
    releaseRoomClaim,
    removeRoomParticipant,
    removeRoomTimer,
    saveRoomTimer,
    saveRoomInstructions,
    saveRoomTitle,
    setRoomAutoContinue,
    setRoomConcurrency,
    setRoomPaused,
    setRoomParticipantPaused,
    wakeRoomParticipant,
    updateRoomParticipantSettings,
    inspectRoomRepository,
  } from "$lib/rooms/api";
  import type {
    CreateRoomInput,
    ParticipantSettings,
    Room,
    RoomParticipant,
    RoomRequest,
    RoomTimer,
    RoomSessionSeed,
    SaveTimerInput,
    RepositoryInspection,
    RoomAttachment,
  } from "$lib/rooms/types";

  const tr: typeof t = t;
  let { boardOnly = false, settingsOnly = false }: { boardOnly?: boolean; settingsOnly?: boolean } =
    $props();
  let sourceSession = $state<RoomSessionSeed | null>(null);
  let sourceRunId = $state("");
  let loadingSource = $state(false);
  let sourceFailure = $state("");
  let setupRequest = 0;
  let previousRouteKey = "";
  let visibilityRevision = $state(0);
  let showHiddenRooms = $state(false);
  let sourceProjectId = $state("");
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
  const activeRooms = $derived.by(() => {
    void visibilityRevision;
    return visibleRooms(rooms);
  });
  const hiddenRooms = $derived(
    rooms.filter((room) => !room.archived && !activeRooms.some((r) => r.id === room.id)),
  );
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
    creating = $state(false),
    repoInspection = $state<RepositoryInspection | null>(null),
    inspectingRepository = $state(false),
    repositoryFolderError = $state(false),
    manualRepository = $state(false),
    selectedRemote = $state("");
  let repositoryRequest = 0,
    repositoryTimer: ReturnType<typeof setTimeout> | undefined;
  let humanMessage = $state(""),
    targetParticipantId = $state(""),
    activeSidechatId = $state("");
  let attachmentDrafts = $state<RoomAttachment[]>([]);
  let requestsOpen = $state(true);
  let requestsWidth = $state(32);
  let requestsRevealToken = $state("");
  let chatLayout = $state<HTMLDivElement>();
  let resizingRequests = false;
  let panelRoomId = "";
  let seenRequestKeys: string[] = [];
  const openRequestCount = $derived(selected?.requests.filter(isOpenRoomRequest).length ?? 0);
  $effect(() => {
    if (!ready || !selected || boardOnly || settingsOnly) return;
    const roomId = selected.id;
    const next = requestInboxKeys(selected.requests);
    untrack(() => {
      if (panelRoomId !== roomId) {
        panelRoomId = roomId;
        requestsRevealToken = "";
        let saved: { open?: boolean; width?: number; seen?: string[] } = {};
        try {
          saved = JSON.parse(localStorage.getItem(`ocv:request-panel:${roomId}`) ?? "{}");
        } catch {
          /* Optional preference. */
        }
        requestsWidth = requestPanelWidth(saved.width ?? 32);
        requestsOpen = saved.open ?? true;
        seenRequestKeys = Array.isArray(saved.seen) ? saved.seen : next;
      }
      if (hasNewRequest(seenRequestKeys, next)) {
        if (!requestsOpen)
          requestsRevealToken = next.find((key) => !seenRequestKeys.includes(key)) ?? "";
        requestsOpen = true;
      }
      seenRequestKeys = next;
      saveRequestPanel();
    });
  });
  function saveRequestPanel() {
    if (!panelRoomId) return;
    try {
      localStorage.setItem(
        `ocv:request-panel:${panelRoomId}`,
        JSON.stringify({ open: requestsOpen, width: requestsWidth, seen: seenRequestKeys }),
      );
    } catch {
      /* Optional preference. */
    }
  }
  function toggleRequests(open = !requestsOpen) {
    requestsOpen = open;
    saveRequestPanel();
  }
  function resizeRequests(event: PointerEvent) {
    if (!resizingRequests || !chatLayout) return;
    const bounds = chatLayout.getBoundingClientRect();
    requestsWidth = requestPanelWidth(((bounds.right - event.clientX) / bounds.width) * 100);
    saveRequestPanel();
  }
  function resizeRequestsKey(event: KeyboardEvent) {
    if (!["ArrowLeft", "ArrowRight", "Home"].includes(event.key)) return;
    event.preventDefault();
    requestsWidth =
      event.key === "Home"
        ? 32
        : requestPanelWidth(requestsWidth + (event.key === "ArrowLeft" ? 2 : -2));
    saveRequestPanel();
  }
  let projectNumber = $state("");
  let editingInstructions = $state(false),
    instructionsDraft = $state(""),
    instructionsOriginal = $state("");
  let editingTitle = $state(false),
    titleDraft = $state(""),
    titleOriginal = $state("");
  let actionsDisabled = $derived(
    loadingRoom || !!busyAction || creating || !selected || selected.archived,
  );

  onMount(() => {
    desktop = getTransport().isDesktop();
    ready = true;
    const visibilityChanged = () => {
      visibilityRevision++;
      if (selected && !visibleRooms([selected]).length) {
        generation++;
        pollGeneration++;
        selected = null;
        notice = "";
        saveSelection(null);
        error = tr("room_unavailableSelection");
      }
    };
    window.addEventListener(roomVisibilityEvent, visibilityChanged);

    const timer = window.setInterval(() => void pollSelected(), 2000);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener(roomVisibilityEvent, visibilityChanged);
      generation++;
      setupRequest++;
      repositoryRequest++;
      if (repositoryTimer) clearTimeout(repositoryTimer);
    };
  });
  $effect(() => {
    if (!ready || !desktop) return;
    const sourceId = $page.url.searchParams.get("fromSession") ?? "";
    const requested = $page.url.searchParams.get("room");
    const routeKey = `${sourceId}|${requested ?? ""}|${boardOnly}|${settingsOnly}`;
    if (previousRouteKey === routeKey) return;
    previousRouteKey = routeKey;
    if (sourceId && !boardOnly) void initializeSource(sourceId);
    else {
      setupRequest++;
      sourceRunId = sourceFailure = "";
      loadingSource = false;
      sourceSession = null;
      showCreate = false;
      if (requested && selected?.id !== requested) {
        selected = null;
        notice = "";
        generation++;
        pollGeneration++;
      }
      untrack(() => void loadRooms(requested, true));
    }
  });
  async function initializeSource(sourceId: string) {
    const request = ++setupRequest;
    listGeneration++;
    generation++;
    pollGeneration++;
    repositoryRequest++;
    sourceRunId = sourceId;
    sourceSession = selected = null;
    showCreate = loadingSource = true;
    loadingRoom = false;
    busyAction = error = notice = sourceFailure = "";
    title = objective = repoPath = repository = "";
    repoInspection = null;
    repositoryFolderError = inspectingRepository = manualRepository = false;
    sourceProjectId = "";
    createProject = false;
    try {
      const [seed, savedRooms] = await Promise.all([getRoomSessionSeed(sourceId), listRooms()]);
      if (request !== setupRequest) return;
      rooms = savedRooms;
      sourceSession = seed;
      title = seed.title;
      objective = seed.objective;
      repoPath = seed.repo_path;
      repository = seed.repository;
      manualRepository = !seed.repository && !!seed.repo_path;
      sourceProjectId = seed.projects.length === 1 ? seed.projects[0].id : "";
      createProject = seed.projects.length === 0 && !seed.project_error;
    } catch (cause) {
      if (request === setupRequest) sourceFailure = String(cause);
    } finally {
      if (request === setupRequest) loadingSource = false;
    }
  }
  function cancelSetup() {
    if (sourceRunId) {
      void goto(`/chat?run=${encodeURIComponent(sourceRunId)}`);
      return;
    }
    showCreate = false;
    title = objective = repoPath = repository = "";
    repoInspection = null;
    repositoryFolderError = manualRepository = inspectingRepository = false;
    repositoryRequest++;
  }
  async function openSessionRoom(restore = false) {
    const seed = sourceSession,
      match = seed?.existing_room;
    if (!seed || !match || creating) return;
    creating = true;
    error = "";
    try {
      if (restore) await attachRoomChat(match.id, seed.run_id, match.participant_name);
      revealRoom(match, await listRooms());
      await goto(`/rooms?room=${encodeURIComponent(match.id)}`);
    } catch (cause) {
      error = String(cause);
    } finally {
      creating = false;
    }
  }
  function onRepositoryPathInput(value: string) {
    repoPath = value;
    repositoryRequest++;
    if (repositoryTimer) clearTimeout(repositoryTimer);
    repoInspection = null;
    repositoryFolderError = false;
    selectedRemote = "";
    repository = "";
    manualRepository = false;
    if (!value.trim()) {
      inspectingRepository = false;
      return;
    }
    inspectingRepository = true;
    const request = repositoryRequest;
    repositoryTimer = setTimeout(() => void detectRepository(value, request), 300);
  }
  async function detectRepository(path: string, request = ++repositoryRequest) {
    inspectingRepository = true;
    try {
      const result = await inspectRoomRepository(path.trim());
      if (request !== repositoryRequest) return;
      repoInspection = result;
      repositoryFolderError = false;
      repoPath = result.repo_path;
      selectedRemote = result.repositories[0]?.remote ?? "";
      repository = result.repository;
      manualRepository = result.repositories.length === 0;
      if (sourceRunId) {
        const seed = await getRoomSessionSeed(sourceRunId, result.repo_path);
        if (request !== repositoryRequest) return;
        sourceSession = seed;
        sourceProjectId = seed.projects.length === 1 ? seed.projects[0].id : "";
        createProject = seed.projects.length === 0 && !seed.project_error;
      }
    } catch {
      if (request === repositoryRequest) {
        repoInspection = null;
        repository = "";
        manualRepository = true;
        repositoryFolderError = true;
      }
    } finally {
      if (request === repositoryRequest) inspectingRepository = false;
    }
  }
  async function browseRepository() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const chosen = await open({
        directory: true,
        multiple: false,
        title: tr("room_chooseFolder"),
        defaultPath: repoPath.trim() || undefined,
      });
      if (typeof chosen === "string") {
        onRepositoryPathInput(chosen);
        if (repositoryTimer) clearTimeout(repositoryTimer);
        await detectRepository(chosen, repositoryRequest);
      }
    } catch {
      error = tr("room_folderPickerError");
    }
  }
  function selectRepositoryRemote(remote: string) {
    selectedRemote = remote;
    repository =
      repoInspection?.repositories.find((item) => item.remote === remote)?.repository ?? "";
  }
  async function loadRooms(preferredRoomId: string | null = null, navigate = false) {
    if (!desktop) return;
    const request = ++listGeneration;
    const selectionRequest = generation;
    loadingList = true;
    error = "";
    try {
      const result = await listRooms();
      if (request !== listGeneration || selectionRequest !== generation || showCreate || creating)
        return;
      rooms = result;
      const available = visibleRooms(result);
      // An explicit URL must never silently open a different room.
      if (preferredRoomId && !available.some((room) => room.id === preferredRoomId)) {
        selected = null;
        notice = "";
        error = tr("room_unavailableSelection");
        return;
      }
      const saved = storedSelection();
      const roomId =
        preferredRoomId ??
        available.find((room) => room.id === selected?.id)?.id ??
        available.find((room) => room.id === saved)?.id ??
        available[0]?.id;
      if (navigate && roomId && $page.url.searchParams.get("room") !== roomId) {
        await goto(`${$page.url.pathname}?room=${encodeURIComponent(roomId)}`, {
          replaceState: true,
        });
        return;
      }
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
    notice = "";
    editingTitle = false;
    editingInstructions = false;
    activeSidechatId = "";
    targetParticipantId = "";
    humanMessage = "";
    attachmentDrafts = [];
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
      if (request === generation && selected?.id === roomId) {
        applyRoom(room);
        window.dispatchEvent(new Event("ocv:room-changed"));
      }
    } catch (cause) {
      if (request === generation && selected?.id === roomId) error = String(cause);
    } finally {
      if (request === generation) busyAction = "";
    }
  }
  async function submitCreate(event: SubmitEvent) {
    event.preventDefault();
    if (
      !desktop ||
      creating ||
      busyAction ||
      loadingRoom ||
      inspectingRepository ||
      loadingSource ||
      sourceFailure ||
      (sourceRunId && !sourceSession)
    )
      return;
    if (sourceSession?.existing_room) return;
    if (sourceSession?.projects.length && !sourceProjectId && !createProject) {
      error = tr("room_chooseProject");
      return;
    }
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
    listGeneration++;
    pollGeneration++;
    creating = true;
    error = "";
    const fromSource = !!sourceSession;
    try {
      const room = sourceSession
        ? await createRoomFromSession(sourceSession.run_id, input, sourceProjectId || null)
        : await createRoom(input);
      if (request !== generation) return;
      revealRoom(room, rooms);
      rooms = [room, ...rooms.filter((r) => r.id !== room.id)];
      selected = room;
      saveSelection(room.id);
      showCreate = false;
      title = objective = repoPath = repository = "";
      repositoryRequest++;
      repoInspection = null;
      manualRepository = false;
      repositoryFolderError = false;
      inspectingRepository = false;
      createProject = true;
      sourceSession = null;
      sourceRunId = "";
      sourceProjectId = "";
      notice = fromSource
        ? tr("room_createdDestination", { title: room.title })
        : room.project
          ? tr("room_created")
          : tr("room_createdNoProject");
      window.dispatchEvent(new Event("ocv:room-changed"));
      await goto(`/rooms?room=${encodeURIComponent(room.id)}`, { replaceState: true });
    } catch (cause) {
      error = String(cause);
    } finally {
      creating = false;
    }
  }
  async function submitMessage(event: SubmitEvent) {
    event.preventDefault();
    const body = humanMessage.trim();
    if ((!body && !attachmentDrafts.length) || !selected || actionsDisabled) return;
    const id = selected.id,
      target = targetParticipantId || null,
      attachmentIds = attachmentDrafts.map((attachment) => attachment.id);
    const request = ++generation;
    pollGeneration++;
    busyAction = "message";
    error = "";
    try {
      const room = await postRoomMessage(id, body, target, activeSidechatId || null, attachmentIds);
      if (request === generation && selected?.id === id) {
        applyRoom(room);
        humanMessage = "";
        attachmentDrafts = [];
      }
    } catch (cause) {
      if (request === generation && selected?.id === id) error = String(cause);
    } finally {
      if (request === generation) busyAction = "";
    }
  }
  async function branchMessage(sourceMessageId: string, title: string, participantIds: string[]) {
    const existingIds = new Set(selected?.sidechats?.map((sidechat) => sidechat.id));
    await perform("sidechat", (id) =>
      createRoomSidechat(id, sourceMessageId, title, participantIds),
    );
    if (!error) {
      const created = selected?.sidechats?.find((sidechat) => !existingIds.has(sidechat.id));
      if (created) {
        activeSidechatId = created.id;
        humanMessage = targetParticipantId = "";
      }
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
  async function saveParticipantSettings(
    participant: RoomParticipant,
    input: ParticipantSettings,
    expected: ParticipantSettings,
  ): Promise<boolean> {
    await perform(`settings:${participant.id}`, (id) =>
      updateRoomParticipantSettings(id, participant.id, input, expected),
    );
    return !error;
  }
  function addParticipant(input: Parameters<typeof addRoomParticipant>[1]) {
    void perform("add-participant", (id) => addRoomParticipant(id, input));
  }
  function resolveRequest(request: RoomRequest, approve: boolean, response: string) {
    void perform(`request:${request.id}`, (id) =>
      resolveRoomRequest(id, request.id, approve, response),
    );
  }
  function approveAgentRequest(request: RoomRequest) {
    void perform(`request:${request.id}`, (id) => approveRoomAgent(id, request.id));
  }
  function closeRequest(request: RoomRequest, reason: string) {
    void perform(`request:${request.id}`, (id) => closeRoomRequest(id, request.id, reason));
  }
  function archiveRequests(requestIds: string[], archived: boolean) {
    void perform("archive-requests", (id) => archiveRoomRequests(id, requestIds, archived));
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
      await loadRooms(null, true);
      notice = tr("room_archived");
    }
  }
</script>

<svelte:head
  ><title
    >{tr(
      boardOnly ? "room_projectBoard" : settingsOnly ? "room_settings" : "room_pageTitle",
    )}</title
  ></svelte:head
>
<main class="room-workspace flex h-full min-h-0 min-w-0 w-full flex-col overflow-hidden p-4">
  <header class="flex shrink-0 flex-wrap items-center justify-between gap-2">
    {#if selected}<label class="min-w-0 w-full basis-full">
        <span class="sr-only">{tr("room_savedRooms")}</span>
        <select
          class="min-h-9 w-full rounded-md border bg-background px-2 text-sm"
          value={selected.id}
          disabled={!!busyAction || loadingRoom || showCreate}
          onchange={(event) =>
            void goto(
              `${$page.url.pathname}?room=${encodeURIComponent(event.currentTarget.value)}`,
            )}
        >
          {#each activeRooms as room (room.id)}<option value={room.id}>{room.title}</option>{/each}
        </select>
      </label>{/if}
    <div class={selected && !settingsOnly ? "sr-only" : ""}>
      <h1 class="text-2xl font-semibold text-foreground">
        {tr(boardOnly ? "room_projectBoard" : settingsOnly ? "room_settings" : "room_pageTitle")}
      </h1>
      <p class="mt-1 text-sm text-muted-foreground">
        {tr(
          boardOnly
            ? "room_projectBoardDescription"
            : settingsOnly
              ? "room_settingsDescription"
              : "room_pageDescription",
        )}
      </p>
    </div>
    {#if desktop}<div class="flex flex-wrap gap-2">
        {#if hiddenRooms.length}<Button
            size="sm"
            variant="outline"
            onclick={() => (showHiddenRooms = !showHiddenRooms)}
            >{tr("room_hiddenRooms", { count: String(hiddenRooms.length) })}</Button
          >{/if}
        <Button
          size="sm"
          variant="outline"
          onclick={() => void loadRooms()}
          loading={loadingList}
          disabled={!!sourceRunId || creating}>{tr("room_reloadRooms")}</Button
        >{#if !boardOnly}<Button
            size="sm"
            disabled={creating}
            onclick={() => {
              if (showCreate) cancelSetup();
              else {
                listGeneration++;
                generation++;
                pollGeneration++;
                notice = "";
                title = objective = repoPath = repository = "";
                sourceSession = null;
                repoInspection = null;
                manualRepository = false;
                repositoryFolderError = false;
                sourceProjectId = "";
                createProject = true;
                showCreate = true;
              }
            }}>{showCreate ? tr("common_cancel") : tr("room_newRoom")}</Button
          >{/if}
      </div>{/if}
  </header>
  {#if ready && !desktop}<Card class="mt-5 p-5 text-sm text-muted-foreground"
      >{tr("room_desktopOnly")}</Card
    >{:else if desktop}
    <div
      class="mt-2 grid min-h-0 flex-1 grid-cols-1 grid-rows-[minmax(0,1fr)] gap-3 overflow-hidden"
    >
      <div
        class="room-content min-h-0 min-w-0 {!boardOnly && !settingsOnly && !showCreate
          ? 'flex flex-col gap-2 overflow-hidden'
          : 'space-y-5 overflow-y-auto'}"
      >
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
        {#if showHiddenRooms && !showCreate}<Card class="p-3 space-y-2">
            <p class="text-xs text-muted-foreground">{tr("room_hiddenRoomsHelp")}</p>
            {#each hiddenRooms as room (room.id)}<div
                class="flex flex-wrap items-center justify-between gap-2 text-sm"
              >
                <span>{room.title}</span><Button
                  size="sm"
                  variant="outline"
                  onclick={() => {
                    revealRoom(room, rooms);
                    showHiddenRooms = false;
                    void selectRoom(room.id);
                    void goto(`/rooms?room=${encodeURIComponent(room.id)}`);
                  }}>{tr("room_showRoom")}</Button
                >
              </div>{/each}
          </Card>{/if}
        {#if showCreate}<Card class="p-4 md:p-5"
            ><h2 class="mb-4 text-base font-semibold">
              {tr(sourceRunId ? "room_fromSession" : "room_createTitle")}
            </h2>
            {#if loadingSource}<p role="status" class="text-sm text-muted-foreground">
                {tr("room_loadingSource")}
              </p>
            {:else if sourceFailure}
              <p role="alert" class="text-sm text-destructive">{sourceFailure}</p>
              <div class="mt-3 flex flex-wrap gap-2">
                <Button size="sm" onclick={() => void initializeSource(sourceRunId)}
                  >{tr("room_retrySetup")}</Button
                ><Button size="sm" variant="outline" onclick={cancelSetup}
                  >{tr("room_backToSource")}</Button
                >
              </div>
            {:else}
              {#if sourceSession}
                <div class="mb-4 space-y-1 rounded-md border bg-background p-3 text-sm">
                  <p>
                    <span class="text-muted-foreground">{tr("room_sourceChat")}: </span><strong
                      >{sourceSession.title}</strong
                    >
                    · {sourceSession.provider}
                  </p>
                  <p class="break-words [overflow-wrap:anywhere]">
                    <span class="text-muted-foreground"
                      >{tr("room_sourceFolder")}:
                    </span>{sourceSession.source_cwd}
                  </p>
                  {#if !sourceSession.existing_room}
                    <p class="text-xs text-muted-foreground">{tr("room_sourceSetupHelp")}</p>
                  {/if}
                </div>
              {/if}
              {#if sourceSession?.existing_room}
                {@const match = sourceSession.existing_room}
                <div class="space-y-3">
                  <h3 class="text-base font-semibold">{tr("room_existingSessionRoom")}</h3>
                  <p><strong>{match.title}</strong></p>
                  <p class="text-sm text-muted-foreground break-words [overflow-wrap:anywhere]">
                    {match.repository} · {match.repo_path}
                  </p>
                  <p class="text-sm text-muted-foreground">
                    {tr(
                      match.source_attached ? "room_sourceAlreadyAttached" : "room_sourceDetached",
                    )}
                  </p>
                  <div class="flex flex-wrap gap-2">
                    <Button size="sm" disabled={creating} onclick={() => void openSessionRoom()}
                      >{tr("room_openNamedRoom", { title: match.title })}</Button
                    >
                    {#if !match.source_attached && !match.archived}<Button
                        size="sm"
                        variant="outline"
                        disabled={creating}
                        onclick={() => void openSessionRoom(true)}
                        >{tr("room_restoreOriginalAgent")}</Button
                      >{/if}
                    <Button size="sm" variant="outline" onclick={cancelSetup}
                      >{tr("room_backToSource")}</Button
                    >
                  </div>
                </div>
              {:else}
                <!-- Prevent implicit Enter submission while retaining native keyboard controls. -->
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <form
                  class="grid gap-3 md:grid-cols-2"
                  onsubmit={submitCreate}
                  onkeydown={(event) => {
                    if (event.key === "Enter" && event.target instanceof HTMLInputElement)
                      event.preventDefault();
                  }}
                >
                  <label class="min-w-0 space-y-1 text-xs text-muted-foreground md:col-span-2"
                    ><span>{tr("room_titleLabel")}</span><Input
                      bind:value={title}
                      disabled={creating}
                    /></label
                  >
                  <div class="min-w-0 space-y-2 text-xs text-muted-foreground md:col-span-2">
                    <span class="block">{tr("room_repoPathLabel")}</span>
                    <div class="flex min-w-0 flex-wrap items-center gap-2">
                      <Button
                        type="button"
                        variant="outline"
                        onclick={() => void browseRepository()}
                        disabled={inspectingRepository || creating}
                      >
                        {tr("room_chooseFolder")}
                      </Button>
                      <span
                        class="min-w-0 flex-1 break-words [overflow-wrap:anywhere]"
                        title={repoPath}>{repoPath || tr("room_noFolderSelected")}</span
                      >
                      {#if inspectingRepository}<span role="status"
                          >{tr("room_detectingRepository")}</span
                        >{/if}
                    </div>
                    {#if sourceSession && !repoPath}<p class="text-muted-foreground" role="status">
                        {tr("room_chooseSourceRepository")}
                      </p>{/if}
                    {#if repositoryFolderError}<p class="text-destructive" role="status">
                        {tr("room_invalidGitFolderHelp")}
                      </p>{/if}
                    <details class="min-w-0">
                      <summary class="cursor-pointer text-primary"
                        >{tr("room_advancedFolderPath")}</summary
                      >
                      <label class="mt-2 block min-w-0 space-y-1"
                        ><span>{tr("room_repoPathLabel")}</span><Input
                          value={repoPath}
                          oninput={(event) =>
                            onRepositoryPathInput((event.currentTarget as HTMLInputElement).value)}
                          placeholder="/path/to/repository"
                        /></label
                      >
                    </details>
                  </div>
                  {#if repoInspection && repoInspection.repositories.length > 1 && !manualRepository}
                    <label class="min-w-0 space-y-1 text-xs text-muted-foreground md:col-span-2">
                      <span>{tr("room_repositoryLabel")}</span><select
                        class="h-9 w-full min-w-0 rounded-md border bg-background px-2 text-sm text-foreground"
                        value={selectedRemote}
                        onchange={(event) => selectRepositoryRemote(event.currentTarget.value)}
                      >
                        {#each repoInspection.repositories as item (item.remote)}<option
                            value={item.remote}>{item.remote} · {item.repository}</option
                          >{/each}
                      </select>
                    </label>
                  {:else if repoInspection && repoInspection.repositories.length && !manualRepository}
                    <div class="flex min-w-0 flex-wrap items-center gap-2 text-xs md:col-span-2">
                      <span
                        class="min-w-0 break-words text-muted-foreground [overflow-wrap:anywhere]"
                        >{tr("room_githubRepository")}: {repository}</span
                      >
                      <button
                        type="button"
                        class="text-primary underline underline-offset-4"
                        onclick={() => (manualRepository = true)}
                        >{tr("room_editRepository")}</button
                      >
                    </div>
                  {/if}
                  {#if manualRepository && !repositoryFolderError}
                    <label class="min-w-0 space-y-1 text-xs text-muted-foreground md:col-span-2">
                      <span>{tr("room_repositoryLabel")}</span><Input
                        bind:value={repository}
                        placeholder="OWNER/REPO"
                      />
                      {#if repoInspection && repoInspection.repositories.length === 0}<span
                          class="block">{tr("room_noGithubRemoteHelp")}</span
                        >{/if}
                      {#if repoInspection && repoInspection.repositories.length > 0}<span
                          class="block">{tr("room_repositoryOverrideHelp")}</span
                        >{/if}
                    </label>
                  {:else if sourceSession && !repoInspection}
                    <div class="flex min-w-0 flex-wrap items-center gap-2 text-xs md:col-span-2">
                      <span
                        class="min-w-0 break-words text-muted-foreground [overflow-wrap:anywhere]"
                        >{tr("room_githubRepository")}: {repository}</span
                      ><button
                        type="button"
                        class="text-primary underline underline-offset-4"
                        onclick={() => (manualRepository = true)}
                        >{tr("room_editRepository")}</button
                      >
                    </div>
                  {/if}
                  <p class="text-xs text-muted-foreground md:col-span-2">
                    {tr("room_repositoryHelp")}
                  </p>
                  <label class="min-w-0 space-y-1 text-xs text-muted-foreground md:col-span-2"
                    ><span>{tr("room_objectiveLabel")}</span><Textarea
                      bind:value={objective}
                      rows={3}
                    /></label
                  >{#if sourceSession?.projects.length}<label
                      class="min-w-0 space-y-1 text-xs text-muted-foreground md:col-span-2"
                    >
                      <span>{tr("room_existingProject")}</span>
                      <select
                        class="h-9 w-full rounded-md border bg-background px-2 text-sm text-foreground"
                        bind:value={sourceProjectId}
                      >
                        <option value="">{tr("room_chooseProject")}</option>
                        {#each sourceSession.projects as project (project.id)}<option
                            value={project.id}>{project.title} · #{project.number}</option
                          >{/each}
                      </select>
                      <p>{tr("room_existingProjectHelp")}</p>
                    </label>{/if}
                  {#if sourceSession?.project_error}<p
                      class="text-sm text-muted-foreground md:col-span-2"
                      role="status"
                    >
                      {tr("room_projectDiscoveryError")}
                    </p>{/if}
                  <label class="flex items-center gap-2 text-sm md:col-span-2"
                    ><input
                      type="checkbox"
                      bind:checked={createProject}
                      disabled={!!sourceProjectId}
                    />{tr("room_createProject")}</label
                  >
                  <div class="md:col-span-2">
                    {#if sourceSession}<p class="mb-2 text-xs text-muted-foreground">
                        {tr("room_roomDestination", {
                          title,
                          folder: repoPath || tr("room_noFolderSelected"),
                        })}
                      </p>{/if}
                    <Button
                      type="submit"
                      loading={creating}
                      disabled={inspectingRepository ||
                        repositoryFolderError ||
                        !repoPath ||
                        !repository ||
                        !title.trim() ||
                        !objective.trim()}
                      >{tr(
                        sourceSession ? "room_createFromSourceAction" : "room_createRoom",
                      )}</Button
                    >
                  </div>
                </form>{/if}{/if}</Card
          >{/if}
        {#if selected && !showCreate}
          {#if !boardOnly && !settingsOnly}
            <div class="flex shrink-0 flex-wrap items-center justify-between gap-2">
              <div class="min-w-0">
                <h2 class="text-base font-semibold break-words">{selected.title}</h2>
                <p
                  class="min-w-0 break-words text-xs text-muted-foreground [overflow-wrap:anywhere]"
                >
                  {selected.repository}
                </p>
              </div>
              <div class="flex flex-wrap items-center gap-2 text-xs">
                <Button size="sm" variant="outline" onclick={() => toggleRequests()}
                  >{tr("room_requestsTitle")} · {openRequestCount}
                  {requestsOpen ? "▾" : "▸"}</Button
                >
                {#if selected.origin}<a
                    class="text-primary underline underline-offset-4"
                    href={`/chat?run=${encodeURIComponent(selected.origin.run_id)}`}
                    title={tr("room_sourceSessionHelp")}>{tr("room_originalConversation")}</a
                  >{/if}
                <Button
                  size="sm"
                  variant="outline"
                  onclick={() =>
                    void goto(`/rooms/settings?room=${encodeURIComponent(selected!.id)}`)}
                  >{tr("room_settings")}</Button
                >
                <Button
                  size="sm"
                  variant="outline"
                  onclick={() =>
                    void perform("pause-room", (id) => setRoomPaused(id, !selected?.paused))}
                  loading={busyAction === "pause-room"}
                  disabled={actionsDisabled}
                  >{selected.paused ? tr("room_resume") : tr("room_pause")}</Button
                >
              </div>
            </div>
          {:else}
            <Card class={boardOnly ? "shrink-0 p-3" : "shrink-0 p-4 md:p-5"}
              ><div class="flex flex-wrap items-start justify-between gap-3">
                <div class="min-w-0">
                  <h2
                    class={`min-w-0 break-words font-semibold [overflow-wrap:anywhere] ${boardOnly ? "text-base" : "text-xl"}`}
                  >
                    {selected.title}
                  </h2>
                  {#if boardOnly}<details class="mt-1 text-xs text-muted-foreground">
                      <summary class="cursor-pointer">{tr("room_objectiveLabel")}</summary>
                      <p class="mt-1 whitespace-pre-wrap">{selected.objective}</p>
                      <p class="mt-1">Local folder: {selected.repo_path}</p>
                    </details>{:else if settingsOnly}<p
                      class="mt-2 whitespace-pre-wrap text-sm text-muted-foreground"
                    >
                      {selected.objective}
                    </p>{/if}
                  <p
                    class="mt-2 break-words text-xs text-muted-foreground [overflow-wrap:anywhere]"
                  >
                    {selected.repository}{#if !boardOnly}
                      · {selected.repo_path}{/if}
                  </p>
                  {#if selected.origin && boardOnly}<p class="mt-2 text-xs text-muted-foreground">
                      {#if boardOnly || settingsOnly}{tr("room_startingConversation", {
                          count: String(selected.origin.message_count),
                        })}{/if}
                      <a
                        class="ml-2 text-primary underline underline-offset-4"
                        href={`/chat?run=${encodeURIComponent(selected.origin.run_id)}`}
                        >{tr("room_originalConversation")}</a
                      >
                    </p>{/if}
                </div>
                {#if !boardOnly}<div class="flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      onclick={() =>
                        void perform("pause-room", (id) => setRoomPaused(id, !selected?.paused))}
                      loading={busyAction === "pause-room"}
                      disabled={actionsDisabled}
                      >{selected.paused ? tr("room_resume") : tr("room_pause")}</Button
                    >{#if settingsOnly}<Button
                        variant="outline"
                        onclick={() =>
                          void perform("auto-continue", (id) =>
                            setRoomAutoContinue(id, !selected?.auto_continue),
                          )}
                        disabled={actionsDisabled}
                        >{selected.auto_continue
                          ? tr("room_autoContinueOn")
                          : tr("room_autoContinueOff")}</Button
                      >{/if}
                  </div>{/if}
              </div>
              {#if settingsOnly}
                <div class="mt-3">
                  {#if editingTitle}<form
                      class="space-y-2"
                      onsubmit={async (event) => {
                        event.preventDefault();
                        await perform("title", (id) =>
                          saveRoomTitle(id, titleDraft, titleOriginal),
                        );
                        if (!error) editingTitle = false;
                      }}
                    >
                      <label class="block text-sm space-y-1"
                        ><span>{tr("room_titleLabel")}</span><Input
                          bind:value={titleDraft}
                          disabled={!!busyAction}
                        /></label
                      >
                      <div class="flex flex-wrap gap-2">
                        <Button type="submit" disabled={actionsDisabled || !titleDraft.trim()}
                          >{tr("room_saveName")}</Button
                        ><Button
                          type="button"
                          variant="outline"
                          onclick={() => (editingTitle = false)}>{tr("common_cancel")}</Button
                        >
                      </div>
                    </form>{:else}<Button
                      variant="outline"
                      disabled={actionsDisabled}
                      onclick={() => {
                        titleDraft = titleOriginal = selected!.title;
                        editingTitle = true;
                      }}>{tr("room_rename")}</Button
                    >{/if}
                </div>
                <div class="mt-4 border-t pt-4">
                  {#if editingInstructions}
                    <form
                      class="space-y-3"
                      onsubmit={async (event) => {
                        event.preventDefault();
                        await perform("instructions", (id) =>
                          saveRoomInstructions(id, instructionsDraft, instructionsOriginal),
                        );
                        if (!error) editingInstructions = false;
                      }}
                    >
                      <label class="block space-y-2 text-sm">
                        <span>{tr("room_instructions")}</span>
                        <Textarea bind:value={instructionsDraft} rows={8} maxlength={32000} />
                      </label>
                      <p class="text-xs text-muted-foreground">{tr("room_instructionsHelp")}</p>
                      <div class="flex flex-wrap gap-2">
                        <Button
                          disabled={actionsDisabled || !instructionsDraft.trim()}
                          loading={busyAction === "instructions"}
                          >{tr("room_saveInstructions")}</Button
                        >
                        <Button
                          type="button"
                          variant="outline"
                          disabled={!!busyAction}
                          onclick={() => {
                            editingInstructions = false;
                          }}>{tr("room_cancelInstructions")}</Button
                        >
                      </div>
                    </form>
                  {:else}
                    <Button
                      variant="outline"
                      disabled={actionsDisabled}
                      onclick={() => {
                        instructionsDraft = instructionsOriginal = selected?.objective ?? "";
                        editingInstructions = true;
                      }}>{tr("room_editInstructions")}</Button
                    >
                  {/if}
                </div>
              {/if}
              {#if selected.runtime_error}<p
                  class="mt-3 rounded border border-destructive/30 bg-destructive/5 p-2 text-sm text-destructive"
                  role="status"
                >
                  {tr("room_runtimeError", { error: selected.runtime_error })}
                </p>{/if}
              {#if settingsOnly}<div class="mt-4 flex flex-wrap items-center gap-3 border-t pt-4">
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
                      <label class="min-w-0 space-y-1 text-xs text-muted-foreground"
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
                    >{:else}<span class="text-xs text-muted-foreground"
                      >{tr("room_roomArchived")}</span
                    >{/if}
                </div>
                <div class="mt-4 space-y-2 border-t pt-4">
                  <label class="flex flex-wrap items-center gap-3 text-sm">
                    <span>{tr("room_concurrency")}</span>
                    <select
                      class="h-9 rounded-md border bg-background px-2 text-sm"
                      value={selected.max_concurrent}
                      disabled={actionsDisabled}
                      onchange={(event) =>
                        void perform("concurrency", (id) =>
                          setRoomConcurrency(id, Number(event.currentTarget.value)),
                        )}
                    >
                      {#each [1, 2, 3, 4, 5] as limit}<option value={limit}>{limit}</option>{/each}
                    </select>
                  </label>
                  <p class="text-xs text-muted-foreground">{tr("room_concurrencyHelp")}</p>
                </div>{/if}
              {#if boardOnly && selected.project}<a
                  class="mt-3 inline-block text-sm text-primary underline underline-offset-4"
                  href={selected.project.url}
                  target="_blank"
                  rel="noreferrer"
                  >{tr("room_openProject", { number: String(selected.project.number) })}</a
                >{/if}
              {#if !boardOnly}<div class="mt-3 flex flex-wrap gap-3 text-sm">
                  {#if settingsOnly}<a
                      class="text-primary underline underline-offset-4"
                      href="/rooms">{tr("room_groupChat")}</a
                    >
                  {:else}<a
                      class="text-muted-foreground underline underline-offset-4"
                      href="/rooms/settings">{tr("room_settings")}</a
                    >{/if}
                </div>{/if}</Card
            >{/if}
          {#if loadingRoom}<p class="text-sm text-muted-foreground" role="status">
              {tr("room_loading")}
            </p>{/if}
          {#if settingsOnly}<RoomAttachChat
              roomId={selected.id}
              disabled={actionsDisabled}
              onAttach={async (runId, name) => {
                await perform("attach-chat", (id) => attachRoomChat(id, runId, name));
                return !error;
              }}
            /><RoomParticipants
              participants={selected.participants}
              claims={selected.claims}
              disabled={actionsDisabled}
              {busyAction}
              onAction={participantAction}
              onAdd={addParticipant}
              onSave={saveParticipantSettings}
            />
            <RoomTimers
              timers={selected.timers}
              participants={selected.participants}
              disabled={actionsDisabled}
              {busyAction}
              onSave={saveTimer}
              onDelete={deleteTimer}
            />{/if}
          {#if boardOnly && selected.project}<RoomBoard
              roomId={selected.id}
              board={selected.board}
              claims={selected.claims}
              participants={selected.participants}
              loading={busyAction === "board"}
              canRefresh={!actionsDisabled}
              onrefresh={() => void perform("board", refreshRoomBoard)}
            />{:else if boardOnly}<p class="text-sm text-muted-foreground">
              {tr("room_boardNeedsProject")}
            </p>{/if}
          {#if !boardOnly && !settingsOnly}
            <div
              class="room-chat-layout min-h-0 min-w-0 flex-1"
              class:requests-hidden={!requestsOpen}
              style={`--requests-width:${requestsWidth}%`}
              bind:this={chatLayout}
            >
              <div class="flex min-h-0 min-w-0 flex-col overflow-hidden">
                <RoomConversation
                  {selected}
                  {actionsDisabled}
                  {busyAction}
                  bind:humanMessage
                  bind:targetParticipantId
                  bind:activeSidechatId
                  bind:attachmentDrafts
                  onBranch={branchMessage}
                  {submitMessage}
                  onParticipantAction={participantAction}
                />
              </div>
              <button
                type="button"
                class="request-resizer"
                class:hidden={!requestsOpen}
                aria-label={tr("room_requestsResize")}
                onkeydown={resizeRequestsKey}
                onpointerdown={(event) => {
                  resizingRequests = true;
                  event.currentTarget.setPointerCapture(event.pointerId);
                }}
                onpointermove={resizeRequests}
                onpointerup={() => {
                  resizingRequests = false;
                }}
                onpointercancel={() => {
                  resizingRequests = false;
                }}
                ondblclick={() => {
                  requestsWidth = 32;
                  saveRequestPanel();
                }}>⋮</button
              >
              <div class="min-h-0 min-w-0" class:hidden={!requestsOpen}>
                {#key selected.id}<RoomRequests
                    room={selected}
                    disabled={loadingRoom || !!busyAction || creating}
                    {busyAction}
                    onResolve={resolveRequest}
                    onApproveAgent={approveAgentRequest}
                    onCloseRequest={closeRequest}
                    onArchive={archiveRequests}
                    onClosePanel={() => toggleRequests(false)}
                    revealToken={requestsRevealToken}
                  />{/key}
              </div>
            </div>
          {/if}
          {#if boardOnly && selected.claims.length > 0}<Card class="p-4"
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
          {#if boardOnly && selected.participants.some((p) => p.worktree_path)}
            <details class="rounded-lg border p-3">
              <summary class="cursor-pointer text-sm font-medium">{tr("room_workspaces")}</summary>
              <div class="mt-3 space-y-2">
                {#each selected.participants.filter((p) => p.worktree_path) as participant (participant.id)}
                  <div class="flex min-w-0 flex-wrap items-center justify-between gap-2 text-xs">
                    <span class="min-w-0 break-words"
                      >{participant.name} · {participant.branch}</span
                    >
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={actionsDisabled ||
                        !!participant.pending_delivery ||
                        ["busy", "running", "starting", "working"].includes(participant.state) ||
                        (!selected.paused && !participant.paused)}
                      onclick={() => participantAction("merge", participant)}
                      >{tr("room_mergeWorktree")}</Button
                    >
                  </div>
                {/each}
              </div>
            </details>
          {/if}
        {:else if !loadingList}<Card variant="subtle" class="p-6 text-sm text-muted-foreground"
            >{tr("room_selectOrCreate")}</Card
          >{/if}
      </div>
    </div>
  {/if}
</main>

<style>
  .room-content {
    container-type: inline-size;
  }
  .room-chat-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
    gap: 0.5rem;
  }
  .request-resizer {
    display: none;
  }
  .room-chat-layout.requests-hidden {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
  }
  @container (min-width: 30rem) {
    .room-chat-layout {
      grid-template-columns: minmax(0, 1fr) 0.375rem clamp(
          14rem,
          var(--requests-width),
          calc(100% - 14rem - 0.875rem)
        );
      grid-template-rows: minmax(0, 1fr);
      gap: 0.25rem;
    }
    .request-resizer:not(.hidden) {
      display: block;
      cursor: col-resize;
      touch-action: none;
      border-radius: 0.25rem;
      font-size: 0.75rem;
      color: hsl(var(--muted-foreground));
    }
    .request-resizer:hover,
    .request-resizer:focus-visible {
      background: hsl(var(--accent));
      outline: 1px solid hsl(var(--ring));
    }
  }
</style>
