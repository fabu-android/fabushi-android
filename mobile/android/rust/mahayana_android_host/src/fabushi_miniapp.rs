//! Fabushi Global Dharma Offering (`全球法布施`) & Zen Room (`禅修室`) Host Bridge.
//!
//! Directly backed by the shared pure-Rust `::fabushi_miniapp` crate (`sutras`, `countries`,
//! `achievements`, `scene_math`, `state`, `storage`) vendored from `bhrum/gpui-telegram`.

use crate::magpie::{UnifiedMessage, UnifiedRequest, UnifiedRole};
use ::fabushi_miniapp::achievements::AchievementSystem;
use ::fabushi_miniapp::countries::{all_countries, country_by_code, search_countries};
use ::fabushi_miniapp::scene_math::{render_home_scene, render_zen_scene};
use ::fabushi_miniapp::state::{
    FabushiClientState, HttpActionEffect, HttpResponsePayload, LockedPractice, UiCommand,
};
use ::fabushi_miniapp::storage::{
    activate_membership, create_manual_practice_record, delete_practice_record,
    get_bootstrap_bundle, get_global_leaderboard, get_locked_practice, get_practice_leaderboard,
    get_profile, get_unlocked_achievements, init_fabushi_tables, list_practice_records,
    record_practice_session, record_transfer, toggle_follow, update_practice_record,
    update_profile, upsert_online_heartbeat,
};
use ::fabushi_miniapp::sutras::{
    all_dharma_packages, all_sutras, dharma_package_by_key, sutra_by_title,
};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::sync::OnceLock;

const DEFAULT_USER_ID: i64 = 100_001;

static CLIENT_STATE: OnceLock<Mutex<FabushiClientState>> = OnceLock::new();

