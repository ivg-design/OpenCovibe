<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  let {
    enabled = $bindable(false),
    count = $bindable("10"),
    disabled = false,
  }: {
    enabled: boolean;
    count: string;
    disabled?: boolean;
  } = $props();
</script>

<div class="min-w-0 space-y-2 sm:col-span-2 lg:col-span-3">
  <label class="flex items-center gap-2 text-sm"
    ><input type="checkbox" bind:checked={enabled} {disabled} />{t("room_limitTurns")}</label
  >
  {#if enabled}<label class="block min-w-0 space-y-1 text-xs text-muted-foreground"
      ><span>{t("room_maxTurns")}</span>
      <input
        type="number"
        min="1"
        max="200"
        step="1"
        required
        bind:value={count}
        {disabled}
        class="h-9 w-full min-w-0 rounded-md border bg-background px-2 text-sm text-foreground"
      />
    </label>{/if}
  <p class="text-xs text-muted-foreground">
    {t(enabled ? "room_turnLimitHelp" : "room_noTurnLimitHelp")}
  </p>
</div>
