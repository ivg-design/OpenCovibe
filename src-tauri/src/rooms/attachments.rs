//! Durable room files. Room payloads contain metadata, never base64 blobs.
use super::{models::RoomAttachment, store::RoomStore};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_FILES: usize = 8;
pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_MESSAGE_BYTES: u64 = 80 * 1024 * 1024;

fn valid_id(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "Attachment not found.".into())
}

fn safe_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err("Choose a file with a name of 1–200 characters.".into());
    }
    Ok(name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\') {
                '_'
            } else {
                c
            }
        })
        .collect())
}

fn mime(name: &str, bytes: &[u8]) -> String {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "image/png".into();
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return "image/jpeg".into();
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return "image/gif".into();
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return "image/webp".into();
    }
    if bytes.starts_with(b"%PDF-") {
        return "application/pdf".into();
    }
    let guessed = mime_guess::from_path(name)
        .first_or_octet_stream()
        .to_string();
    // Do not send an arbitrary file as provider vision/document input on extension alone.
    if is_vision_image(&guessed) || guessed == "application/pdf" {
        "application/octet-stream".into()
    } else {
        guessed
    }
}

pub fn is_vision_image(mime: &str) -> bool {
    matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|_| {
        "Cannot read this file. Check that it still exists and is accessible.".to_string()
    })?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Choose files rather than folders.".into());
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err("Each room attachment can be up to 20 MB.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("Each room attachment can be up to 20 MB.".into());
    }
    Ok(bytes)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

impl RoomStore {
    fn attachment_dir(&self, room_id: &str, attachment_id: &str) -> Result<PathBuf, String> {
        valid_id(room_id)?;
        valid_id(attachment_id)?;
        Ok(self.attachment_root.join(room_id).join(attachment_id))
    }

    fn ensure_can_attach(&self, room_id: &str) -> Result<(), String> {
        if self.get(room_id)?.archived {
            return Err("This room is archived.".into());
        }
        Ok(())
    }

    fn save_attachment(
        &self,
        room_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<RoomAttachment, String> {
        self.ensure_can_attach(room_id)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("Each room attachment can be up to 20 MB.".into());
        }
        let name = safe_name(name)?;
        let attachment = RoomAttachment {
            id: uuid::Uuid::new_v4().to_string(),
            mime_type: mime(&name, bytes),
            name,
            size: bytes.len() as u64,
        };
        let dir = self.attachment_dir(room_id, &attachment.id)?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        write_private(&dir.join(format!("file-{}", attachment.name)), bytes)?;
        write_private(
            &dir.join("meta.json"),
            &serde_json::to_vec(&attachment).map_err(|e| e.to_string())?,
        )?;
        Ok(attachment)
    }

    pub fn attach_files(
        &self,
        room_id: &str,
        paths: &[String],
    ) -> Result<Vec<RoomAttachment>, String> {
        self.ensure_can_attach(room_id)?;
        if paths.is_empty() || paths.len() > MAX_FILES {
            return Err("Choose up to 8 files at a time.".into());
        }
        let mut files = Vec::new();
        let mut total = 0u64;
        // Validate the entire batch before persisting any file.
        for path in paths {
            let path = Path::new(path);
            if !path.is_absolute() {
                return Err("Choose a file using its full local path.".into());
            }
            let name = safe_name(
                &path
                    .file_name()
                    .ok_or("File has no name.")?
                    .to_string_lossy(),
            )?;
            let bytes = read_bounded(path)?;
            total += bytes.len() as u64;
            if total > MAX_MESSAGE_BYTES {
                return Err("Room attachments can total up to 80 MB per message.".into());
            }
            files.push((name, bytes));
        }
        files
            .into_iter()
            .map(|(name, bytes)| self.save_attachment(room_id, &name, &bytes))
            .collect()
    }

    pub fn upload_attachment(
        &self,
        room_id: &str,
        name: &str,
        base64: &str,
    ) -> Result<RoomAttachment, String> {
        self.ensure_can_attach(room_id)?;
        if base64.len() as u64 > MAX_FILE_BYTES.div_ceil(3) * 4 {
            return Err("Each room attachment can be up to 20 MB.".into());
        }
        let bytes = STANDARD
            .decode(base64)
            .map_err(|_| "Cannot read the pasted file. Try attaching it again.".to_string())?;
        self.save_attachment(room_id, name, &bytes)
    }

    pub fn attachment_path(
        &self,
        room_id: &str,
        id: &str,
    ) -> Result<(RoomAttachment, PathBuf), String> {
        self.get(room_id)?;
        let dir = self.attachment_dir(room_id, id)?;
        let metadata =
            fs::read(dir.join("meta.json")).map_err(|_| "Attachment not found.".to_string())?;
        if metadata.len() > 4096 {
            return Err("Attachment metadata is invalid.".into());
        }
        let attachment: RoomAttachment = serde_json::from_slice(&metadata)
            .map_err(|_| "Attachment metadata is invalid.".to_string())?;
        if attachment.id != id
            || safe_name(&attachment.name)? != attachment.name
            || attachment.size > MAX_FILE_BYTES
        {
            return Err("Attachment metadata is invalid.".into());
        }
        let path = dir
            .join(format!("file-{}", attachment.name))
            .canonicalize()
            .map_err(|_| "Attachment file is missing.".to_string())?;
        let expected = dir
            .canonicalize()
            .map_err(|_| "Attachment not found.".to_string())?;
        if !path.starts_with(expected) || !path.is_file() {
            return Err("Attachment path is invalid.".into());
        }
        Ok((attachment, path))
    }

    pub fn resolve_attachments(
        &self,
        room_id: &str,
        ids: &[String],
    ) -> Result<Vec<RoomAttachment>, String> {
        if ids.len() > MAX_FILES {
            return Err("Attach up to 8 files per message.".into());
        }
        let mut seen = HashSet::new();
        let mut total = 0u64;
        let mut attachments = Vec::new();
        for id in ids {
            if !seen.insert(id) {
                return Err("The same attachment was added twice.".into());
            }
            let (attachment, _) = self.attachment_path(room_id, id)?;
            total += attachment.size;
            if total > MAX_MESSAGE_BYTES {
                return Err("Room attachments can total up to 80 MB per message.".into());
            }
            attachments.push(attachment);
        }
        Ok(attachments)
    }

    pub fn read_attachment(
        &self,
        room_id: &str,
        id: &str,
    ) -> Result<crate::models::Attachment, String> {
        let (attachment, path) = self.attachment_path(room_id, id)?;
        let bytes = read_bounded(&path)?;
        if bytes.len() as u64 != attachment.size {
            return Err("The saved attachment has changed. Attach it again.".into());
        }
        Ok(crate::models::Attachment {
            name: attachment.name,
            mime_type: attachment.mime_type,
            size: attachment.size,
            content_base64: STANDARD.encode(bytes),
        })
    }
}
