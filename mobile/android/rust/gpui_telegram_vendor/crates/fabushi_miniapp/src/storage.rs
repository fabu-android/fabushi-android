use crate::achievements::AchievementSystem;
use crate::state::{
    LeaderboardEntryDto, LockedPractice, PracticeRecordDto, PracticeStatsDto, SubjectStatsDto,
    UserProfileDto,
};
use crate::sutras::format_leaderboard_bytes;
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecordResult {
    pub total_transfer_bytes: u64,
    pub total_transfer_count: u64,
    pub leaderboard: Vec<LeaderboardEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PracticeSaveResult {
    pub record: PracticeRecordDto,
    pub practice_stats: PracticeStatsDto,
    pub profile: UserProfileDto,
    pub unlocked_achievement_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowToggleResult {
    pub target_username: String,
    pub is_following: bool,
    pub follower_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapBundle {
    pub profile: UserProfileDto,
    pub locked_practice: Option<LockedPractice>,
    pub leaderboard: Vec<LeaderboardEntryDto>,
    pub practice_leaderboard: Vec<LeaderboardEntryDto>,
    pub practice_records: Vec<PracticeRecordDto>,
    pub practice_stats: PracticeStatsDto,
    pub unlocked_achievement_ids: Vec<String>,
    pub online_global_sending_count: u64,
    pub online_zen_room_count: u64,
}

struct SeedPractitioner {
    user_id: i64,
    username: &'static str,
    display_name: &'static str,
    bio: &'static str,
    sutra: &'static str,
    total_bytes: i64,
    total_transfers: i64,
    sessions: i64,
    minutes: i64,
    chants: i64,
    days: i64,
}

const SEED_PRACTITIONERS: &[SeedPractitioner] = &[
    SeedPractitioner {
        user_id: 9_000_001,
        username: "lotus_master_bj",
        display_name: "莲海居士",
        bio: "北京法海寺义工 · 每日精进持诵《金刚经》",
        sutra: "金刚经",
        total_bytes: 128_500_000_000,
        total_transfers: 2480,
        sessions: 142,
        minutes: 4260,
        chants: 15800,
        days: 108,
    },
    SeedPractitioner {
        user_id: 9_000_002,
        username: "prajna_dharma_tw",
        display_name: "般若清风",
        bio: "台北共修小组 · 愿十方众生同闻法音",
        sutra: "心经",
        total_bytes: 94_200_000_000,
        total_transfers: 1860,
        sessions: 96,
        minutes: 3120,
        chants: 12400,
        days: 68,
    },
    SeedPractitioner {
        user_id: 9_000_003,
        username: "bodhi_sea_sg",
        display_name: "星海行者",
        bio: "新加坡狮城禅修苑 · 全球普渡法布施",
        sutra: "华严经",
        total_bytes: 67_800_000_000,
        total_transfers: 1420,
        sessions: 84,
        minutes: 2540,
        chants: 8900,
        days: 45,
    },
    SeedPractitioner {
        user_id: 9_000_004,
        username: "avalokita_jp",
        display_name: "东京禅心",
        bio: "早课晨钟 · 一门深入",
        sutra: "法华经",
        total_bytes: 48_300_000_000,
        total_transfers: 980,
        sessions: 64,
        minutes: 1980,
        chants: 6200,
        days: 33,
    },
    SeedPractitioner {
        user_id: 9_000_005,
        username: "manjushri_us",
        display_name: "西岸净土",
        bio: "San Francisco Dharma Hub",
        sutra: "楞严经",
        total_bytes: 31_400_000_000,
        total_transfers: 720,
        sessions: 52,
        minutes: 1560,
        chants: 4100,
        days: 24,
    },
    SeedPractitioner {
        user_id: 9_000_006,
        username: "samantabhadra_uk",
        display_name: "泰晤士法音",
        bio: "伦敦菩提书院共修护持",
        sutra: "圆觉经",
        total_bytes: 19_600_000_000,
        total_transfers: 510,
        sessions: 40,
        minutes: 1200,
        chants: 2900,
        days: 21,
    },
    SeedPractitioner {
        user_id: 9_000_007,
        username: "amitabha_my",
        display_name: "吉隆坡念佛堂",
        bio: "昼夜念佛 · 随喜赞叹",
        sutra: "地藏经",
        total_bytes: 11_200_000_000,
        total_transfers: 310,
        sessions: 35,
        minutes: 980,
        chants: 2100,
        days: 15,
    },
    SeedPractitioner {
        user_id: 9_000_008,
        username: "kshitigarbha_kr",
        display_name: "汉城华严社",
        bio: "清净持戒 · 法喜充满",
        sutra: "维摩诘经",
        total_bytes: 4_850_000_000,
        total_transfers: 145,
        sessions: 21,
        minutes: 630,
        chants: 1080,
        days: 9,
    },
];

/// Creates all 5 `fabushi_*` SQLite tables + indexes and idempotently seeds
/// realistic global Dharma practitioners for the leaderboards.
pub fn init_fabushi_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS fabushi_profiles (
            user_id INTEGER PRIMARY KEY,
            username TEXT NOT NULL DEFAULT '',
            display_name TEXT NOT NULL DEFAULT '',
            avatar_url TEXT NOT NULL DEFAULT '',
            bio TEXT NOT NULL DEFAULT '',
            user_no INTEGER NOT NULL,
            is_vip INTEGER NOT NULL DEFAULT 0,
            vip_plan TEXT NOT NULL DEFAULT 'free',
            vip_expires_at TEXT,
            is_admin INTEGER NOT NULL DEFAULT 0,
            locked_practice_title TEXT,
            locked_practice_path TEXT,
            locked_practice_at TEXT,
            locale TEXT NOT NULL DEFAULT 'zh-CN',
            total_transfer_bytes INTEGER NOT NULL DEFAULT 0,
            total_transfer_count INTEGER NOT NULL DEFAULT 0,
            total_practice_sessions INTEGER NOT NULL DEFAULT 0,
            total_practice_minutes INTEGER NOT NULL DEFAULT 0,
            total_practice_chants INTEGER NOT NULL DEFAULT 0,
            consecutive_days INTEGER NOT NULL DEFAULT 0,
            last_practice_date TEXT,
            achievements_json TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_fabushi_profiles_username
            ON fabushi_profiles(username);
        CREATE INDEX IF NOT EXISTS idx_fabushi_profiles_transfer_bytes
            ON fabushi_profiles(total_transfer_bytes DESC);

        CREATE TABLE IF NOT EXISTS fabushi_transfer_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            content_title TEXT NOT NULL,
            content_kind TEXT NOT NULL,
            target_countries_json TEXT NOT NULL,
            bytes_sent INTEGER NOT NULL,
            beam_count INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_fabushi_transfer_user
            ON fabushi_transfer_logs(user_id, created_at DESC);

        CREATE TABLE IF NOT EXISTS fabushi_practice_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            sutra_name TEXT NOT NULL,
            sutra_source TEXT NOT NULL DEFAULT 'auto',
            duration_minutes INTEGER NOT NULL DEFAULT 0,
            chant_count INTEGER NOT NULL DEFAULT 0,
            record_date TEXT NOT NULL,
            local_time TEXT NOT NULL DEFAULT '',
            is_manual INTEGER NOT NULL DEFAULT 0,
            notes TEXT,
            start_time TEXT,
            end_time TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_fabushi_practice_user_date
            ON fabushi_practice_records(user_id, record_date DESC, id DESC);

        CREATE TABLE IF NOT EXISTS fabushi_follows (
            follower_user_id INTEGER NOT NULL,
            target_username TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (follower_user_id, target_username)
        );

        CREATE INDEX IF NOT EXISTS idx_fabushi_follows_target
            ON fabushi_follows(target_username);

        CREATE TABLE IF NOT EXISTS fabushi_online_heartbeats (
            session_key TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL,
            activity_type TEXT NOT NULL,
            last_seen_epoch_secs INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_fabushi_heartbeats_activity
            ON fabushi_online_heartbeats(activity_type, last_seen_epoch_secs);
        "#,
    )?;

    seed_practitioners(conn)?;
    Ok(())
}

fn seed_practitioners(conn: &Connection) -> rusqlite::Result<()> {
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let today_str = now.format("%Y-%m-%d").to_string();

    for (idx, seed) in SEED_PRACTITIONERS.iter().enumerate() {
        let user_no = 1001 + idx as i64;
        let achievements = serde_json::json!(["daily_start", "first_session", "chant_108"]).to_string();
        conn.execute(
            r#"
            INSERT OR IGNORE INTO fabushi_profiles (
                user_id, username, display_name, avatar_url, bio, user_no,
                is_vip, vip_plan, vip_expires_at, is_admin,
                locked_practice_title, locked_practice_path, locked_practice_at,
                locale, total_transfer_bytes, total_transfer_count,
                total_practice_sessions, total_practice_minutes, total_practice_chants,
                consecutive_days, last_practice_date, achievements_json, updated_at
            ) VALUES (
                ?1, ?2, ?3, '', ?4, ?5,
                1, 'yearly', ?6, 0,
                ?7, ?8, ?9,
                'zh-CN', ?10, ?11,
                ?12, ?13, ?14,
                ?15, ?16, ?17, ?9
            )
            "#,
            params![
                seed.user_id,
                seed.username,
                seed.display_name,
                seed.bio,
                user_no,
                (now + Duration::days(300)).to_rfc3339(),
                seed.sutra,
                format!("sutra:{}", seed.sutra),
                now_str,
                seed.total_bytes,
                seed.total_transfers,
                seed.sessions,
                seed.minutes,
                seed.chants,
                seed.days,
                today_str,
                achievements,
            ],
        )?;

        let existing_records: i64 = conn.query_row(
            "SELECT COUNT(*) FROM fabushi_practice_records WHERE user_id = ?1",
            params![seed.user_id],
            |row| row.get(0),
        )?;
        if existing_records == 0 {
            conn.execute(
                r#"
                INSERT INTO fabushi_practice_records (
                    user_id, sutra_name, sutra_source, duration_minutes, chant_count,
                    record_date, local_time, is_manual, notes, start_time, end_time, created_at
                ) VALUES (?1, ?2, 'auto', ?3, ?4, ?5, '06:30', 0, '每日清晨精进共修', ?6, ?6, ?6)
                "#,
                params![
                    seed.user_id,
                    seed.sutra,
                    seed.minutes,
                    seed.chants,
                    today_str,
                    now_str,
                ],
            )?;
        }
    }

    Ok(())
}

/// Ensures a `fabushi_profiles` row exists for the authenticated user and syncs identity fields.
pub fn ensure_profile(
    conn: &Connection,
    user_id: i64,
    username: &str,
    display_name: &str,
    avatar_url: &str,
    is_admin: bool,
) -> rusqlite::Result<UserProfileDto> {
    let clean_username = if username.trim().is_empty() {
        format!("user_{user_id}")
    } else {
        username.trim().to_string()
    };
    let clean_display = if display_name.trim().is_empty() {
        clean_username.clone()
    } else {
        display_name.trim().to_string()
    };
    let now_str = Utc::now().to_rfc3339();
    let user_no = 10_000 + (user_id.abs() % 900_000);

    conn.execute(
        r#"
        INSERT OR IGNORE INTO fabushi_profiles (
            user_id, username, display_name, avatar_url, bio, user_no,
            is_vip, vip_plan, vip_expires_at, is_admin,
            locale, updated_at
        ) VALUES (?1, ?2, ?3, ?4, '', ?5, 0, 'free', NULL, ?6, 'zh-CN', ?7)
        "#,
        params![
            user_id,
            clean_username,
            clean_display,
            avatar_url,
            user_no,
            is_admin as i64,
            now_str,
        ],
    )?;

    // Keep username/avatar/admin fresh if user signed in with updated Telegram profile
    conn.execute(
        r#"
        UPDATE fabushi_profiles
        SET username = CASE WHEN ?2 != '' THEN ?2 ELSE username END,
            display_name = CASE WHEN display_name = '' OR display_name = username THEN ?3 ELSE display_name END,
            avatar_url = CASE WHEN ?4 != '' THEN ?4 ELSE avatar_url END,
            is_admin = CASE WHEN ?5 = 1 THEN 1 ELSE is_admin END,
            updated_at = ?6
        WHERE user_id = ?1
        "#,
        params![
            user_id,
            clean_username,
            clean_display,
            avatar_url,
            is_admin as i64,
            now_str,
        ],
    )?;

    get_profile(conn, user_id)
}

fn ensure_profile_row(conn: &Connection, user_id: i64) -> rusqlite::Result<()> {
    let now_str = Utc::now().to_rfc3339();
    let user_no = 10_000 + (user_id.abs() % 900_000);
    conn.execute(
        r#"
        INSERT OR IGNORE INTO fabushi_profiles (
            user_id, username, display_name, avatar_url, bio, user_no,
            is_vip, vip_plan, vip_expires_at, is_admin,
            locale, updated_at
        ) VALUES (?1, ?2, ?2, '', '', ?3, 0, 'free', NULL, 0, 'zh-CN', ?4)
        "#,
        params![user_id, format!("user_{user_id}"), user_no, now_str],
    )?;
    Ok(())
}

pub fn get_profile(conn: &Connection, user_id: i64) -> rusqlite::Result<UserProfileDto> {
    ensure_profile_row(conn, user_id)?;
    conn.query_row(
        r#"
        SELECT user_id, username, display_name, avatar_url, bio, user_no,
               is_vip, vip_plan, vip_expires_at, is_admin, locale,
               total_transfer_bytes, total_transfer_count,
               total_practice_sessions, total_practice_minutes, total_practice_chants,
               consecutive_days, locked_practice_title
        FROM fabushi_profiles
        WHERE user_id = ?1
        "#,
        params![user_id],
        |row| {
            Ok(UserProfileDto {
                user_id: row.get(0)?,
                username: row.get(1)?,
                display_name: row.get(2)?,
                avatar_url: row.get(3)?,
                bio: row.get(4)?,
                user_no: row.get(5)?,
                is_vip: row.get::<_, i64>(6)? != 0,
                vip_plan: row.get(7)?,
                vip_expires_at: row.get(8)?,
                is_admin: row.get::<_, i64>(9)? != 0,
                locale: row.get(10)?,
                total_transfer_bytes: row.get::<_, i64>(11)?.max(0) as u64,
                total_transfer_count: row.get::<_, i64>(12)?.max(0) as u64,
                total_practice_sessions: row.get::<_, i64>(13)?.max(0) as u64,
                total_practice_minutes: row.get::<_, i64>(14)?.max(0) as u64,
                total_practice_chants: row.get::<_, i64>(15)?.max(0) as u64,
                consecutive_days: row.get::<_, i64>(16)?.max(0) as u64,
                locked_practice_title: row.get(17)?,
            })
        },
    )
}

pub fn get_locked_practice(conn: &Connection, user_id: i64) -> rusqlite::Result<Option<LockedPractice>> {
    conn.query_row(
        r#"
        SELECT locked_practice_title, locked_practice_path, locked_practice_at
        FROM fabushi_profiles
        WHERE user_id = ?1
        "#,
        params![user_id],
        |row| {
            let title: Option<String> = row.get(0)?;
            let path: Option<String> = row.get(1)?;
            let at: Option<String> = row.get(2)?;
            Ok(title.filter(|t| !t.trim().is_empty()).map(|t| {
                let sutra = crate::sutras::sutra_by_title(&t);
                let selected_at_ms = at
                    .as_deref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or(0);
                LockedPractice {
                    title: t.clone(),
                    category: sutra
                        .map(|s| s.category.clone())
                        .unwrap_or_else(|| "自选功课".to_string()),
                    file_path: path.unwrap_or_else(|| format!("manual:{t}")),
                    selected_at_ms,
                    is_locked: true,
                }
            }))
        },
    )
    .optional()
    .map(|opt| opt.flatten())
}

pub fn get_unlocked_achievements(conn: &Connection, user_id: i64) -> rusqlite::Result<Vec<String>> {
    let json_str: Option<String> = conn
        .query_row(
            "SELECT achievements_json FROM fabushi_profiles WHERE user_id = ?1",
            params![user_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(json_str
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default())
}

pub fn update_profile(
    conn: &Connection,
    user_id: i64,
    display_name: Option<&str>,
    bio: Option<&str>,
    locked_practice: Option<&LockedPractice>,
    locale: Option<&str>,
) -> rusqlite::Result<(UserProfileDto, Option<LockedPractice>)> {
    ensure_profile_row(conn, user_id)?;
    let now_str = Utc::now().to_rfc3339();
    if let Some(name) = display_name {
        let clean = name.trim();
        if !clean.is_empty() {
            conn.execute(
                "UPDATE fabushi_profiles SET display_name = ?2, updated_at = ?3 WHERE user_id = ?1",
                params![user_id, clean, now_str],
            )?;
        }
    }
    if let Some(bio_text) = bio {
        conn.execute(
            "UPDATE fabushi_profiles SET bio = ?2, updated_at = ?3 WHERE user_id = ?1",
            params![user_id, bio_text.trim(), now_str],
        )?;
    }
    if let Some(loc) = locale {
        let clean_loc = if loc.trim().to_ascii_lowercase().starts_with("en") {
            "en"
        } else {
            "zh-CN"
        };
        conn.execute(
            "UPDATE fabushi_profiles SET locale = ?2, updated_at = ?3 WHERE user_id = ?1",
            params![user_id, clean_loc, now_str],
        )?;
    }
    if let Some(lp) = locked_practice {
        // Enforce 一门深入 permanent lock: only set if not already locked
        let existing = get_locked_practice(conn, user_id)?;
        if existing.is_none() && !lp.title.trim().is_empty() {
            conn.execute(
                r#"
                UPDATE fabushi_profiles
                SET locked_practice_title = ?2,
                    locked_practice_path = ?3,
                    locked_practice_at = ?4,
                    updated_at = ?4
                WHERE user_id = ?1
                "#,
                params![user_id, lp.title.trim(), lp.file_path, now_str],
            )?;
        }
    }

    let profile = get_profile(conn, user_id)?;
    let locked = get_locked_practice(conn, user_id)?;
    Ok((profile, locked))
}

pub fn activate_membership(
    conn: &Connection,
    user_id: i64,
    plan: &str,
) -> rusqlite::Result<UserProfileDto> {
    ensure_profile_row(conn, user_id)?;
    let (norm_plan, days) = match plan.trim().to_ascii_lowercase().as_str() {
        "monthly" => ("monthly", 30_i64),
        "quarterly" => ("quarterly", 90_i64),
        _ => ("yearly", 365_i64),
    };
    let now = Utc::now();
    let expires_at = (now + Duration::days(days)).to_rfc3339();
    let now_str = now.to_rfc3339();

    conn.execute(
        r#"
        UPDATE fabushi_profiles
        SET is_vip = 1,
            vip_plan = ?2,
            vip_expires_at = ?3,
            updated_at = ?4
        WHERE user_id = ?1
        "#,
        params![user_id, norm_plan, expires_at, now_str],
    )?;

    get_profile(conn, user_id)
}

pub fn record_transfer(
    conn: &Connection,
    user_id: i64,
    content_title: &str,
    content_kind: &str,
    target_countries: &[String],
    bytes_sent: u64,
    beam_count: u32,
) -> rusqlite::Result<TransferRecordResult> {
    ensure_profile_row(conn, user_id)?;
    let now_str = Utc::now().to_rfc3339();
    let targets_json = serde_json::to_string(target_countries).unwrap_or_else(|_| "[\"ALL\"]".to_string());
    let safe_bytes = bytes_sent.min(i64::MAX as u64) as i64;

    conn.execute(
        r#"
        INSERT INTO fabushi_transfer_logs (
            user_id, content_title, content_kind, target_countries_json, bytes_sent, beam_count, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            user_id,
            content_title,
            content_kind,
            targets_json,
            safe_bytes,
            beam_count as i64,
            now_str,
        ],
    )?;

    conn.execute(
        r#"
        UPDATE fabushi_profiles
        SET total_transfer_bytes = total_transfer_bytes + ?2,
            total_transfer_count = total_transfer_count + 1,
            updated_at = ?3
        WHERE user_id = ?1
        "#,
        params![user_id, safe_bytes, now_str],
    )?;

    let profile = get_profile(conn, user_id)?;
    let leaderboard = get_global_leaderboard(conn, user_id, 50)?;
    Ok(TransferRecordResult {
        total_transfer_bytes: profile.total_transfer_bytes,
        total_transfer_count: profile.total_transfer_count,
        leaderboard,
    })
}

pub fn record_practice_session(
    conn: &Connection,
    user_id: i64,
    sutra: &str,
    duration_minutes: u32,
    chant_count: u32,
    start_time_ms: Option<i64>,
    end_time_ms: Option<i64>,
    notes: Option<&str>,
) -> rusqlite::Result<PracticeSaveResult> {
    ensure_profile_row(conn, user_id)?;
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let record_date = now.format("%Y-%m-%d").to_string();
    let local_time = now.format("%H:%M").to_string();
    let start_iso = start_time_ms
        .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms))
        .map(|dt| dt.to_rfc3339());
    let end_iso = end_time_ms
        .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms))
        .map(|dt| dt.to_rfc3339());
    let clean_notes = notes.map(|s| s.trim()).filter(|s| !s.is_empty());

    conn.execute(
        r#"
        INSERT INTO fabushi_practice_records (
            user_id, sutra_name, sutra_source, duration_minutes, chant_count,
            record_date, local_time, is_manual, notes, start_time, end_time, created_at
        ) VALUES (?1, ?2, 'auto', ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?10)
        "#,
        params![
            user_id,
            sutra,
            duration_minutes as i64,
            chant_count as i64,
            record_date,
            local_time,
            clean_notes,
            start_iso,
            end_iso,
            now_str,
        ],
    )?;
    let record_id = conn.last_insert_rowid();

    // Update streak & cumulative totals + achievement evaluation
    let (prev_last_date, prev_streak, prev_sessions, prev_mins, prev_chants, prev_ach_json): (
        Option<String>,
        i64,
        i64,
        i64,
        i64,
        String,
    ) = conn.query_row(
        r#"
        SELECT last_practice_date, consecutive_days, total_practice_sessions,
               total_practice_minutes, total_practice_chants, achievements_json
        FROM fabushi_profiles WHERE user_id = ?1
        "#,
        params![user_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        },
    )?;

    let yesterday_str = (now - Duration::days(1)).format("%Y-%m-%d").to_string();
    let new_streak = match prev_last_date.as_deref() {
        Some(d) if d == record_date => prev_streak.max(1),
        Some(d) if d == yesterday_str => prev_streak + 1,
        _ => 1,
    };

    let existing_unlocked: Vec<String> = serde_json::from_str(&prev_ach_json).unwrap_or_default();
    let mut ach_sys = AchievementSystem::from_persisted(
        prev_sessions.max(0) as u64,
        prev_mins.max(0) as u64,
        prev_chants.max(0) as u64,
        new_streak.max(0) as u64,
        None,
        existing_unlocked,
    );
    let now_ms = now.timestamp_millis() as f64;
    let _ = ach_sys.on_session_start(now_ms, 0);
    let _ = ach_sys.on_session_end(
        (duration_minutes as u64) * 60,
        chant_count as u64,
        sutra,
        now_ms,
    );
    let unlocked_ids = ach_sys.unlocked_list();
    let ach_json = serde_json::to_string(&unlocked_ids).unwrap_or_else(|_| "[]".to_string());

    conn.execute(
        r#"
        UPDATE fabushi_profiles
        SET total_practice_sessions = total_practice_sessions + 1,
            total_practice_minutes = total_practice_minutes + ?2,
            total_practice_chants = total_practice_chants + ?3,
            consecutive_days = ?4,
            last_practice_date = ?5,
            achievements_json = ?6,
            updated_at = ?7
        WHERE user_id = ?1
        "#,
        params![
            user_id,
            duration_minutes as i64,
            chant_count as i64,
            new_streak,
            record_date,
            ach_json,
            now_str,
        ],
    )?;

    let record = get_practice_record_by_id(conn, user_id, record_id)?.expect("inserted record exists");
    let practice_stats = compute_practice_stats(conn, user_id)?;
    let profile = get_profile(conn, user_id)?;

    Ok(PracticeSaveResult {
        record,
        practice_stats,
        profile,
        unlocked_achievement_ids: unlocked_ids,
    })
}

pub fn create_manual_practice_record(
    conn: &Connection,
    user_id: i64,
    sutra_name: &str,
    chant_count: u32,
    duration_minutes: u32,
    record_date: &str,
    local_time: &str,
    notes: Option<&str>,
) -> rusqlite::Result<(PracticeRecordDto, PracticeStatsDto)> {
    ensure_profile_row(conn, user_id)?;
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let date_val = if record_date.trim().is_empty() {
        now.format("%Y-%m-%d").to_string()
    } else {
        record_date.trim().to_string()
    };
    let time_val = if local_time.trim().is_empty() {
        now.format("%H:%M").to_string()
    } else {
        local_time.trim().to_string()
    };
    let clean_notes = notes.map(|s| s.trim()).filter(|s| !s.is_empty());

    conn.execute(
        r#"
        INSERT INTO fabushi_practice_records (
            user_id, sutra_name, sutra_source, duration_minutes, chant_count,
            record_date, local_time, is_manual, notes, created_at
        ) VALUES (?1, ?2, 'custom', ?3, ?4, ?5, ?6, 1, ?7, ?8)
        "#,
        params![
            user_id,
            sutra_name.trim(),
            duration_minutes as i64,
            chant_count as i64,
            date_val,
            time_val,
            clean_notes,
            now_str,
        ],
    )?;
    let record_id = conn.last_insert_rowid();

    recompute_profile_practice_totals(conn, user_id)?;
    let record = get_practice_record_by_id(conn, user_id, record_id)?.expect("created record exists");
    let stats = compute_practice_stats(conn, user_id)?;
    Ok((record, stats))
}

pub fn update_practice_record(
    conn: &Connection,
    user_id: i64,
    record_id: i64,
    sutra_name: &str,
    chant_count: u32,
    duration_minutes: u32,
    record_date: &str,
    local_time: &str,
    notes: Option<&str>,
) -> rusqlite::Result<Option<(PracticeRecordDto, PracticeStatsDto)>> {
    let clean_notes = notes.map(|s| s.trim()).filter(|s| !s.is_empty());
    let updated = conn.execute(
        r#"
        UPDATE fabushi_practice_records
        SET sutra_name = ?3,
            chant_count = ?4,
            duration_minutes = ?5,
            record_date = ?6,
            local_time = ?7,
            notes = ?8
        WHERE id = ?1 AND user_id = ?2
        "#,
        params![
            record_id,
            user_id,
            sutra_name.trim(),
            chant_count as i64,
            duration_minutes as i64,
            record_date.trim(),
            local_time.trim(),
            clean_notes,
        ],
    )?;
    if updated == 0 {
        return Ok(None);
    }
    recompute_profile_practice_totals(conn, user_id)?;
    let record = get_practice_record_by_id(conn, user_id, record_id)?;
    let stats = compute_practice_stats(conn, user_id)?;
    Ok(record.map(|r| (r, stats)))
}

pub fn delete_practice_record(
    conn: &Connection,
    user_id: i64,
    record_id: i64,
) -> rusqlite::Result<(bool, PracticeStatsDto)> {
    let deleted = conn.execute(
        "DELETE FROM fabushi_practice_records WHERE id = ?1 AND user_id = ?2",
        params![record_id, user_id],
    )?;
    recompute_profile_practice_totals(conn, user_id)?;
    let stats = compute_practice_stats(conn, user_id)?;
    Ok((deleted > 0, stats))
}

fn recompute_profile_practice_totals(conn: &Connection, user_id: i64) -> rusqlite::Result<()> {
    let (sessions, mins, chants): (i64, i64, i64) = conn.query_row(
        r#"
        SELECT COUNT(*), COALESCE(SUM(duration_minutes), 0), COALESCE(SUM(chant_count), 0)
        FROM fabushi_practice_records
        WHERE user_id = ?1
        "#,
        params![user_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let now_str = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE fabushi_profiles
        SET total_practice_sessions = ?2,
            total_practice_minutes = ?3,
            total_practice_chants = ?4,
            updated_at = ?5
        WHERE user_id = ?1
        "#,
        params![user_id, sessions, mins, chants, now_str],
    )?;
    Ok(())
}

fn get_practice_record_by_id(
    conn: &Connection,
    user_id: i64,
    record_id: i64,
) -> rusqlite::Result<Option<PracticeRecordDto>> {
    conn.query_row(
        r#"
        SELECT id, sutra_name, sutra_source, duration_minutes, chant_count,
               record_date, local_time, is_manual, notes, start_time, end_time, created_at
        FROM fabushi_practice_records
        WHERE id = ?1 AND user_id = ?2
        "#,
        params![record_id, user_id],
        row_to_practice_record,
    )
    .optional()
}

fn row_to_practice_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<PracticeRecordDto> {
    Ok(PracticeRecordDto {
        id: row.get(0)?,
        sutra_name: row.get(1)?,
        sutra_source: row.get(2)?,
        duration_minutes: row.get::<_, i64>(3)?.max(0) as u32,
        chant_count: row.get::<_, i64>(4)?.max(0) as u32,
        record_date: row.get(5)?,
        local_time: row.get(6)?,
        is_manual: row.get::<_, i64>(7)? != 0,
        notes: row.get(8)?,
        start_time: row.get(9)?,
        end_time: row.get(10)?,
        created_at: row.get(11)?,
    })
}

pub fn list_practice_records(
    conn: &Connection,
    user_id: i64,
    limit: usize,
    offset: usize,
) -> rusqlite::Result<Vec<PracticeRecordDto>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, sutra_name, sutra_source, duration_minutes, chant_count,
               record_date, local_time, is_manual, notes, start_time, end_time, created_at
        FROM fabushi_practice_records
        WHERE user_id = ?1
        ORDER BY record_date DESC, id DESC
        LIMIT ?2 OFFSET ?3
        "#,
    )?;
    let rows = stmt.query_map(
        params![user_id, limit as i64, offset as i64],
        row_to_practice_record,
    )?;
    rows.collect()
}

