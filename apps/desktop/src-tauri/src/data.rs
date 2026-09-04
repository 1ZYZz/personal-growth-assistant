use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::Ordering,
};

use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, SqlitePool};
use tauri::State;

use crate::{core::CoreStore, product::AppState};

const DATABASE_NAME: &str = "personal-growth.sqlite3";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub created_at_utc: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataExportInfo {
    pub path: String,
    pub exported_at_utc: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePreferencesInput {
    pub theme: String,
    pub week_starts_on: String,
    pub clock_format: String,
    pub close_to_tray: bool,
    pub notifications_enabled: bool,
    pub app_timezone: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct Preferences {
    pub theme: String,
    pub week_starts_on: String,
    pub clock_format: String,
    pub close_to_tray: bool,
    pub notifications_enabled: bool,
    pub app_timezone: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "system".to_owned(),
            week_starts_on: "monday".to_owned(),
            clock_format: "24h".to_owned(),
            close_to_tray: true,
            notifications_enabled: true,
            app_timezone: "Asia/Shanghai".to_owned(),
        }
    }
}

#[tauri::command]
pub async fn create_backup(state: State<'_, AppState>) -> Result<BackupInfo, String> {
    create_consistent_backup(&state.store, Path::new(&state.data_directory), "manual").await
}

#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupInfo>, String> {
    backup_inventory(Path::new(&state.data_directory))
}

#[tauri::command]
pub async fn restore_backup(state: State<'_, AppState>, name: String) -> Result<String, String> {
    let data_directory = Path::new(&state.data_directory);
    let backup_directory = data_directory.join("backups");
    let safe_name = Path::new(&name)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| *value == name && value.ends_with(".sqlite3"))
        .ok_or_else(|| "备份文件名无效".to_owned())?;
    let source = backup_directory.join(safe_name);
    if !source.is_file() {
        return Err("备份文件不存在".to_owned());
    }
    validate_database(&source).await?;
    create_consistent_backup(&state.store, data_directory, "before-restore").await?;
    let pending = data_directory.join("restore-pending.sqlite3");
    if pending.exists() {
        fs::remove_file(&pending).map_err(io_error)?;
    }
    fs::copy(&source, &pending).map_err(io_error)?;
    fs::write(
        data_directory.join("restore-pending.txt"),
        format!("{}\n{}\n", safe_name, utc_text(Utc::now())),
    )
    .map_err(io_error)?;
    Ok("restart_required".to_owned())
}

