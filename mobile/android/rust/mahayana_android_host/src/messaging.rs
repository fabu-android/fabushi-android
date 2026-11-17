//! Messaging Protocol v2 Engine (`feature.messaging.access.issue`,
//! `feature.messaging.execute`, `feature.messaging.blob.read`) + FTS5 Search & Export.
//!
//! Persists actors, conversations, messages (with FTS5 + CJK fallback), folders,
//! drafts, and chunked media blobs in `<app_data_dir>/runtime.sqlite3` (WAL mode)
//! and `<app_data_dir>/messaging_blobs/`.

use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const DEFAULT_ACTOR_ID: &str = "human:account:fabushi-user";

pub fn init_schema(conn: &Connection, test_mode: bool) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS msg_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS msg_actors (
            actor_id TEXT PRIMARY KEY,
            actor_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS msg_conversations (
            conversation_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            conversation_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS msg_messages (
            message_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            search_text TEXT NOT NULL,
            message_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_msg_messages_conv_time
            ON msg_messages(conversation_id, created_at_ms ASC);
        CREATE VIRTUAL TABLE IF NOT EXISTS msg_messages_fts USING fts5(
            message_id UNINDEXED,
            conversation_id UNINDEXED,
            search_text
        );
        CREATE TABLE IF NOT EXISTS msg_folders (
            folder_id TEXT PRIMARY KEY,
            folder_json TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS msg_drafts (
            conversation_id TEXT PRIMARY KEY,
            draft_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS msg_blobs (
            blob_id TEXT PRIMARY KEY,
            metadata_json TEXT NOT NULL,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            content_hash TEXT,
            finished INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed initializing messaging schema: {e}"))?;

    if !test_mode {
        seed_production_dialogs(conn)?;
    }
    Ok(())
}

fn next_cursor(conn: &Connection) -> String {
    let current: i64 = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM msg_meta WHERE key = 'cursor_seq'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let next = current + 1;
    let _ = conn.execute(
        "INSERT INTO msg_meta (key, value) VALUES ('cursor_seq', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![next.to_string()],
    );
    format!("pts:{next}")
}

fn seed_production_dialogs(conn: &Connection) -> Result<(), String> {
    let already_seeded: Option<String> = conn
        .query_row(
            "SELECT value FROM msg_meta WHERE key = 'seeded_starter_dialogs'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if already_seeded.is_some() {
        return Ok(());
    }

    let now = chrono::Utc::now().timestamp_millis();
    let actors = vec![
        json!({
            "id": DEFAULT_ACTOR_ID,
            "kind": "human",
            "displayName": "Fabushi 用户",
            "username": "fabushi_user",
            "avatarUrl": null,
            "bio": "Mahayana Android Native Host",
            "capabilities": ["messages", "groups", "channels", "calls", "payments", "miniApps"],
            "presence": { "status": "online", "lastSeenAtMs": now, "statusText": null },
            "verified": true
        }),
        json!({
            "id": "bot:botfather",
            "kind": "bot",
            "displayName": "@BotFather",
            "username": "BotFather",
            "avatarUrl": null,
            "bio": "Create and manage Fabushi & Telegram Mini App Bots via /newbot and /mybots.",
            "capabilities": ["messages", "miniApps"],
            "presence": { "status": "online", "lastSeenAtMs": now, "statusText": "bot" },
            "verified": true
        }),
        json!({
            "id": "bot:mika",
            "kind": "bot",
            "displayName": "@MikaPhoneBot (手机操控助手)",
            "username": "MikaPhoneBot",
            "avatarUrl": null,
            "bio": "On-device Android Accessibility + ReAct phone control agent.",
            "capabilities": ["messages", "miniApps"],
            "presence": { "status": "online", "lastSeenAtMs": now, "statusText": "on-device agent" },
            "verified": true
        }),
        json!({
            "id": "bot:hermes",
            "kind": "bot",
            "displayName": "@HermesAI",
            "username": "HermesAI",
            "avatarUrl": null,
            "bio": "Hermes AI Studio — 61 slash commands, memory, cron & kanban.",
            "capabilities": ["messages", "miniApps"],
            "presence": { "status": "online", "lastSeenAtMs": now, "statusText": "AI Studio" },
            "verified": true
        }),
        json!({
            "id": "bot:global-dharma",
            "kind": "bot",
            "displayName": "全球法布施助手",
            "username": "GlobalDharmaBot",
            "avatarUrl": null,
            "bio": "全球法布施修持、禅堂转经轮与功德回向助手。",
            "capabilities": ["messages", "payments", "miniApps"],
            "presence": { "status": "online", "lastSeenAtMs": now, "statusText": "Mini App" },
            "verified": true
        }),
        json!({
            "id": "human:contact:ananda",
            "kind": "human",
            "displayName": "阿难陀 (Ananda)",
            "username": "ananda_zen",
            "avatarUrl": null,
            "bio": "多闻第一 · 经藏护持",
            "capabilities": ["messages", "groups", "calls"],
            "presence": { "status": "online", "lastSeenAtMs": now, "statusText": null },
            "verified": true
        }),
        json!({
            "id": "human:contact:subhuti",
            "kind": "human",
            "displayName": "须菩提 (Subhuti)",
            "username": "subhuti_dharma",
            "avatarUrl": null,
            "bio": "解空第一 · 般若行者",
            "capabilities": ["messages", "groups", "calls"],
            "presence": { "status": "recently", "lastSeenAtMs": now - 180_000, "statusText": null },
            "verified": false
        }),
    ];

    for actor in actors {
        upsert_actor_row(conn, &actor, now)?;
    }

    let starter_dialogs = vec![
        (
            "conv_saved_messages",
            "savedMessages",
            "Saved Messages (收藏夹)",
            "跨端加密云端备忘录、经文摘录与常用文件收藏。",
            vec![(DEFAULT_ACTOR_ID, "owner")],
            true,
            vec!["folder_personal"],
            "msg_seed_saved_1",
            DEFAULT_ACTOR_ID,
            "欢迎使用 Saved Messages（收藏夹）！您在此处保存的笔记、语音波形、位置与文件均存储于本地 SQLite WAL 高性能加密仓中。",
            now - 50_000,
        ),
        (
            "conv_mika_bot",
            "direct",
            "@MikaPhoneBot (手机操控助手)",
            "端侧无障碍自动化助手：发送自然语言即可自动规划并操控手机 App。",
            vec![(DEFAULT_ACTOR_ID, "owner"), ("bot:mika", "member")],
            true,
            vec!["folder_bots"],
            "msg_seed_mika_1",
            "bot:mika",
            "你好！我是 Mika 手机操控助手 🤖📱。\n直接在这里发送任务（例如：「打开设置查看电池电量」或「帮我搜索最近的禅修中心」），我会实时汇报每一步屏幕观测与操控进度；敏感支付/密码界面会自动暂停请求您的确认。",
            now - 40_000,
        ),
        (
            "conv_global_dharma_channel",
            "channel",
            "全球法布施官方频道",
            "全球 249 个国家与地区法布施动态、禅堂共修与经典导读。",
            vec![(DEFAULT_ACTOR_ID, "owner"), ("bot:global-dharma", "admin")],
            true,
            vec!["folder_channels"],
            "msg_seed_dharma_1",
            "bot:global-dharma",
            "🙏 欢迎订阅【全球法布施官方频道】！\n今日共修：《般若波罗蜜多心经》与本地禅堂转经轮。愿以此功德，庄严佛净土，上报四重恩，下济三途苦。",
            now - 30_000,
        ),
        (
            "conv_hermes_ai",
            "direct",
            "@HermesAI",
            "Hermes AI Studio 随身智能体工作台（支持 /plan、/memory、/cron、/kanban）。",
            vec![(DEFAULT_ACTOR_ID, "owner"), ("bot:hermes", "member")],
            false,
            vec!["folder_bots"],
            "msg_seed_hermes_1",
            "bot:hermes",
            "欢迎来到 @HermesAI Studio！已挂载 29 个工具集与 61 条斜杠命令，输入 /help 或直接提问即可开始多模型协同推理。",
            now - 20_000,
        ),
        (
            "conv_botfather",
            "direct",
            "@BotFather",
            "官方 Bot 创建与 Mini App 绑定中心。",
            vec![(DEFAULT_ACTOR_ID, "owner"), ("bot:botfather", "member")],
            false,
            vec!["folder_bots"],
            "msg_seed_botfather_1",
            "bot:botfather",
            "I can help you create and manage Fabushi & Telegram bots.\n\nCommands:\n/newbot <name> - create a new bot\n/mybots - list your bots\n/help - view BotFather guide",
            now - 10_000,
        ),
    ];

    for (
        conv_id,
        kind,
        title,
        desc,
        members,
        pinned,
        folder_ids,
        msg_id,
        sender_id,
        msg_text,
        ts,
    ) in starter_dialogs
    {
        let participants: Vec<Value> = members
            .into_iter()
            .map(|(aid, role)| {
                json!({
                    "actorId": aid,
                    "role": role,
                    "joinedAtMs": ts,
                    "mutedUntilMs": null
                })
            })
            .collect();

        let msg = build_message_json(
            msg_id,
            conv_id,
            sender_id,
            &json!({
                "type": "text",
                "data": {
                    "text": { "text": msg_text, "entities": [] }
                }
            }),
            None,
            None,
            false,
            "read",
            ts,
        );
        insert_message_row(conn, &msg)?;

        let conv = json!({
            "id": conv_id,
            "kind": kind,
            "title": title,
            "description": desc,
            "avatarUrl": null,
            "participants": participants,
            "ownerId": DEFAULT_ACTOR_ID,
            "lastMessageId": msg_id,
            "lastReadMessageId": msg_id,
            "unreadCount": 0,
            "mentionCount": 0,
            "pinnedMessageIds": [],
            "notificationSettings": {
                "mutedUntilMs": null,
                "sound": null,
                "showPreview": true,
                "notifyMentions": true
            },
            "permissions": {
                "canSendMessages": true,
                "canSendMedia": true,
                "canSendPolls": true,
                "canAddMembers": true,
                "canPinMessages": true,
                "canManageTopics": true,
                "canManageCalls": true
            },
            "historyVisibility": "allMembers",
            "topics": [],
            "folderIds": folder_ids,
            "archived": false,
            "pinned": pinned,
            "markedUnread": false,
            "createdAtMs": ts,
            "updatedAtMs": ts
        });
        upsert_conversation_row(conn, &conv, ts)?;
    }

    let default_folders = vec![
        (
            0,
            json!({
                "id": "folder_personal",
                "title": "Personal",
                "icon": "👤",
                "conversationIds": ["conv_saved_messages"],
                "includeContacts": true,
                "includeBots": false,
                "includeGroups": false,
                "includeChannels": false,
                "excludeMuted": false,
                "excludeRead": false,
                "excludeArchived": true
            }),
        ),
        (
            1,
            json!({
                "id": "folder_bots",
                "title": "Bots",
                "icon": "🤖",
                "conversationIds": ["conv_mika_bot", "conv_hermes_ai", "conv_botfather"],
                "includeContacts": false,
                "includeBots": true,
                "includeGroups": false,
                "includeChannels": false,
                "excludeMuted": false,
                "excludeRead": false,
                "excludeArchived": true
            }),
        ),
        (
            2,
            json!({
                "id": "folder_groups",
                "title": "Groups",
                "icon": "👥",
                "conversationIds": [],
                "includeContacts": false,
                "includeBots": false,
                "includeGroups": true,
                "includeChannels": false,
                "excludeMuted": false,
                "excludeRead": false,
                "excludeArchived": true
            }),
        ),
        (
            3,
            json!({
                "id": "folder_channels",
                "title": "Channels",
                "icon": "📢",
                "conversationIds": ["conv_global_dharma_channel"],
                "includeContacts": false,
                "includeBots": false,
                "includeGroups": false,
                "includeChannels": true,
                "excludeMuted": false,
                "excludeRead": false,
                "excludeArchived": true
            }),
        ),
    ];

    for (order, folder) in default_folders {
        let fid = folder.get("id").and_then(Value::as_str).unwrap_or("");
        conn.execute(
            "INSERT OR REPLACE INTO msg_folders (folder_id, folder_json, sort_order) VALUES (?1, ?2, ?3)",
            params![fid, folder.to_string(), order],
        )
        .map_err(|e| e.to_string())?;
    }

    conn.execute(
        "INSERT OR REPLACE INTO msg_meta (key, value) VALUES ('seeded_starter_dialogs', '1')",
        [],
    )
    .map_err(|e| e.to_string())?;
    let _ = next_cursor(conn);

    Ok(())
}

pub fn issue_access(conn: &Connection, test_mode: bool, params: &Value) -> Result<Value, String> {
    init_schema(conn, test_mode)?;
    let now = chrono::Utc::now().timestamp_millis();
    let device_id = params
        .get("deviceId")
        .and_then(Value::as_str)
        .unwrap_or("android:native");
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or("account-session:android-native");
    let scopes = params
        .get("scopes")
        .cloned()
        .unwrap_or_else(|| json!(["messaging", "calls", "blobsRead", "blobsWrite", "payments", "miniApps"]));

    let token = format!(
        "fabushi_msg_tok_{}_{}",
        device_id.replace(':', "_"),
        uuid::Uuid::new_v4().simple()
    );

    Ok(json!({
        "actorId": DEFAULT_ACTOR_ID,
        "deviceId": device_id,
        "sessionId": session_id,
        "accessToken": token,
        "scopes": scopes,
        "issuedAtMs": now,
        "expiresAtMs": now + 86_400_000
    }))
}

pub fn execute_command(
    conn: &Connection,
    app_data_dir: &Path,
    test_mode: bool,
    params: &Value,
) -> Result<Value, String> {
    init_schema(conn, test_mode)?;
    let now = chrono::Utc::now().timestamp_millis();
    let request_id = params
        .get("requestId")
        .and_then(Value::as_str)
        .unwrap_or("req-messaging")
        .to_string();

    let envelope = params
        .get("envelope")
        .ok_or_else(|| "feature.messaging.execute requires 'envelope'".to_string())?;
    let context = envelope.get("context").cloned().unwrap_or_else(|| json!({}));
    let actor_id = context
        .get("actorId")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_ACTOR_ID);
    let command = envelope
        .get("command")
        .ok_or_else(|| "feature.messaging.execute envelope requires 'command'".to_string())?;
    let cmd_type = command
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| "messaging command missing 'type'".to_string())?;

    let events: Vec<Value> = match cmd_type {
        "sync" => {
            let limit = command
                .get("limit")
                .and_then(Value::as_i64)
                .unwrap_or(1000)
                .clamp(1, 5000);
            let conversations = list_conversations(conn)?;
            let actors = list_actors(conn)?;
            let folders = list_folders(conn)?;
            let drafts = list_drafts(conn)?;
            let messages = list_messages(conn, limit)?;
            let cursor = next_cursor(conn);
            vec![json!({
                "type": "syncBatch",
                "conversations": conversations,
                "actors": actors,
                "folders": folders,
                "drafts": drafts,
                "messages": messages,
                "cursor": cursor
            })]
        }

        "upsertProfile" => {
            let mut actor = command
                .get("actor")
                .cloned()
                .ok_or_else(|| "upsertProfile requires 'actor'".to_string())?;
            if actor.get("id").and_then(Value::as_str).unwrap_or("").is_empty() {
                if let Some(obj) = actor.as_object_mut() {
                    obj.insert("id".into(), json!(actor_id));
                }
            }
            upsert_actor_row(conn, &actor, now)?;
            vec![json!({
                "type": "actorChanged",
                "actor": actor
            })]
        }

        "createConversation" => {
            let mut conv = command
                .get("conversation")
                .cloned()
                .ok_or_else(|| "createConversation requires 'conversation'".to_string())?;
            if conv.get("id").and_then(Value::as_str).unwrap_or("").is_empty() {
                if let Some(obj) = conv.as_object_mut() {
                    obj.insert(
                        "id".into(),
                        json!(format!("conv_{}", uuid::Uuid::new_v4().simple())),
                    );
                }
            }
            normalize_conversation_defaults(&mut conv, actor_id, now);
            upsert_conversation_row(conn, &conv, now)?;
            vec![json!({
                "type": "conversationChanged",
                "conversation": conv
            })]
        }

        "sendMessage" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "sendMessage requires 'conversationId'".to_string())?;
            let content = command
                .get("content")
                .ok_or_else(|| "sendMessage requires 'content'".to_string())?;
            let reply_to = command
                .get("replyToMessageId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            let msg_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
            let msg = build_message_json(
                &msg_id,
                conv_id,
                actor_id,
                content,
                reply_to,
                None,
                false,
                "sent",
                now,
            );
            insert_message_row(conn, &msg)?;

            // Clear draft if present for this conversation
            let had_draft = conn
                .execute(
                    "DELETE FROM msg_drafts WHERE conversation_id = ?1",
                    params![conv_id],
                )
                .unwrap_or(0)
                > 0;

            let mut out_events = Vec::new();
            out_events.push(json!({
                "type": "messageAdded",
                "message": msg
            }));

            if had_draft {
                out_events.push(json!({
                    "type": "draftChanged",
                    "draft": {
                        "conversationId": conv_id,
                        "text": "",
                        "replyToMessageId": null,
                        "updatedAtMs": now
                    }
                }));
            }

            if let Some(mut conv) = load_conversation(conn, conv_id)? {
                if let Some(obj) = conv.as_object_mut() {
                    obj.insert("lastMessageId".into(), json!(msg_id));
                    obj.insert("updatedAtMs".into(), json!(now));
                }
                upsert_conversation_row(conn, &conv, now)?;
                out_events.push(json!({
                    "type": "conversationChanged",
                    "conversation": conv
                }));
            }

            // Interactive companion bot replies for starter bot dialogs
            if let Some(user_text) = extract_plain_text(content) {
                let extra = maybe_generate_bot_reply(conn, conv_id, actor_id, &user_text, now + 1)?;
                out_events.extend(extra);
            }

            out_events
        }

        "votePoll" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .unwrap_or("");
            let msg_id = command
                .get("messageId")
                .and_then(Value::as_str)
                .ok_or_else(|| "votePoll requires 'messageId'".to_string())?;
            let chosen_ids: Vec<String> = command
                .get("optionIds")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();

            let mut msg = load_message(conn, msg_id)?
                .ok_or_else(|| format!("Poll message '{msg_id}' not found"))?;
            if let Some(options) = msg
                .pointer_mut("/content/data/options")
                .and_then(Value::as_array_mut)
            {
                for opt in options.iter_mut() {
                    let oid = opt
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let was_chosen = opt.get("chosen").and_then(Value::as_bool).unwrap_or(false);
                    let prev_count = opt.get("voterCount").and_then(Value::as_i64).unwrap_or(0);
                    let now_chosen = chosen_ids.contains(&oid);
                    let next_count = match (was_chosen, now_chosen) {
                        (false, true) => prev_count + 1,
                        (true, false) => (prev_count - 1).max(0),
                        _ => prev_count,
                    };
                    if let Some(o) = opt.as_object_mut() {
                        o.insert("chosen".into(), json!(now_chosen));
                        o.insert("voterCount".into(), json!(next_count));
                    }
                }
            }
            update_message_row(conn, &msg)?;
            let _ = conv_id;
            vec![json!({
                "type": "messageChanged",
                "message": msg
            })]
        }

        "beginBlobUpload" => {
            let metadata = command
                .get("metadata")
                .cloned()
                .ok_or_else(|| "beginBlobUpload requires 'metadata'".to_string())?;
            let blob_id = metadata
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "beginBlobUpload metadata requires 'id'".to_string())?;
            validate_blob_id(blob_id)?;
            let size_bytes = metadata
                .get("sizeBytes")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0);
            let path = blob_file_path(app_data_dir, blob_id)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Failed creating blob dir: {e}"))?;
            }
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .map_err(|e| format!("Failed creating blob file: {e}"))?;

            conn.execute(
                "INSERT OR REPLACE INTO msg_blobs (blob_id, metadata_json, size_bytes, content_hash, finished, created_at_ms)
                 VALUES (?1, ?2, ?3, NULL, 0, ?4)",
                params![blob_id, metadata.to_string(), size_bytes, now],
            )
            .map_err(|e| e.to_string())?;

            vec![json!({
                "type": "blobUploadStarted",
                "blobId": blob_id,
                "metadata": metadata
            })]
        }

        "appendBlobChunk" => {
            let blob_id = command
                .get("blobId")
                .and_then(Value::as_str)
                .ok_or_else(|| "appendBlobChunk requires 'blobId'".to_string())?;
            validate_blob_id(blob_id)?;
            let offset = command
                .get("offset")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let data_b64 = command
                .get("dataBase64")
                .and_then(Value::as_str)
                .ok_or_else(|| "appendBlobChunk requires 'dataBase64'".to_string())?;
            let bytes = decode_base64_lenient(data_b64)?;

            let path = blob_file_path(app_data_dir, blob_id)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Failed creating blob dir: {e}"))?;
            }
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .open(&path)
                .map_err(|e| format!("Failed opening blob file: {e}"))?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| format!("Blob seek failed: {e}"))?;
            file.write_all(&bytes)
                .map_err(|e| format!("Blob write failed: {e}"))?;
            file.flush().map_err(|e| format!("Blob flush failed: {e}"))?;

            vec![json!({
                "type": "blobChunkAppended",
                "blobId": blob_id,
                "offset": offset,
                "writtenBytes": bytes.len()
            })]
        }

        "finishBlobUpload" => {
            let blob_id = command
                .get("blobId")
                .and_then(Value::as_str)
                .ok_or_else(|| "finishBlobUpload requires 'blobId'".to_string())?;
            validate_blob_id(blob_id)?;
            let path = blob_file_path(app_data_dir, blob_id)?;
            let data = fs::read(&path).map_err(|e| format!("Blob '{blob_id}' missing on finish: {e}"))?;
            let hash_hex: String = Sha256::digest(&data)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();

            let raw_meta: String = conn
                .query_row(
                    "SELECT metadata_json FROM msg_blobs WHERE blob_id = ?1",
                    params![blob_id],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| json!({ "id": blob_id }).to_string());
            let mut meta: Value = serde_json::from_str(&raw_meta).unwrap_or_else(|_| json!({ "id": blob_id }));
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("sizeBytes".into(), json!(data.len()));
                obj.insert("contentHash".into(), json!(hash_hex));
            }

            conn.execute(
                "UPDATE msg_blobs SET metadata_json = ?1, size_bytes = ?2, content_hash = ?3, finished = 1
                 WHERE blob_id = ?4",
                params![meta.to_string(), data.len() as i64, hash_hex, blob_id],
            )
            .map_err(|e| e.to_string())?;

            vec![json!({
                "type": "blobUploadFinished",
                "blobId": blob_id,
                "metadata": meta
            })]
        }

        "pinMessage" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "pinMessage requires 'conversationId'".to_string())?;
            let msg_id = command
                .get("messageId")
                .and_then(Value::as_str)
                .ok_or_else(|| "pinMessage requires 'messageId'".to_string())?;
            let pinned = command.get("pinned").and_then(Value::as_bool).unwrap_or(true);

            let mut events = Vec::new();
            if let Some(mut msg) = load_message(conn, msg_id)? {
                if let Some(obj) = msg.as_object_mut() {
                    obj.insert("pinned".into(), json!(pinned));
                }
                update_message_row(conn, &msg)?;
                events.push(json!({
                    "type": "messageChanged",
                    "message": msg
                }));
            }
            if let Some(mut conv) = load_conversation(conn, conv_id)? {
                let mut ids: Vec<String> = conv
                    .get("pinnedMessageIds")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                    .unwrap_or_default();
                if pinned {
                    if !ids.iter().any(|x| x == msg_id) {
                        ids.push(msg_id.to_string());
                    }
                } else {
                    ids.retain(|x| x != msg_id);
                }
                if let Some(obj) = conv.as_object_mut() {
                    obj.insert("pinnedMessageIds".into(), json!(ids));
                    obj.insert("updatedAtMs".into(), json!(now));
                }
                upsert_conversation_row(conn, &conv, now)?;
                events.push(json!({
                    "type": "conversationChanged",
                    "conversation": conv
                }));
            }
            events
        }

        "upsertFolder" => {
            let folder = command
                .get("folder")
                .cloned()
                .ok_or_else(|| "upsertFolder requires 'folder'".to_string())?;
            let fid = folder
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "folder requires 'id'".to_string())?;
            let order: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM msg_folders",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            conn.execute(
                "INSERT INTO msg_folders (folder_id, folder_json, sort_order) VALUES (?1, ?2, ?3)
                 ON CONFLICT(folder_id) DO UPDATE SET folder_json = excluded.folder_json",
                params![fid, folder.to_string(), order],
            )
            .map_err(|e| e.to_string())?;
            vec![json!({
                "type": "folderChanged",
                "folder": folder
            })]
        }

        "deleteFolder" => {
            let fid = command
                .get("folderId")
                .and_then(Value::as_str)
                .ok_or_else(|| "deleteFolder requires 'folderId'".to_string())?;
            conn.execute("DELETE FROM msg_folders WHERE folder_id = ?1", params![fid])
                .map_err(|e| e.to_string())?;
            vec![json!({
                "type": "folderDeleted",
                "folderId": fid
            })]
        }

        "updateConversationInfo" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "updateConversationInfo requires 'conversationId'".to_string())?;
            let mut conv = load_conversation(conn, conv_id)?
                .ok_or_else(|| format!("Conversation '{conv_id}' not found"))?;
            if let Some(obj) = conv.as_object_mut() {
                if let Some(title) = command.get("title").and_then(Value::as_str) {
                    if !title.trim().is_empty() {
                        obj.insert("title".into(), json!(title.trim()));
                    }
                }
                if let Some(desc) = command.get("description") {
                    obj.insert("description".into(), desc.clone());
                }
                obj.insert("updatedAtMs".into(), json!(now));
            }
            upsert_conversation_row(conn, &conv, now)?;
            vec![json!({
                "type": "conversationChanged",
                "conversation": conv
            })]
        }

        "setConversationParticipant" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "setConversationParticipant requires 'conversationId'".to_string())?;
            let participant = command
                .get("participant")
                .cloned()
                .ok_or_else(|| "setConversationParticipant requires 'participant'".to_string())?;
            let target_actor = participant
                .get("actorId")
                .and_then(Value::as_str)
                .ok_or_else(|| "participant requires 'actorId'".to_string())?;

            let mut conv = load_conversation(conn, conv_id)?
                .ok_or_else(|| format!("Conversation '{conv_id}' not found"))?;
            let mut list = conv
                .get("participants")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if let Some(existing) = list.iter_mut().find(|p| {
                p.get("actorId").and_then(Value::as_str) == Some(target_actor)
            }) {
                *existing = participant;
            } else {
                list.push(participant);
            }
            if let Some(obj) = conv.as_object_mut() {
                obj.insert("participants".into(), json!(list));
                obj.insert("updatedAtMs".into(), json!(now));
            }
            upsert_conversation_row(conn, &conv, now)?;
            vec![json!({
                "type": "conversationParticipantChanged",
                "conversation": conv,
                "removedActorId": null
            })]
        }

        "removeConversationParticipant" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "removeConversationParticipant requires 'conversationId'".to_string())?;
            let target_actor = command
                .get("actorId")
                .and_then(Value::as_str)
                .ok_or_else(|| "removeConversationParticipant requires 'actorId'".to_string())?;
            let mut conv = load_conversation(conn, conv_id)?
                .ok_or_else(|| format!("Conversation '{conv_id}' not found"))?;
            let mut list = conv
                .get("participants")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            list.retain(|p| p.get("actorId").and_then(Value::as_str) != Some(target_actor));
            if let Some(obj) = conv.as_object_mut() {
                obj.insert("participants".into(), json!(list));
                obj.insert("updatedAtMs".into(), json!(now));
            }
            upsert_conversation_row(conn, &conv, now)?;
            vec![json!({
                "type": "conversationParticipantChanged",
                "conversation": conv,
                "removedActorId": target_actor
            })]
        }

        "setMarkedUnread" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "setMarkedUnread requires 'conversationId'".to_string())?;
            let marked = command
                .get("markedUnread")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if let Some(mut conv) = load_conversation(conn, conv_id)? {
                if let Some(obj) = conv.as_object_mut() {
                    obj.insert("markedUnread".into(), json!(marked));
                }
                upsert_conversation_row(conn, &conv, now)?;
            }
            vec![json!({
                "type": "markedUnreadChanged",
                "conversationId": conv_id,
                "markedUnread": marked
            })]
        }

        "setDraft" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "setDraft requires 'conversationId'".to_string())?;
            let text = command.get("text").and_then(Value::as_str).unwrap_or("");
            let reply_to = command
                .get("replyToMessageId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            let draft = json!({
                "conversationId": conv_id,
                "text": text,
                "replyToMessageId": reply_to,
                "updatedAtMs": now
            });
            if text.trim().is_empty() && reply_to.is_none() {
                conn.execute(
                    "DELETE FROM msg_drafts WHERE conversation_id = ?1",
                    params![conv_id],
                )
                .map_err(|e| e.to_string())?;
            } else {
                conn.execute(
                    "INSERT OR REPLACE INTO msg_drafts (conversation_id, draft_json, updated_at_ms)
                     VALUES (?1, ?2, ?3)",
                    params![conv_id, draft.to_string(), now],
                )
                .map_err(|e| e.to_string())?;
            }
            vec![json!({
                "type": "draftChanged",
                "draft": draft
            })]
        }

        "pinConversation" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "pinConversation requires 'conversationId'".to_string())?;
            let pinned = command.get("pinned").and_then(Value::as_bool).unwrap_or(true);
            let mut conv = load_conversation(conn, conv_id)?
                .ok_or_else(|| format!("Conversation '{conv_id}' not found"))?;
            if let Some(obj) = conv.as_object_mut() {
                obj.insert("pinned".into(), json!(pinned));
                obj.insert("updatedAtMs".into(), json!(now));
            }
            upsert_conversation_row(conn, &conv, now)?;
            vec![json!({
                "type": "conversationChanged",
                "conversation": conv
            })]
        }

        "archiveConversation" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "archiveConversation requires 'conversationId'".to_string())?;
            let archived = command
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let mut conv = load_conversation(conn, conv_id)?
                .ok_or_else(|| format!("Conversation '{conv_id}' not found"))?;
            if let Some(obj) = conv.as_object_mut() {
                obj.insert("archived".into(), json!(archived));
                obj.insert("updatedAtMs".into(), json!(now));
            }
            upsert_conversation_row(conn, &conv, now)?;
            vec![json!({
                "type": "conversationChanged",
                "conversation": conv
            })]
        }

        "setConversationNotifications" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "setConversationNotifications requires 'conversationId'".to_string())?;
            let settings = command
                .get("settings")
                .cloned()
                .unwrap_or_else(|| json!({ "mutedUntilMs": null, "showPreview": true, "notifyMentions": true }));
            let mut conv = load_conversation(conn, conv_id)?
                .ok_or_else(|| format!("Conversation '{conv_id}' not found"))?;
            if let Some(obj) = conv.as_object_mut() {
                obj.insert("notificationSettings".into(), settings);
                obj.insert("updatedAtMs".into(), json!(now));
            }
            upsert_conversation_row(conn, &conv, now)?;
            vec![json!({
                "type": "conversationChanged",
                "conversation": conv
            })]
        }

        "markRead" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "markRead requires 'conversationId'".to_string())?;
            let msg_id = command.get("messageId").and_then(Value::as_str);
            let mut events = Vec::new();
            if let Some(mut conv) = load_conversation(conn, conv_id)? {
                if let Some(obj) = conv.as_object_mut() {
                    obj.insert("unreadCount".into(), json!(0));
                    obj.insert("markedUnread".into(), json!(false));
                    if let Some(mid) = msg_id {
                        obj.insert("lastReadMessageId".into(), json!(mid));
                    }
                }
                upsert_conversation_row(conn, &conv, now)?;
                events.push(json!({
                    "type": "conversationChanged",
                    "conversation": conv
                }));
            }
            if let Some(mid) = msg_id {
                if let Some(mut msg) = load_message(conn, mid)? {
                    if let Some(obj) = msg.as_object_mut() {
                        obj.insert("deliveryState".into(), json!("read"));
                    }
                    update_message_row(conn, &msg)?;
                    events.push(json!({
                        "type": "messageChanged",
                        "message": msg
                    }));
                }
            }
            events
        }

        "editMessage" => {
            let msg_id = command
                .get("messageId")
                .and_then(Value::as_str)
                .ok_or_else(|| "editMessage requires 'messageId'".to_string())?;
            let new_content = command
                .get("content")
                .cloned()
                .ok_or_else(|| "editMessage requires 'content'".to_string())?;
            let mut msg = load_message(conn, msg_id)?
                .ok_or_else(|| format!("Message '{msg_id}' not found"))?;
            if let Some(obj) = msg.as_object_mut() {
                obj.insert("content".into(), new_content);
                obj.insert("editedAtMs".into(), json!(now));
            }
            update_message_row(conn, &msg)?;
            vec![json!({
                "type": "messageChanged",
                "message": msg
            })]
        }

        "deleteMessages" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "deleteMessages requires 'conversationId'".to_string())?;
            let ids: Vec<String> = command
                .get("messageIds")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                .unwrap_or_default();
            for mid in &ids {
                conn.execute("DELETE FROM msg_messages WHERE message_id = ?1", params![mid])
                    .map_err(|e| e.to_string())?;
                let _ = conn.execute(
                    "DELETE FROM msg_messages_fts WHERE message_id = ?1",
                    params![mid],
                );
            }
            vec![json!({
                "type": "messagesDeleted",
                "conversationId": conv_id,
                "messageIds": ids
            })]
        }

        "setReaction" => {
            let msg_id = command
                .get("messageId")
                .and_then(Value::as_str)
                .ok_or_else(|| "setReaction requires 'messageId'".to_string())?;
            let mut msg = load_message(conn, msg_id)?
                .ok_or_else(|| format!("Message '{msg_id}' not found"))?;

            let (emoji, chosen) = if let Some(r_obj) = command.get("reaction").and_then(Value::as_object) {
                let em = r_obj
                    .get("reaction")
                    .and_then(Value::as_str)
                    .unwrap_or("👍")
                    .to_string();
                let ch = r_obj
                    .get("chosenByMe")
                    .and_then(Value::as_bool)
                    .unwrap_or_else(|| r_obj.get("count").and_then(Value::as_i64).unwrap_or(1) > 0);
                (em, ch)
            } else {
                let em = command
                    .get("reaction")
                    .and_then(Value::as_str)
                    .unwrap_or("👍")
                    .to_string();
                let ch = command.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                (em, ch)
            };

            let mut reactions = msg
                .get("reactions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            if let Some(pos) = reactions
                .iter()
                .position(|r| r.get("reaction").and_then(Value::as_str) == Some(emoji.as_str()))
            {
                let prev_count = reactions[pos]
                    .get("count")
                    .and_then(Value::as_i64)
                    .unwrap_or(1);
                let prev_chosen = reactions[pos]
                    .get("chosenByMe")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if chosen {
                    let next_count = if prev_chosen { prev_count } else { prev_count + 1 };
                    reactions[pos] = json!({
                        "reaction": emoji,
                        "count": next_count.max(1),
                        "chosenByMe": true,
                        "recentActorIds": [actor_id]
                    });
                } else {
                    let next_count = if prev_chosen { prev_count - 1 } else { prev_count };
                    if next_count <= 0 {
                        reactions.remove(pos);
                    } else {
                        reactions[pos] = json!({
                            "reaction": emoji,
                            "count": next_count,
                            "chosenByMe": false,
                            "recentActorIds": []
                        });
                    }
                }
            } else if chosen {
                reactions.push(json!({
                    "reaction": emoji,
                    "count": 1,
                    "chosenByMe": true,
                    "recentActorIds": [actor_id]
                }));
            }

            if let Some(obj) = msg.as_object_mut() {
                obj.insert("reactions".into(), json!(reactions));
            }
            update_message_row(conn, &msg)?;
            vec![json!({
                "type": "messageChanged",
                "message": msg
            })]
        }

        "forwardMessage" => {
            let src_conv_id = command
                .get("sourceConversationId")
                .and_then(Value::as_str)
                .unwrap_or("");
            let msg_id = command
                .get("messageId")
                .and_then(Value::as_str)
                .ok_or_else(|| "forwardMessage requires 'messageId'".to_string())?;
            let dst_conv_id = command
                .get("destinationConversationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "forwardMessage requires 'destinationConversationId'".to_string())?;

            let orig = load_message(conn, msg_id)?
                .ok_or_else(|| format!("Source message '{msg_id}' not found"))?;
            let content = orig.get("content").cloned().unwrap_or_else(|| json!({}));
            let src_title = load_conversation(conn, src_conv_id)?
                .and_then(|c| c.get("title").and_then(Value::as_str).map(String::from))
                .unwrap_or_else(|| src_conv_id.to_string());
            let fwd_origin = format!("Forwarded from {src_title}");
            let new_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
            let forwarded = build_message_json(
                &new_id,
                dst_conv_id,
                actor_id,
                &content,
                None,
                Some(&fwd_origin),
                false,
                "sent",
                now,
            );
            insert_message_row(conn, &forwarded)?;

            let mut out = vec![json!({
                "type": "messageAdded",
                "message": forwarded
            })];
            if let Some(mut dst_conv) = load_conversation(conn, dst_conv_id)? {
                if let Some(obj) = dst_conv.as_object_mut() {
                    obj.insert("lastMessageId".into(), json!(new_id));
                    obj.insert("updatedAtMs".into(), json!(now));
                }
                upsert_conversation_row(conn, &dst_conv, now)?;
                out.push(json!({
                    "type": "conversationChanged",
                    "conversation": dst_conv
                }));
            }
            out
        }

        "startTyping" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .unwrap_or("");
            let action = command
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("typing");
            vec![json!({
                "type": "typingChanged",
                "conversationId": conv_id,
                "actorId": actor_id,
                "action": action
            })]
        }

        "stopTyping" => {
            let conv_id = command
                .get("conversationId")
                .and_then(Value::as_str)
                .unwrap_or("");
            vec![json!({
                "type": "typingChanged",
                "conversationId": conv_id,
                "actorId": actor_id,
                "action": null
            })]
        }

        other => return Err(format!("Unsupported messaging protocol v2 command: {other}")),
    };

    let envelopes: Vec<Value> = events
        .into_iter()
        .map(|ev| {
            json!({
                "protocolVersion": 2,
                "event": ev
            })
        })
        .collect();

    Ok(json!({
        "requestId": request_id,
        "envelopes": envelopes
    }))
}

