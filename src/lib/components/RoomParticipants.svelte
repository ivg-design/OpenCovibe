<script lang="ts">
  import Button from "$lib/components/Button.svelte";
  import RoomModelEffort from "$lib/components/RoomModelEffort.svelte";
  import RoomTurnLimit from "$lib/components/RoomTurnLimit.svelte";
  import Card from "$lib/components/Card.svelte";
  import Input from "$lib/components/Input.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { roomAgentStateLabel } from "$lib/rooms/state-label";
  import type { RoomParticipant, ParticipantSettings } from "$lib/rooms/types";

  let {
    participants,
    claims,
    disabled,
    roomPaused,
    busyAction,
    onAction,
    onAdd,
    onSave,
  }: {
    participants: RoomParticipant[];
    claims: { task_id: string; participant_id: string; state: string }[];
    disabled: boolean;
    roomPaused: boolean;
    busyAction: string;
    onAction: (action: string, participant: RoomParticipant, message?: string) => void;
    onSave: (
      participant: RoomParticipant,
      input: ParticipantSettings,
      expected: ParticipantSettings,
    ) => Promise<boolean>;
    onAdd: (input: {
      name: string;
      provider: "codex" | "claude";
      model: string | null;
      effort: string | null;
      use_worktree: boolean;
      max_turns: number;
    }) => void;
  } = $props();
  let name = $state("");
  let provider = $state<"codex" | "claude">("codex");
  let model = $state("");
  let effort = $state("");
  let useWorktree = $state(false);
  let maxTurns = $state("10");
  let limitTurns = $state(false);
  let editingId = $state("");
  let editName = $state("");
  let editModel = $state("");
  let editEffort = $state("");
  let editMaxTurns = $state("10");
  let editLimitTurns = $state(false);
  let expectedSettings = $state<ParticipantSettings | null>(null);
  const validLimit = (enabled: boolean, count: string) =>
    !enabled || (Number.isInteger(Number(count)) && Number(count) >= 1 && Number(count) <= 200);
  function editAgent(participant: RoomParticipant) {
    editingId = participant.id;
    editName = participant.name;
    editModel = participant.model ?? "";
    editEffort = participant.effort ?? "";
    editMaxTurns = String(participant.max_turns || 10);
    editLimitTurns = participant.max_turns > 0;
    expectedSettings = {
      name: participant.name,
      model: participant.model,
      effort: participant.effort,
      max_turns: participant.max_turns,
    };
  }
  async function saveAgent(event: SubmitEvent, participant: RoomParticipant) {
    event.preventDefault();
    if (!expectedSettings || !editName.trim() || !validLimit(editLimitTurns, editMaxTurns)) return;
    if (
      await onSave(
        participant,
        {
          name: editName.trim(),
          model: editModel || null,
          effort: editEffort || null,
          max_turns: editLimitTurns ? Number(editMaxTurns) : 0,
        },
        expectedSettings,
      )
    )
      editingId = "";
  }
  const executionChanged = (p: RoomParticipant) =>
    editModel !== (p.model ?? "") ||
    editEffort !== (p.effort ?? "") ||
    (editLimitTurns ? Number(editMaxTurns) : 0) !== p.max_turns;
  const editable = (p: RoomParticipant) =>
    p.paused &&
    !p.pending_delivery &&
    !["busy", "running", "starting", "working", "waiting"].includes(p.state);

  let wakeMessages = $state<Record<string, string>>({});

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!name.trim() || !validLimit(limitTurns, maxTurns)) return;
    onAdd({
      name: name.trim(),
      provider,
      model: model.trim() || null,
      effort: effort.trim() || null,
      use_worktree: useWorktree,
      max_turns: limitTurns ? Number(maxTurns) : 0,
    });
    name = "";
  }
  const active = (p: RoomParticipant) =>
    !p.paused &&
    ["running", "working", "active", "starting", "busy"].includes(p.state.toLowerCase());
</script>