fn shared_client_state() -> &'static Mutex<FabushiClientState> {
    CLIENT_STATE.get_or_init(|| {
        let now_ms = chrono::Utc::now().timestamp_millis() as f64;
        Mutex::new(FabushiClientState::new(now_ms))
    })
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    init_fabushi_tables(conn).map_err(|e| format!("Failed to initialize fabushi_miniapp tables: {e}"))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS dharma_android_aux (
            user_id INTEGER PRIMARY KEY,
            merit_points INTEGER NOT NULL DEFAULT 0,
            wheel_rotations INTEGER NOT NULL DEFAULT 0,
            updated_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize dharma_android_aux table: {e}"))?;

    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT OR IGNORE INTO dharma_android_aux (user_id, merit_points, wheel_rotations, updated_at_ms)
         VALUES (?1, 0, 0, ?2)",
        params![DEFAULT_USER_ID, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn resolve_user_id(params: &Value) -> i64 {
    params
        .get("userId")
        .and_then(Value::as_i64)
        .filter(|&id| id > 0)
        .unwrap_or(DEFAULT_USER_ID)
}

fn hydrate_client_state_from_db(
    conn: &Connection,
    st: &mut FabushiClientState,
    user_id: i64,
) -> Result<(), String> {
    let bundle = get_bootstrap_bundle(conn, user_id, "practitioner_local", "本机行者", "", false)
        .map_err(|e| e.to_string())?;
    let _ = st.apply_http_effect(HttpResponsePayload::Bootstrap {
        profile: bundle.profile,
        locked_practice: bundle.locked_practice,
        leaderboard: bundle.leaderboard,
        practice_leaderboard: bundle.practice_leaderboard,
        practice_records: bundle.practice_records,
        practice_stats: bundle.practice_stats,
        unlocked_achievement_ids: bundle.unlocked_achievement_ids,
        online_global_sending_count: bundle.online_global_sending_count,
        online_zen_room_count: bundle.online_zen_room_count,
    });
    Ok(())
}

fn execute_local_effects_against_sqlite(
    conn: &Connection,
    st: &mut FabushiClientState,
    user_id: i64,
    effects: Vec<HttpActionEffect>,
) -> Result<(usize, Vec<(String, Option<String>)>), String> {
    let mut applied = 0usize;
    let mut pending_ai_chats: Vec<(String, Option<String>)> = Vec::new();
    for effect in effects {
        applied += 1;
        match effect {
            HttpActionEffect::Bootstrap => {
                hydrate_client_state_from_db(conn, st, user_id)?;
            }
            HttpActionEffect::RecordTransfer {
                content_title,
                content_kind,
                target_countries,
                bytes_sent,
                beam_count,
                notify_receipt,
                receipt_only,
                session_bytes: _,
                session_beams: _,
            } => {
                if !receipt_only && (bytes_sent > 0 || beam_count > 0) {
                    let res = record_transfer(
                        conn,
                        user_id,
                        &content_title,
                        &content_kind,
                        &target_countries,
                        bytes_sent,
                        beam_count,
                    )
                    .map_err(|e| e.to_string())?;
                    let merit = (beam_count as i64).max(1) * 18;
                    conn.execute(
                        "UPDATE dharma_android_aux SET merit_points = merit_points + ?1, updated_at_ms = ?2 WHERE user_id = ?3",
                        params![merit, chrono::Utc::now().timestamp_millis(), user_id],
                    )
                    .map_err(|e| e.to_string())?;
                    let _ = st.apply_http_effect(HttpResponsePayload::TransferRecorded {
                        total_transfer_bytes: res.total_transfer_bytes,
                        total_transfer_count: res.total_transfer_count,
                        leaderboard: Some(res.leaderboard),
                    });
                }
                let _ = notify_receipt;
            }
            HttpActionEffect::SavePracticeSession {
                sutra,
                duration_minutes,
                chant_count,
                start_time_ms,
                end_time_ms,
                notes,
            } => {
                let res = record_practice_session(
                    conn,
                    user_id,
                    &sutra,
                    duration_minutes.max(1),
                    chant_count,
                    Some(start_time_ms),
                    Some(end_time_ms),
                    notes.as_deref(),
                )
                .map_err(|e| e.to_string())?;
                let _ = st.apply_http_effect(HttpResponsePayload::PracticeSessionSaved {
                    record: res.record,
                    practice_stats: res.practice_stats,
                    profile: res.profile,
                });
            }
            HttpActionEffect::CreatePracticeRecord {
                sutra_name,
                chant_count,
                duration_minutes,
                record_date,
                local_time,
                notes,
            } => {
                let (_rec, stats) = create_manual_practice_record(
                    conn,
                    user_id,
                    &sutra_name,
                    chant_count,
                    duration_minutes,
                    &record_date,
                    &local_time,
                    notes.as_deref(),
                )
                .map_err(|e| e.to_string())?;
                let records = list_practice_records(conn, user_id, 50, 0).map_err(|e| e.to_string())?;
                let _ = st.apply_http_effect(HttpResponsePayload::PracticeRecordsRefreshed {
                    practice_records: records,
                    practice_stats: stats,
                });
            }
            HttpActionEffect::UpdatePracticeRecord {
                id,
                sutra_name,
                chant_count,
                duration_minutes,
                record_date,
                local_time,
                notes,
            } => {
                if let Some((_rec, stats)) = update_practice_record(
                    conn,
                    user_id,
                    id,
                    &sutra_name,
                    chant_count,
                    duration_minutes,
                    &record_date,
                    &local_time,
                    notes.as_deref(),
                )
                .map_err(|e| e.to_string())?
                {
                    let records =
                        list_practice_records(conn, user_id, 50, 0).map_err(|e| e.to_string())?;
                    let _ = st.apply_http_effect(HttpResponsePayload::PracticeRecordsRefreshed {
                        practice_records: records,
                        practice_stats: stats,
                    });
                }
            }
            HttpActionEffect::DeletePracticeRecord { id } => {
                let (_deleted, stats) =
                    delete_practice_record(conn, user_id, id).map_err(|e| e.to_string())?;
                let records = list_practice_records(conn, user_id, 50, 0).map_err(|e| e.to_string())?;
                let _ = st.apply_http_effect(HttpResponsePayload::PracticeRecordsRefreshed {
                    practice_records: records,
                    practice_stats: stats,
                });
            }
            HttpActionEffect::UpdateProfile {
                display_name,
                bio,
                locked_practice,
                locale,
            } => {
                let (profile, locked) = update_profile(
                    conn,
                    user_id,
                    display_name.as_deref(),
                    bio.as_deref(),
                    locked_practice.as_ref(),
                    locale.as_deref(),
                )
                .map_err(|e| e.to_string())?;
                let _ = st.apply_http_effect(HttpResponsePayload::ProfileUpdated {
                    profile,
                    locked_practice: locked,
                });
            }
            HttpActionEffect::ActivateMembership { plan } => {
                let profile = activate_membership(conn, user_id, &plan).map_err(|e| e.to_string())?;
                let _ = st.apply_http_effect(HttpResponsePayload::MembershipActivated { profile });
            }
            HttpActionEffect::ToggleFollow { target_username } => {
                let res = toggle_follow(conn, user_id, &target_username).map_err(|e| e.to_string())?;
                let _ = st.apply_http_effect(HttpResponsePayload::FollowToggled {
                    target_username: res.target_username,
                    is_following: res.is_following,
                    follower_count: res.follower_count,
                });
            }
            HttpActionEffect::FetchLeaderboard { kind } => {
                let entries = if kind == "practice" {
                    get_practice_leaderboard(conn, user_id, 50).map_err(|e| e.to_string())?
                } else {
                    get_global_leaderboard(conn, user_id, 50).map_err(|e| e.to_string())?
                };
                let _ = st.apply_http_effect(HttpResponsePayload::LeaderboardFetched {
                    kind,
                    entries,
                });
            }
            HttpActionEffect::Heartbeat { activity_type } => {
                let online_count =
                    upsert_online_heartbeat(conn, user_id, &activity_type).map_err(|e| e.to_string())?;
                let _ = st.apply_http_effect(HttpResponsePayload::HeartbeatAck {
                    activity_type,
                    online_count,
                });
            }
            HttpActionEffect::SendAiChat { message, .. } => {
                pending_ai_chats.push((
                    message,
                    st.locked_practice.as_ref().map(|lp| lp.title.clone()),
                ));
            }
            HttpActionEffect::SyncCloudStorage { .. } | HttpActionEffect::EmitWebAppData { .. } => {}
        }
    }
    Ok((applied, pending_ai_chats))
}

fn run_dharma_ai_query(
    db: &std::sync::Mutex<Connection>,
    prompt: &str,
    locked_practice: Option<&str>,
) -> Result<(String, &'static str), String> {
    let has_seat = crate::with_db(db, |conn| Ok(crate::magpie::has_configured_live_seat(conn)))?;
    if !has_seat {
        return Err(
            "PROVIDER_NOT_CONFIGURED: Configure an LLM API key in Magpie Gateway before using Dharma AI Chat"
                .into(),
        );
    }
    let sys = format!(
        "你是全球法布施与禅修室的佛学参学顾问。请以温和、恭敬、契合大乘般若经典的语言解答问题。{}",
        locked_practice
            .map(|s| format!("修行者当前一门深入的主修功课为《{s}》。"))
            .unwrap_or_default()
    );
    let req = UnifiedRequest {
        model: "auto".into(),
        system_prompt: sys,
        messages: vec![UnifiedMessage {
            role: UnifiedRole::User,
            content: prompt.to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            reasoning: None,
        }],
        tools: Vec::new(),
        temperature: Some(0.5),
        max_tokens: Some(512),
        thinking_budget_tokens: None,
        stream: true,
    };
    let live = crate::magpie::execute_live_request(db, &req, None, None, None, |_| {})?;
    if live.reply.trim().is_empty() {
        return Err(
            "UPSTREAM_EMPTY_REPLY: Configured Magpie provider returned an empty response".into(),
        );
    }
    Ok((live.reply, "magpie_live"))
}

pub fn handle_rpc(
    db: &std::sync::Mutex<Connection>,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    let user_id = resolve_user_id(params);
    match submethod {
        "dispatchUi" => {
            let cmd_val = params
                .get("command")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let cmd: UiCommand = serde_json::from_value(cmd_val)
                .map_err(|e| format!("Invalid fabushiMiniapp UiCommand JSON: {e}"))?;
            let (applied, pending_ai) = crate::with_db(db, |conn| {
                init_schema(conn)?;
                let mut st = shared_client_state().lock();
                if st.profile.user_id != user_id {
                    let _ = hydrate_client_state_from_db(conn, &mut st, user_id);
                }
                let dispatched = st.dispatch(cmd);
                execute_local_effects_against_sqlite(conn, &mut st, user_id, dispatched.effects)
            })?;
            // Run any AI chat queries with BOTH shared_client_state and DB mutexes unlocked!
            for (msg, locked_title) in pending_ai {
                let (reply, _) = run_dharma_ai_query(db, &msg, locked_title.as_deref())?;
                let mut st = shared_client_state().lock();
                let _ = st.apply_http_effect(HttpResponsePayload::AiChatReply { reply });
            }
            let st = shared_client_state().lock();
            let snapshot = st.snapshot();
            let draw_commands = st.render_scene(st.last_viewport_w, st.last_viewport_h);
            Ok(json!({
                "effectsApplied": applied,
                "snapshot": snapshot,
                "drawCommands": draw_commands,
            }))
        }

        "tickScene" => {
            let dt_ms = params.get("dtMs").and_then(Value::as_f64).unwrap_or(16.0);
            let width = params.get("width").and_then(Value::as_f64).unwrap_or(420.0);
            let height = params.get("height").and_then(Value::as_f64).unwrap_or(780.0);
            let now_ms = chrono::Utc::now().timestamp_millis() as f64;
            let (applied, pending_ai) = crate::with_db(db, |conn| {
                init_schema(conn)?;
                let mut st = shared_client_state().lock();
                let res = st.tick(dt_ms, now_ms, width, height);
                execute_local_effects_against_sqlite(conn, &mut st, user_id, res.effects)
            })?;
            // Run any AI chat queries with BOTH shared_client_state and DB mutexes unlocked!
            for (msg, locked_title) in pending_ai {
                let (reply, _) = run_dharma_ai_query(db, &msg, locked_title.as_deref())?;
                let mut st = shared_client_state().lock();
                let _ = st.apply_http_effect(HttpResponsePayload::AiChatReply { reply });
            }
            let st = shared_client_state().lock();
            Ok(json!({
                "effectsApplied": applied,
                "snapshot": st.snapshot(),
                "drawCommands": st.render_scene(width, height),
            }))
        }

        "renderHomeScene" => {
            let width = params.get("width").and_then(Value::as_f64).unwrap_or(420.0);
            let height = params.get("height").and_then(Value::as_f64).unwrap_or(780.0);
            let dt_ms = params.get("dtMs").and_then(Value::as_f64).unwrap_or(16.0);
            let is_3d = params.get("is3d").and_then(Value::as_bool).unwrap_or(false);
            let now_ms = chrono::Utc::now().timestamp_millis() as f64;
            let mut st = shared_client_state().lock();
            if let Some(dx) = params.get("dragDx").and_then(Value::as_f64) {
                let dy = params.get("dragDy").and_then(Value::as_f64).unwrap_or(0.0);
                st.globe_scene.drag(dx, dy, is_3d);
            } else {
                st.globe_scene.advance(dt_ms, now_ms);
            }
            let cmds = render_home_scene(&st.globe_scene, width, height, now_ms, is_3d);
            Ok(json!({
                "rotationLonDeg": st.globe_scene.rotation_lon_deg,
                "tiltLatDeg": st.globe_scene.tilt_lat_deg,
                "activeBeams": st.globe_scene.beams.len(),
                "drawCommands": cmds,
            }))
        }

        "aiDharmaChat" => {
            let prompt = params
                .get("prompt")
                .or_else(|| params.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("如何安住当下？");
            let locked = crate::with_db(db, |conn| {
                init_schema(conn)?;
                Ok(get_locked_practice(conn, user_id)
                    .ok()
                    .flatten()
                    .map(|lp| lp.title))
            })?;
            let (reply, source) = run_dharma_ai_query(db, prompt, locked.as_deref())?;
            Ok(json!({
                "prompt": prompt,
                "reply": reply,
                "source": source,
            }))
        }

        _ => crate::with_db(db, |conn| handle_rpc_local(conn, user_id, submethod, params)),
    }
}

fn handle_rpc_local(
    conn: &Connection,
    user_id: i64,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    init_schema(conn)?;

    match submethod {
        "status" => {
            let bundle =
                get_bootstrap_bundle(conn, user_id, "practitioner_local", "本机行者", "", false)
                    .map_err(|e| e.to_string())?;
            let (merit, wheel_rot): (i64, i64) = conn
                .query_row(
                    "SELECT merit_points, wheel_rotations FROM dharma_android_aux WHERE user_id = ?1",
                    params![user_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or((0, 0));

            Ok(json!({
                "dharmaState": {
                    "meritPoints": merit,
                    "wheelRotations": wheel_rot,
                    "totalOfferings": bundle.profile.total_transfer_count,
                    "totalTransferBytes": bundle.profile.total_transfer_bytes,
                    "totalPracticeSeconds": bundle.profile.total_practice_minutes.saturating_mul(60),
                    "totalPracticeMinutes": bundle.profile.total_practice_minutes,
                    "totalPracticeSessions": bundle.profile.total_practice_sessions,
                    "totalChantCount": bundle.profile.total_practice_chants,
                    "streakDays": bundle.profile.consecutive_days,
                    "lockedPracticeSutra": bundle.profile.locked_practice_title,
                    "vipActive": bundle.profile.is_vip,
                    "vipPlan": bundle.profile.vip_plan,
                    "onlineGlobalSendingCount": bundle.online_global_sending_count,
                    "onlineZenRoomCount": bundle.online_zen_room_count,
                    "sutraCatalogCount": all_sutras().len(),
                    "packageCatalogCount": all_dharma_packages().len(),
                    "countryNodeCount": all_countries().len(),
                    "achievementCatalogCount": AchievementSystem::new().catalog_snapshot().len(),
                }
            }))
        }

        "bootstrap" => {
            let username = params
                .get("username")
                .and_then(Value::as_str)
                .unwrap_or("practitioner_local");
            let display_name = params
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or("本机行者");
            let bundle = get_bootstrap_bundle(conn, user_id, username, display_name, "", false)
                .map_err(|e| e.to_string())?;
            {
                let mut st = shared_client_state().lock();
                let _ = st.apply_http_effect(HttpResponsePayload::Bootstrap {
                    profile: bundle.profile.clone(),
                    locked_practice: bundle.locked_practice.clone(),
                    leaderboard: bundle.leaderboard.clone(),
                    practice_leaderboard: bundle.practice_leaderboard.clone(),
                    practice_records: bundle.practice_records.clone(),
                    practice_stats: bundle.practice_stats.clone(),
                    unlocked_achievement_ids: bundle.unlocked_achievement_ids.clone(),
                    online_global_sending_count: bundle.online_global_sending_count,
                    online_zen_room_count: bundle.online_zen_room_count,
                });
            }
            serde_json::to_value(bundle).map_err(|e| e.to_string())
        }

        "sutras" => {
            let sutras_json: Vec<Value> = all_sutras()
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "title": s.title,
                        "category": s.category,
                        "content": s.content,
                        "bytes": s.content.len(),
                    })
                })
                .collect();
            let packages_json: Vec<Value> = all_dharma_packages()
                .iter()
                .map(|p| {
                    json!({
                        "key": p.key,
                        "title": p.display_name,
                        "displayName": p.display_name,
                        "sizeBytes": p.size_bytes,
                        "uploaded": p.uploaded,
                        "meritBase": 18,
                    })
                })
                .collect();
            Ok(json!({
                "sutras": sutras_json,
                "packages": packages_json,
            }))
        }

        "countries" => {
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            let region = params
                .get("region")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let matched = search_countries(query);
            let nodes: Vec<Value> = matched
                .into_iter()
                .filter(|c| region.is_empty() || c.continent.eq_ignore_ascii_case(region))
                .map(|c| {
                    json!({
                        "code": c.code,
                        "name_zh": c.name_zh,
                        "name_en": c.name_en,
                        "capital": c.capital,
                        "region": c.continent,
                        "continent": c.continent,
                        "lat": c.latitude,
                        "lng": c.longitude,
                        "lon": c.longitude,
                    })
                })
                .collect();
            Ok(json!({
                "countries": nodes,
                "total": all_countries().len(),
            }))
        }

        "offerDharma" => {
            let country_code = params
                .get("countryCode")
                .and_then(Value::as_str)
                .unwrap_or("CN")
                .to_ascii_uppercase();
            let package_key = params
                .get("packageKey")
                .and_then(Value::as_str)
                .unwrap_or("heart_sutra");

            let node = country_by_code(&country_code)
                .or_else(|| all_countries().first())
                .ok_or_else(|| "Country catalog empty".to_string())?;

            let (content_title, content_kind, size_bytes) =
                if let Some(s) = sutra_by_title(package_key).or_else(|| {
                    all_sutras().iter().find(|s| s.id == package_key)
                }) {
                    (s.title.clone(), "sutra".to_string(), s.content.len().max(4096) as u64)
                } else if let Some(pkg) = dharma_package_by_key(package_key) {
                    (pkg.display_name.clone(), "package".to_string(), pkg.size_bytes)
                } else {
                    (package_key.to_string(), "sutra".to_string(), 8_192_u64)
                };

            let count = params
                .get("count")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 108) as u32;
            let merit_earned = (count as i64) * 18;
            let now_ms = chrono::Utc::now().timestamp_millis();

            let tr = record_transfer(
                conn,
                user_id,
                &content_title,
                &content_kind,
                std::slice::from_ref(&node.code),
                size_bytes.saturating_mul(count as u64),
                count,
            )
            .map_err(|e| e.to_string())?;

            conn.execute(
                "UPDATE dharma_android_aux SET merit_points = merit_points + ?1, updated_at_ms = ?2 WHERE user_id = ?3",
                params![merit_earned, now_ms, user_id],
            )
            .map_err(|e| e.to_string())?;

            {
                let mut st = shared_client_state().lock();
                let (ulat, ulng) = (st.user_lat, st.user_lng);
                st.globe_scene.add_beam(
                    ulat,
                    ulng,
                    node.latitude,
                    node.longitude,
                    Some(node.name_zh.clone()),
                    now_ms as f64,
                    2200.0,
                );
            }

            let new_merit: i64 = conn
                .query_row(
                    "SELECT merit_points FROM dharma_android_aux WHERE user_id = ?1",
                    params![user_id],
                    |r| r.get(0),
                )
                .unwrap_or(merit_earned);

            Ok(json!({
                "offeringId": format!("offer_{}", uuid::Uuid::new_v4().simple()),
                "country": {
                    "code": node.code,
                    "name_zh": node.name_zh,
                    "name_en": node.name_en,
                    "continent": node.continent,
                    "lat": node.latitude,
                    "lng": node.longitude,
                },
                "packageKey": package_key,
                "contentTitle": content_title,
                "count": count,
                "meritEarned": merit_earned,
                "totalMeritPoints": new_merit,
                "totalOfferings": tr.total_transfer_count,
                "totalTransferBytes": tr.total_transfer_bytes,
                "beamArc": {
                    "originLat": 34.0,
                    "originLon": 108.0,
                    "destLat": node.latitude,
                    "destLon": node.longitude,
                }
            }))
        }

        "spinPrayerWheel" => {
            let rotations = params
                .get("rotations")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .clamp(1, 1080);
            let merit_delta = rotations * 3;
            let now = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "UPDATE dharma_android_aux
                 SET wheel_rotations = wheel_rotations + ?1,
                     merit_points = merit_points + ?2,
                     updated_at_ms = ?3
                 WHERE user_id = ?4",
                params![rotations, merit_delta, now, user_id],
            )
            .map_err(|e| e.to_string())?;
            let (wheel, merit): (i64, i64) = conn
                .query_row(
                    "SELECT wheel_rotations, merit_points FROM dharma_android_aux WHERE user_id = ?1",
                    params![user_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "rotationsAdded": rotations,
                "wheelRotations": wheel,
                "meritEarned": merit_delta,
                "totalMeritPoints": merit,
            }))
        }

        "recordPractice" => {
            let raw_sutra = params
                .get("sutraId")
                .or_else(|| params.get("sutra"))
                .and_then(Value::as_str)
                .unwrap_or("heart_sutra");
            let sutra_title = all_sutras()
                .iter()
                .find(|s| s.id == raw_sutra || s.title == raw_sutra)
                .map(|s| s.title.clone())
                .unwrap_or_else(|| raw_sutra.to_string());
            const MAX_PRACTICE_DURATION_SECS: u64 = 604_800; // 7 days
            const MAX_PRACTICE_CHANT_COUNT: u64 = 1_000_000;
            let duration_sec = params
                .get("durationSeconds")
                .and_then(Value::as_u64)
                .unwrap_or(600)
                .min(MAX_PRACTICE_DURATION_SECS);
            let duration_mins = (duration_sec.saturating_add(59) / 60).max(1) as u32;
            let chant_count = params
                .get("chantCount")
                .and_then(Value::as_u64)
                .unwrap_or(21)
                .min(MAX_PRACTICE_CHANT_COUNT) as u32;
            let lock_sutra = params
                .get("lockPractice")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let notes = params.get("notes").and_then(Value::as_str);

            if lock_sutra {
                let lp = LockedPractice {
                    title: sutra_title.clone(),
                    category: "一门深入".to_string(),
                    file_path: format!("sutra:{sutra_title}"),
                    selected_at_ms: chrono::Utc::now().timestamp_millis(),
                    is_locked: true,
                };
                let _ = update_profile(conn, user_id, None, None, Some(&lp), None);
            }

            let now_ms = chrono::Utc::now().timestamp_millis();
            let saved = record_practice_session(
                conn,
                user_id,
                &sutra_title,
                duration_mins,
                chant_count,
                Some(now_ms.saturating_sub((duration_sec as i64).saturating_mul(1000))),
                Some(now_ms),
                notes,
            )
            .map_err(|e| e.to_string())?;

            let merit_delta = ((duration_sec / 60) as i64)
                .saturating_mul(5)
                .saturating_add((chant_count as i64) / 3);
            conn.execute(
                "UPDATE dharma_android_aux SET merit_points = merit_points + ?1, updated_at_ms = ?2 WHERE user_id = ?3",
                params![merit_delta, now_ms, user_id],
            )
            .map_err(|e| e.to_string())?;
            let merit: i64 = conn
                .query_row(
                    "SELECT merit_points FROM dharma_android_aux WHERE user_id = ?1",
                    params![user_id],
                    |r| r.get(0),
                )
                .unwrap_or(merit_delta);

            Ok(json!({
                "sessionId": format!("zen_{}", saved.record.id),
                "record": saved.record,
                "sutraId": raw_sutra,
                "sutraTitle": sutra_title,
                "durationSeconds": duration_sec,
                "chantCount": chant_count,
                "totalSessions": saved.profile.total_practice_sessions,
                "streakDays": saved.profile.consecutive_days,
                "totalMeritPoints": merit,
                "practiceStats": saved.practice_stats,
                "unlockedAchievementIds": saved.unlocked_achievement_ids,
            }))
        }

        "achievements" => {
            let profile = get_profile(conn, user_id).map_err(|e| e.to_string())?;
            let unlocked_ids = get_unlocked_achievements(conn, user_id).map_err(|e| e.to_string())?;
            let sys = AchievementSystem::from_persisted(
                profile.total_practice_sessions,
                profile.total_practice_minutes,
                profile.total_practice_chants,
                profile.consecutive_days,
                None,
                unlocked_ids,
            );
            let catalog = sys.catalog_snapshot();
            let unlocked_count = catalog.iter().filter(|a| a.unlocked).count();
            Ok(json!({
                "achievements": catalog,
                "unlockedCount": unlocked_count,
                "totalCount": catalog.len(),
            }))
        }

        "leaderboard" => {
            let kind = params
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("global");
            let list = if kind == "practice" {
                get_practice_leaderboard(conn, user_id, 50).map_err(|e| e.to_string())?
            } else {
                get_global_leaderboard(conn, user_id, 50).map_err(|e| e.to_string())?
            };
            Ok(json!({
                "kind": kind,
                "leaderboard": list,
            }))
        }

        "renderZenScene" => {
            let width = params.get("width").and_then(Value::as_f64).unwrap_or(420.0);
            let height = params.get("height").and_then(Value::as_f64).unwrap_or(780.0);
            let dt_ms = params.get("dtMs").and_then(Value::as_f64).unwrap_or(16.0);
            let is_3d = params.get("is3d").and_then(Value::as_bool).unwrap_or(false);
            let locked = get_locked_practice(conn, user_id)
                .ok()
                .flatten()
                .map(|lp| lp.title);
            let mut st = shared_client_state().lock();
            st.zen_scene.advance(dt_ms);
            let cmds = render_zen_scene(
                &st.zen_scene,
                width,
                height,
                is_3d,
                !st.zen_scene.is_burning,
                locked.as_deref(),
            );
            Ok(json!({
                "incenseProgress": st.zen_scene.incense_progress,
                "isBurning": st.zen_scene.is_burning,
                "drawCommands": cmds,
            }))
        }

        other => Err(format!("Unsupported fabushiMiniapp method: {other}")),
    }
}

