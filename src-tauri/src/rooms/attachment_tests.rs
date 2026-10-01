use super::{
    attachments::{MAX_FILES, MAX_FILE_BYTES},
    models::{CreateRoomInput, Participant},
    store::RoomStore,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::fs;
use tempfile::TempDir;

struct Fixture {
    temp: TempDir,
    db: std::path::PathBuf,
    store: RoomStore,
    room_id: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("rooms.sqlite3");
        let store = RoomStore::open(&db).unwrap();
        let room = store
            .create(CreateRoomInput {
                title: "Attachment tests".into(),
                objective: "Exercise durable room attachments".into(),
                repo_path: temp.path().to_string_lossy().into_owned(),
                repository: "owner/repository".into(),
                create_project: false,
            })
            .unwrap();
        Self {
            temp,
            db,
            store,
            room_id: room.id,
        }
    }

    fn second_room(&self) -> String {
        self.store
            .create(CreateRoomInput {
                title: "Second room".into(),
                objective: "Test attachment scope".into(),
                repo_path: self.temp.path().to_string_lossy().into_owned(),
                repository: "owner/repository".into(),
                create_project: false,
            })
            .unwrap()
            .id
    }

    fn source(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.temp.path().join(name);
        fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }
}

fn participant(id: &str, name: &str) -> Participant {
    Participant {
        id: id.into(),
        name: name.into(),
        ..Default::default()
    }
}

