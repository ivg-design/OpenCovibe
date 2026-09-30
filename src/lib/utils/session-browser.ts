import type { CliSessionSummary } from "$lib/types";

export function sessionProject(session: CliSessionSummary): string {
  return session.projectPath || session.cwd;
}

export function sessionTitle(session: CliSessionSummary): string {
  if (session.title?.trim()) return session.title.trim();
  const prompt = session.firstPrompt.trim();
  if (prompt && !/^[[{<]/.test(prompt)) return prompt;
  return session.isSubagent ? "Agent conversation" : "Untitled conversation";
}

export function filterSessions(
  sessions: CliSessionSummary[],
  query: string,
  includeSubagents: boolean,
  includeArchived: boolean,
): CliSessionSummary[] {
  const needle = query.trim().toLowerCase();
  return sessions.filter(
    (s) =>
      (includeSubagents || (!s.isSubagent && !s.isAutomated)) &&
      (includeArchived || !s.archived) &&
      (!needle ||
        [sessionTitle(s), s.firstPrompt, s.cwd, sessionProject(s), s.model || "", s.sessionId].some(
          (text) => text.toLowerCase().includes(needle),
        )),
  );
}

export function sessionPreview(session: CliSessionSummary): string {
  return session.firstPrompt
    .replace(/&#x20;|&#32;|&nbsp;/gi, " ")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/\s+/g, " ")
    .trim();
}