<section class="space-y-3">
  <div class="flex items-center justify-between">
    <h2 class="text-base font-semibold">{t("room_participants")}</h2>
    <span class="text-xs text-muted-foreground">{participants.length}</span>
  </div>
  <form class="room-form grid gap-2 rounded-lg border bg-card p-3" onsubmit={submit}>
    <label class="min-w-0 space-y-1 text-xs text-muted-foreground"
      ><span>{t("room_participantName")}</span><Input bind:value={name} /></label
    >
    <label class="min-w-0 space-y-1 text-xs text-muted-foreground"
      ><span>{t("room_provider")}</span><select
        class="h-9 w-full min-w-0 rounded-md border bg-background px-2 text-sm text-foreground"
        bind:value={provider}
        onchange={() => {
          model = effort = "";
        }}><option value="codex">Codex</option><option value="claude">Claude</option></select
      ></label
    >
    <RoomModelEffort {provider} bind:model bind:effort {disabled} />
    <RoomTurnLimit bind:enabled={limitTurns} bind:count={maxTurns} {disabled} />
    <label class="flex items-center gap-2 text-sm"
      ><input type="checkbox" bind:checked={useWorktree} />{t("room_useWorktree")}</label
    >
    <div class="sm:col-span-2 lg:col-span-3">
      <Button
        disabled={disabled || !name.trim() || !validLimit(limitTurns, maxTurns)}
        loading={busyAction === "add-participant"}>{t("room_addParticipant")}</Button
      >
    </div>
  </form>
  {#if participants.length === 0}<Card variant="subtle" class="p-4 text-sm text-muted-foreground"
      >{t("room_noParticipants")}</Card
    >{/if}
  <div class="room-peer-grid grid gap-3">
    {#each participants as participant, index (participant.id)}
      {@const isActive = active(participant)}
      {@const claimsOwned = claims.filter(
        (claim) =>
          claim.participant_id === participant.id &&
          !["done", "released"].includes(claim.state.toLowerCase()),
      )}
      <Card class="space-y-3 p-4">
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div class="min-w-0">
            <h3 class="break-words font-medium text-foreground">
              <span
                class="mr-2 inline-block h-2.5 w-2.5 rounded-full"
                style={`background-color: ${["#5677c8", "#b36a9a", "#538b70", "#bd8250", "#7481aa", "#ad665d"][index % 6]}`}
                aria-hidden="true"
              ></span>{participant.name}
            </h3>
            <p class="break-words text-xs text-muted-foreground">
              {participant.provider} · {participant.model ?? t("room_defaultModel")} · {participant.effort ??
                t("room_defaultEffort")}
            </p>
          </div>
          <span class="shrink-0 whitespace-nowrap rounded bg-muted px-2 py-1 text-xs"
            >{roomAgentStateLabel(participant.state)}</span
          >
        </div>
        <div class="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
          <div class="min-w-0 break-words">
            {participant.max_turns === 0
              ? t("room_budgetUnlimited", { used: String(participant.wake_count) })
              : t("room_budget", {
                  used: String(participant.wake_count),
                  max: String(participant.max_turns),
                })}
          </div>
          <div class="min-w-0 break-all">
            {participant.branch
              ? t("room_branch", { branch: participant.branch })
              : t("room_noBranch")}
          </div>
        </div>
        {#if participant.last_error}<p
            class="break-words rounded border border-destructive/30 bg-destructive/5 p-2 text-xs text-destructive"
            role="status"
          >
            {participant.last_error}
          </p>{/if}
        {#if participant.pending_delivery}<p
            class="break-words rounded border border-amber-500/30 bg-amber-500/5 p-2 text-xs"
          >
            {t("room_pendingDelivery", { reason: participant.pending_delivery.reason })}
          </p>{/if}
        {#if editingId === participant.id}
          <form
            class="room-form grid gap-3 rounded-md border p-3"
            onsubmit={(event) => saveAgent(event, participant)}
          >
            <label
              class="min-w-0 space-y-1 text-xs text-muted-foreground sm:col-span-2 lg:col-span-3"
              ><span>{t("room_participantName")}</span><Input
                bind:value={editName}
                {disabled}
              /></label
            >
            <RoomModelEffort
              provider={participant.provider}
              bind:model={editModel}
              bind:effort={editEffort}
              disabled={disabled || !editable(participant)}
            />
            <RoomTurnLimit
              bind:enabled={editLimitTurns}
              bind:count={editMaxTurns}
              disabled={disabled || !editable(participant)}
            />
            <p class="text-xs text-muted-foreground sm:col-span-2 lg:col-span-3">
              {t("room_agentSettingsHelp")}
            </p>
            <div class="flex flex-wrap gap-2 sm:col-span-2 lg:col-span-3">
              <Button
                size="sm"
                loading={busyAction === `settings:${participant.id}`}
                disabled={disabled ||
                  !editName.trim() ||
                  (!editable(participant) && executionChanged(participant)) ||
                  !validLimit(editLimitTurns, editMaxTurns)}>{t("common_save")}</Button
              >
              <Button
                type="button"
                size="sm"
                variant="outline"
                {disabled}
                onclick={() => {
                  editingId = "";
                }}>{t("common_cancel")}</Button
              >
            </div>
          </form>
        {/if}
        <div class="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            variant="outline"
            {disabled}
            onclick={() => editAgent(participant)}>{t("room_editAgentSettings")}</Button
          >
          <a
            class="inline-flex h-8 items-center rounded-md border px-3 text-xs hover:bg-accent"
            href={`/chat?run=${encodeURIComponent(participant.run_id)}`}>{t("room_openSession")}</a
          >
          <Button
            size="sm"
            variant="outline"
            disabled={disabled ||
              isActive ||
              !participant.worktree_path ||
              (!roomPaused && !participant.paused)}
            onclick={() => onAction("merge", participant)}>{t("room_mergeWorktree")}</Button
          >
          <Button
            size="sm"
            variant="outline"
            {disabled}
            onclick={() =>
              onAction(
                participant.paused ? "resume-participant" : "pause-participant",
                participant,
              )}
            >{participant.paused ? t("room_resumeParticipant") : t("room_pauseParticipant")}</Button
          >
          <Button
            size="sm"
            variant="outline"
            disabled={disabled || isActive || claimsOwned.length > 0}
            onclick={() => onAction("remove-participant", participant)}
            >{t("room_removeParticipant")}</Button
          >
        </div>
        {#if !editable(participant)}<p class="text-xs text-muted-foreground">
            {t("room_pauseToEditAgent")}
          </p>{/if}
        <form
          class="flex flex-wrap gap-2"
          onsubmit={(event) => {
            event.preventDefault();
            const message = wakeMessages[participant.id]?.trim();
            if (message) {
              onAction("wake", participant, message);
              wakeMessages[participant.id] = "";
            }
          }}
        >
          <label class="min-w-0 flex-1"
            ><span class="sr-only">{t("room_wakeMessageLabel", { name: participant.name })}</span
            ><Input
              class="w-full"
              value={wakeMessages[participant.id] ?? ""}
              oninput={(event) => {
                wakeMessages[participant.id] = (event.currentTarget as HTMLInputElement).value;
              }}
              placeholder={t("room_wakeMessagePlaceholder")}
            /></label
          >
          <Button size="sm" disabled={disabled || !wakeMessages[participant.id]?.trim()}
            >{t("room_wake")}</Button
          >
        </form>
        {#if claimsOwned.length > 0}<div class="space-y-1 border-t pt-2">
            <p class="text-xs font-medium">{t("room_ownedClaims")}</p>
            {#each claimsOwned as claim (claim.task_id)}<div
                class="flex items-center justify-between gap-2 text-xs"
              >
                <span>{claim.task_id} · {claim.state}</span><Button
                  size="sm"
                  variant="outline"
                  disabled={disabled || isActive || (!roomPaused && !participant.paused)}
                  onclick={() => onAction(`release:${claim.task_id}`, participant)}
                  >{t("room_releaseClaim")}</Button
                >
              </div>{/each}
          </div>{/if}
      </Card>
    {/each}
  </div>
</section>
