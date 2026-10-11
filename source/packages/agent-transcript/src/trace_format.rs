#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceRecord {
    pub sequence: u64,
    pub role: String,
    pub kind: String,
    pub payload: String,
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' { out.push(ch); continue; }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => { out.push('\\'); out.push(other); }
            None => out.push('\\'),
        }
    }
    out
}

pub fn encode_record(record: &TraceRecord) -> String {
    format!("{}\t{}\t{}\t{}", record.sequence, escape(&record.role), escape(&record.kind), escape(&record.payload))
}

pub fn decode_record(line: &str) -> Result<TraceRecord, &'static str> {
    let mut parts = line.splitn(4, '\t');
    let sequence = parts.next().ok_or("missing sequence")?.parse().map_err(|_| "invalid sequence")?;
    let role = unescape(parts.next().ok_or("missing role")?);
    let kind = unescape(parts.next().ok_or("missing kind")?);
    let payload = unescape(parts.next().ok_or("missing payload")?);
    Ok(TraceRecord { sequence, role, kind, payload })
}
