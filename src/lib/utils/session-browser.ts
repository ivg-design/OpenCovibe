import type { CliSessionSummary } from "$lib/types";

export function sessionProject(session: CliSessionSummary): string {
  return session.projectPath || session.cwd;
}

export function sessionTitle(
  session: CliSessionSummary,
  projectFallback?: (project: string) => string,
): string {
  if (session.title?.trim()) return session.title.trim();
  const prompt = session.firstPrompt.trim();
  if (isReadablePrompt(prompt)) return prompt;
  if (session.isSubagent) return "Agent conversation";
  const project = session.cwd.split(/[\\/]/).filter(Boolean).at(-1);
  return project
    ? (projectFallback?.(project) ?? `Session in ${project}`)
    : "Untitled conversation";
}

function isReadablePrompt(prompt: string): boolean {
  return !!prompt && !/^[[{<]/.test(prompt) && !/^(?:system|assistant|user)\s*:/i.test(prompt);
}

/** Identify known helper/background sessions from summary metadata and source paths. */
export function isBackgroundSession(session: CliSessionSummary): boolean {
  if (session.isAutomated || session.isSubagent || hasBackgroundPath(session)) return true;
  return false;
}

function hasBackgroundPath(session: CliSessionSummary): boolean {
  const metadata = [session.filePath, session.cwd, session.projectPath, session.title]
    .filter(Boolean)
    .join(" ")
    .toLowerCase()
    .replaceAll("\\", "/");
  return /(?:^|[ /_.-])(?:\.claude-mem|observer-sessions|automations?|scheduled-sessions|heartbeat-sessions)(?:[ /_.-]|$)/.test(
    metadata,
  );
}

export function filterSessions(
  sessions: CliSessionSummary[],
  query: string,
  includeSubagents: boolean,
  includeArchived: boolean,
  includeBackground = false,
): CliSessionSummary[] {
  const needle = query.trim().toLowerCase();
  return sessions.filter(
    (s) =>
      (includeSubagents || !s.isSubagent) &&
      (includeSubagents || !s.isAutomated) &&
      (includeBackground || !hasBackgroundPath(s)) &&
      (includeArchived || !s.archived) &&
      (!needle ||
        [sessionTitle(s), s.firstPrompt, s.cwd, sessionProject(s), s.model || "", s.sessionId].some(
          (text) => text.toLowerCase().includes(needle),
        )),
  );
}

export function sessionPreview(session: CliSessionSummary): string {
  const preview = session.firstPrompt
    .replace(/&#x20;|&#32;|&nbsp;/gi, " ")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/\s+/g, " ")
    .trim();
  return isReadablePrompt(preview) ? preview : "";
}
