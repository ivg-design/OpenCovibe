<script lang="ts">
  import { onMount } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import {
    getModelsForAgent,
    getCodexDefaultModel,
    getCliCurrentModel,
    loadCodexModels,
    loadCliInfo,
  } from "$lib/stores/cli-info.svelte";
  import { roomEffortLevels, effortAfterModelChange } from "$lib/rooms/model-options";

  let {
    provider,
    model = $bindable(""),
    effort = $bindable(""),
    disabled = false,
  }: {
    provider: "codex" | "claude";
    model: string;
    effort: string;
    disabled?: boolean;
  } = $props();
  let refreshing = $state(false);
  let models = $derived(getModelsForAgent(provider));
  let defaultModel = $derived(provider === "codex" ? getCodexDefaultModel() : getCliCurrentModel());
  let efforts = $derived(roomEffortLevels(models, model, defaultModel));
  let unknownModel = $derived(!!model && !models.some((entry) => entry.value === model));

  async function refresh(force = false) {
    refreshing = true;
    try {
      await Promise.all([loadCodexModels(force), loadCliInfo(force)]);
    } finally {
      refreshing = false;
    }
  }
  onMount(() => {
    void refresh();
  });
</script>

<label class="min-w-0 space-y-1 text-xs text-muted-foreground">
  <span>{t("room_modelOptional")}</span>
  <select
    class="h-9 w-full min-w-0 rounded-md border bg-background px-2 text-sm text-foreground"
    value={model}
    {disabled}
    onchange={(event) => {
      model = event.currentTarget.value;
      effort = effortAfterModelChange(models, model, effort, defaultModel);
    }}
  >
    <option value="">{t("room_defaultModel")}</option>
    {#if unknownModel}<option value={model}>{model} ({t("room_currentModel")})</option>{/if}
    {#each models as entry (entry.value)}<option value={entry.value}>{entry.displayName}</option
      >{/each}
  </select>
</label>
<label class="min-w-0 space-y-1 text-xs text-muted-foreground">
  <span>{t("room_effortOptional")}</span>
  <select
    class="h-9 w-full min-w-0 rounded-md border bg-background px-2 text-sm text-foreground"
    bind:value={effort}
    disabled={disabled || efforts.length === 0}
  >
    <option value="">{t("room_defaultEffort")}</option>
    {#if effort && !efforts.includes(effort)}<option value={effort}
        >{effort} ({t("room_currentModel")})</option
      >{/if}
    {#each efforts as level}<option value={level}>{level}</option>{/each}
  </select>
  {#if efforts.length === 0}<span class="block">{t("room_effortAutomatic")}</span>{/if}
</label>
<div
  class="flex min-w-0 flex-wrap items-center gap-2 text-xs text-muted-foreground sm:col-span-2 lg:col-span-3"
>
  <Button
    type="button"
    size="sm"
    variant="ghost"
    disabled={disabled || refreshing}
    loading={refreshing}
    onclick={() => refresh(true)}>{t("room_refreshModels")}</Button
  >
  {#if !refreshing && models.length === 0}<span role="status"
      >{t("statusbar_modelsUnavailable")}</span
    >{/if}
</div>