#[tauri::command]
pub async fn export_data(state: State<'_, AppState>) -> Result<DataExportInfo, String> {
    let exported_at = Utc::now();
    let export_directory = Path::new(&state.data_directory).join("exports");
    fs::create_dir_all(&export_directory).map_err(io_error)?;
    let file_name = format!("pga-export-{}.json", exported_at.format("%Y%m%d-%H%M%S"));
    let path = export_directory.join(file_name);
    let table_queries: [(&'static str, &'static str); 23] = [
        ("settings", "SELECT COUNT(*) FROM settings"),
        ("tasks", "SELECT COUNT(*) FROM tasks"),
        ("task_events", "SELECT COUNT(*) FROM task_events"),
        (
            "recurrence_series",
            "SELECT COUNT(*) FROM recurrence_series",
        ),
        (
            "occurrence_overrides",
            "SELECT COUNT(*) FROM occurrence_overrides",
        ),
        ("reminder_rules", "SELECT COUNT(*) FROM reminder_rules"),
        ("watch_fields", "SELECT COUNT(*) FROM watch_fields"),
        ("watch_sources", "SELECT COUNT(*) FROM watch_sources"),
        ("news_items", "SELECT COUNT(*) FROM news_items"),
        ("learning_goals", "SELECT COUNT(*) FROM learning_goals"),
        ("knowledge_nodes", "SELECT COUNT(*) FROM knowledge_nodes"),
        (
            "learning_resources",
            "SELECT COUNT(*) FROM learning_resources",
        ),
        ("quiz_items", "SELECT COUNT(*) FROM quiz_items"),
        ("quiz_attempts", "SELECT COUNT(*) FROM quiz_attempts"),
        ("mistake_book", "SELECT COUNT(*) FROM mistake_book"),
        ("fsrs_cards", "SELECT COUNT(*) FROM fsrs_cards"),
        ("review_logs", "SELECT COUNT(*) FROM review_logs"),
        ("mastery_evidence", "SELECT COUNT(*) FROM mastery_evidence"),
        ("digests", "SELECT COUNT(*) FROM digests"),
        ("lesson_plans", "SELECT COUNT(*) FROM lesson_plans"),
        (
            "lesson_plan_items",
            "SELECT COUNT(*) FROM lesson_plan_items",
        ),
        ("ingestion_runs", "SELECT COUNT(*) FROM ingestion_runs"),
        ("app_logs", "SELECT COUNT(*) FROM app_logs"),
    ];
    let mut counts = serde_json::Map::new();
    for (table, query) in table_queries {
        let count: i64 = sqlx::query_scalar(query)
            .fetch_one(&state.store.pool)
            .await
            .map_err(db_error)?;
        counts.insert(table.to_owned(), count.into());
    }
    let tasks: String = sqlx::query_scalar(
        "SELECT COALESCE(json_group_array(json_object(
            'id', id, 'seriesId', series_id, 'occurrenceKey', occurrence_key,
            'title', title, 'notes', notes, 'priority', priority,
            'estimatedMinutes', estimated_minutes, 'status', status,
            'timeMode', time_mode, 'scheduledLocal', scheduled_local,
            'scheduledDate', scheduled_date, 'scheduledUtc', scheduled_utc,
            'tzid', tzid, 'progressPercent', progress_percent,
            'progressNote', progress_note, 'version', version,
            'createdAtUtc', created_at_utc, 'updatedAtUtc', updated_at_utc,
            'deletedAtUtc', deleted_at_utc
        )), '[]') FROM tasks",
    )
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let series: String = sqlx::query_scalar(
        "SELECT COALESCE(json_group_array(json_object(
            'id', id, 'title', title, 'notes', notes, 'priority', priority,
            'estimatedMinutes', estimated_minutes, 'timeMode', time_mode,
            'dtstartLocal', dtstart_local, 'tzid', tzid, 'rrule', rrule_text,
            'rdates', json(rdates_json), 'exdates', json(exdates_json),
            'version', series_version, 'deletedAtUtc', deleted_at_utc
        )), '[]') FROM recurrence_series",
    )
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let watch_fields: String = sqlx::query_scalar(
        "SELECT COALESCE(json_group_array(json_object(
            'id', id, 'name', name, 'description', description,
            'includeTerms', json(include_terms_json), 'excludeTerms', json(exclude_terms_json),
            'regions', json(regions_json), 'languages', json(languages_json),
            'maxItems', max_items, 'readingMinutes', reading_minutes,
            'relevanceWeight', relevance_weight, 'authorityWeight', authority_weight,
            'recencyWeight', recency_weight, 'heatWeight', heat_weight,
            'breakingAlerts', breaking_alerts, 'enabled', enabled, 'version', version,
            'deletedAtUtc', deleted_at_utc
        )), '[]') FROM watch_fields",
    )
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let watch_sources: String = sqlx::query_scalar(
        "SELECT COALESCE(json_group_array(json_object(
            'id', id, 'fieldId', field_id, 'name', name, 'url', url,
            'sourceType', source_type, 'selector', selector, 'authority', authority,
            'isPrimary', is_primary, 'enabled', enabled, 'sortOrder', sort_order,
            'version', version, 'deletedAtUtc', deleted_at_utc
        )), '[]') FROM watch_sources",
    )
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let news_items: String = sqlx::query_scalar(
        "SELECT COALESCE(json_group_array(json_object(
            'id', id, 'fieldId', field_id, 'sourceId', source_id,
            'canonicalUrl', canonical_url, 'title', title, 'summary', summary,
            'publishedAtUtc', published_at_utc, 'eventDate', event_date,
            'informationKind', information_kind, 'score', score,
            'uncertainty', uncertainty, 'isRead', is_read, 'isSaved', is_saved,
            'feedback', feedback, 'createdAtUtc', created_at_utc
        )), '[]') FROM news_items",
    )
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let learning_goals: String = sqlx::query_scalar(
        "SELECT COALESCE(json_group_array(json_object(
            'id', id, 'name', name, 'purpose', purpose, 'currentLevel', current_level,
            'targetLevel', target_level, 'targetDate', target_date,
            'weeklyMinutes', weekly_minutes, 'dailyMinutes', daily_minutes,
            'language', language, 'resourcePreferences', json(resource_preferences_json),
            'budgetMode', budget_mode, 'autoAddLessons', auto_add_lessons,
            'status', status, 'version', version, 'deletedAtUtc', deleted_at_utc
        )), '[]') FROM learning_goals",
    )
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let knowledge_nodes: String = sqlx::query_scalar(
        "SELECT COALESCE(json_group_array(json_object(
            'id', id, 'goalId', goal_id, 'name', name,
            'plainExplanation', plain_explanation, 'stageOutcome', stage_outcome,
            'estimatedMinutes', estimated_minutes, 'status', status,
            'masteryScore', mastery_score, 'sortOrder', sort_order,
            'version', version, 'deletedAtUtc', deleted_at_utc
        )), '[]') FROM knowledge_nodes",
    )
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let learning_resources: String = sqlx::query_scalar(
        "SELECT COALESCE(json_group_array(json_object(
            'id', id, 'nodeId', node_id, 'title', title, 'author', author,
            'publishedDate', published_date, 'resourceType', resource_type,
            'language', language, 'durationMinutes', duration_minutes,
            'difficulty', difficulty, 'cost', cost, 'versionFit', version_fit,
            'staleRisk', stale_risk, 'url', url,
            'recommendationReason', recommendation_reason
        )), '[]') FROM learning_resources",
    )
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let digests: String = sqlx::query_scalar(
        "SELECT COALESCE(json_group_array(json_object(
            'id', id, 'digestType', digest_type, 'periodStart', period_start,
            'periodEnd', period_end, 'content', json(content_json),
            'generator', generator, 'createdAtUtc', created_at_utc
        )), '[]') FROM digests",
    )
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let manifest = serde_json::json!({
        "format": "pga-portable-export",
        "formatVersion": 1,
        "appVersion": env!("CARGO_PKG_VERSION"),
        "exportedAtUtc": utc_text(exported_at),
        "databaseEngine": "SQLite",
        "counts": counts,
        "data": {
            "tasks": serde_json::from_str::<serde_json::Value>(&tasks).map_err(json_error)?,
            "recurrenceSeries": serde_json::from_str::<serde_json::Value>(&series).map_err(json_error)?,
            "watchFields": serde_json::from_str::<serde_json::Value>(&watch_fields).map_err(json_error)?,
            "watchSources": serde_json::from_str::<serde_json::Value>(&watch_sources).map_err(json_error)?,
            "newsItems": serde_json::from_str::<serde_json::Value>(&news_items).map_err(json_error)?,
            "learningGoals": serde_json::from_str::<serde_json::Value>(&learning_goals).map_err(json_error)?,
            "knowledgeNodes": serde_json::from_str::<serde_json::Value>(&knowledge_nodes).map_err(json_error)?,
            "learningResources": serde_json::from_str::<serde_json::Value>(&learning_resources).map_err(json_error)?,
            "digests": serde_json::from_str::<serde_json::Value>(&digests).map_err(json_error)?,
        },
        "excluded": ["credentials", "approvalTokens", "notificationNonces", "aiPrompts"]
    });
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(json_error)?;
    fs::write(&path, bytes).map_err(io_error)?;
    Ok(DataExportInfo {
        path: path.to_string_lossy().into_owned(),
        exported_at_utc: utc_text(exported_at),
    })
}

