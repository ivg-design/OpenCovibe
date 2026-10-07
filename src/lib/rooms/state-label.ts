import { t } from "$lib/i18n/index.svelte";

export function roomAgentStateLabel(state: string): string {
  if (state === "budget_exhausted") return t("room_stateBudgetExhausted");
  if (state === "no_progress") return t("room_stateNoProgress");
  return state.replaceAll("_", " ");
}
