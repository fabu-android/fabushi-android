use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SandAttachmentKind {
    Image,
    Video,
    Audio,
    Pdf,
    Markdown,
    Table,
    Json,
    Text,
    Document,
    Archive,
    File,
}

impl SandAttachmentKind {
    pub fn parse(value: &str) -> Self {
        match value {
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "pdf" => Self::Pdf,
            "markdown" => Self::Markdown,
            "table" => Self::Table,
            "json" => Self::Json,
            "text" => Self::Text,
            "document" => Self::Document,
            "archive" => Self::Archive,
            _ => Self::File,
        }
    }

    fn labels(self) -> (&'static str, &'static str) {
        match self {
            Self::Image => ("image", "images"),
            Self::Video => ("video", "videos"),
            Self::Audio => ("audio file", "audio files"),
            Self::Pdf => ("PDF", "PDFs"),
            Self::Markdown => ("Markdown file", "Markdown files"),
            Self::Table => ("spreadsheet", "spreadsheets"),
            Self::Json => ("JSON file", "JSON files"),
            Self::Text => ("text file", "text files"),
            Self::Document => ("document", "documents"),
            Self::Archive => ("archive", "archives"),
            Self::File => ("file", "files"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentKindCount {
    pub kind: String,
    pub count: i64,
}

pub fn kind_phrase(kind: SandAttachmentKind, count: usize) -> String {
    let (singular, plural) = kind.labels();
    format!("{count} {}", if count == 1 { singular } else { plural })
}

pub fn merge_kind_counts(kinds: &[AttachmentKindCount]) -> Vec<(SandAttachmentKind, usize)> {
    let mut merged = BTreeMap::<SandAttachmentKind, usize>::new();
    for entry in kinds.iter().filter(|entry| entry.count > 0) {
        let kind = SandAttachmentKind::parse(&entry.kind);
        *merged.entry(kind).or_default() += entry.count as usize;
    }
    merged.into_iter().collect()
}

pub fn format_attachment_sent_summary(
    count: i64,
    kinds: Option<&[AttachmentKindCount]>,
) -> String {
    let total = count.max(1) as usize;
    let merged = kinds.map(merge_kind_counts).unwrap_or_default();
    if merged.is_empty() {
        return format!("Sent {}", kind_phrase(SandAttachmentKind::File, total));
    }
    if merged.len() == 1 {
        return format!("Sent {}", kind_phrase(merged[0].0, total));
    }
    let breakdown = merged
        .iter()
        .map(|(kind, count)| kind_phrase(*kind, *count))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Sent {} · {breakdown}",
        kind_phrase(SandAttachmentKind::File, total)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_kinds_merge_into_file_and_summary_is_deterministic() {
        let kinds = vec![
            AttachmentKindCount {
                kind: "image".into(),
                count: 2,
            },
            AttachmentKindCount {
                kind: "mystery".into(),
                count: 1,
            },
        ];
        let summary = format_attachment_sent_summary(3, Some(&kinds));
        assert!(summary.starts_with("Sent 3 files ·"));
        assert!(summary.contains("2 images"));
        assert!(summary.contains("1 file"));
    }
}