/// Generic Mini App WebMCP & Human Takeover RPC surface (`miniapp.webmcp.*`),
/// aligned with `bhrumom/gpui-telegram` (`053012a`, `9623c2c`, `e9d5b37`).
pub fn handle_webmcp_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS miniapp_webmcp_sessions (
            session_key TEXT PRIMARY KEY,
            bot_username TEXT NOT NULL,
            short_name TEXT NOT NULL,
            app_title TEXT NOT NULL,
            control_mode TEXT NOT NULL DEFAULT 'idle',
            status_text TEXT NOT NULL DEFAULT '',
            active_tool TEXT,
            tools_json TEXT NOT NULL DEFAULT '[]',
            state_snapshot_json TEXT,
            pip_minimized INTEGER NOT NULL DEFAULT 0,
            updated_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed initializing miniapp_webmcp_sessions: {e}"))?;

    let now = chrono::Utc::now().timestamp_millis();
    let bot_username = params
        .get("botUsername")
        .or_else(|| params.get("bot_username"))
        .and_then(Value::as_str)
        .unwrap_or("fabushi_bot")
        .trim();
    let short_name = params
        .get("shortName")
        .or_else(|| params.get("short_name"))
        .and_then(Value::as_str)
        .unwrap_or("global_dharma")
        .trim();
    let session_key = format!("{bot_username}:{short_name}");

    match submethod {
        "registerTools" => {
            let app_title = params
                .get("appTitle")
                .and_then(Value::as_str)
                .unwrap_or("Mini App");
            let tools_val = params
                .get("tools")
                .cloned()
                .unwrap_or_else(|| json!([]));
            let tools: Vec<gpui_telegram_proto::miniapp_contracts::WebMcpToolDescriptor> =
                serde_json::from_value(tools_val)
                    .map_err(|e| format!("Invalid WebMcpToolDescriptor list: {e}"))?;
            let tools_json = serde_json::to_string(&tools).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO miniapp_webmcp_sessions
                 (session_key, bot_username, short_name, app_title, tools_json, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(session_key) DO UPDATE SET
                   app_title = excluded.app_title,
                   tools_json = excluded.tools_json,
                   updated_at_ms = excluded.updated_at_ms",
                params![session_key, bot_username, short_name, app_title, tools_json, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "sessionKey": session_key,
                "registeredTools": tools.len(),
                "tools": tools,
            }))
        }

        "setControlMode" => {
            let mode_raw = params
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("idle");
            let mode: gpui_telegram_proto::miniapp_contracts::MiniAppControlMode =
                serde_json::from_value(Value::String(mode_raw.to_string()))
                    .map_err(|e| format!("Invalid MiniAppControlMode '{mode_raw}': {e}"))?;
            let status_text = params
                .get("statusText")
                .and_then(Value::as_str)
                .unwrap_or("");
            let active_tool = params.get("activeTool").and_then(Value::as_str);
            let pip_minimized = params
                .get("pipMinimized")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let state_snapshot = params
                .get("stateSnapshot")
                .map(|v| v.to_string());
            let app_title = params
                .get("appTitle")
                .and_then(Value::as_str)
                .unwrap_or("Mini App");

            conn.execute(
                "INSERT INTO miniapp_webmcp_sessions
                 (session_key, bot_username, short_name, app_title, control_mode, status_text, active_tool, state_snapshot_json, pip_minimized, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(session_key) DO UPDATE SET
                   control_mode = excluded.control_mode,
                   status_text = excluded.status_text,
                   active_tool = excluded.active_tool,
                   state_snapshot_json = COALESCE(excluded.state_snapshot_json, miniapp_webmcp_sessions.state_snapshot_json),
                   pip_minimized = excluded.pip_minimized,
                   updated_at_ms = excluded.updated_at_ms",
                params![
                    session_key,
                    bot_username,
                    short_name,
                    app_title,
                    mode_raw,
                    status_text,
                    active_tool,
                    state_snapshot,
                    if pip_minimized { 1 } else { 0 },
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "sessionKey": session_key,
                "mode": mode,
                "statusText": status_text,
                "activeTool": active_tool,
                "pipMinimized": pip_minimized,
                "updatedAtMs": now,
            }))
        }

        "dispatchCommand" => {
            let cmd_val = params
                .get("command")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let cmd: gpui_telegram_proto::miniapp_contracts::MiniAppWebMcpCommand =
                serde_json::from_value(cmd_val)
                    .map_err(|e| format!("Invalid MiniAppWebMcpCommand: {e}"))?;
            let next_mode = if cmd.operation == "request_takeover" {
                "waiting_for_takeover"
            } else {
                "ai_controlling"
            };
            let sk = format!("{}:{}", cmd.bot_username, cmd.short_name);
            conn.execute(
                "INSERT INTO miniapp_webmcp_sessions
                 (session_key, bot_username, short_name, app_title, control_mode, status_text, active_tool, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(session_key) DO UPDATE SET
                   control_mode = excluded.control_mode,
                   status_text = excluded.status_text,
                   active_tool = excluded.active_tool,
                   updated_at_ms = excluded.updated_at_ms",
                params![
                    sk,
                    cmd.bot_username,
                    cmd.short_name,
                    cmd.app_title,
                    next_mode,
                    cmd.status_message.as_deref().unwrap_or("WebMCP command active"),
                    cmd.tool_name,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "accepted": true,
                "requestId": cmd.request_id,
                "operation": cmd.operation,
                "controlMode": next_mode,
                "toolName": cmd.tool_name,
            }))
        }

        "submitResponse" => {
            let resp_val = params
                .get("response")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let resp: gpui_telegram_proto::miniapp_contracts::MiniAppWebMcpResponse =
                serde_json::from_value(resp_val)
                    .map_err(|e| format!("Invalid MiniAppWebMcpResponse: {e}"))?;
            Ok(json!({
                "recorded": true,
                "requestId": resp.request_id,
                "ok": resp.ok,
                "controlMode": resp.control_mode,
                "toolsCount": resp.tools.len(),
            }))
        }

        "getControlStatus" => {
            let row: Option<(String, String, String, Option<String>, String, Option<String>, i64, i64)> =
                conn.query_row(
                    "SELECT app_title, control_mode, status_text, active_tool, tools_json, state_snapshot_json, pip_minimized, updated_at_ms
                     FROM miniapp_webmcp_sessions WHERE session_key = ?1",
                    params![session_key],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| e.to_string())?;

            match row {
                Some((title, mode, status_text, active_tool, tools_json, snap, pip, updated)) => {
                    Ok(json!({
                        "sessionKey": session_key,
                        "appTitle": title,
                        "controlMode": mode,
                        "statusText": status_text,
                        "activeTool": active_tool,
                        "tools": serde_json::from_str::<Value>(&tools_json).unwrap_or_else(|_| json!([])),
                        "stateSnapshot": snap.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                        "pipMinimized": pip != 0,
                        "updatedAtMs": updated,
                    }))
                }
                None => Ok(json!({
                    "sessionKey": session_key,
                    "controlMode": "idle",
                    "statusText": "",
                    "tools": [],
                    "pipMinimized": false,
                })),
            }
        }

        other => Err(format!("Unsupported miniapp.webmcp method: {other}")),
    }
}