pub fn compute_practice_stats(conn: &Connection, user_id: i64) -> rusqlite::Result<PracticeStatsDto> {
    let today_str = Utc::now().format("%Y-%m-%d").to_string();
    let records = list_practice_records(conn, user_id, 500, 0)?;

    let mut today_sutra: Option<String> = None;
    let mut today_count = 0_u64;
    let mut today_duration = 0_u64;
    let mut total_count = 0_u64;
    let mut total_duration = 0_u64;
    let mut unique_days: BTreeSet<String> = BTreeSet::new();
    let mut by_subj: HashMap<String, (u64, u64, BTreeSet<String>)> = HashMap::new();

    for r in &records {
        total_count += r.chant_count as u64;
        total_duration += r.duration_minutes as u64;
        unique_days.insert(r.record_date.clone());
        if r.record_date == today_str {
            if today_sutra.is_none() {
                today_sutra = Some(r.sutra_name.clone());
            }
            today_count += r.chant_count as u64;
            today_duration += r.duration_minutes as u64;
        }
        let entry = by_subj
            .entry(r.sutra_name.clone())
            .or_insert_with(|| (0, 0, BTreeSet::new()));
        entry.0 += r.chant_count as u64;
        entry.1 += r.duration_minutes as u64;
        entry.2.insert(r.record_date.clone());
    }

    let mut by_subject: Vec<SubjectStatsDto> = by_subj
        .into_iter()
        .map(|(sutra_name, (count, duration_minutes, days))| SubjectStatsDto {
            sutra_name,
            count,
            duration_minutes,
            days: days.len() as u64,
        })
        .collect();
    by_subject.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.sutra_name.cmp(&b.sutra_name)));

    let consecutive_days: u64 = conn
        .query_row(
            "SELECT consecutive_days FROM fabushi_profiles WHERE user_id = ?1",
            params![user_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0)
        .max(0) as u64;

    Ok(PracticeStatsDto {
        today_sutra,
        today_count,
        today_duration_minutes: today_duration,
        total_records: records.len() as u64,
        total_count,
        total_duration_minutes: total_duration,
        total_days: unique_days.len() as u64,
        consecutive_days,
        by_subject,
    })
}

