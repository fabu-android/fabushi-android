use url::Url;

use crate::agent_attachment::{
    DurableAgentAttachmentOwner, MEDIA_READ_CHUNK_LIMIT_BYTES,
};

pub const CHANNEL_ATTACHMENT_MAX_UPLOAD_BYTES: u64 = 50 * 1024 * 1024;
pub const GENERIC_BINARY_MIME: &str = "application/octet-stream";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedChannelAttachment {
    Url { is_image: bool, url: String },
    Upload {
        is_image: bool,
        bytes: Vec<u8>,
        filename: String,
        mime: String,
    },
}

fn remote_url_looks_like_image(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".avif", ".heic", ".heif"]
        .iter()
        .any(|suffix| path.ends_with(suffix))
}

fn canonical_attachment_id(raw: &str) -> Result<Option<String>, String> {
    let Ok(url) = Url::parse(raw) else {
        return Err("channel attachment must use http(s) or canonical Fabushi attachment identity".into());
    };
    match url.scheme() {
        "http" | "https" => Ok(None),
        "fabushi-attachment" => {
            if !url.path().is_empty() && url.path() != "/" {
                return Err("canonical Fabushi attachment identity has an invalid path".into());
            }
            let id = url
                .host_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or("canonical Fabushi attachment identity is missing its id")?;
            if url.query().is_some() || url.fragment().is_some() {
                return Err("canonical Fabushi attachment identity cannot include query or fragment".into());
            }
            Ok(Some(id.to_string()))
        }
        _ => Err("channel attachment scheme is not permitted on Android".into()),
    }
}

pub fn resolve_android_channel_attachment(
    owner: &DurableAgentAttachmentOwner,
    account_slot: Option<&str>,
    account_fence: &str,
    agent_id: &str,
    raw_url: Option<&str>,
) -> Result<Option<ResolvedChannelAttachment>, String> {
    let Some(raw) = raw_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let url = Url::parse(raw)
        .map_err(|_| "channel attachment must use http(s) or canonical Fabushi attachment identity".to_string())?;
    if matches!(url.scheme(), "http" | "https") {
        return Ok(Some(ResolvedChannelAttachment::Url {
            is_image: remote_url_looks_like_image(&url),
            url: raw.to_string(),
        }));
    }

    let Some(attachment_id) = canonical_attachment_id(raw)? else {
        return Err("channel attachment identity could not be resolved".into());
    };
    let (attachment, _) = owner.read_completed_media_chunk(
        account_slot,
        account_fence,
        agent_id,
        &attachment_id,
        0,
        0,
    )?;
    if attachment.size_bytes == 0 {
        return Err("channel attachment is empty".into());
    }
    if attachment.size_bytes > CHANNEL_ATTACHMENT_MAX_UPLOAD_BYTES {
        return Err("channel attachment exceeds the 50 MiB delivery limit".into());
    }

    let mut bytes = Vec::with_capacity(attachment.size_bytes as usize);
    let mut offset = 0u64;
    while offset < attachment.size_bytes {
        let requested = attachment
            .size_bytes
            .saturating_sub(offset)
            .min(MEDIA_READ_CHUNK_LIMIT_BYTES);
        let (_, chunk) = owner.read_completed_media_chunk(
            account_slot,
            account_fence,
            agent_id,
            &attachment_id,
            offset,
            requested,
        )?;
        if chunk.is_empty() {
            return Err("canonical channel attachment ended before its durable length".into());
        }
        offset = offset.saturating_add(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != attachment.size_bytes {
        return Err("canonical channel attachment length changed during delivery resolution".into());
    }
    let mime = attachment
        .mime_type
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| GENERIC_BINARY_MIME.to_string());
    Ok(Some(ResolvedChannelAttachment::Upload {
        is_image: mime.starts_with("image/"),
        bytes,
        filename: attachment.name,
        mime,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged_owner(label: &str) -> (tempfile::TempDir, DurableAgentAttachmentOwner) {
        let root = tempfile::Builder::new()
            .prefix(&format!("fabushi-channel-attachment-{label}-"))
            .tempdir()
            .unwrap();
        let owner = DurableAgentAttachmentOwner::open(root.path()).unwrap();
        (root, owner)
    }

    fn finish_attachment(
        owner: &mut DurableAgentAttachmentOwner,
        fence: &str,
        agent_id: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> String {
        let id = owner
            .begin(None, fence, agent_id, name, Some(mime), Some(bytes.len() as u64))
            .unwrap();
        owner.append(None, fence, agent_id, &id, 0, bytes).unwrap();
        owner.finish(None, fence, agent_id, &id).unwrap().path
    }

    #[test]
    fn remote_http_urls_pass_through_without_local_file_access() {
        let (_root, owner) = staged_owner("remote");
        let resolved = resolve_android_channel_attachment(
            &owner,
            None,
            "session:test",
            "agent-a",
            Some("https://example.com/photo.png?token=opaque"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            resolved,
            ResolvedChannelAttachment::Url {
                is_image: true,
                url: "https://example.com/photo.png?token=opaque".into(),
            }
        );
        assert!(resolve_android_channel_attachment(
            &owner,
            None,
            "session:test",
            "agent-a",
            Some("file:///sdcard/secret.txt"),
        )
        .is_err());
        assert!(resolve_android_channel_attachment(
            &owner,
            None,
            "session:test",
            "agent-a",
            Some("/sdcard/secret.txt"),
        )
        .is_err());
    }

    #[test]
    fn canonical_private_attachment_resolves_only_inside_its_scope() {
        let (_root, mut owner) = staged_owner("scope");
        let identity = finish_attachment(
            &mut owner,
            "session:a",
            "agent-a",
            "note.txt",
            "text/plain",
            b"hello channel",
        );
        let resolved = resolve_android_channel_attachment(
            &owner,
            None,
            "session:a",
            "agent-a",
            Some(&identity),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            resolved,
            ResolvedChannelAttachment::Upload {
                is_image: false,
                bytes: b"hello channel".to_vec(),
                filename: "note.txt".into(),
                mime: "text/plain".into(),
            }
        );
        assert!(resolve_android_channel_attachment(
            &owner,
            None,
            "session:other",
            "agent-a",
            Some(&identity),
        )
        .is_err());
        assert!(resolve_android_channel_attachment(
            &owner,
            None,
            "session:a",
            "agent-other",
            Some(&identity),
        )
        .is_err());
    }
}
