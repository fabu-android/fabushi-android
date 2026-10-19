use std::{collections::BTreeSet, fs, path::{Path, PathBuf}};

use rusqlite::{params, Connection, Error, ErrorCode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

use super::search_index_db::{
    ensure_search_index_schema, open_search_index_db, read_reconcile_done,
    read_search_index_schema_version, search_media, search_messages,
    stamp_search_index_schema_version, write_reconcile_done, AttachmentKind,
    MediaSearchResult, MessageSearchResult, SEARCH_INDEX_SCHEMA_VERSION,
};

const TRANSCRIPT_FINGERPRINT_KEY: &str = "android_transcript_fingerprint";
const INDEXED_BODY_MAX_CHARS: usize = 20_000;
const SQLITE_DB_SIDECAR_SUFFIXES: &[&str] = &["-wal", "-shm", "-journal"];

pub struct AndroidContentSearchIndex {
    path: PathBuf,
    db: Connection,
    fingerprint: Option<String>,
    reconciled: bool,
    rebuild_count: usize,
}

impl AndroidContentSearchIndex {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let db = open_or_recreate(&path)?;
        let fingerprint = read_meta(&db, TRANSCRIPT_FINGERPRINT_KEY).ok().flatten();
        let reconciled = read_reconcile_done(&db).unwrap_or(false);
        Ok(Self { path, db, fingerprint, reconciled, rebuild_count: 0 })
    }

    pub fn is_ready(&self) -> bool { self.reconciled }

    pub fn search(
        &mut self,
        entries: &[Value],
        query: &str,
        message_limit: usize,
        media_limit: usize,
    ) -> Result<(Vec<MessageSearchResult>, Vec<MediaSearchResult>), String> {
        self.reconcile(entries)?;
        match self.search_once(query, message_limit, media_limit) {
            Ok(result) => Ok(result),
            Err(error) if is_corrupt_error(&error) => {
                self.rebuild(entries)?;
                self.search_once(query, message_limit, media_limit).map_err(|error| error.to_string())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn reconcile(&mut self, entries: &[Value]) -> Result<bool, String> {
        let fingerprint = transcript_fingerprint(entries)?;
        if self.reconciled && self.fingerprint.as_deref() == Some(fingerprint.as_str()) {
            return Ok(false);
        }
        match rebuild_rows(&self.db, entries, &fingerprint) {
            Ok(()) => {
                self.fingerprint = Some(fingerprint);
                self.reconciled = true;
                Ok(true)
            }
            Err(error) if is_corrupt_error(&error) => {
                self.rebuild(entries)?;
                Ok(true)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn rebuild(&mut self, entries: &[Value]) -> Result<(), String> {
        self.db = recreate(&self.path)?;
        self.fingerprint = None;
        self.reconciled = false;
        self.rebuild_count = self.rebuild_count.saturating_add(1);
        let fingerprint = transcript_fingerprint(entries)?;
        rebuild_rows(&self.db, entries, &fingerprint).map_err(|error| error.to_string())?;
        self.fingerprint = Some(fingerprint);
        self.reconciled = true;
        Ok(())
    }

    fn search_once(
        &self,
        query: &str,
        message_limit: usize,
        media_limit: usize,
    ) -> rusqlite::Result<(Vec<MessageSearchResult>, Vec<MediaSearchResult>)> {
        Ok((search_messages(&self.db, query, message_limit)?, search_media(&self.db, query, media_limit)?))
    }
}

fn open_or_recreate(path: &Path) -> Result<Connection, String> {
    match open_search_index_db(path).and_then(|db| {
        ensure_search_index_schema(&db)?;
        let version = read_search_index_schema_version(&db)?;
        if version == 0 {
            stamp_search_index_schema_version(&db)?;
        } else if version != SEARCH_INDEX_SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(db)
    }) {
        Ok(db) => Ok(db),
        Err(error) if error.to_string().contains("no such module: fts5") => Err(error.to_string()),
        Err(_) => recreate(path),
    }
}

fn recreate(path: &Path) -> Result<Connection, String> {
    let base = path.to_string_lossy();
    for suffix in std::iter::once("").chain(SQLITE_DB_SIDECAR_SUFFIXES.iter().copied()) {
        let _ = fs::remove_file(format!("{base}{suffix}"));
    }
    let db = open_search_index_db(path).map_err(|error| error.to_string())?;
    ensure_search_index_schema(&db).map_err(|error| error.to_string())?;
    stamp_search_index_schema_version(&db).map_err(|error| error.to_string())?;
    Ok(db)
}

fn read_meta(db: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut statement = db.prepare("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = statement.query([key])?;
    Ok(rows.next()?.and_then(|row| row.get::<_, String>(0).ok()))
}

fn transcript_fingerprint(entries: &[Value]) -> Result<String, String> {
    let serialized = serde_json::to_vec(entries).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    digest.update(serialized);
    Ok(format!("{:x}", digest.finalize()))
}

fn rebuild_rows(db: &Connection, entries: &[Value], fingerprint: &str) -> rusqlite::Result<()> {
    db.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        db.execute("DELETE FROM messages", [])?;
        db.execute("DELETE FROM media", [])?;
        db.execute("DELETE FROM agents", [])?;
        let mut agents = BTreeSet::new();
        for entry in entries {
            let Some(agent_id) = entry.get("agentId").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) else { continue; };
            let Some(entry_id) = entry.get("id").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) else { continue; };
            agents.insert(agent_id.to_string());
            if let Some((role, timestamp_ms, body)) = indexed_message(entry) {
                db.execute("INSERT INTO messages (agent_id, entry_id, role, timestamp_ms, body) VALUES (?1, ?2, ?3, ?4, ?5)", params![agent_id, entry_id, role, timestamp_ms, body])?;
            }
            if let Some(media) = indexed_media(entry) {
                db.execute("INSERT INTO media (agent_id, entry_id, file_name, ext, mime, kind, timestamp_ms, width, height) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)", params![agent_id, entry_id, media.file_name, media.ext, media.mime, media.kind, media.timestamp_ms, media.width, media.height])?;
            }
        }
        for agent_id in agents {
            db.execute("INSERT INTO agents (agent_id, fingerprint) VALUES (?1, ?2)", params![agent_id, fingerprint])?;
        }
        db.execute("INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![TRANSCRIPT_FINGERPRINT_KEY, fingerprint])?;
        write_reconcile_done(db)?;
        Ok::<(), rusqlite::Error>(())
    })();
    match result { Ok(()) => db.execute_batch("COMMIT"), Err(error) => { let _ = db.execute_batch("ROLLBACK"); Err(error) } }
}

fn indexed_message(entry: &Value) -> Option<(String, i64, String)> {
    if entry.get("hiddenOutboundAgentPeerMessage").and_then(Value::as_bool).unwrap_or(false) { return None; }
    if entry.get("toAgent")
        .and_then(|value| value.get("kind"))
        .and_then(Value::as_str) == Some("agent")
    {
        return None;
    }
    if entry.get("hidden").and_then(Value::as_bool).unwrap_or(false) && entry.get("toAgent").is_some() { return None; }
    let kind = entry.get("kind").and_then(Value::as_str).unwrap_or_default();
    let body = match kind {
        "message" => entry.get("content").and_then(Value::as_str).unwrap_or_default(),
        "send-message" if entry.get("message").and_then(Value::as_object).and_then(|m|m.get("type")).and_then(Value::as_str)==Some("text") => entry.get("message").and_then(Value::as_object).and_then(|m|m.get("content")).and_then(Value::as_str).unwrap_or_default(),
        "notice" => entry.get("text").and_then(Value::as_str).unwrap_or_default(),
        _ => "",
    }.trim();
    if body.is_empty() { return None; }
    let role = if kind=="message" && entry.get("role").and_then(Value::as_str)==Some("user") { "user" } else { "assistant" };
    Some((role.into(), timestamp(entry), body.chars().take(INDEXED_BODY_MAX_CHARS).collect()))
}

struct IndexedMedia { file_name:String, ext:String, mime:Option<String>, kind:String, timestamp_ms:i64, width:Option<i64>, height:Option<i64> }
fn indexed_media(entry:&Value)->Option<IndexedMedia>{
    let kind=entry.get("kind").and_then(Value::as_str).unwrap_or_default();
    let (source,name,width,height)=if kind=="user-attachment"{(entry.get("filePath")?.as_str()?,entry.get("fileName").and_then(Value::as_str),whole(entry.get("width")),whole(entry.get("height")))}else if kind=="send-message"&&entry.get("message").and_then(Value::as_object).and_then(|m|m.get("type")).and_then(Value::as_str)==Some("attachment"){let m=entry.get("message")?.as_object()?;(m.get("url")?.as_str()?,m.get("fileName").and_then(Value::as_str),None,None)}else{return None;};
    let file_name=media_file_name(name,source); if file_name.is_empty(){return None;} let ext=extension(&file_name).map(|v|format!(".{v}")).unwrap_or_default(); let mime=media_mime(&file_name).map(str::to_string); let kind=classify_attachment(&file_name,source); Some(IndexedMedia{file_name,ext,mime,kind,timestamp_ms:timestamp(entry),width,height})
}
fn timestamp(entry:&Value)->i64{entry.get("timestampMs").and_then(Value::as_f64).filter(|v|v.is_finite()).map(|v|(v+0.5).floor() as i64).unwrap_or(0)}
fn whole(v:Option<&Value>)->Option<i64>{v.and_then(Value::as_f64).filter(|v|v.is_finite()).map(|v|(v+0.5).floor() as i64)}
fn extension(name:&str)->Option<String>{let base=name.rsplit(['/', '\\']).next().unwrap_or("");let dot=base.rfind('.')?;if dot==0||dot+1>=base.len(){None}else{Some(base[dot+1..].to_ascii_lowercase())}}
fn media_file_name(explicit:Option<&str>,source:&str)->String{if let Some(name)=explicit.map(str::trim).filter(|v|!v.is_empty()){return name.to_string();}let path=Url::parse(source).ok().map(|u|u.path().to_string()).unwrap_or_else(||source.to_string());path.rsplit(['/', '\\']).next().unwrap_or("").to_string()}
fn media_mime(name:&str)->Option<&'static str>{match extension(name)?.as_str(){"avif"=>Some("image/avif"),"bmp"=>Some("image/bmp"),"gif"=>Some("image/gif"),"jpeg"|"jpg"=>Some("image/jpeg"),"png"=>Some("image/png"),"svg"=>Some("image/svg+xml"),"webp"=>Some("image/webp"),"mp4"|"m4v"=>Some("video/mp4"),"mov"=>Some("video/quicktime"),"webm"=>Some("video/webm"),"mp3"=>Some("audio/mpeg"),"m4a"=>Some("audio/mp4"),"wav"=>Some("audio/wav"),"ogg"|"oga"|"opus"=>Some("audio/ogg"),_=>None}}
fn classify_attachment(name:&str,source:&str)->String{let ext=extension(name).or_else(||extension(source)).unwrap_or_default();let kind=match ext.as_str(){"avif"|"bmp"|"gif"|"jpeg"|"jpg"|"png"|"svg"|"webp"=>AttachmentKind::Image,"mp4"|"m4v"|"mov"|"webm"=>AttachmentKind::Video,"mp3"|"m4a"|"wav"|"ogg"|"oga"|"opus"=>AttachmentKind::Audio,"pdf"=>AttachmentKind::Pdf,"md"|"markdown"|"mdx"=>AttachmentKind::Markdown,"csv"|"tsv"|"xlsx"|"xls"=>AttachmentKind::Table,"json"|"jsonc"|"json5"|"ndjson"=>AttachmentKind::Json,"docx"=>AttachmentKind::Document,"zip"|"tar"|"gz"|"7z"|"rar"=>AttachmentKind::Archive,"txt"|"log"|"xml"|"yaml"|"yml"|"toml"|"kt"|"java"|"rs"|"py"|"js"|"ts"|"tsx"=>AttachmentKind::Text,_=>AttachmentKind::File};match kind{AttachmentKind::Image=>"image",AttachmentKind::Video=>"video",AttachmentKind::Audio=>"audio",AttachmentKind::Pdf=>"pdf",AttachmentKind::Markdown=>"markdown",AttachmentKind::Table=>"table",AttachmentKind::Json=>"json",AttachmentKind::Text=>"text",AttachmentKind::Document=>"document",AttachmentKind::Archive=>"archive",AttachmentKind::File=>"file"}.into()}
fn is_corrupt_error(error:&Error)->bool{matches!(error,Error::SqliteFailure(rusqlite::ffi::Error{code:ErrorCode::DatabaseCorrupt|ErrorCode::NotADatabase,..},_))}

#[cfg(test)]
mod tests{
 use super::*; use serde_json::json;
 #[test] fn persistent_index_reconciles_content_and_media_and_survives_reopen(){let root=tempfile::tempdir().unwrap();let path=root.path().join("search-index.db");let entries=vec![json!({"id":"m1","agentId":"a","kind":"message","role":"user","content":"Alpha beta","timestampMs":1}),json!({"id":"m2","agentId":"b","kind":"message","role":"assistant","content":"alpha newest","timestampMs":2}),json!({"id":"p1","agentId":"a","kind":"user-attachment","filePath":"/tmp/report.pdf","fileName":"report.pdf","timestampMs":3})];{let mut index=AndroidContentSearchIndex::open(&path).unwrap();let (messages,media)=index.search(&entries,"alpha",50,50).unwrap();assert_eq!(messages.iter().map(|m|m.entry_id.as_str()).collect::<Vec<_>>(),vec!["m2","m1"]);assert!(media.is_empty());let (_,media)=index.search(&entries,"report",50,50).unwrap();assert_eq!(media[0].file_name,"report.pdf");assert!(index.is_ready());}let mut reopened=AndroidContentSearchIndex::open(&path).unwrap();assert_eq!(reopened.search(&entries,"alpha",50,50).unwrap().0.len(),2);}
 #[test] fn changed_canonical_transcript_rebuilds_and_corrupt_database_is_recovered(){let root=tempfile::tempdir().unwrap();let path=root.path().join("search-index.db");let first=vec![json!({"id":"m1","agentId":"a","kind":"message","role":"user","content":"before","timestampMs":1})];let changed=vec![json!({"id":"m1","agentId":"a","kind":"message","role":"user","content":"after","timestampMs":1})];let mut index=AndroidContentSearchIndex::open(&path).unwrap();index.search(&first,"before",50,50).unwrap();assert_eq!(index.search(&changed,"before",50,50).unwrap().0.len(),0);assert_eq!(index.search(&changed,"after",50,50).unwrap().0.len(),1);drop(index);fs::write(&path,b"not a sqlite database").unwrap();let mut recovered=AndroidContentSearchIndex::open(&path).unwrap();assert_eq!(recovered.search(&changed,"after",50,50).unwrap().0.len(),1);}
}