pub fn toggle_follow(
    conn: &Connection,
    follower_user_id: i64,
    target_username: &str,
) -> rusqlite::Result<FollowToggleResult> {
    let target = target_username.trim().to_string();
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM fabushi_follows WHERE follower_user_id = ?1 AND target_username = ?2",
            params![follower_user_id, target],
            |row| row.get(0),
        )
        .optional()?;

    let is_following = if exists.is_some() {
        conn.execute(
            "DELETE FROM fabushi_follows WHERE follower_user_id = ?1 AND target_username = ?2",
            params![follower_user_id, target],
        )?;
        false
    } else {
        let now_str = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO fabushi_follows (follower_user_id, target_username, created_at) VALUES (?1, ?2, ?3)",
            params![follower_user_id, target, now_str],
        )?;
        true
    };

    let follower_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM fabushi_follows WHERE target_username = ?1",
        params![target],
        |row| row.get(0),
    )?;

    Ok(FollowToggleResult {
        target_username: target,
        is_following,
        follower_count: follower_count.max(0) as u64,
    })
}

fn query_leaderboard(
    conn: &Connection,
    viewer_user_id: i64,
    limit: usize,
    order_by_practice: bool,
) -> rusqlite::Result<Vec<LeaderboardEntryDto>> {
    let order_clause = if order_by_practice {
        "p.total_practice_minutes DESC, p.total_practice_chants DESC, p.user_id ASC"
    } else {
        "p.total_transfer_bytes DESC, p.total_practice_minutes DESC, p.user_id ASC"
    };
    let sql = format!(
        r#"
        SELECT
            p.user_id, p.username, p.display_name, p.avatar_url,
            p.total_transfer_bytes, p.total_practice_sessions,
            p.total_practice_chants, p.total_practice_minutes,
            p.consecutive_days, p.locked_practice_title, p.last_practice_date,
            (SELECT COUNT(*) FROM fabushi_follows f WHERE f.target_username = p.username) AS follower_count,
            (SELECT COUNT(*) FROM fabushi_follows f WHERE f.follower_user_id = p.user_id) AS following_count,
            EXISTS(SELECT 1 FROM fabushi_follows f WHERE f.follower_user_id = ?1 AND f.target_username = p.username) AS is_following
        FROM fabushi_profiles p
        ORDER BY {order_clause}
        LIMIT ?2
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![viewer_user_id, limit as i64], |row| {
        let uid: i64 = row.get(0)?;
        let avatar_raw: String = row.get(3)?;
        let total_bytes = row.get::<_, i64>(4)?.max(0) as u64;
        Ok(LeaderboardEntryDto {
            user_id: uid,
            username: row.get(1)?,
            display_name: row.get(2)?,
            avatar: if avatar_raw.is_empty() {
                None
            } else {
                Some(avatar_raw)
            },
            total_bytes,
            total_bytes_formatted: format_leaderboard_bytes(total_bytes),
            total_records: row.get::<_, i64>(5)?.max(0) as u64,
            total_count: row.get::<_, i64>(6)?.max(0) as u64,
            total_duration: row.get::<_, i64>(7)?.max(0) as u64,
            total_days: row.get::<_, i64>(8)?.max(0) as u64,
            latest_sutra: row.get(9)?,
            latest_record_date: row.get(10)?,
            rank: 0,
            follower_count: row.get::<_, i64>(11)?.max(0) as u64,
            following_count: row.get::<_, i64>(12)?.max(0) as u64,
            is_following: row.get::<_, i64>(13)? != 0,
            is_self: uid == viewer_user_id,
        })
    })?;

    let mut out = Vec::new();
    for (idx, item) in rows.enumerate() {
        let mut entry = item?;
        entry.rank = (idx + 1) as u32;
        out.push(entry);
    }
    Ok(out)
}

