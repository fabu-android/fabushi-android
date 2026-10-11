pub fn transcript_file_name(session_id: &str) -> String {
    let mut safe: String = session_id.chars()
        .take(180)
        .map(|ch| if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') { ch } else { '_' })
        .collect();
    if safe.is_empty() { safe.push_str("session"); }
    format!("{safe}.jsonl")
}
