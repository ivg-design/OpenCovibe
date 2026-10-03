//! Verified, signed MCP Events callbacks. No subscription or credentials are created by default.
use super::{decode, Principal, Reply};
use crate::rooms::store::RoomStore;
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const EVENT: &str = "ocv.reply.created";
#[derive(Clone, Serialize, Deserialize)]
struct Subscription {
    id: String,
    principal: String,
    conversation: String,
    url: String,
    secret: String,
    cursor: i64,
    expires_at: i64,
    next_attempt: i64,
    attempts: u32,
    status: String,
    generation: String,
    #[serde(default)]
    verified_at: i64,
    #[serde(default)]
    previous_secret: Option<String>,
    #[serde(default)]
    rotation_until: i64,
}

pub(super) fn definitions() -> Value {
    json!({"events":[{"name":EVENT,"description":"A durable visible room reply correlated to a queued external message. Receipt status can be read separately.","delivery":["webhook"],"inputSchema":{"type":"object","properties":{"conversation_ref":{"type":"string"}},"required":["conversation_ref"],"additionalProperties":false},"payloadSchema":{"type":"object","properties":{"event_id":{"type":"string"},"message_id":{"type":"string"},"reply_to_message_id":{"type":"string"},"conversation_ref":{"type":"string"},"agent_id":{"type":"string"},"sequence":{"type":"integer"},"timestamp":{"type":"string"},"status":{"type":"string"},"text":{"type":"string"},"room_message_id":{"type":"string"}},"required":["event_id","message_id","reply_to_message_id","conversation_ref","agent_id","sequence","timestamp","status","text","room_message_id"]}}]})
}
fn identity(principal: &Principal, input: &Value) -> Result<(String, String, String), String> {
    principal.scope("subscribe")?;
    if input["name"].as_str() != Some(EVENT)
        || input["delivery"]["mode"].as_str() != Some("webhook")
    {
        return Err("Unsupported event or delivery mode".into());
    }
    let conversation = input["arguments"]["conversation_ref"]
        .as_str()
        .ok_or("Missing conversation_ref")?;
    if input["arguments"].as_object().is_none_or(|a| a.len() != 1) {
        return Err("Unsupported event filters".into());
    }
    principal.conversation(conversation)?;
    let url = input["delivery"]["url"]
        .as_str()
        .ok_or("Missing callback URL")?;
    callback_url(principal, url)?;
    let bytes = serde_json::to_vec(&json!([principal.id, EVENT, conversation, url]))
        .map_err(|e| e.to_string())?;
    let id = format!("ocv-{:x}", Sha256::digest(bytes));
    Ok((id, conversation.into(), url.into()))
}
fn key(secret: &str) -> Result<Vec<u8>, String> {
    let key = STANDARD
        .decode(
            secret
                .strip_prefix("whsec_")
                .ok_or("Invalid webhook secret")?,
        )
        .map_err(|_| "Invalid webhook secret")?;
    if !(24..=64).contains(&key.len()) {
        return Err("Invalid webhook key length".into());
    }
    Ok(key)
}
// HMAC-SHA256, Standard Webhooks signing format: id.timestamp.exact-body.
fn signature(secret: &str, id: &str, timestamp: i64, body: &[u8]) -> Result<String, String> {
    let key = key(secret)?;
    let mut inner = [0x36u8; 64];
    let mut outer = [0x5cu8; 64];
    for (index, byte) in key.iter().enumerate() {
        inner[index] ^= byte;
        outer[index] ^= byte;
    }
    let mut hash = Sha256::new();
    hash.update(inner);
    hash.update(format!("{id}.{timestamp}."));
    hash.update(body);
    let digest = hash.finalize();
    let mut hash = Sha256::new();
    hash.update(outer);
    hash.update(digest);
    Ok(format!("v1,{}", STANDARD.encode(hash.finalize())))
}
fn signatures(sub: &Subscription, id: &str, timestamp: i64, body: &[u8]) -> Result<String, String> {
    let current = signature(&sub.secret, id, timestamp, body)?;
    if let Some(old) = sub
        .previous_secret
        .as_deref()
        .filter(|_| sub.rotation_until > timestamp)
    {
        Ok(format!(
            "{} {}",
            current,
            signature(old, id, timestamp, body)?
        ))
    } else {
        Ok(current)
    }
}
fn callback_url(principal: &Principal, raw: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(raw).map_err(|_| "Invalid callback URL")?;
    let host = url.host_str().ok_or("Callback host required")?;
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !principal
            .callback_hosts
            .iter()
            .any(|allowed| allowed == host)
    {
        return Err("Callback must use HTTPS on an owner-allowed host".into());
    }
    if host.parse::<IpAddr>().is_ok() {
        return Err("Callback must use a verified DNS hostname".into());
    }
    Ok(url)
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0)
                || (a == 192 && b == 2)
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] == 0xdb8 || s[1] < 0x200))
                && s[0] != 0x2002
                && s[0] != 0x3fff
        }
    }
}
async fn client(principal: &Principal, raw: &str) -> Result<(reqwest::Client, url::Url), String> {
    let url = callback_url(principal, raw)?;
    let host = url.host_str().ok_or("Missing host")?;
    let addresses: Vec<SocketAddr> =
        tokio::time::timeout(Duration::from_secs(5), tokio::net::lookup_host((host, 443)))
            .await
            .map_err(|_| "Callback DNS timeout")?
            .map_err(|_| "Callback DNS unavailable")?
            .collect();
    if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
        return Err("Callback resolved to a non-public address".into());
    }
    // Resolve once, validate every address, pin the connect address, keep URL host for TLS.
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .resolve(host, addresses[0])
        .build()
        .map_err(|_| "Callback client unavailable")?;
    Ok((client, url))
}
async fn post(
    principal: &Principal,
    sub: &Subscription,
    event_id: &str,
    body: Vec<u8>,
) -> Result<reqwest::Response, String> {
    if body.len() > 262_144 {
        return Err("Event payload exceeds webhook limit".into());
    }
    let (client, url) = client(principal, &sub.url).await?;
    let timestamp = chrono::Utc::now().timestamp();
    client
        .post(url)
        .header("Content-Type", "application/json")
        .header("webhook-id", event_id)
        .header("webhook-timestamp", timestamp.to_string())
        .header(
            "webhook-signature",
            signatures(sub, event_id, timestamp, &body)?,
        )
        .header("X-MCP-Subscription-Id", &sub.id)
        .body(body)
        .send()
        .await
        .map_err(|_| "Callback delivery failed".to_string())
}
fn prepare(
    store: &RoomStore,
    principal: &Principal,
    input: &Value,
) -> Result<Subscription, String> {
    let (id, conversation, url) = identity(principal, input)?;
    let secret = input["delivery"]["secret"]
        .as_str()
        .ok_or("Missing webhook secret")?;
    key(secret)?;
    let cursor = match input.get("cursor") {
        None | Some(Value::Null) => None,
        Some(Value::String(c)) => Some(c.parse::<i64>().map_err(|_| "Invalid cursor")?),
        Some(v) => Some(v.as_i64().ok_or("Invalid cursor")?),
    };
    let ttl = input
        .get("ttlMs")
        .filter(|v| !v.is_null())
        .map(|v| v.as_u64().ok_or("Invalid ttlMs"))
        .transpose()?
        .unwrap_or(3_600_000)
        .clamp(60_000, 86_400_000);
    let sub = {
        let conn = store.connection.lock().map_err(|e| e.to_string())?;
        let old: Option<String> = conn
            .query_row(
                "SELECT payload FROM bridge_subscriptions WHERE id=?1 AND principal=?2",
                params![id, principal.id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let old: Option<Subscription> = old.map(decode).transpose()?;
        let cursor = cursor.unwrap_or(old.as_ref().map(|s| s.cursor).unwrap_or(0));
        let cursor_exists = if cursor == 0 {
            true
        } else if cursor < 0 {
            false
        } else {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM bridge_outbox WHERE principal=?1 AND conversation=?2 AND seq=?3)",
                params![principal.id, conversation, cursor],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?
        };
        if !cursor_exists {
            return Err("Replay cursor is outside this conversation's retained event log".into());
        }
        let now = chrono::Utc::now().timestamp();
        let (verified_at, previous_secret, rotation_until) = match old.as_ref() {
            Some(old) if old.secret == secret => {
                let keep_rotation = old.previous_secret.is_some() && old.rotation_until > now;
                (
                    old.verified_at,
                    keep_rotation.then(|| old.previous_secret.clone()).flatten(),
                    if keep_rotation {
                        old.rotation_until
                    } else {
                        now + 60
                    },
                )
            }
            Some(old) => (0, Some(old.secret.clone()), now + 60),
            None => (0, None, now + 60),
        };
        Subscription {
            id: id.clone(),
            principal: principal.id.clone(),
            conversation,
            url,
            secret: secret.into(),
            cursor,
            expires_at: chrono::Utc::now().timestamp() + ttl as i64 / 1000,
            next_attempt: 0,
            attempts: 0,
            status: "active".into(),
            generation: uuid::Uuid::new_v4().to_string(),
            verified_at,
            previous_secret,
            rotation_until,
        }
    };
    Ok(sub)
}

async fn verify(principal: &Principal, sub: &Subscription) -> Result<(), String> {
    let challenge = uuid::Uuid::new_v4().to_string();
    let verification_id = format!("verify-{}", uuid::Uuid::new_v4());
    let response = post(
        principal,
        sub,
        &verification_id,
        serde_json::to_vec(&json!({"type":"verification","challenge":challenge}))
            .map_err(|e| e.to_string())?,
    )
    .await?;
    if !response.status().is_success() {
        return Err("Callback verification was rejected".into());
    }
    if response.content_length().is_some_and(|n| n > 4096) {
        return Err("Callback verification response too large".into());
    }
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "Cannot read callback verification")?;
        if bytes.len() + chunk.len() > 4096 {
            return Err("Callback verification response too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let answer: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid verification response")?;
    if !answer["challenge"]
        .as_str()
        .is_some_and(|a| super::constant_eq(a.as_bytes(), challenge.as_bytes()))
    {
        return Err("Callback challenge mismatch".into());
    }
    Ok(())
}

fn activate(
    store: &RoomStore,
    principal: &Principal,
    mut sub: Subscription,
) -> Result<Value, String> {
    principal.scope("subscribe")?;
    principal.conversation(&sub.conversation)?;
    callback_url(principal, &sub.url)?;
    let now = chrono::Utc::now().timestamp();
    sub.next_attempt = now;
    sub.verified_at = now;
    let mut conn = store.connection.lock().map_err(|e| e.to_string())?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let current: Option<String> = tx
        .query_row(
            "SELECT payload FROM bridge_subscriptions WHERE id=?1 AND principal=?2",
            params![sub.id, principal.id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    // A refresh cannot jump past an event that still awaits delivery.
    if let Some(raw) = current {
        let old: Subscription = decode(raw)?;
        sub.cursor = sub.cursor.min(old.cursor);
    }
    tx.execute("INSERT INTO bridge_subscriptions VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![sub.id,principal.id,serde_json::to_string(&sub).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(
        json!({"id":sub.id,"cursor":sub.cursor.to_string(),"refreshBefore":chrono::DateTime::from_timestamp(sub.expires_at,0).map(|t|t.to_rfc3339()),"truncated":false}),
    )
}
pub(super) async fn subscribe(
    store: &RoomStore,
    principal: &Principal,
    input: &Value,
) -> Result<Value, String> {
    let sub = prepare(store, principal, input)?;
    if sub.verified_at + 300 <= chrono::Utc::now().timestamp() {
        verify(principal, &sub).await?;
    }
    let current = super::current_principal(&principal.id)?;
    activate(store, &current, sub)
}
pub(super) fn unsubscribe(
    store: &RoomStore,
    principal: &Principal,
    input: &Value,
) -> Result<Value, String> {
    let (id, _, _) = identity(principal, input)?;
    let conn = store.connection.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM bridge_subscriptions WHERE id=?1 AND principal=?2",
        params![id, principal.id],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({}))
}
fn claim(store: &RoomStore, now: i64) -> Result<Option<(Subscription, Reply)>, String> {
    let mut conn = store.connection.lock().map_err(|e| e.to_string())?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let candidates = {
        let mut q = tx
            .prepare("SELECT payload FROM bridge_subscriptions")
            .map_err(|e| e.to_string())?;
        let rows = q
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    for raw in candidates {
        let mut sub: Subscription = decode(raw)?;
        if sub.status != "active" || sub.expires_at <= now || sub.next_attempt > now {
            continue;
        }
        let reply:Option<String>=tx.query_row("SELECT payload FROM bridge_outbox WHERE principal=?1 AND conversation=?2 AND seq>?3 ORDER BY seq LIMIT 1",params![sub.principal,sub.conversation,sub.cursor],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        if let Some(reply) = reply {
            sub.next_attempt = now + 60;
            tx.execute(
                "UPDATE bridge_subscriptions SET payload=?1 WHERE id=?2",
                params![
                    serde_json::to_string(&sub).map_err(|e| e.to_string())?,
                    sub.id
                ],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
            return Ok(Some((sub, decode(reply)?)));
        }
    }
    Ok(None)
}
fn finish(
    store: &RoomStore,
    claimed: &Subscription,
    seq: i64,
    status: Option<u16>,
    now: i64,
) -> Result<(), String> {
    let mut conn = store.connection.lock().map_err(|e| e.to_string())?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let raw: Option<String> = tx
        .query_row(
            "SELECT payload FROM bridge_subscriptions WHERE id=?1",
            [&claimed.id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(raw) = raw {
        let mut sub: Subscription = decode(raw)?;
        if sub.generation == claimed.generation
            && sub.cursor == claimed.cursor
            && sub.next_attempt == claimed.next_attempt
        {
            if status.is_some_and(|s| (200..300).contains(&s)) {
                sub.cursor = seq;
                sub.attempts = 0;
                sub.next_attempt = now;
            } else {
                sub.attempts += 1;
                sub.next_attempt = now + (2_i64.pow(sub.attempts.min(8))).min(300);
                if matches!(status, Some(410 | 413)) || sub.attempts >= 8 {
                    sub.status = "suspended".into();
                }
            }
            tx.execute(
                "UPDATE bridge_subscriptions SET payload=?1 WHERE id=?2",
                params![
                    serde_json::to_string(&sub).map_err(|e| e.to_string())?,
                    sub.id
                ],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}
async fn deliver(store: &RoomStore, sub: &Subscription, reply: &Reply) -> Result<u16, String> {
    let principal = super::current_principal(&sub.principal)?;
    principal.scope("subscribe")?;
    principal.scope("read")?;
    principal.conversation(&sub.conversation)?;
    store.bridge_get(&principal, &reply.reply_to_message_id)?;
    let mut data = serde_json::to_value(reply).map_err(|e| e.to_string())?;
    // Preserve the stable room-message reference when a reply needs a bounded webhook preview.
    if reply.text.len() > 64_000 {
        data["text"] = json!(crate::rooms::session_seed::short_text(&reply.text, 64_000));
        data["text_truncated"] = json!(true);
    }
    let body=serde_json::to_vec(&json!({"eventId":reply.event_id,"name":EVENT,"timestamp":reply.timestamp,"data":data,"cursor":reply.sequence.to_string()})).map_err(|e|e.to_string())?;
    Ok(post(&principal, sub, &reply.event_id, body)
        .await?
        .status()
        .as_u16())
}
pub(super) fn start(store: Arc<RoomStore>, cancel: CancellationToken) {
    tokio::spawn(async move {
        loop {
            tokio::select! { _=cancel.cancelled()=>break, _=tokio::time::sleep(Duration::from_secs(1))=>{} }
            let claimed = match claim(&store, chrono::Utc::now().timestamp()) {
                Ok(Some(value)) => value,
                Ok(None) => continue,
                Err(_) => {
                    log::warn!("[bridge] event outbox unavailable");
                    continue;
                }
            };
            let (sub, reply) = claimed;
            let status = tokio::select! {_=cancel.cancelled()=>break,result=deliver(&store,&sub,&reply)=>result.ok()};
            if finish(
                &store,
                &sub,
                reply.sequence,
                status,
                chrono::Utc::now().timestamp(),
            )
            .is_err()
            {
                log::warn!("[bridge] cannot save webhook receipt");
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event_fixture() -> (crate::rooms::bridge::tests::Fixture, Value) {
        let f = crate::rooms::bridge::tests::fixture();
        let reply = Reply {
            event_id: "fixture-event".into(),
            message_id: "fixture-visible".into(),
            reply_to_message_id: "fixture-receipt".into(),
            conversation_ref: "fixture-conversation".into(),
            agent_id: f.agent.clone(),
            sequence: 1,
            timestamp: crate::models::now_iso(),
            status: "visible_reply".into(),
            text: "visible fixture reply".into(),
            room_message_id: "fixture-visible".into(),
        };
        f.store.connection.lock().unwrap().execute("INSERT INTO bridge_outbox(principal,conversation,event_key,payload) VALUES (?1,?2,?3,?4)",params![f.principal.id,"fixture-conversation","fixture-event",serde_json::to_string(&reply).unwrap()]).unwrap();
        let input = json!({"name":EVENT,"arguments":{"conversation_ref":"fixture-conversation"},"delivery":{"mode":"webhook","url":"https://callback.example.org/events","secret":format!("whsec_{}",STANDARD.encode([11u8;32]))},"cursor":null,"ttlMs":60000});
        (f, input)
    }
    #[test]
    fn subscriptions_replay_retry_and_unsubscribe_are_durable_and_do_not_skip_unacknowledged_events(
    ) {
        let (f, input) = event_fixture();
        let sub = prepare(&f.store, &f.principal, &input).unwrap();
        let result = activate(&f.store, &f.principal, sub).unwrap();
        assert!(result["id"].as_str().unwrap().starts_with("ocv-"));
        let now = chrono::Utc::now().timestamp();
        let (claim1, reply) = claim(&f.store, now).unwrap().unwrap();
        assert_eq!(reply.event_id, "fixture-event");
        assert!(claim(&f.store, now).unwrap().is_none());
        finish(&f.store, &claim1, reply.sequence, Some(503), now).unwrap();
        assert!(claim(&f.store, now + 1).unwrap().is_none());
        let reopened = RoomStore::open(&f.dir.path().join("rooms.sqlite3")).unwrap();
        let (claim2, retry) = claim(&reopened, now + 10).unwrap().unwrap();
        assert_eq!(reply.event_id, retry.event_id);
        let mut refresh = input.clone();
        refresh["cursor"] = json!("1");
        let next = prepare(&reopened, &f.principal, &refresh).unwrap();
        assert_eq!(
            activate(&reopened, &f.principal, next).unwrap()["cursor"],
            "0"
        );
        // The replaced generation rejects a stale worker acknowledgement.
        finish(&reopened, &claim2, 1, Some(200), now + 10).unwrap();
        let (claim3, _) = claim(&reopened, now + 11).unwrap().unwrap();
        finish(&reopened, &claim3, 1, Some(200), now + 11).unwrap();
        assert!(claim(&reopened, now + 12).unwrap().is_none());
        assert_eq!(
            activate(
                &reopened,
                &f.principal,
                prepare(&reopened, &f.principal, &input).unwrap()
            )
            .unwrap()["cursor"],
            "1"
        );
        assert!(unsubscribe(&reopened, &f.principal, &input).is_ok());
        assert!(unsubscribe(&reopened, &f.principal, &input).is_ok());
        assert!(claim(&reopened, now + 100).unwrap().is_none());
    }
    #[test]
    fn terminal_http_errors_suspend_and_expiration_blocks_delivery() {
        for status in [410, 413] {
            let (f, input) = event_fixture();
            activate(
                &f.store,
                &f.principal,
                prepare(&f.store, &f.principal, &input).unwrap(),
            )
            .unwrap();
            let now = chrono::Utc::now().timestamp();
            let (sub, reply) = claim(&f.store, now).unwrap().unwrap();
            finish(&f.store, &sub, reply.sequence, Some(status), now).unwrap();
            assert!(claim(&f.store, now + 30).unwrap().is_none());
        }
        let (f, input) = event_fixture();
        activate(
            &f.store,
            &f.principal,
            prepare(&f.store, &f.principal, &input).unwrap(),
        )
        .unwrap();
        assert!(claim(&f.store, chrono::Utc::now().timestamp() + 61)
            .unwrap()
            .is_none());
    }
    #[test]
    fn callback_grants_schema_and_rotation_match_protocol() {
        let (f, input) = event_fixture();
        let old = prepare(&f.store, &f.principal, &input).unwrap();
        activate(&f.store, &f.principal, old.clone()).unwrap();
        let mut rotation = input.clone();
        rotation["delivery"]["secret"] = json!(format!("whsec_{}", STANDARD.encode([12u8; 32])));
        let next = prepare(&f.store, &f.principal, &rotation).unwrap();
        assert!(next.previous_secret.is_some());
        assert_eq!(
            signatures(&next, "evt", chrono::Utc::now().timestamp(), b"{}")
                .unwrap()
                .split_whitespace()
                .count(),
            2
        );
        assert_eq!(
            signatures(&next, "evt", next.rotation_until + 1, b"{}")
                .unwrap()
                .split_whitespace()
                .count(),
            1
        );
        let rotation_deadline = next.rotation_until;
        activate(&f.store, &f.principal, next).unwrap();
        let refreshed = prepare(&f.store, &f.principal, &rotation).unwrap();
        assert_eq!(
            refreshed.previous_secret.as_deref(),
            Some(old.secret.as_str())
        );
        assert_eq!(refreshed.rotation_until, rotation_deadline);
        assert_eq!(
            signatures(&refreshed, "evt", chrono::Utc::now().timestamp(), b"{}")
                .unwrap()
                .split_whitespace()
                .count(),
            2
        );
        for raw in [
            "http://callback.example.org/events",
            "https://127.0.0.1/events",
            "https://callback.example.org:8443/events",
            "https://attacker.example.org/events",
            "https://user:secret@callback.example.org/events",
        ] {
            assert!(callback_url(&f.principal, raw).is_err());
        }
        let mut denied = f.principal.clone();
        denied.scopes.clear();
        assert!(prepare(&f.store, &denied, &input).is_err());
        let (_, reply) = claim(&f.store, chrono::Utc::now().timestamp())
            .unwrap()
            .unwrap();
        let data = serde_json::to_value(reply).unwrap();
        let defs = definitions();
        for required in defs["events"][0]["payloadSchema"]["required"]
            .as_array()
            .unwrap()
        {
            assert!(data.get(required.as_str().unwrap()).is_some());
        }
    }
    #[test]
    fn rejects_nonzero_replay_cursor_from_another_stream() {
        let (f, input) = event_fixture();
        let own_cursor = f
            .store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT seq FROM bridge_outbox WHERE principal=?1 AND conversation=?2",
                params![f.principal.id, "fixture-conversation"],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        f.store
            .connection
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO bridge_outbox(principal,conversation,event_key,payload) VALUES ('other-principal','other-conversation','other-event','{}')",
                [],
            )
            .unwrap();
        let foreign_cursor: i64 = f
            .store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT seq FROM bridge_outbox WHERE event_key='other-event'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mut valid = input.clone();
        valid["cursor"] = json!(own_cursor);
        assert!(prepare(&f.store, &f.principal, &valid).is_ok());
        let mut foreign = input;
        foreign["cursor"] = json!(foreign_cursor);
        assert!(prepare(&f.store, &f.principal, &foreign).is_err());
    }
    #[test]
    fn signing_matches_known_hmac_vector() {
        // Independent Python/OpenSSL HMAC-SHA256 fixture for Standard Webhooks signed content.
        let secret = format!("whsec_{}", STANDARD.encode([11u8; 32]));
        assert_eq!(
            signature(&secret, "evt-1", 123, b"{\"ok\":true}").unwrap(),
            "v1,8LJ/SCExHE2GVthu+epjRgohckj7RACAi4aQnatSSKk="
        );
    }
    #[test]
    fn blocks_private_and_special_addresses() {
        for raw in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.2.1",
            "100.64.1.1",
            "169.254.0.1",
            "192.0.2.1",
            "198.51.100.2",
            "203.0.113.1",
            "198.18.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "2001:db8::1",
            "2002:7f00:1::",
            "2001::1",
        ] {
            assert!(!public_ip(raw.parse().unwrap()), "{raw}");
        }
        for raw in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(public_ip(raw.parse().unwrap()));
        }
    }
}