pub fn get_global_leaderboard(
    conn: &Connection,
    viewer_user_id: i64,
    limit: usize,
) -> rusqlite::Result<Vec<LeaderboardEntryDto>> {
    query_leaderboard(conn, viewer_user_id, limit, false)
}

pub fn get_practice_leaderboard(
    conn: &Connection,
    viewer_user_id: i64,
    limit: usize,
) -> rusqlite::Result<Vec<LeaderboardEntryDto>> {
    query_leaderboard(conn, viewer_user_id, limit, true)
}

pub fn upsert_online_heartbeat(
    conn: &Connection,
    user_id: i64,
    activity_type: &str,
) -> rusqlite::Result<u64> {
    let now_secs = Utc::now().timestamp();
    let cutoff = now_secs - 120;
    let activity = if activity_type.trim() == "zen_room" {
        "zen_room"
    } else {
        "global_sending"
    };
    let key = format!("{user_id}:{activity}");

    conn.execute(
        "DELETE FROM fabushi_online_heartbeats WHERE last_seen_epoch_secs < ?1",
        params![cutoff],
    )?;
    conn.execute(
        r#"
        INSERT INTO fabushi_online_heartbeats (session_key, user_id, activity_type, last_seen_epoch_secs)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(session_key) DO UPDATE SET last_seen_epoch_secs = excluded.last_seen_epoch_secs
        "#,
        params![key, user_id, activity, now_secs],
    )?;

    get_online_count(conn, activity)
}