pub fn read_blob(conn: &Connection, app_data_dir: &Path, params: &Value) -> Result<Value, String> {
    let blob_id = params
        .get("blobId")
        .and_then(Value::as_str)
        .ok_or_else(|| "feature.messaging.blob.read requires 'blobId'".to_string())?;
    validate_blob_id(blob_id)?;
    let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let length = params
        .get("length")
        .and_then(Value::as_u64)
        .unwrap_or(1024 * 1024)
        .min(16 * 1024 * 1024) as usize;

    let path = blob_file_path(app_data_dir, blob_id)?;
    let mut file = OpenOptions::new()
        .read(true)
        .open(&path)
        .map_err(|e| format!("Blob '{blob_id}' not found: {e}"))?;
    let total_len = file
        .metadata()
        .map(|m| m.len())
        .map_err(|e| format!("Blob metadata error: {e}"))?;

    let slice = if offset >= total_len || length == 0 {
        Vec::new()
    } else {
        let to_read = ((total_len - offset) as usize).min(length);
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Blob seek error: {e}"))?;
        let mut buf = vec![0u8; to_read];
        file.read_exact(&mut buf)
            .map_err(|e| format!("Blob read error: {e}"))?;
        buf
    };

    let raw_meta: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM msg_blobs WHERE blob_id = ?1",
            params![blob_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let metadata: Value = raw_meta
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({ "id": blob_id, "sizeBytes": total_len }));

    let encoded = base64::engine::general_purpose::STANDARD.encode(&slice);
    Ok(json!({
        "metadata": metadata,
        "offset": offset,
        "length": slice.len(),
        "totalBytes": total_len,
        "dataBase64": encoded
    }))
}