#[test]
fn file_copy_survives_source_deletion_and_store_reopen() {
    let f = Fixture::new();
    let bytes = b"private room copy";
    let source = f.source("note.txt", bytes);
    let attachment = f
        .store
        .attach_files(&f.room_id, &[source.clone()])
        .unwrap()
        .remove(0);
    fs::remove_file(source).unwrap();
    drop(f.store);

    let reopened = RoomStore::open(&f.db).unwrap();
    let loaded = reopened
        .read_attachment(&f.room_id, &attachment.id)
        .unwrap();
    assert_eq!(loaded.name, "note.txt");
    assert_eq!(STANDARD.decode(loaded.content_base64).unwrap(), bytes);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let (_, path) = reopened
            .attachment_path(&f.room_id, &attachment.id)
            .unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn pasted_binary_is_decoded_and_file_type_comes_from_content_when_recognizable() {
    let f = Fixture::new();
    let binary = [0, 255, 1, 128, 13, 10];
    let pasted = f
        .store
        .upload_attachment(&f.room_id, "paste.bin", &STANDARD.encode(binary))
        .unwrap();
    let saved = f.store.read_attachment(&f.room_id, &pasted.id).unwrap();
    assert_eq!(STANDARD.decode(saved.content_base64).unwrap(), binary);
    assert_eq!(pasted.mime_type, "application/octet-stream");

    let png_header = b"\x89PNG\r\n\x1a\nspoofed-but-sniffable";
    let source = f.source("not-really.jpg", png_header);
    let sniffed = f
        .store
        .attach_files(&f.room_id, &[source])
        .unwrap()
        .remove(0);
    assert_eq!(sniffed.mime_type, "image/png");

    // A vision-like extension alone must not make arbitrary bytes a vision input.
    let source = f.source("unknown.png", b"not an image");
    let spoofed = f
        .store
        .attach_files(&f.room_id, &[source])
        .unwrap()
        .remove(0);
    assert_eq!(spoofed.mime_type, "application/octet-stream");
}

#[test]
fn arbitrary_files_and_hostile_names_are_safely_preserved_as_payload() {
    let f = Fixture::new();
    let source = f.source("opaque.data", &[0, 1, 2, 255]);
    let attachment = f
        .store
        .attach_files(&f.room_id, &[source])
        .unwrap()
        .remove(0);
    assert_eq!(attachment.mime_type, "application/octet-stream");
    assert_eq!(attachment.name, "opaque.data");

    let pasted = f
        .store
        .upload_attachment(&f.room_id, "../private\\x.bin", &STANDARD.encode(b"x"))
        .unwrap();
    assert_eq!(pasted.name, ".._private_x.bin");
    let (_, stored_path) = f.store.attachment_path(&f.room_id, &pasted.id).unwrap();
    let room_attachment_root = f
        .store
        .attachment_root
        .join(&f.room_id)
        .canonicalize()
        .unwrap();
    assert!(stored_path.starts_with(room_attachment_root));
}

#[test]
fn attachment_ids_are_uuid_scoped_and_rejected_across_rooms() {
    let f = Fixture::new();
    let source = f.source("scoped.txt", b"room one");
    let item = f
        .store
        .attach_files(&f.room_id, &[source])
        .unwrap()
        .remove(0);
    let other_room = f.second_room();
    assert!(f.store.attachment_path(&other_room, &item.id).is_err());
    assert!(f
        .store
        .attachment_path(&f.room_id, "../../rooms.sqlite3")
        .is_err());
    assert!(f
        .store
        .resolve_attachments(&f.room_id, &[item.id.clone(), item.id])
        .unwrap_err()
        .contains("twice"));
}

#[test]
fn count_size_duplicate_and_attachment_only_message_rules_hold() {
    let f = Fixture::new();
    let too_many: Vec<_> = (0..=MAX_FILES)
        .map(|i| f.source(&format!("f{i}.txt"), b"x"))
        .collect();
    assert!(f
        .store
        .attach_files(&f.room_id, &too_many)
        .unwrap_err()
        .contains("up to 8"));
    let large = vec![0u8; MAX_FILE_BYTES as usize + 1];
    let too_large = f.source("large.bin", &large);
    assert!(f
        .store
        .attach_files(&f.room_id, &[too_large])
        .unwrap_err()
        .contains("20 MB"));

    let source = f.source("only.txt", b"attachment body");
    let item = f
        .store
        .attach_files(&f.room_id, &[source])
        .unwrap()
        .remove(0);
    assert!(f
        .store
        .append_message_with_attachments(
            &f.room_id,
            "Human",
            String::new(),
            None,
            None,
            None,
            None,
            &[]
        )
        .is_err());
    let saved = f
        .store
        .append_message_with_attachments(
            &f.room_id,
            "Human",
            "  ".into(),
            None,
            None,
            None,
            None,
            &[item.id],
        )
        .unwrap();
    let message = saved.messages.last().unwrap();
    assert_eq!(message.body, "");
    assert_eq!(message.attachments.len(), 1);
}

#[test]
fn mentions_override_picker_for_one_many_and_everyone_and_sidechat_checks_membership() {
    let f = Fixture::new();
    f.store
        .update(&f.room_id, |room| {
            room.participants = vec![
                participant("a", "Claude delegate"),
                participant("b", "Codex"),
            ];
            Ok(())
        })
        .unwrap();
    let one = f
        .store
        .append_message_with_attachments(
            &f.room_id,
            "Human",
            "Please ask @Claude delegate".into(),
            None,
            Some("b".into()),
            None,
            None,
            &[],
        )
        .unwrap();
    assert_eq!(
        one.messages
            .last()
            .unwrap()
            .target_participant_id
            .as_deref(),
        Some("a")
    );

    let many = f
        .store
        .append_message_with_attachments(
            &f.room_id,
            "Human",
            "@Codex and @Claude delegate".into(),
            None,
            Some("b".into()),
            None,
            None,
            &[],
        )
        .unwrap();
    assert_eq!(many.messages.last().unwrap().target_participant_id, None);
    assert_eq!(
        many.messages.last().unwrap().target_participant_ids,
        vec!["b", "a"]
    );

    let everyone = f
        .store
        .append_message_with_attachments(
            &f.room_id,
            "Human",
            "@everyone".into(),
            None,
            Some("a".into()),
            None,
            None,
            &[],
        )
        .unwrap();
    assert_eq!(
        everyone.messages.last().unwrap().target_participant_id,
        None
    );
    assert!(everyone
        .messages
        .last()
        .unwrap()
        .target_participant_ids
        .is_empty());

    // A sidechat can only be created from an existing message.
    let source = everyone.messages.last().unwrap().id.clone();
    let sidechat = f
        .store
        .create_sidechat(&f.room_id, &source, "pair", vec!["a".into()])
        .unwrap();
    let sidechat_id = sidechat.sidechats.last().unwrap().id.clone();
    assert!(f
        .store
        .append_message_with_attachments(
            &f.room_id,
            "Human",
            "@Codex".into(),
            None,
            None,
            None,
            Some(sidechat_id),
            &[]
        )
        .is_err());
}
