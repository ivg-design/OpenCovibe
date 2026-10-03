export interface Env {
  BROKER: DurableObjectNamespace;
  ROOM_ID: string;
  CONVERSATION_REF: string;
  MAC_TOKEN_SHA256: string;
}

export const now = () => Math.floor(Date.now() / 1000);
export const random = (bytes = 32) => {
  const a = crypto.getRandomValues(new Uint8Array(bytes));
  return Array.from(a, x => x.toString(16).padStart(2, "0")).join("");
};
export async function sha256(s: string): Promise<string> {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s));
  return Array.from(new Uint8Array(d), x => x.toString(16).padStart(2, "0")).join("");
}
export function equal(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let n = 0;
  for (let i = 0; i < a.length; i++) n |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return n === 0;
}
export function bearer(req: Request): string | null {
  const h = req.headers.get("authorization") ?? "";
  return /^Bearer [^\s]{32,4096}$/.test(h) ? h.slice(7) : null;
}
export function json(status: number, value: unknown, extra: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store", ...extra } });
}
export function error(status: number, code: string, message: string): Response {
  return json(status, { error: code, error_description: message });
}
export async function limited(req: Request, max: number): Promise<string | null> {
  const len = Number(req.headers.get("content-length") ?? 0);
  if (len > max) return null;
  const raw = await req.text();
  return new TextEncoder().encode(raw).length <= max ? raw : null;
}
export function object(x: unknown): x is Record<string, unknown> {
  return !!x && typeof x === "object" && !Array.isArray(x);
}
export function htmlEscape(s: string): string {
  return s.replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}
export function page(title: string, body: string, status = 200): Response {
  return new Response(`<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${htmlEscape(title)}</title><style>body{font:16px system-ui;max-width:40rem;margin:4rem auto;padding:0 1rem;color:#202533;background:#f7f8fb}main{background:white;padding:2rem;border-radius:1rem;box-shadow:0 8px 30px #101b2b12}h1{font-size:1.5rem}label{display:block;margin:1rem 0}input{font:inherit;padding:.6rem;width:100%;box-sizing:border-box}button{font:inherit;background:#273e70;color:white;border:0;border-radius:.4rem;padding:.7rem 1rem}small{color:#526077}</style><main>${body}</main></html>`, { status, headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store", "content-security-policy": "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'", "x-content-type-options": "nosniff" } });
}
export function base(url: URL): string { return url.origin; }
export function resource(url: URL): string { return base(url) + "/mcp"; }

export function redirectOk(raw: string): boolean {
  try {
    const u = new URL(raw);
    return u.protocol === "https:" && !!u.hostname && !u.username && !u.password && !u.hash;
  } catch { return false; }
}
