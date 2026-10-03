//! Room reconciliation needs conversation/lifecycle fields, never tool output blobs.
use super::events::BusEventPage;
use serde::{
    de::{IgnoredAny, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{Map, Value};
use std::{
    fmt,
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
};

#[derive(Default)]
struct RoomEvent(Map<String, Value>);
impl<'de> Deserialize<'de> for RoomEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = RoomEvent;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a room event")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<RoomEvent, M::Error> {
                let mut fields = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if [
                        "type",
                        "turn_id",
                        "message_id",
                        "text",
                        "state",
                        "error",
                        "tool_name",
                        "status",
                        "parent_tool_use_id",
                    ]
                    .contains(&key.as_str())
                    {
                        let mut value: Value = map.next_value()?;
                        if let Some(text) = value.as_str() {
                            value =
                                Value::String(crate::rooms::session_seed::short_text(text, 32_000));
                        }
                        fields.insert(key, value);
                    } else {
                        // Streaming ignored values avoid materializing megabytes of output,
                        // input, deltas, usage or embedded attachment data.
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(RoomEvent(fields))
            }
        }
        deserializer.deserialize_map(Fields)
    }
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(default, rename = "_bus")]
    bus: bool,
    #[serde(default)]
    seq: u64,
    #[serde(default)]
    ts: String,
    #[serde(default)]
    event: RoomEvent,
}

pub fn list_room_events_page(
    run_id: &str,
    since_seq: u64,
    offset: Option<u64>,
) -> Result<BusEventPage, String> {
    let path = super::run_dir(run_id).join("events.jsonl");
    if !path.exists() {
        return Ok(BusEventPage {
            events: vec![],
            last_seq: since_seq,
            has_more: false,
            next_offset: offset.unwrap_or(0),
        });
    }
    let file = File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    // Legacy participants lack an offset. A replaced/truncated log also falls
    // back to a sequence scan, without replaying events behind the watermark.
    let start = offset.filter(|offset| *offset <= length).unwrap_or(0);
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    page_from_reader(&mut reader, since_seq)
}

pub(crate) fn page_from_reader<R: BufRead + Seek>(
    reader: &mut R,
    since_seq: u64,
) -> Result<BusEventPage, String> {
    let mut events = Vec::new();
    let mut bytes = 0;
    let mut last_seq = since_seq;
    let mut next_offset = reader.stream_position().map_err(|e| e.to_string())?;
    let mut has_more = false;
    loop {
        let start = reader.stream_position().map_err(|e| e.to_string())?;
        let result =
            Envelope::deserialize(&mut serde_json::Deserializer::from_reader(&mut *reader));
        let envelope = match result {
            Ok(envelope) => envelope,
            Err(error) if error.is_eof() => break, // Retry a partially appended event later.
            Err(error) if error.is_io() => {
                return Err(format!("Cannot read room history: {error}"))
            }
            Err(_) => {
                reader.skip_until(b'\n').map_err(|e| e.to_string())?;
                next_offset = reader.stream_position().map_err(|e| e.to_string())?;
                continue;
            }
        };
        let end = reader.stream_position().map_err(|e| e.to_string())?;
        if envelope.bus && envelope.seq > since_seq {
            let kind = envelope
                .event
                .0
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("");
            if [
                "provider_turn_started",
                "message_complete",
                "permission_prompt",
                "elicitation_prompt",
                "hook_callback",
                "tool_start",
                "interaction_resolved",
                "control_cancelled",
                "run_state",
                "rate_limit_event",
            ]
            .contains(&kind)
            {
                let mut event = envelope.event.0;
                event.insert("_seq".into(), Value::from(envelope.seq));
                event.insert("ts".into(), Value::from(envelope.ts));
                let event = Value::Object(event);
                let size = serde_json::to_vec(&event).map_err(|e| e.to_string())?.len();
                if !events.is_empty() && (events.len() >= 100 || bytes + size > 1024 * 1024) {
                    next_offset = start;
                    has_more = true;
                    break;
                }
                bytes += size;
                events.push(event);
            }
            last_seq = last_seq.max(envelope.seq);
        }
        next_offset = end;
    }
    Ok(BusEventPage {
        events,
        last_seq,
        has_more,
        next_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn oversized_tool_output_does_not_hide_following_messages_or_completion() {
        let source = format!(
            "{}\n{}\n{}\n{}\n",
            serde_json::json!({"_bus":true,"seq":1,"event":{"type":"tool_start","tool_name":"AskUserQuestion","input":{"text":"x".repeat(2_000_000)}}}),
            serde_json::json!({"_bus":true,"seq":2,"event":{"type":"tool_end","output":"x".repeat(4_000_000)}}),
            serde_json::json!({"_bus":true,"seq":3,"ts":"now","event":{"type":"message_complete","text":"Reply","parent_tool_use_id":null}}),
            serde_json::json!({"_bus":true,"seq":4,"event":{"type":"run_state","state":"idle"}})
        );
        let mut reader = Cursor::new(source.as_bytes());
        let page = page_from_reader(&mut reader, 0).unwrap();
        assert_eq!(page.last_seq, 4);
        assert_eq!(page.events.len(), 3);
        assert_eq!(page.events[0]["tool_name"], "AskUserQuestion");
        assert!(page.events[0].get("input").is_none());
        assert_eq!(page.events[1]["text"], "Reply");
        assert_eq!(page.events[2]["state"], "idle");
        reader.set_position(page.next_offset);
        assert!(page_from_reader(&mut reader, 4).unwrap().events.is_empty());
    }
    #[test]
    fn pagination_and_partial_tail_do_not_skip_or_replay() {
        let source = (1..=105).map(|seq| format!("{}\n",serde_json::json!({"_bus":true,"seq":seq,"event":{"type":"message_complete","text":"hello"}}))).collect::<String>() + r#"{"_bus":true,"seq":106,"event":{"type":"message_complete","text":"partial""#;
        let mut reader = Cursor::new(source.into_bytes());
        let first = page_from_reader(&mut reader, 0).unwrap();
        assert!(first.has_more);
        assert_eq!(first.last_seq, 100);
        reader.set_position(first.next_offset);
        let second = page_from_reader(&mut reader, 100).unwrap();
        assert_eq!(second.events.len(), 5);
        assert_eq!(second.last_seq, 105);
        let offset = second.next_offset;
        reader.get_mut().extend_from_slice(b"}}\n");
        reader.set_position(offset);
        let final_page = page_from_reader(&mut reader, 105).unwrap();
        assert_eq!(final_page.events.len(), 1);
        assert_eq!(final_page.last_seq, 106);
    }
}
