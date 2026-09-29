<script lang="ts">
  import { getContext, onMount, tick } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import {
    PALETTE_FIELDS,
    contrastRatio,
    hslChannelsToHex,
    isHexColor,
    readCustomPalette,
    saveCustomPalette,
    type CustomPalette,
    type PaletteField,
  } from "$lib/utils/custom-palette";

  interface PaletteController {
    readonly palette: CustomPalette;
    setPalette(palette: CustomPalette): boolean;
  }
  const controller = getContext<PaletteController>("customPalette");

  const labels: Record<PaletteField, () => string> = {
    primary: () => t("palette_primary"),
    background: () => t("palette_background"),
    sidebar: () => t("palette_sidebar"),
    foreground: () => t("palette_foreground"),
    border: () => t("palette_border"),
  };
  const themeVariables: Record<PaletteField, string> = {
    primary: "--primary",
    background: "--background",
    sidebar: "--sidebar-background",
    foreground: "--foreground",
    border: "--border",
  };
  let palette = $state<CustomPalette>({});
  let drafts = $state<Record<PaletteField, string>>({
    primary: "#000000",
    background: "#000000",
    sidebar: "#000000",
    foreground: "#000000",
    border: "#000000",
  });
  let effectiveColors = $state<Record<PaletteField, string>>({ ...drafts });
  let colorsReady = $state(false);
  let storageAvailable = $state(true);

  function storage(): Storage | null {
    try {
      return typeof window === "undefined" ? null : window.localStorage;
    } catch {
      return null;
    }
  }

  function readEffectiveColors(): Record<PaletteField, string> | null {
    if (typeof window === "undefined" || typeof document === "undefined") return null;
    try {
      const computed = window.getComputedStyle(document.documentElement);
      const colors = {} as Record<PaletteField, string>;
      for (const field of PALETTE_FIELDS) {
        const hex = hslChannelsToHex(computed.getPropertyValue(themeVariables[field]));
        if (!hex) return null;
        colors[field] = hex;
      }
      return colors;
    } catch {
      return null;
    }
  }

  function refreshDisplayedColors() {
    const colors = readEffectiveColors();
    if (!colors) return;
    effectiveColors = colors;
    for (const field of PALETTE_FIELDS) drafts[field] = palette[field] ?? colors[field];
    colorsReady = true;
  }

  onMount(() => {
    const localStorage = storage();
    palette = controller?.palette ?? readCustomPalette(localStorage);
    if (!localStorage) storageAvailable = false;
    void tick().then(refreshDisplayedColors);
  });

  function update(field: PaletteField, raw: string) {
    drafts[field] = raw;
    if (!isHexColor(raw)) return;
    const next = { ...palette, [field]: raw.toUpperCase() };
    palette = next;
    effectiveColors[field] = raw.toUpperCase();
    storageAvailable = controller?.setPalette(next) ?? saveCustomPalette(storage(), next);
  }

  async function reset() {
    palette = {};
    storageAvailable = controller?.setPalette(palette) ?? saveCustomPalette(storage(), palette);
    await tick();
    refreshDisplayedColors();
  }

  let contrast = $derived(contrastRatio(effectiveColors.foreground, effectiveColors.background));
</script>

<div class="space-y-3">
  <div class="grid gap-3 sm:grid-cols-2">
    {#each PALETTE_FIELDS as field (field)}
      <label class="flex items-center gap-3 rounded-md border border-border/60 p-3">
        <input
          type="color"
          value={isHexColor(drafts[field]) ? drafts[field] : effectiveColors[field]}
          aria-label={labels[field]()}
          class="h-9 w-10 cursor-pointer rounded border-0 bg-transparent p-0"
          oninput={(event) => update(field, (event.currentTarget as HTMLInputElement).value)}
        />
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium">{labels[field]()}</span>
          <input
            type="text"
            value={drafts[field]}
            aria-label={labels[field]() + " HEX"}
            aria-invalid={drafts[field].length > 0 && !isHexColor(drafts[field])}
            maxlength="7"
            spellcheck="false"
            class="mt-1 w-24 rounded border border-input bg-background px-2 py-1 font-mono text-xs"
            oninput={(event) => update(field, (event.currentTarget as HTMLInputElement).value)}
          />
          {#if drafts[field].length > 0 && !isHexColor(drafts[field])}
            <span class="ml-2 text-xs text-destructive">{t("palette_invalidHex")}</span>
          {/if}
        </span>
      </label>
    {/each}
  </div>
  {#if colorsReady && contrast !== null}
    <p class="text-xs text-muted-foreground" aria-live="polite">
      {t("palette_contrast", { ratio: contrast.toFixed(2) })}
      <span class={contrast >= 4.5 ? "text-emerald-500" : "text-amber-500"}>
        {contrast >= 4.5 ? t("palette_contrastPass") : t("palette_contrastLow")}
      </span>
    </p>
  {/if}
  <div class="flex items-center justify-between gap-3">
    {#if !storageAvailable}
      <p class="text-xs text-muted-foreground">{t("palette_storageUnavailable")}</p>
    {:else}
      <span></span>
    {/if}
    <button
      type="button"
      class="rounded-md border border-border px-3 py-1.5 text-xs hover:bg-accent"
      onclick={reset}>{t("palette_reset")}</button
    >
  </div>
</div>
