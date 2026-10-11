pub mod context_stripping;
pub mod index;
pub mod paths;
pub mod trace_format;

#[cfg(test)]
mod tests {
    use super::{
        context_stripping::strip_context_fields,
        paths::transcript_file_name,
        trace_format::{decode_record, encode_record, TraceRecord},
    };

    #[test]
    fn trace_roundtrip_and_safe_paths() {
        let row = TraceRecord { sequence: 4, role: "assistant".into(), kind: "delta".into(), payload: "hello".into() };
        assert_eq!(decode_record(&encode_record(&row)).unwrap(), row);
        assert_eq!(transcript_file_name("session:/bad"), "session__bad.jsonl");
        assert_eq!(strip_context_fields("visible\n<context>secret</context>\nend"), "visible\n[context stripped]\nend");
    }
}