#[tauri::command]
pub async fn get_preferences(state: State<'_, AppState>) -> Result<Preferences, String> {
    load_preferences(&state.store).await
}

#[tauri::command]
pub async fn get_startup_enabled() -> Result<bool, String> {
    startup_registry_command("query", None).await
}

#[tauri::command]
pub async fn set_startup_enabled(enabled: bool) -> Result<bool, String> {
    let executable = enabled
        .then(env::current_exe)
        .transpose()
        .map_err(io_error)?;
    startup_registry_command(
        if enabled { "enable" } else { "disable" },
        executable.as_deref(),
    )
    .await?;
    get_startup_enabled().await
}

#[cfg(windows)]
async fn startup_registry_command(action: &str, executable: Option<&Path>) -> Result<bool, String> {
    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "PersonalGrowthAssistant";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let system_root = env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let reg = PathBuf::from(system_root).join("System32").join("reg.exe");
    let mut command = tokio::process::Command::new(reg);
    match action {
        "query" => {
            command.args(["query", RUN_KEY, "/v", VALUE_NAME]);
        }
        "enable" => {
            let executable = executable.ok_or_else(|| "缺少应用程序路径".to_owned())?;
            let value = format!("\"{}\"", executable.display());
            command.args([
                "add", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d", &value, "/f",
            ]);
        }
        "disable" => {
            command.args(["delete", RUN_KEY, "/v", VALUE_NAME, "/f"]);
        }
        _ => return Err("开机启动操作无效".to_owned()),
    }
    let status = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .await
        .map_err(io_error)?;
    if action == "query" {
        return Ok(status.success());
    }
    if action == "disable" && !status.success() {
        return Ok(false);
    }
    if !status.success() {
        return Err("Windows 开机启动设置失败".to_owned());
    }
    Ok(action == "enable")
}

