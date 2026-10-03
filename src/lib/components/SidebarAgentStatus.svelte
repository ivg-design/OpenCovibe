<script lang="ts">
  import StatusBadge from "./StatusBadge.svelte";
  import { sidebarAgentStatus } from "$lib/utils/sidebar-status";
  import { t } from "$lib/i18n/index.svelte";
  let { state, attention = false }: { state: string; attention?: boolean } = $props();
  const badge = $derived(sidebarAgentStatus(state, attention));
</script>

<StatusBadge status={badge.status} label={t(badge.label)} compact class="shrink-0" />