pub fn get_online_count(conn: &Connection, activity_type: &str) -> rusqlite::Result<u64> {
    let cutoff = Utc::now().timestamp() - 120;
    let activity = if activity_type.trim() == "zen_room" {
        "zen_room"
    } else {
        "global_sending"
    };
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM fabushi_online_heartbeats WHERE activity_type = ?1 AND last_seen_epoch_secs >= ?2",
        params![activity, cutoff],
        |row| row.get(0),
    )?;
    Ok((count.max(0) as u64).max(1))
}

pub fn get_bootstrap_bundle(
    conn: &Connection,
    user_id: i64,
    username: &str,
    display_name: &str,
    avatar_url: &str,
    is_admin: bool,
) -> rusqlite::Result<BootstrapBundle> {
    let profile = ensure_profile(conn, user_id, username, display_name, avatar_url, is_admin)?;
    let locked_practice = get_locked_practice(conn, user_id)?;
    let leaderboard = get_global_leaderboard(conn, user_id, 50)?;
    let practice_leaderboard = get_practice_leaderboard(conn, user_id, 50)?;
    let practice_records = list_practice_records(conn, user_id, 50, 0)?;
    let practice_stats = compute_practice_stats(conn, user_id)?;
    let unlocked_achievement_ids = get_unlocked_achievements(conn, user_id)?;
    let online_global_sending_count = upsert_online_heartbeat(conn, user_id, "global_sending")?;
    let online_zen_room_count = get_online_count(conn, "zen_room")?;

    Ok(BootstrapBundle {
        profile,
        locked_practice,
        leaderboard,
        practice_leaderboard,
        practice_records,
        practice_stats,
        unlocked_achievement_ids,
        online_global_sending_count,
        online_zen_room_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_schema_crud_leaderboard_and_follows_work() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        init_fabushi_tables(&conn).expect("init tables");

        // Idempotent re-init
        init_fabushi_tables(&conn).expect("re-init idempotent");

        let bundle = get_bootstrap_bundle(&conn, 100_001, "alice", "Alice", "", false)
            .expect("bootstrap bundle");
        assert_eq!(bundle.profile.username, "alice");
        assert!(bundle.leaderboard.len() >= 8);
        assert_eq!(bundle.leaderboard[0].rank, 1);
        assert_eq!(bundle.leaderboard[0].username, "lotus_master_bj");

        // Lock practice (一门深入)
        let lp = LockedPractice {
            title: "金刚经".into(),
            category: "般若部".into(),
            file_path: "sutra:diamond_sutra".into(),
            selected_at_ms: 1000,
            is_locked: true,
        };
        let (prof, locked) = update_profile(&conn, 100_001, Some("慧明"), Some("精进修行"), Some(&lp), Some("zh-CN"))
            .expect("update profile");
        assert_eq!(prof.display_name, "慧明");
        assert_eq!(locked.as_ref().unwrap().title, "金刚经");

        // Attempt overwrite of locked practice should preserve original
        let lp2 = LockedPractice {
            title: "心经".into(),
            category: "般若部".into(),
            file_path: "sutra:heart_sutra".into(),
            selected_at_ms: 2000,
            is_locked: true,
        };
        let (_, locked2) = update_profile(&conn, 100_001, None, None, Some(&lp2), None).expect("lock stays");
        assert_eq!(locked2.unwrap().title, "金刚经");

        // Record transfer
        let tx = record_transfer(&conn, 100_001, "心经", "佛典经文", &["CN".into(), "JP".into()], 2_500_000, 2)
            .expect("record transfer");
        assert_eq!(tx.total_transfer_bytes, 2_500_000);
        assert_eq!(tx.total_transfer_count, 1);

        // Record practice session + manual record + update + delete
        let saved = record_practice_session(&conn, 100_001, "金刚经", 35, 108, Some(1000), Some(2000), Some("清净心"))
            .expect("save practice session");
        assert!(saved.unlocked_achievement_ids.contains(&"first_session".to_string()));
        assert!(saved.unlocked_achievement_ids.contains(&"chant_108".to_string()));

        let (manual, stats) = create_manual_practice_record(
            &conn,
            100_001,
            "心经",
            10,
            15,
            "2026-11-16",
            "08:00",
            Some("补录早课"),
        )
        .expect("create manual");
        assert_eq!(stats.total_records, 2);
        assert_eq!(stats.total_count, 118);

        let updated = update_practice_record(
            &conn,
            100_001,
            manual.id,
            "心经",
            20,
            25,
            "2026-11-16",
            "08:30",
            Some("更新心得"),
        )
        .expect("update manual")
        .unwrap();
        assert_eq!(updated.0.chant_count, 20);
        assert_eq!(updated.1.total_count, 128);

        let (deleted, stats_after) = delete_practice_record(&conn, 100_001, manual.id).expect("delete manual");
        assert!(deleted);
        assert_eq!(stats_after.total_records, 1);

        // Follow / Unfollow
        let f1 = toggle_follow(&conn, 100_001, "lotus_master_bj").expect("follow");
        assert!(f1.is_following);
        assert_eq!(f1.follower_count, 1);
        let f2 = toggle_follow(&conn, 100_001, "lotus_master_bj").expect("unfollow");
        assert!(!f2.is_following);
        assert_eq!(f2.follower_count, 0);

        // Membership unlock
        let vip_prof = activate_membership(&conn, 100_001, "yearly").expect("activate vip");
        assert!(vip_prof.is_vip);
        assert_eq!(vip_prof.vip_plan, "yearly");
        assert!(vip_prof.vip_expires_at.is_some());

        // Online heartbeat
        let online = upsert_online_heartbeat(&conn, 100_001, "zen_room").expect("heartbeat");
        assert!(online >= 1);

        // Direct mutations on a fresh user_id without calling bootstrap first
        let fresh_tx = record_transfer(&conn, 200_002, "金刚经", "佛典经文", &["US".into()], 4096, 1)
            .expect("direct transfer for fresh user");
        assert_eq!(fresh_tx.total_transfer_bytes, 4096);
        let fresh_vip = activate_membership(&conn, 200_003, "monthly").expect("direct vip");
        assert!(fresh_vip.is_vip);
    }
}
