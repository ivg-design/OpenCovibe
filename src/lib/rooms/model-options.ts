import type { CliModelInfo } from "$lib/types";

export function roomEffortLevels(
  models: CliModelInfo[],
  model: string,
  defaultModel?: string,
): string[] {
  const selected = models.find((entry) => entry.value === (model || defaultModel || "default"));
  return selected?.supportsEffort ? (selected.supportedEffortLevels ?? []) : [];
}

export function effortAfterModelChange(
  models: CliModelInfo[],
  model: string,
  effort: string,
  defaultModel?: string,
): string {
  return roomEffortLevels(models, model, defaultModel).includes(effort) ? effort : "";
}
