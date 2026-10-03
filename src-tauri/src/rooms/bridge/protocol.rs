use super::{events, Principal, Send};
use crate::{rooms::store::RoomStore, web_server::state::AppState};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

pub async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    // The existing listener must remain private. No cookie/full-app token escalation.
    if !state
        .bind_addr
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
    {
        return (
            StatusCode::FORBIDDEN,
            "External messaging requires a loopback listener",
        )
            .into_response();
    }
    let Some(token) = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .filter(|s| !s.is_empty())
    else {
        return (
            StatusCode::UNAUTHORIZED,
            "Scoped bearer authentication required",
        )
            .into_response();
    };
    let principal = match super::authenticate(token) {
        Ok(principal) => principal,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                "External messaging unavailable or unauthorized",
            )
                .into_response()
        }
    };
    if request.get("id").is_none()
        && matches!(
            request["method"].as_str(),
            Some("notifications/initialized" | "notifications/cancelled")
        )
    {
        return StatusCode::NO_CONTENT.into_response();
    }
    Json(dispatch(&state.rooms, &principal, &request).await).into_response()
}

pub(super) async fn dispatch(store: &RoomStore, principal: &Principal, request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return json!({"jsonrpc":"2.0","id":id,"error":{"code":-32600,"message":"Invalid JSON-RPC request"}});
    }
    let result = match request.get("method").and_then(Value::as_str).unwrap_or("") {
        "initialize" => {
            let requested = request["params"]["protocolVersion"].as_str().unwrap_or("");
            let version = if requested == "2025-06-18" {
                "2025-06-18"
            } else {
                "2025-11-25"
            };
            Ok(
                json!({"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"OpenCovibe private room bridge","version":env!("CARGO_PKG_VERSION")}}),
            )
        }
        "server/discover" => Ok(
            json!({"resultType":"complete","supportedVersions":["2026-07-28"],"capabilities":{"tools":{},"events":{}}}),
        ),
        "ping" => Ok(json!({})),
        "tools/list" => principal
            .scope("read")
            .map(|_| json!({"tools":tool_definitions()})),
        "tools/call" => {
            let p = &request["params"];
            call_tool(store, principal, p["name"].as_str().unwrap_or(""), &p["arguments"])
                .map(|value| json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false}))
        }
        "events/list" => principal.scope("subscribe").map(|_| events::definitions()),
        "events/subscribe" => events::subscribe(store, principal, &request["params"]).await,
        "events/unsubscribe" => events::unsubscribe(store, principal, &request["params"]),
        _ => Err("Unsupported method".into()),
    };
    match result {
        Ok(mut result) => {
            if request["method"] != "initialize"
                && request["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"]
                    == "2026-07-28"
            {
                if let Some(object) = result.as_object_mut() {
                    object.entry("resultType").or_insert(json!("complete"));
                }
            }
            if request["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] == "2026-07-28"
            {
                json!({"jsonrpc":"2.0","id":id,"result":result,"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"OpenCovibe private bridge","version":env!("CARGO_PKG_VERSION")}}})
            } else {
                json!({"jsonrpc":"2.0","id":id,"result":result})
            }
        }
        Err(error) => {
            if request["method"] == "events/subscribe"
                && (error.starts_with("Callback")
                    || error.contains("verification")
                    || error.starts_with("Invalid verification"))
            {
                let reason = if error.contains("timeout") {
                    "timeout"
                } else if error.contains("challenge") {
                    "challenge_failed"
                } else {
                    "endpoint_rejected"
                };
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32015,"message":error,"data":{"reason":reason}}})
            } else {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":error}})
            }
        }
    }
}

fn string<'a>(v: &'a Value, field: &str) -> Result<&'a str, String> {
    v[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing {field}"))
}
fn call_tool(
    store: &RoomStore,
    principal: &Principal,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    match name {
        "ocv.list_agents" => {
            store.bridge_agents(principal, args.get("room_id").and_then(Value::as_str))
        }
        "ocv.send_message" => {
            let input: Send = serde_json::from_value(args.clone())
                .map_err(|_| "Invalid send_message arguments")?;
            serde_json::to_value(store.bridge_send(principal, &input)?).map_err(|e| e.to_string())
        }
        "ocv.get_message" => {
            serde_json::to_value(store.bridge_get(principal, string(args, "message_id")?)?)
                .map_err(|e| e.to_string())
        }
        "ocv.read_replies" => {
            let cursor = match args.get("cursor") {
                None | Some(Value::Null) => 0,
                Some(Value::String(value)) => value.parse::<i64>().map_err(|_| "Invalid cursor")?,
                Some(value) => value.as_i64().ok_or("Invalid cursor")?,
            };
            let limit = args
                .get("limit")
                .map(|v| v.as_u64().ok_or("Invalid limit"))
                .transpose()?
                .unwrap_or(50);
            store.bridge_replies(
                principal,
                string(args, "conversation_ref")?,
                cursor,
                usize::try_from(limit).map_err(|_| "Invalid limit")?,
            )
        }
        _ => Err("Unknown tool".into()),
    }
}
fn tool_definitions() -> Value {
    json!([
        {"name":"ocv.list_agents","description":"List authorized room participants and standalone OCV Codex sessions. Session IDs use session/<run_id>; room participants use <room_id>/<participant_id>. No provider processes are started.","inputSchema":{"type":"object","properties":{"room_id":{"type":"string"}},"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"ocv.send_message","description":"Persist one idempotent queued message for an authorized room participant or standalone session. Room work uses its owning scheduler; standalone work uses only an already connected, idle OCV actor. Pauses, approvals and active turns remain enforced. Attachments are existing room IDs and are unsupported for standalone sessions. Queue only; never implicitly resumes, forks or steers.","inputSchema":{"type":"object","properties":{"agent_id":{"type":"string"},"text":{"type":"string","maxLength":32000},"client_message_id":{"type":"string","maxLength":200},"conversation_ref":{"type":"string"},"reply_to_message_id":{"type":"string"},"mode":{"enum":["queue"]},"attachments":{"type":"array","items":{"type":"string"},"maxItems":8}},"required":["agent_id","text","client_message_id","conversation_ref"],"additionalProperties":false}},
        {"name":"ocv.get_message","description":"Read an authorized durable delivery receipt, including blocked or ambiguous state.","inputSchema":{"type":"object","properties":{"message_id":{"type":"string"}},"required":["message_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"ocv.read_replies","description":"Read durable correlated visible replies from room participants or standalone sessions. Hidden reasoning and raw tool output are excluded.","inputSchema":{"type":"object","properties":{"conversation_ref":{"type":"string"},"cursor":{"type":["string","integer","null"]},"limit":{"type":"integer","minimum":1,"maximum":100}},"required":["conversation_ref"],"additionalProperties":false},"annotations":{"readOnlyHint":true}}
    ])
}
