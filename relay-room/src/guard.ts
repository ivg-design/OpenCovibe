import { object } from "./common";

export type Rpc = { jsonrpc: "2.0"; id: string | number; method: string; params?: Record<string, unknown> };
export type GuardResult = { request: Rpc } | { error: string } | { notification: true };

/** Validate native methods; broad access is an explicit owner setting. */
export function guardRpc(raw: unknown, room: string, conversation: string): GuardResult {
  const allSessions = room === "*";
  const uuid = "[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}";
  if (!object(raw) || raw.jsonrpc !== "2.0" || typeof raw.method !== "string" || raw.method.length > 64)
    return { error: "Invalid JSON-RPC request" };
  if (raw.id === undefined) return raw.method === "notifications/initialized" && Object.keys(raw).every(k => ["jsonrpc", "method", "params"].includes(k)) ? { notification: true } : { error: "Unsupported notification" };
  if (!(typeof raw.id === "string" && raw.id.length > 0 && raw.id.length <= 120) && !(typeof raw.id === "number" && Number.isSafeInteger(raw.id))) return { error: "Invalid request id" };
  if (Array.isArray(raw.params) || (raw.params !== undefined && !object(raw.params))) return { error: "Invalid params" };
  const params: Record<string, unknown> = { ...(raw.params as Record<string, unknown> | undefined) };
  const method = raw.method;
  if (["initialize", "ping", "server/discover", "tools/list", "events/list"].includes(method)) return { request: { jsonrpc: "2.0", id: raw.id, method, params } };
  if (method === "tools/call") {
    const name = params.name, args = params.arguments;
    if (typeof name !== "string" || !["ocv.list_agents", "ocv.send_message", "ocv.get_message", "ocv.read_replies"].includes(name) || !object(args)) return { error: "Unsupported tool or arguments" };
    const a = { ...args };
    if (name === "ocv.list_agents") {
      if (a.room_id !== undefined && (allSessions ? typeof a.room_id !== "string" || !new RegExp(`^${uuid}$`).test(a.room_id) : a.room_id !== room)) return { error: "Forbidden room" };
      if (Object.keys(a).some(k => k !== "room_id")) return { error: "Unsupported room filter" };
      if (!allSessions) a.room_id = room;
    } else if (name === "ocv.send_message") {
      if (a.conversation_ref !== undefined && a.conversation_ref !== conversation) return { error: "Forbidden conversation" };
      const agentPattern = allSessions ? `^(?:${uuid}/${uuid}|session/${uuid})$` : `^${room}/${uuid}$`;
      if (typeof a.agent_id !== "string" || !new RegExp(agentPattern).test(a.agent_id)) return { error: "Forbidden agent" };
      if (typeof a.client_message_id !== "string" || a.client_message_id.length < 1 || a.client_message_id.length > 200 || typeof a.text !== "string" || a.text.length > 32000 || (a.mode !== undefined && a.mode !== "queue")) return { error: "Invalid queued message" };
      a.conversation_ref = conversation;
      a.mode = "queue";
    } else if (name === "ocv.read_replies") {
      if (a.conversation_ref !== undefined && a.conversation_ref !== conversation) return { error: "Forbidden conversation" };
      if (Object.keys(a).some(k => !["conversation_ref", "cursor", "limit"].includes(k))) return { error: "Unsupported reply filter" };
      a.conversation_ref = conversation;
    } else if (name === "ocv.get_message" && (typeof a.message_id !== "string" || a.message_id.length > 200)) return { error: "Invalid message id" };
    params.arguments = a;
  } else if (method === "events/subscribe" || method === "events/unsubscribe") {
    if (params.name !== "ocv.reply.created" || !object(params.arguments)) return { error: "Unsupported event" };
    if (params.arguments.conversation_ref !== undefined && params.arguments.conversation_ref !== conversation) return { error: "Forbidden conversation" };
    if (Object.keys(params.arguments).some(k => k !== "conversation_ref")) return { error: "Unsupported event filter" };
    params.arguments = { conversation_ref: conversation };
    if (!object(params.delivery) || params.delivery.mode !== "webhook" || typeof params.delivery.url !== "string") return { error: "Invalid webhook delivery" };
  } else return { error: "Unsupported method" };
  return { request: { jsonrpc: "2.0", id: raw.id, method, params } };
}
