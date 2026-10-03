//! Stable identity for the same provider message projected through room tools
//! and the provider transcript.

use super::models::Message;
use sha2::{Digest, Sha256};

/// Return a shared source ID for an MCP-posted message and its matching
/// `message_complete` transcript event. A room delivery identifies the turn,
/// including for providers that do not expose their own turn ID.
pub fn delivery_source_event_id(
    run_id: &str,
    delivery_id: &str,
    participant_id: &str,
    sidechat_id: Option<&str>,
    target_participant_id: Option<&str>,
    body: &str,
) -> Option<String> {
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    let prefix = delivery_scope_source_prefix(run_id, delivery_id, participant_id, sidechat_id)?;
    let base = format!("{prefix}{}", digest(body.as_bytes()));
    Some(match target_participant_id {
        Some(target_id) => format!("{base}:target:{}", digest(target_id.as_bytes())),
        None => base,
    })
}

/// Prefix shared by every room-tool post within one delivery and conversation scope.
pub fn delivery_scope_source_prefix(
    run_id: &str,
    delivery_id: &str,
    participant_id: &str,
    sidechat_id: Option<&str>,
) -> Option<String> {
    if delivery_id.trim().is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, b"room-delivery-scope-v2");
    hash_field(&mut hasher, run_id.as_bytes());
    hash_field(&mut hasher, delivery_id.as_bytes());
    hash_field(&mut hasher, participant_id.as_bytes());
    match sidechat_id {
        Some(sidechat_id) => {
            hash_field(&mut hasher, b"sidechat");
            hash_field(&mut hasher, sidechat_id.as_bytes());
        }
        None => hash_field(&mut hasher, b"main-room"),
    }
    Some(format!("room-delivery-message:v2:{:x}:", hasher.finalize()))
}

/// Transcript events have no recipient metadata. If the same delivery already
/// contains a routed MCP post, keep that message and discard the transcript echo.
pub fn is_delivery_projection(source_event_id: &str, unaddressed_source_id: &str) -> bool {
    source_event_id == unaddressed_source_id
        || source_event_id
            .strip_prefix(unaddressed_source_id)
            .is_some_and(|suffix| suffix.starts_with(":target:"))
}

/// Internal room-tool source IDs still need the normal mention and wake rules.
pub fn is_room_tool_source_event_id(source_event_id: &str) -> bool {
    source_event_id.starts_with("room-delivery-message:v1:")
        || source_event_id.starts_with("room-delivery-message:v2:")
}

/// Check whether the delivery already emitted a public main-room message.
/// Directed messages and sidechat posts intentionally do not count.
pub fn has_public_main_room_post(
    messages: &[Message],
    run_id: &str,
    delivery_id: &str,
    participant_id: &str,
) -> bool {
    let Some(prefix) = delivery_scope_source_prefix(run_id, delivery_id, participant_id, None)
    else {
        return false;
    };
    messages.iter().any(|message| {
        message.sidechat_id.is_none()
            && !message.is_directed()
            && message
                .source_event_id
                .as_deref()
                .is_some_and(|source| source.starts_with(&prefix))
    })
}

/// Stable fallback for transcript events that arrive outside a room delivery.
pub fn provider_message_source_event_id(run_id: &str, message_id: &str) -> Option<String> {
    let message_id = message_id.trim();
    if message_id.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, b"provider-message-v1");
    hash_field(&mut hasher, run_id.as_bytes());
    hash_field(&mut hasher, message_id.as_bytes());
    Some(format!("provider-message:v1:{:x}", hasher.finalize()))
}

fn digest(input: &[u8]) -> String {
    format!("{:x}", Sha256::digest(input))
}

fn hash_field(hasher: &mut Sha256, field: &[u8]) {
    hasher.update((field.len() as u64).to_be_bytes());
    hasher.update(field);
}

#[cfg(test)]
mod tests {
    use super::{
        delivery_source_event_id, is_delivery_projection, is_room_tool_source_event_id,
        provider_message_source_event_id,
    };

    #[test]
    fn matches_the_same_delivery_without_collapsing_explicit_targets() {
        let source =
            delivery_source_event_id("run-1", "delivery-1", "peer-1", None, None, "Shared update")
                .unwrap();
        assert_eq!(
            Some(source.clone()),
            delivery_source_event_id(
                "run-1",
                "delivery-1",
                "peer-1",
                None,
                None,
                " Shared update\n",
            )
        );
        for changed in [
            delivery_source_event_id("run-1", "delivery-2", "peer-1", None, None, "Shared update"),
            delivery_source_event_id("run-1", "delivery-1", "peer-2", None, None, "Shared update"),
            delivery_source_event_id(
                "run-1",
                "delivery-1",
                "peer-1",
                Some("sidechat-1"),
                None,
                "Shared update",
            ),
            delivery_source_event_id("run-1", "delivery-1", "peer-1", None, None, "Other update"),
        ] {
            assert_ne!(Some(source.clone()), changed);
        }

        let target_one = delivery_source_event_id(
            "run-1",
            "delivery-1",
            "peer-1",
            None,
            Some("peer-2"),
            "Shared update",
        )
        .unwrap();
        let target_two = delivery_source_event_id(
            "run-1",
            "delivery-1",
            "peer-1",
            None,
            Some("peer-3"),
            "Shared update",
        )
        .unwrap();
        assert_ne!(target_one, target_two);
        assert!(is_delivery_projection(&target_one, &source));
        assert!(is_delivery_projection(&target_two, &source));
        assert!(is_delivery_projection(&source, &source));
        assert!(!is_delivery_projection("unrelated", &source));
        assert!(is_room_tool_source_event_id(&source));
        assert!(!is_room_tool_source_event_id("run-1:42"));
    }

    #[test]
    fn requires_a_delivery_and_nonempty_message() {
        assert!(delivery_source_event_id("run-1", " ", "peer-1", None, None, "Hello").is_none());
        assert!(
            delivery_source_event_id("run-1", "delivery-1", "peer-1", None, None, " ").is_none()
        );
    }

    #[test]
    fn provider_message_fallback_is_stable_and_run_scoped() {
        let source = provider_message_source_event_id("run-1", "message-1").unwrap();
        assert_eq!(
            Some(source.clone()),
            provider_message_source_event_id("run-1", " message-1 ")
        );
        assert_ne!(
            Some(source),
            provider_message_source_event_id("run-2", "message-1")
        );
        assert!(provider_message_source_event_id("run-1", " ").is_none());
    }
}