#[cfg(not(windows))]
async fn startup_registry_command(
    _action: &str,
    _executable: Option<&Path>,
) -> Result<bool, String> {
    Err("开机启动仅支持 Windows".to_owned())
}

#[tauri::command]
pub fn third_party_notices() -> String {
    include_str!("../../../../THIRD_PARTY_NOTICES.md").to_owned()
}

pub async fn load_preferences(store: &CoreStore) -> Result<Preferences, String> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT value_json FROM settings WHERE key = 'preferences'")
            .fetch_optional(&store.pool)
            .await
            .map_err(db_error)?;
    value
        .map(|raw| serde_json::from_str(&raw).map_err(json_error))
        .unwrap_or_else(|| Ok(Preferences::default()))
}

#[tauri::command]
pub async fn save_preferences(
    state: State<'_, AppState>,
    input: SavePreferencesInput,
) -> Result<Preferences, String> {
    if !matches!(input.theme.as_str(), "system" | "light" | "dark")
        || !matches!(input.week_starts_on.as_str(), "monday" | "sunday")
        || !matches!(input.clock_format.as_str(), "24h" | "12h")
        || input.app_timezone.parse::<chrono_tz::Tz>().is_err()
    {
        return Err("界面偏好设置无效".to_owned());
    }
    let preferences = Preferences {
        theme: input.theme,
        week_starts_on: input.week_starts_on,
        clock_format: input.clock_format,
        close_to_tray: input.close_to_tray,
        notifications_enabled: input.notifications_enabled,
        app_timezone: input.app_timezone,
    };
    let now = utc_text(Utc::now());
    let value = serde_json::to_string(&preferences).map_err(json_error)?;
    sqlx::query(
        "INSERT INTO settings(key, value_json, updated_at_utc, version)
         VALUES ('preferences', ?, ?, 1)
         ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at_utc = excluded.updated_at_utc,
            version = settings.version + 1",
    )
    .bind(value)
    .bind(&now)
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO user_profile(id, app_timezone, created_at_utc, updated_at_utc)
         VALUES ('default', ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            app_timezone = excluded.app_timezone,
            updated_at_utc = excluded.updated_at_utc,
            version = user_profile.version + 1",
    )
    .bind(&preferences.app_timezone)
    .bind(&now)
    .bind(&now)
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?;
    state
        .close_to_tray
        .store(preferences.close_to_tray, Ordering::Relaxed);
    state.scheduler_wake.send_modify(|version| *version += 1);
    Ok(preferences)
}

pub async fn create_daily_backup(store: &CoreStore, data_directory: &Path) -> Result<(), String> {
    let today = Utc::now().date_naive();
    if backup_inventory(data_directory)?.iter().any(|backup| {
        backup
            .name
            .starts_with(&format!("daily-{}", today.format("%Y%m%d")))
    }) {
        return Ok(());
    }
    create_consistent_backup(store, data_directory, "daily").await?;
    prune_backups(data_directory, Utc::now())
}

pub fn apply_pending_restore(data_directory: &Path) -> Result<(), String> {
    let pending = data_directory.join("restore-pending.sqlite3");
    if !pending.is_file() {
        return Ok(());
    }
    let database = data_directory.join(DATABASE_NAME);
    if database.is_file() {
        let rollback_directory = data_directory.join("backups");
        fs::create_dir_all(&rollback_directory).map_err(io_error)?;
        let rollback = rollback_directory.join(format!(
            "replaced-{}.sqlite3",
            Utc::now().format("%Y%m%d-%H%M%S")
        ));
        fs::rename(&database, rollback).map_err(io_error)?;
    }
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", database.to_string_lossy()));
        if sidecar.is_file() {
            fs::remove_file(sidecar).map_err(io_error)?;
        }
    }
    fs::rename(pending, database).map_err(io_error)?;
    let marker = data_directory.join("restore-pending.txt");
    if marker.is_file() {
        fs::remove_file(marker).map_err(io_error)?;
    }
    Ok(())
}

