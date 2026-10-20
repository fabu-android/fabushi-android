//! Platform-neutral Cloud Agent transcript-dump semantics from Desktop current main.
//!
//! The external CloudAgent executor supplies the backend transcript and bounded file writer.
//! Android Presentation never owns or synthesizes canonical transcript dump state.

pub fn cloud_agent_transcript_dump_path(agent_id: &str) -> Result<String, String> {
    let id = agent_id.trim();
    if id.is_empty()
        || id.len() > 512
        || id.chars().any(char::is_control)
        || id.contains('/')
        || id.contains('\\')
        || id == "."
        || id == ".."
    {
        return Err("Cloud Agent transcript id is invalid".into());
    }
    Ok(format!("cloud-agent-transcripts/{id}.jsonl"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudAgentTranscriptDump {
    pub status: String,
    pub line_count: usize,
    pub jsonl: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudAgentDumpFile {
    pub path: String,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudAgentDumpOutcome {
    pub status: String,
    pub line_count: usize,
    pub file: Option<CloudAgentDumpFile>,
}

pub trait CloudAgentDumpApi {
    fn get_transcript_dump(
        &self,
        agent_id: &str,
    ) -> Result<Option<CloudAgentTranscriptDump>, String>;
}

pub trait CloudAgentBoxFileWriter {
    fn write_box_file(&self, path: &str, data: &[u8]) -> Result<(), String>;
}

pub fn dump_cloud_agent_transcript(
    api: &dyn CloudAgentDumpApi,
    writer: &dyn CloudAgentBoxFileWriter,
    agent_id: &str,
) -> Result<Option<CloudAgentDumpOutcome>, String> {
    let Some(dump) = api.get_transcript_dump(agent_id)? else {
        return Ok(None);
    };
    if dump.line_count == 0 {
        return Ok(Some(CloudAgentDumpOutcome {
            status: dump.status,
            line_count: 0,
            file: None,
        }));
    }
    let path = cloud_agent_transcript_dump_path(agent_id)?;
    let data = dump.jsonl.as_bytes();
    writer.write_box_file(&path, data)?;
    Ok(Some(CloudAgentDumpOutcome {
        status: dump.status,
        line_count: dump.line_count,
        file: Some(CloudAgentDumpFile {
            path,
            size_bytes: data.len(),
        }),
    }))
}

pub fn format_cloud_agent_dump_completion_line(
    file: &CloudAgentDumpFile,
    line_count: usize,
) -> String {
    format!(
        "Full transcript dumped to {} ({} bytes, {} message lines). Read it with your shell tools; `tail -n 1 {}` gives the final assistant report.",
        file.path, file.size_bytes, line_count, file.path
    )
}

pub trait CloudAgentWatchResult: Clone {
    fn text(&self) -> &str;
    fn with_text(&self, text: String) -> Self;
}

pub fn augment_watch_result_with_transcript_dump<T: CloudAgentWatchResult>(
    api: &dyn CloudAgentDumpApi,
    writer: &dyn CloudAgentBoxFileWriter,
    agent_id: &str,
    result: &T,
) -> T {
    let Ok(Some(outcome)) = dump_cloud_agent_transcript(api, writer, agent_id) else {
        return result.clone();
    };
    let Some(file) = outcome.file else {
        return result.clone();
    };
    result.with_text(format!(
        "{}\n\n{}",
        result.text(),
        format_cloud_agent_dump_completion_line(&file, outcome.line_count)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Api(Option<CloudAgentTranscriptDump>);
    impl CloudAgentDumpApi for Api {
        fn get_transcript_dump(
            &self,
            _agent_id: &str,
        ) -> Result<Option<CloudAgentTranscriptDump>, String> {
            Ok(self.0.clone())
        }
    }

    #[derive(Default)]
    struct Writer(RefCell<Vec<(String, Vec<u8>)>>);
    impl CloudAgentBoxFileWriter for Writer {
        fn write_box_file(&self, path: &str, data: &[u8]) -> Result<(), String> {
            self.0.borrow_mut().push((path.to_string(), data.to_vec()));
            Ok(())
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Watch(String);
    impl CloudAgentWatchResult for Watch {
        fn text(&self) -> &str {
            &self.0
        }
        fn with_text(&self, text: String) -> Self {
            Self(text)
        }
    }

    #[test]
    fn non_empty_dump_writes_desktop_path_and_completion_line() {
        let api = Api(Some(CloudAgentTranscriptDump {
            status: "completed".into(),
            line_count: 2,
            jsonl: "{\"role\":\"user\"}\n{\"role\":\"assistant\"}\n".into(),
        }));
        let writer = Writer::default();
        let outcome = dump_cloud_agent_transcript(&api, &writer, "bc-1")
            .unwrap()
            .unwrap();
        assert_eq!(outcome.line_count, 2);
        assert_eq!(
            outcome.file.as_ref().unwrap().path,
            "cloud-agent-transcripts/bc-1.jsonl"
        );
        assert_eq!(writer.0.borrow().len(), 1);
        let line = format_cloud_agent_dump_completion_line(
            outcome.file.as_ref().unwrap(),
            outcome.line_count,
        );
        assert!(line.contains("2 message lines"));
        assert!(line.contains("tail -n 1 cloud-agent-transcripts/bc-1.jsonl"));
    }

    #[test]
    fn zero_line_dump_does_not_create_a_file() {
        let api = Api(Some(CloudAgentTranscriptDump {
            status: "running".into(),
            line_count: 0,
            jsonl: String::new(),
        }));
        let writer = Writer::default();
        let outcome = dump_cloud_agent_transcript(&api, &writer, "bc-1")
            .unwrap()
            .unwrap();
        assert!(outcome.file.is_none());
        assert!(writer.0.borrow().is_empty());
    }

    #[test]
    fn watch_augmentation_is_best_effort_and_preserves_original_text() {
        let api = Api(Some(CloudAgentTranscriptDump {
            status: "completed".into(),
            line_count: 1,
            jsonl: "{\"role\":\"assistant\"}\n".into(),
        }));
        let writer = Writer::default();
        let augmented =
            augment_watch_result_with_transcript_dump(&api, &writer, "bc-1", &Watch("done".into()));
        assert!(augmented.0.starts_with("done\n\nFull transcript dumped"));
    }

    #[test]
    fn transcript_path_rejects_traversal_and_control_ids() {
        for invalid in ["", "../escape", "a/b", "a\\b", "line\nfeed"] {
            assert!(cloud_agent_transcript_dump_path(invalid).is_err());
        }
    }
}