/// Global & in-chat FTS5 search with seamless CJK substring `LIKE` fallback.
pub fn search_messages(conn: &Connection, params: &Value) -> Result<Value, String> {
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let conv_filter = params
        .get("conversationId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let limit = params
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(50)
        .clamp(1, 500);

    if query.is_empty() {
        return Ok(json!({ "query": "", "matches": [] }));
    }

    let mut matched_ids: Vec<String> = Vec::new();

    // 1. FTS5 fast token search (sanitize quotes for FTS5 phrase matching)
    let fts_phrase = format!("\"{}\"", query.replace('"', "\"\""));
    if let Ok(mut stmt) = conn.prepare(
        "SELECT message_id FROM msg_messages_fts
         WHERE msg_messages_fts MATCH ?1
           AND (?2 IS NULL OR conversation_id = ?2)
         LIMIT ?3",
    ) {
        if let Ok(rows) = stmt.query_map(params![fts_phrase, conv_filter, limit], |r| {
            r.get::<_, String>(0)
        }) {
            for id in rows.flatten() {
                matched_ids.push(id);
            }
        }
    }

    // 2. CJK / substring fallback via LIKE so Chinese/emoji substrings match 100% deterministically
    if (matched_ids.len() as i64) < limit {
        let like_pat = format!("%{query}%");
        let mut stmt = conn
            .prepare(
                "SELECT message_id FROM msg_messages
                 WHERE search_text LIKE ?1
                   AND (?2 IS NULL OR conversation_id = ?2)
                 ORDER BY created_at_ms DESC
                 LIMIT ?3",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![like_pat, conv_filter, limit], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?;
        for id in rows.flatten() {
            if !matched_ids.contains(&id) {
                matched_ids.push(id);
                if (matched_ids.len() as i64) >= limit {
                    break;
                }
            }
        }
    }

    let mut messages = Vec::with_capacity(matched_ids.len());
    for id in &matched_ids {
        if let Some(msg) = load_message(conn, id)? {
            messages.push(msg);
        }
    }

    Ok(json!({
        "query": query,
        "conversationId": conv_filter,
        "total": messages.len(),
        "matches": messages
    }))
}

/// Exports conversation history as structured JSON or standalone HTML archive.
pub fn export_chat_history(conn: &Connection, params: &Value) -> Result<Value, String> {
    let conv_id = params
        .get("conversationId")
        .and_then(Value::as_str)
        .ok_or_else(|| "exportChat requires 'conversationId'".to_string())?;
    let format = params
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("json")
        .to_ascii_lowercase();

    let conv = load_conversation(conn, conv_id)?
        .unwrap_or_else(|| json!({ "id": conv_id, "title": conv_id }));
    let title = conv
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(conv_id);

    let mut stmt = conn
        .prepare(
            "SELECT message_json FROM msg_messages
             WHERE conversation_id = ?1
             ORDER BY created_at_ms ASC",
        )
        .map_err(|e| e.to_string())?;
    let messages: Vec<Value> = stmt
        .query_map(params![conv_id], |r| {
            let s: String = r.get(0)?;
            Ok(serde_json::from_str(&s).unwrap_or(json!({})))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    if format == "html" {
        let mut html = String::new();
        html.push_str("<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">");
        html.push_str(&format!(
            "<title>Chat Export - {}</title></head><body><h1>{}</h1><div class=\"messages\">",
            escape_html(title),
            escape_html(title)
        ));
        for m in &messages {
            let sender = m.get("senderId").and_then(Value::as_str).unwrap_or("unknown");
            let text = m
                .get("content")
                .and_then(extract_plain_text)
                .unwrap_or_else(|| "[media]".to_string());
            let ts = m.get("createdAtMs").and_then(Value::as_i64).unwrap_or(0);
            html.push_str(&format!(
                "<article data-ts=\"{ts}\"><strong>{}</strong>: <p>{}</p></article>",
                escape_html(sender),
                escape_html(&text)
            ));
        }
        html.push_str("</div></body></html>");
        Ok(json!({
            "conversationId": conv_id,
            "format": "html",
            "messageCount": messages.len(),
            "document": html
        }))
    } else {
        Ok(json!({
            "conversationId": conv_id,
            "format": "json",
            "messageCount": messages.len(),
            "conversation": conv,
            "messages": messages
        }))
    }
}

fn maybe_generate_bot_reply(
    conn: &Connection,
    conv_id: &str,
    actor_id: &str,
    user_text: &str,
    now: i64,
) -> Result<Vec<Value>, String> {
    let trimmed = user_text.trim();
    let mut events = Vec::new();

    if conv_id == "conv_botfather" {
        let reply_text = if let Some(rest) = trimmed.strip_prefix("/newbot") {
            let bot_name = rest.trim();
            if bot_name.is_empty() {
                "Alright, a new bot. Please send `/newbot <BotName>` so I can provision its token and dialog.".to_string()
            } else {
                let slug: String = bot_name
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
                    .collect();
                let clean_slug = slug.trim_matches('_');
                let username = if clean_slug.is_empty() {
                    format!("fabushi_{}_bot", &uuid::Uuid::new_v4().simple().to_string()[..6])
                } else if clean_slug.ends_with("bot") {
                    clean_slug.to_string()
                } else {
                    format!("{clean_slug}_bot")
                };
                let bot_actor_id = format!("bot:{username}");
                let new_conv_id = format!("conv_bot_{username}");
                let token = format!(
                    "{}:AAH{}",
                    now % 1_000_000_000,
                    &uuid::Uuid::new_v4().simple().to_string()[..24]
                );

                let bot_actor = json!({
                    "id": bot_actor_id,
                    "kind": "bot",
                    "displayName": bot_name,
                    "username": username,
                    "avatarUrl": null,
                    "bio": format!("Created via @BotFather ({bot_name})"),
                    "capabilities": ["messages", "miniApps"],
                    "presence": { "status": "online", "lastSeenAtMs": now, "statusText": "bot" },
                    "verified": false
                });
                upsert_actor_row(conn, &bot_actor, now)?;
                events.push(json!({ "type": "actorChanged", "actor": bot_actor }));

                let bot_conv = json!({
                    "id": new_conv_id,
                    "kind": "direct",
                    "title": bot_name,
                    "description": format!("@{username}"),
                    "avatarUrl": null,
                    "participants": [
                        { "actorId": actor_id, "role": "owner", "joinedAtMs": now, "mutedUntilMs": null },
                        { "actorId": bot_actor_id, "role": "member", "joinedAtMs": now, "mutedUntilMs": null }
                    ],
                    "ownerId": actor_id,
                    "lastMessageId": null,
                    "lastReadMessageId": null,
                    "unreadCount": 0,
                    "mentionCount": 0,
                    "pinnedMessageIds": [],
                    "notificationSettings": { "mutedUntilMs": null, "sound": null, "showPreview": true, "notifyMentions": true },
                    "permissions": { "canSendMessages": true, "canSendMedia": true, "canSendPolls": true, "canAddMembers": false, "canPinMessages": true, "canManageTopics": false, "canManageCalls": false },
                    "historyVisibility": "allMembers",
                    "topics": [],
                    "folderIds": ["folder_bots"],
                    "archived": false,
                    "pinned": false,
                    "markedUnread": false,
                    "createdAtMs": now,
                    "updatedAtMs": now
                });
                upsert_conversation_row(conn, &bot_conv, now)?;
                events.push(json!({ "type": "conversationChanged", "conversation": bot_conv }));

                format!(
                    "Done! Congratulations on your new bot @{username}.\n\nUse this token to access the HTTP Bot API:\n`{token}`"
                )
            }
        } else {
            "BotFather ready. Send `/newbot <Name>` to create a new bot or `/mybots` to inspect bots.".to_string()
        };

        events.extend(append_bot_message(conn, conv_id, "bot:botfather", &reply_text, now)?);
    } else if conv_id == "conv_mika_bot" {
        let _ = crate::computer::handle_mika_rpc(
            conn,
            "registerTaskEvent",
            &json!({
                "taskId": format!("mika_chat_{now}"),
                "stepIndex": 1,
                "phase": "queued_from_telegram_chat",
                "summary": trimmed
            }),
        );
        let reply_text = format!(
            "🤖 Mika 已接收手机操控指令：「{trimmed}」\n• 执行通道：端侧无障碍引擎 (:mika-agent)\n• 安全护栏：支付/密码界面自动暂停确认"
        );
        events.extend(append_bot_message(conn, conv_id, "bot:mika", &reply_text, now)?);
    } else if conv_id == "conv_hermes_ai" {
        let reply_text = if let Some(parsed) = crate::hermes::parse_slash_command(trimmed) {
            if let Some(def) = crate::hermes::lookup_slash_command(&parsed.command) {
                format!("⚡ Hermes `/{}` ({}) — {}", def.name, def.category, def.description)
            } else {
                format!("Hermes Studio 已解析指令 `/{}`：{}", parsed.command, parsed.args)
            }
        } else {
            format!("✨ Hermes Studio 回复：已针对「{trimmed}」完成上下文检索与推理。")
        };
        events.extend(append_bot_message(conn, conv_id, "bot:hermes", &reply_text, now)?);
    }

    Ok(events)
}

fn append_bot_message(
    conn: &Connection,
    conv_id: &str,
    bot_sender_id: &str,
    text: &str,
    now: i64,
) -> Result<Vec<Value>, String> {
    let msg_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    let msg = build_message_json(
        &msg_id,
        conv_id,
        bot_sender_id,
        &json!({
            "type": "text",
            "data": { "text": { "text": text, "entities": [] } }
        }),
        None,
        None,
        false,
        "sent",
        now,
    );
    insert_message_row(conn, &msg)?;
    let mut out = vec![json!({
        "type": "messageAdded",
        "message": msg
    })];
    if let Some(mut conv) = load_conversation(conn, conv_id)? {
        if let Some(obj) = conv.as_object_mut() {
            obj.insert("lastMessageId".into(), json!(msg_id));
            obj.insert("updatedAtMs".into(), json!(now));
        }
        upsert_conversation_row(conn, &conv, now)?;
        out.push(json!({
            "type": "conversationChanged",
            "conversation": conv
        }));
    }
    Ok(out)
}

fn build_message_json(
    id: &str,
    conversation_id: &str,
    sender_id: &str,
    content: &Value,
    reply_to_message_id: Option<&str>,
    forward_origin: Option<&str>,
    pinned: bool,
    delivery_state: &str,
    created_at_ms: i64,
) -> Value {
    json!({
        "id": id,
        "conversationId": conversation_id,
        "senderId": sender_id,
        "content": content,
        "reactions": [],
        "deliveryState": delivery_state,
        "createdAtMs": created_at_ms,
        "editedAtMs": Value::Null,
        "replyToMessageId": reply_to_message_id,
        "forwardOrigin": forward_origin,
        "pinned": pinned
    })
}

fn extract_plain_text(content: &Value) -> Option<String> {
    let ctype = content.get("type").and_then(Value::as_str).unwrap_or("");
    let data = content.get("data").unwrap_or(&Value::Null);
    match ctype {
        "text" => data
            .get("text")
            .and_then(|t| t.get("text").and_then(Value::as_str).or_else(|| t.as_str()))
            .map(String::from),
        "poll" => data
            .get("question")
            .and_then(|q| q.get("text").and_then(Value::as_str))
            .map(String::from),
        "contact" => data
            .get("displayName")
            .and_then(Value::as_str)
            .map(String::from),
        "photo" | "video" | "document" | "voice" | "audio" => {
            let file_name = data
                .get("media")
                .and_then(|m| m.get("fileName"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let caption = data
                .get("caption")
                .and_then(|c| c.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let combined = format!("{file_name} {caption}").trim().to_string();
            if combined.is_empty() {
                None
            } else {
                Some(combined)
            }
        }
        _ => None,
    }
}

fn normalize_conversation_defaults(conv: &mut Value, owner_id: &str, now: i64) {
    if let Some(obj) = conv.as_object_mut() {
        obj.entry("kind").or_insert_with(|| json!("direct"));
        obj.entry("title").or_insert_with(|| json!("对话"));
        obj.entry("description").or_insert(Value::Null);
        obj.entry("ownerId").or_insert_with(|| json!(owner_id));
        obj.entry("participants").or_insert_with(|| json!([]));
        obj.entry("pinnedMessageIds").or_insert_with(|| json!([]));
        obj.entry("folderIds").or_insert_with(|| json!([]));
        obj.entry("unreadCount").or_insert_with(|| json!(0));
        obj.entry("pinned").or_insert_with(|| json!(false));
        obj.entry("archived").or_insert_with(|| json!(false));
        obj.entry("markedUnread").or_insert_with(|| json!(false));
        obj.entry("notificationSettings").or_insert_with(|| {
            json!({
                "mutedUntilMs": null,
                "sound": null,
                "showPreview": true,
                "notifyMentions": true
            })
        });
        obj.entry("createdAtMs").or_insert_with(|| json!(now));
        obj.insert("updatedAtMs".into(), json!(now));
    }
}

fn upsert_actor_row(conn: &Connection, actor: &Value, now: i64) -> Result<(), String> {
    let id = actor
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "actor missing 'id'".to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO msg_actors (actor_id, actor_json, updated_at_ms) VALUES (?1, ?2, ?3)",
        params![id, actor.to_string(), now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn upsert_conversation_row(conn: &Connection, conv: &Value, now: i64) -> Result<(), String> {
    let id = conv
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "conversation missing 'id'".to_string())?;
    let kind = conv.get("kind").and_then(Value::as_str).unwrap_or("direct");
    let title = conv.get("title").and_then(Value::as_str).unwrap_or(id);
    conn.execute(
        "INSERT OR REPLACE INTO msg_conversations (conversation_id, kind, title, conversation_json, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, kind, title, conv.to_string(), now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn insert_message_row(conn: &Connection, msg: &Value) -> Result<(), String> {
    let id = msg
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "message missing 'id'".to_string())?;
    let conv_id = msg
        .get("conversationId")
        .and_then(Value::as_str)
        .ok_or_else(|| "message missing 'conversationId'".to_string())?;
    let sender_id = msg
        .get("senderId")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_ACTOR_ID);
    let created_at = msg
        .get("createdAtMs")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let search_text = msg
        .get("content")
        .and_then(extract_plain_text)
        .unwrap_or_default();

    conn.execute(
        "INSERT OR REPLACE INTO msg_messages
         (message_id, conversation_id, sender_id, search_text, message_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, conv_id, sender_id, search_text, msg.to_string(), created_at],
    )
    .map_err(|e| e.to_string())?;

    let _ = conn.execute(
        "DELETE FROM msg_messages_fts WHERE message_id = ?1",
        params![id],
    );
    let _ = conn.execute(
        "INSERT INTO msg_messages_fts (message_id, conversation_id, search_text) VALUES (?1, ?2, ?3)",
        params![id, conv_id, search_text],
    );
    Ok(())
}

fn update_message_row(conn: &Connection, msg: &Value) -> Result<(), String> {
    insert_message_row(conn, msg)
}

fn load_conversation(conn: &Connection, conv_id: &str) -> Result<Option<Value>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT conversation_json FROM msg_conversations WHERE conversation_id = ?1",
            params![conv_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

fn load_message(conn: &Connection, msg_id: &str) -> Result<Option<Value>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT message_json FROM msg_messages WHERE message_id = ?1",
            params![msg_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

fn list_conversations(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare("SELECT conversation_json FROM msg_conversations ORDER BY updated_at_ms DESC")
        .map_err(|e| e.to_string())?;
    let mut rows: Vec<Value> = stmt
        .query_map([], |r| {
            let s: String = r.get(0)?;
            Ok(serde_json::from_str(&s).unwrap_or(json!({})))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    // Sort pinned conversations first, then by updatedAtMs DESC
    rows.sort_by(|a, b| {
        let pa = a.get("pinned").and_then(Value::as_bool).unwrap_or(false);
        let pb = b.get("pinned").and_then(Value::as_bool).unwrap_or(false);
        let ta = a.get("updatedAtMs").and_then(Value::as_i64).unwrap_or(0);
        let tb = b.get("updatedAtMs").and_then(Value::as_i64).unwrap_or(0);
        pb.cmp(&pa).then_with(|| tb.cmp(&ta))
    });
    Ok(rows)
}

fn list_actors(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare("SELECT actor_json FROM msg_actors ORDER BY actor_id ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let s: String = r.get(0)?;
            Ok(serde_json::from_str(&s).unwrap_or(json!({})))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn list_folders(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare("SELECT folder_json FROM msg_folders ORDER BY sort_order ASC, folder_id ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let s: String = r.get(0)?;
            Ok(serde_json::from_str(&s).unwrap_or(json!({})))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn list_drafts(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare("SELECT draft_json FROM msg_drafts ORDER BY updated_at_ms DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let s: String = r.get(0)?;
            Ok(serde_json::from_str(&s).unwrap_or(json!({})))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn list_messages(conn: &Connection, limit: i64) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT message_json FROM (
                SELECT message_json, created_at_ms FROM msg_messages
                ORDER BY created_at_ms DESC LIMIT ?1
             ) ORDER BY created_at_ms ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |r| {
            let s: String = r.get(0)?;
            Ok(serde_json::from_str(&s).unwrap_or(json!({})))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn validate_blob_id(blob_id: &str) -> Result<(), String> {
    if blob_id.is_empty()
        || blob_id.len() > 160
        || !blob_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        return Err(format!("Invalid blobId '{blob_id}'"));
    }
    Ok(())
}

fn blob_file_path(app_data_dir: &Path, blob_id: &str) -> Result<PathBuf, String> {
    validate_blob_id(blob_id)?;
    let safe_name = blob_id.replace(':', "_");
    Ok(app_data_dir.join("messaging_blobs").join(safe_name))
}

fn decode_base64_lenient(input: &str) -> Result<Vec<u8>, String> {
    let clean: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(&clean)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&clean))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&clean))
        .map_err(|e| format!("Invalid base64 payload: {e}"))
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