async fn create_consistent_backup(
    store: &CoreStore,
    data_directory: &Path,
    kind: &str,
) -> Result<BackupInfo, String> {
    let now = Utc::now();
    let backup_directory = data_directory.join("backups");
    fs::create_dir_all(&backup_directory).map_err(io_error)?;
    let name = format!("{kind}-{}.sqlite3", now.format("%Y%m%d-%H%M%S-%3f"));
    let path = backup_directory.join(&name);
    sqlx::query("VACUUM INTO ?")
        .bind(path.to_string_lossy().into_owned())
        .execute(&store.pool)
        .await
        .map_err(db_error)?;
    validate_database(&path).await?;
    let size_bytes = fs::metadata(&path).map_err(io_error)?.len();
    Ok(BackupInfo {
        name,
        created_at_utc: utc_text(now),
        size_bytes,
    })
}

async fn validate_database(path: &Path) -> Result<(), String> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .disable_statement_logging();
    let pool = SqlitePool::connect_with(options).await.map_err(db_error)?;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .map_err(db_error)?;
    let schema_version: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_migrations")
            .fetch_one(&pool)
            .await
            .map_err(db_error)?;
    pool.close().await;
    if integrity != "ok" || schema_version < 1 {
        return Err("备份完整性校验失败".to_owned());
    }
    Ok(())
}

fn backup_inventory(data_directory: &Path) -> Result<Vec<BackupInfo>, String> {
    let directory = data_directory.join("backups");
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut backups = fs::read_dir(directory)
        .map_err(io_error)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_owned();
            if !name.ends_with(".sqlite3") {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            let modified = metadata.modified().ok()?;
            Some(BackupInfo {
                name,
                created_at_utc: utc_text(DateTime::<Utc>::from(modified)),
                size_bytes: metadata.len(),
            })
        })
        .collect::<Vec<_>>();
    backups.sort_by(|left, right| right.created_at_utc.cmp(&left.created_at_utc));
    Ok(backups)
}

fn prune_backups(data_directory: &Path, now: DateTime<Utc>) -> Result<(), String> {
    let directory = data_directory.join("backups");
    let backups = backup_inventory(data_directory)?;
    let mut daily_kept = 0_u8;
    let mut weekly_kept = 0_u8;
    for backup in backups {
        if !backup.name.starts_with("daily-") {
            continue;
        }
        let keep = backup
            .name
            .get(6..14)
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y%m%d").ok())
            .is_some_and(|date| {
                let age = now.date_naive() - date;
                if age < Duration::days(7) && daily_kept < 7 {
                    daily_kept += 1;
                    true
                } else if age < Duration::days(35) && weekly_kept < 4 {
                    weekly_kept += 1;
                    true
                } else {
                    false
                }
            });
        if !keep {
            let target = directory.join(&backup.name);
            if target.is_file() {
                fs::remove_file(target).map_err(io_error)?;
            }
        }
    }
    Ok(())
}

fn utc_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn db_error(error: sqlx::Error) -> String {
    format!("本地数据库操作失败：{error}")
}

fn io_error(error: std::io::Error) -> String {
    format!("本地文件操作失败：{error}")
}

fn json_error(error: serde_json::Error) -> String {
    format!("数据格式错误：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_names_cannot_escape_the_backup_directory() {
        for invalid in ["../other.sqlite3", "C:\\tmp\\other.sqlite3", "not-a-db"] {
            let candidate = Path::new(invalid)
                .file_name()
                .and_then(|value| value.to_str());
            assert!(candidate != Some(invalid) || !invalid.ends_with(".sqlite3"));
        }
    }

    #[test]
    fn older_preferences_receive_a_safe_timezone_default() {
        let preferences: Preferences = serde_json::from_str(
            r#"{"theme":"dark","weekStartsOn":"monday","clockFormat":"24h","closeToTray":true,"notificationsEnabled":true}"#,
        )
        .unwrap();
        assert_eq!(preferences.app_timezone, "Asia/Shanghai");
    }
}
