use std::{
    collections::HashSet,
    sync::{atomic::AtomicBool, Arc},
};

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Acquire, Row, Sqlite, Transaction};
use tauri::State;
use uuid::Uuid;

use crate::core::{
    expand_virtual, project_from_key, resolve_zoned_local, CoreStore, MaterializationInput,
    OccurrenceKey, RecurrenceDefinition, SeriesStart, TimeMode,
};

pub struct AppState {
    pub store: CoreStore,
    pub data_directory: String,
    pub scheduler_wake: tokio::sync::watch::Sender<u64>,
    pub close_to_tray: Arc<AtomicBool>,
    pub agent_cancel: tokio::sync::Mutex<Option<tokio::sync::watch::Sender<bool>>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub title: String,
    pub notes: Option<String>,
    pub date: String,
    pub time: Option<String>,
    pub time_mode: Option<String>,
    pub tzid: Option<String>,
    pub priority: String,
    pub estimated_minutes: Option<i64>,
    pub reminder_minutes_before: Option<i64>,
    pub completion_criteria: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecurringTaskInput {
    pub title: String,
    pub notes: Option<String>,
    pub start_date: String,
    pub time: Option<String>,
    pub time_mode: Option<String>,
    pub tzid: Option<String>,
    pub priority: String,
    pub estimated_minutes: Option<i64>,
    pub rrule: String,
    pub reminder_minutes_before: Option<i64>,
    pub completion_criteria: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskQuery {
    pub range_start: String,
    pub range_end: String,
    #[serde(default)]
    pub include_completed: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusInput {
    pub task_id: Option<String>,
    pub series_id: Option<String>,
    pub occurrence_key: Option<String>,
    pub status: String,
    pub expected_version: Option<i64>,
    pub blocked_reason: Option<String>,
    pub snoozed_minutes: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskInput {
    pub task_id: Option<String>,
    pub series_id: Option<String>,
    pub occurrence_key: Option<String>,
    pub expected_version: Option<i64>,
    pub title: String,
    pub notes: Option<String>,
    pub date: String,
    pub time: Option<String>,
    pub time_mode: Option<String>,
    pub tzid: Option<String>,
    pub priority: String,
    pub estimated_minutes: Option<i64>,
    pub reminder_minutes_before: Option<i64>,
    #[serde(default)]
    pub preserve_reminder: bool,
    pub completion_criteria: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecurringSeriesInput {
    pub task_id: Option<String>,
    pub series_id: String,
    pub occurrence_key: String,
    pub expected_version: Option<i64>,
    pub expected_series_version: i64,
    pub edit_scope: String,
    pub title: String,
    pub notes: Option<String>,
    pub date: String,
    pub time: Option<String>,
    pub time_mode: Option<String>,
    pub tzid: Option<String>,
    pub priority: String,
    pub estimated_minutes: Option<i64>,
    pub reminder_minutes_before: Option<i64>,
    pub completion_criteria: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTargetInput {
    pub task_id: Option<String>,
    pub series_id: Option<String>,
    pub occurrence_key: Option<String>,
    pub expected_version: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewProgressInput {
    pub task_id: Option<String>,
    pub series_id: Option<String>,
    pub occurrence_key: Option<String>,
    pub expected_version: Option<i64>,
    pub progress_percent: Option<i64>,
    pub progress_note: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOutcomeInput {
    pub task_id: Option<String>,
    pub series_id: Option<String>,
    pub occurrence_key: Option<String>,
    pub expected_version: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub id: Option<String>,
    pub series_id: Option<String>,
    pub occurrence_key: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    pub priority: String,
    pub estimated_minutes: Option<i64>,
    #[serde(default)]
    pub completion_criteria: Option<String>,
    pub status: String,
    pub time_mode: String,
    pub scheduled_date: Option<String>,
    pub scheduled_local: Option<String>,
    pub scheduled_utc: Option<String>,
    pub tzid: Option<String>,
    pub dst_adjusted: bool,
    pub is_virtual: bool,
    pub version: i64,
    #[serde(default)]
    pub series_version: Option<i64>,
    pub progress_percent: Option<i64>,
    pub progress_note: Option<String>,
    pub blocked_reason: Option<String>,
    pub snoozed_until_utc: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashItem {
    pub id: String,
    pub deleted_at_utc: String,
    pub purge_after_utc: String,
    pub task: TaskItem,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummary {
    pub total: usize,
    pub completed: usize,
    pub remaining: usize,
    pub focus_minutes: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub app_version: &'static str,
    pub data_directory: String,
    pub database_engine: &'static str,
    pub recurrence_engine: &'static str,
    pub timezone_database: &'static str,
    pub agent_bridge: String,
}

#[derive(Clone, Debug)]
struct StoredSeries {
    definition: RecurrenceDefinition,
    title: String,
    notes: Option<String>,
    priority: String,
    estimated_minutes: Option<i64>,
    completion_criteria: Option<String>,
    series_version: i64,
    ends_before_local: Option<NaiveDateTime>,
}

impl StoredSeries {
    fn includes(&self, original_local: NaiveDateTime) -> bool {
        self.ends_before_local
            .is_none_or(|cutoff| original_local < cutoff)
    }
}

#[tauri::command]
pub async fn create_task(
    state: State<'_, AppState>,
    input: CreateTaskInput,
) -> Result<TaskItem, String> {
    validate_title_priority(&input.title, &input.priority, input.estimated_minutes)?;
    validate_completion_criteria(input.completion_criteria.as_deref())?;
    validate_reminder(input.reminder_minutes_before, input.time.as_deref())?;
    let date = parse_date(&input.date)?;
    let task_id = Uuid::now_v7();
    let now = utc_text(Utc::now());
    let (mode, scheduled_local, scheduled_date, scheduled_utc, tzid, chosen_offset, adjusted) =
        task_time_columns(
            date,
            input.time.as_deref(),
            input.time_mode.as_deref(),
            input.tzid.as_deref(),
        )?;

    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    sqlx::query(
        "INSERT INTO tasks(
            id, title, notes, priority, estimated_minutes, completion_criteria, status, time_mode,
            scheduled_local, scheduled_date, scheduled_utc, tzid,
            chosen_offset_seconds, dst_adjusted, created_at_utc, updated_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, 'todo', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(task_id.to_string())
    .bind(input.title.trim())
    .bind(
        input
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    )
    .bind(&input.priority)
    .bind(input.estimated_minutes)
    .bind(clean_optional_text(input.completion_criteria.as_deref()))
    .bind(&mode)
    .bind(&scheduled_local)
    .bind(&scheduled_date)
    .bind(&scheduled_utc)
    .bind(&tzid)
    .bind(chosen_offset)
    .bind(i64::from(adjusted))
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    if let Some(minutes) = input.reminder_minutes_before {
        insert_task_reminder(
            &mut transaction,
            task_id,
            minutes,
            scheduled_utc
                .as_deref()
                .ok_or_else(|| "提醒需要可换算的任务时间".to_owned())?,
            &now,
        )
        .await?;
    }
    transaction.commit().await.map_err(db_error)?;
    state.scheduler_wake.send_modify(|version| *version += 1);

    fetch_task(&state.store, task_id).await
}

#[tauri::command]
pub async fn create_recurring_task(
    state: State<'_, AppState>,
    input: CreateRecurringTaskInput,
) -> Result<String, String> {
    validate_title_priority(&input.title, &input.priority, input.estimated_minutes)?;
    validate_completion_criteria(input.completion_criteria.as_deref())?;
    validate_reminder(input.reminder_minutes_before, input.time.as_deref())?;
    if input.rrule.trim().is_empty() || input.rrule.contains(['\r', '\n']) {
        return Err("重复规则无效".to_owned());
    }
    let date = parse_date(&input.start_date)?;
    let start = series_start(
        date,
        input.time.as_deref(),
        input.time_mode.as_deref(),
        input.tzid.as_deref(),
    )?;
    let definition = RecurrenceDefinition {
        series_id: Uuid::now_v7(),
        start,
        rrule: input.rrule.trim().to_ascii_uppercase(),
        rdates: Vec::new(),
        exdates: Vec::new(),
    };
    let probe_start = definition.start.canonical_date_for_product()?;
    expand_virtual(
        &definition,
        probe_start,
        probe_start + Duration::days(366),
        512,
    )
    .map_err(|error| error.to_string())?;

    let now = utc_text(Utc::now());
    let (mode, start_text, tzid) = series_columns(&definition.start);
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    sqlx::query(
        "INSERT INTO recurrence_series(
            id, title, notes, priority, estimated_minutes, completion_criteria, time_mode,
            dtstart_local, tzid, rrule_text, rdates_json, exdates_json,
            created_at_utc, updated_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '[]', '[]', ?, ?)",
    )
    .bind(definition.series_id.to_string())
    .bind(input.title.trim())
    .bind(
        input
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    )
    .bind(&input.priority)
    .bind(input.estimated_minutes)
    .bind(clean_optional_text(input.completion_criteria.as_deref()))
    .bind(mode)
    .bind(start_text)
    .bind(tzid)
    .bind(&definition.rrule)
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    if let Some(minutes) = input.reminder_minutes_before {
        sqlx::query(
            "INSERT INTO reminder_rules(
                id, series_id, minutes_before, enabled, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, 1, ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(definition.series_id.to_string())
        .bind(minutes)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    }
    transaction.commit().await.map_err(db_error)?;
    state.scheduler_wake.send_modify(|version| *version += 1);
    Ok(definition.series_id.to_string())
}

#[tauri::command]
pub async fn list_tasks(
    state: State<'_, AppState>,
    input: TaskQuery,
) -> Result<Vec<TaskItem>, String> {
    list_tasks_for_store(&state.store, input).await
}

pub(crate) async fn list_tasks_for_store(
    store: &CoreStore,
    input: TaskQuery,
) -> Result<Vec<TaskItem>, String> {
    let start_date = parse_date(&input.range_start)?;
    let end_date = parse_date(&input.range_end)?;
    if end_date <= start_date || (end_date - start_date).num_days() > 370 {
        return Err("查询范围必须为 1 到 370 天".to_owned());
    }
    let start = start_date.and_time(NaiveTime::MIN);
    let end = end_date.and_time(NaiveTime::MIN);

    let mut items =
        load_materialized_range(store, start_date, end_date, input.include_completed).await?;
    let materialized_refs = load_materialized_refs(store, start_date, end_date).await?;

    for series in load_series(store).await? {
        let projections = expand_virtual(&series.definition, start, end, 10_000)
            .map_err(|error| error.to_string())?;
        for projection in projections
            .into_iter()
            .filter(|projection| series.includes(projection.original_local))
        {
            let series_id = series.definition.series_id.to_string();
            let occurrence_key = projection.occurrence_ref.occurrence_key.to_string();
            if materialized_refs.contains(&(series_id.clone(), occurrence_key.clone())) {
                continue;
            }
            items.push(TaskItem {
                id: None,
                series_id: Some(series_id),
                occurrence_key: Some(occurrence_key),
                title: series.title.clone(),
                notes: series.notes.clone(),
                priority: series.priority.clone(),
                estimated_minutes: series.estimated_minutes,
                completion_criteria: series.completion_criteria.clone(),
                status: "todo".to_owned(),
                time_mode: mode_text(projection.time_mode).to_owned(),
                scheduled_date: (projection.time_mode == TimeMode::AllDay).then(|| {
                    projection
                        .scheduled_local
                        .date()
                        .format("%Y-%m-%d")
                        .to_string()
                }),
                scheduled_local: (projection.time_mode != TimeMode::AllDay).then(|| {
                    projection
                        .scheduled_local
                        .format("%Y-%m-%dT%H:%M:%S")
                        .to_string()
                }),
                scheduled_utc: projection.scheduled_utc.map(utc_text),
                tzid: match &projection.occurrence_ref.occurrence_key {
                    OccurrenceKey::Zoned { tzid, .. } => Some(tzid.clone()),
                    _ => None,
                },
                dst_adjusted: projection.dst_adjusted,
                is_virtual: true,
                version: 0,
                series_version: Some(series.series_version),
                progress_percent: None,
                progress_note: None,
                blocked_reason: None,
                snoozed_until_utc: None,
            });
        }
    }
    items.sort_by_key(task_sort_key);
    Ok(items)
}

#[tauri::command]
pub async fn set_task_status(
    state: State<'_, AppState>,
    input: TaskStatusInput,
) -> Result<TaskItem, String> {
    if !matches!(
        input.status.as_str(),
        "todo" | "in_progress" | "snoozed" | "blocked" | "completed" | "canceled"
    ) {
        return Err("任务状态无效".to_owned());
    }
    let task_id = if let Some(task_id) = input.task_id.as_deref() {
        Uuid::parse_str(task_id).map_err(|_| "任务标识无效".to_owned())?
    } else {
        materialize_target(&state.store, &input).await?
    };
    let expected_version = input.expected_version.filter(|value| *value > 0);
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;

    let current = sqlx::query(
        "SELECT version, status, series_id, occurrence_key FROM tasks
         WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(task_id.to_string())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "任务不存在或已删除".to_owned())?;
    let current_version: i64 = current.try_get("version").map_err(db_error)?;
    let current_status: String = current.try_get("status").map_err(db_error)?;
    if expected_version.is_some_and(|version| version != current_version) {
        return Err("任务已在其他位置修改，请刷新后重试".to_owned());
    }
    if !valid_status_transition(&current_status, &input.status) {
        return Err(format!(
            "不允许从 {current_status} 直接进入 {}",
            input.status
        ));
    }
    let blocked_reason = if input.status == "blocked" {
        let value = input.blocked_reason.as_deref().unwrap_or_default().trim();
        if value.is_empty() || value.chars().count() > 500 {
            return Err("阻塞原因不能为空且不能超过 500 个字符".to_owned());
        }
        Some(value.to_owned())
    } else {
        None
    };
    let snoozed_until = if input.status == "snoozed" {
        let minutes = input.snoozed_minutes.unwrap_or(60);
        if !(1..=43_200).contains(&minutes) {
            return Err("稍后时长必须在 1 分钟到 30 天之间".to_owned());
        }
        Some(utc_text(Utc::now() + Duration::minutes(minutes)))
    } else {
        None
    };
    let completed_at = (input.status == "completed").then_some(now.as_str());
    sqlx::query(
        "UPDATE tasks
         SET status = ?, completed_at_utc = ?, blocked_reason = ?, snoozed_until_utc = ?,
             progress_percent = CASE WHEN ? = 'completed' THEN 100 ELSE progress_percent END,
             updated_at_utc = ?, version = version + 1
         WHERE id = ? AND version = ?",
    )
    .bind(&input.status)
    .bind(completed_at)
    .bind(&blocked_reason)
    .bind(&snoozed_until)
    .bind(&input.status)
    .bind(&now)
    .bind(task_id.to_string())
    .bind(current_version)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;

    let series_id: Option<String> = current.try_get("series_id").map_err(db_error)?;
    let occurrence_key: Option<String> = current.try_get("occurrence_key").map_err(db_error)?;
    sqlx::query(
        "INSERT INTO task_events(
            id, task_id, event_type, occurrence_series_id, occurrence_key,
            payload_json, created_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(task_id.to_string())
    .bind(format!("status_{}", input.status))
    .bind(series_id)
    .bind(occurrence_key)
    .bind(
        serde_json::json!({
            "from": current_status,
            "to": input.status,
            "fromVersion": current_version,
            "blockedReason": blocked_reason,
            "snoozedUntilUtc": snoozed_until,
        })
        .to_string(),
    )
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    fetch_task(&state.store, task_id).await
}

#[tauri::command]
pub async fn update_task(
    state: State<'_, AppState>,
    input: UpdateTaskInput,
) -> Result<TaskItem, String> {
    validate_title_priority(&input.title, &input.priority, input.estimated_minutes)?;
    validate_completion_criteria(input.completion_criteria.as_deref())?;
    validate_reminder(input.reminder_minutes_before, input.time.as_deref())?;
    let date = parse_date(&input.date)?;
    let task_id = resolve_task_target(
        &state.store,
        input.task_id.as_deref(),
        input.series_id.as_deref(),
        input.occurrence_key.as_deref(),
    )
    .await?;
    let (mode, scheduled_local, scheduled_date, scheduled_utc, tzid, chosen_offset, adjusted) =
        task_time_columns(
            date,
            input.time.as_deref(),
            input.time_mode.as_deref(),
            input.tzid.as_deref(),
        )?;
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    let current = sqlx::query(
        "SELECT version, series_id, occurrence_key
         FROM tasks WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(task_id.to_string())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "任务不存在或已删除".to_owned())?;
    let current_version: i64 = current.try_get("version").map_err(db_error)?;
    let current_series_id: Option<String> = current.try_get("series_id").map_err(db_error)?;
    let current_occurrence_key: Option<String> =
        current.try_get("occurrence_key").map_err(db_error)?;
    let effective_reminder = if input.preserve_reminder {
        sqlx::query_scalar(
            "SELECT minutes_before FROM reminder_rules
             WHERE task_id = ? AND enabled = 1 ORDER BY updated_at_utc DESC LIMIT 1",
        )
        .bind(task_id.to_string())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(db_error)?
    } else {
        input.reminder_minutes_before
    };
    if input
        .expected_version
        .filter(|value| *value > 0)
        .is_some_and(|value| value != current_version)
    {
        return Err("任务已在其他位置修改，请刷新后重试".to_owned());
    }
    sqlx::query(
        "UPDATE tasks SET
            title = ?, notes = ?, priority = ?, estimated_minutes = ?, completion_criteria = ?,
            time_mode = ?, scheduled_local = ?, scheduled_date = ?, scheduled_utc = ?,
            tzid = ?, chosen_offset_seconds = ?, dst_adjusted = ?,
            updated_at_utc = ?, version = version + 1
         WHERE id = ? AND version = ?",
    )
    .bind(input.title.trim())
    .bind(
        input
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    )
    .bind(&input.priority)
    .bind(input.estimated_minutes)
    .bind(clean_optional_text(input.completion_criteria.as_deref()))
    .bind(&mode)
    .bind(&scheduled_local)
    .bind(&scheduled_date)
    .bind(&scheduled_utc)
    .bind(&tzid)
    .bind(chosen_offset)
    .bind(i64::from(adjusted))
    .bind(&now)
    .bind(task_id.to_string())
    .bind(current_version)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    if let (Some(series_id), Some(occurrence_key)) = (
        current_series_id.as_deref(),
        current_occurrence_key.as_deref(),
    ) {
        let patch = serde_json::json!({
            "title": input.title.trim(),
            "notes": input.notes.as_deref().map(str::trim).filter(|value| !value.is_empty()),
            "priority": input.priority,
            "estimatedMinutes": input.estimated_minutes,
            "completionCriteria": input.completion_criteria,
            "timeMode": mode,
            "scheduledLocal": scheduled_local,
            "scheduledDate": scheduled_date,
            "scheduledUtc": &scheduled_utc,
            "tzid": tzid,
            "chosenOffsetSeconds": chosen_offset,
            "dstAdjusted": adjusted
        });
        sqlx::query(
            "INSERT INTO occurrence_overrides(
                id, series_id, occurrence_key, patch_json, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(series_id, occurrence_key) DO UPDATE SET
                patch_json = excluded.patch_json,
                updated_at_utc = excluded.updated_at_utc,
                version = occurrence_overrides.version + 1,
                deleted_at_utc = NULL",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(series_id)
        .bind(occurrence_key)
        .bind(serde_json::to_string(&patch).map_err(json_error)?)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    }
    sqlx::query(
        "UPDATE reminders SET status = 'cancelled', updated_at_utc = ?, version = version + 1
         WHERE task_id = ? AND status IN ('pending', 'leased')",
    )
    .bind(&now)
    .bind(task_id.to_string())
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query("UPDATE reminder_rules SET enabled = 0, updated_at_utc = ? WHERE task_id = ?")
        .bind(&now)
        .bind(task_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    if let Some(minutes) = effective_reminder {
        insert_task_reminder(
            &mut transaction,
            task_id,
            minutes,
            scheduled_utc
                .as_deref()
                .ok_or_else(|| "提醒需要可换算的任务时间".to_owned())?,
            &now,
        )
        .await?;
    }
    insert_task_event(
        &mut transaction,
        task_id,
        "task_updated",
        serde_json::json!({"fromVersion": current_version}),
        &now,
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    state.scheduler_wake.send_modify(|version| *version += 1);
    fetch_task(&state.store, task_id).await
}

#[tauri::command]
pub async fn update_recurring_series(
    state: State<'_, AppState>,
    input: UpdateRecurringSeriesInput,
) -> Result<String, String> {
    validate_title_priority(&input.title, &input.priority, input.estimated_minutes)?;
    validate_completion_criteria(input.completion_criteria.as_deref())?;
    validate_reminder(input.reminder_minutes_before, input.time.as_deref())?;
    if !matches!(
        input.edit_scope.as_str(),
        "this_and_future" | "entire_series"
    ) {
        return Err("系列编辑范围无效".to_owned());
    }
    let series_id = Uuid::parse_str(&input.series_id).map_err(|_| "重复系列标识无效".to_owned())?;
    let occurrence_key = input
        .occurrence_key
        .parse::<OccurrenceKey>()
        .map_err(|error| error.to_string())?;
    let original_series = load_one_series(&state.store, series_id).await?;
    if original_series.series_version != input.expected_series_version {
        return Err("重复系列已在其他位置修改，请刷新后重试".to_owned());
    }
    let selected_projection = project_from_key(&original_series.definition, &occurrence_key)
        .map_err(|error| error.to_string())?;
    if !original_series.includes(selected_projection.original_local) {
        return Err("该实例已不属于当前重复系列".to_owned());
    }
    let members = expand_virtual(
        &original_series.definition,
        selected_projection.original_local - Duration::seconds(1),
        selected_projection.original_local + Duration::seconds(1),
        u16::MAX,
    )
    .map_err(|error| error.to_string())?;
    if !members
        .iter()
        .any(|member| member.occurrence_ref.occurrence_key == occurrence_key)
    {
        return Err("该实例不属于当前重复规则".to_owned());
    }

    let date = parse_date(&input.date)?;
    let requested_start = series_start(
        date,
        input.time.as_deref(),
        input.time_mode.as_deref(),
        input.tzid.as_deref(),
    )?;
    let (
        task_mode,
        scheduled_local,
        scheduled_date,
        scheduled_utc,
        task_tzid,
        chosen_offset,
        adjusted,
    ) = task_time_columns(
        date,
        input.time.as_deref(),
        input.time_mode.as_deref(),
        input.tzid.as_deref(),
    )?;
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;

    let resulting_series_id = if input.edit_scope == "entire_series" {
        let definition = RecurrenceDefinition {
            series_id,
            start: requested_start,
            rrule: original_series.definition.rrule.clone(),
            rdates: original_series.definition.rdates.clone(),
            exdates: original_series.definition.exdates.clone(),
        };
        validate_recurrence_definition(&definition)?;
        let (mode, start_text, tzid) = series_columns(&definition.start);
        let changed = sqlx::query(
            "UPDATE recurrence_series SET
                title = ?, notes = ?, priority = ?, estimated_minutes = ?, completion_criteria = ?,
                time_mode = ?, dtstart_local = ?, tzid = ?,
                updated_at_utc = ?, series_version = series_version + 1
             WHERE id = ? AND series_version = ? AND deleted_at_utc IS NULL",
        )
        .bind(input.title.trim())
        .bind(clean_optional_text(input.notes.as_deref()))
        .bind(&input.priority)
        .bind(input.estimated_minutes)
        .bind(clean_optional_text(input.completion_criteria.as_deref()))
        .bind(mode)
        .bind(start_text)
        .bind(tzid)
        .bind(&now)
        .bind(series_id.to_string())
        .bind(input.expected_series_version)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?
        .rows_affected();
        if changed != 1 {
            return Err("重复系列已在其他位置修改，请刷新后重试".to_owned());
        }
        replace_series_reminder_rule(
            &mut transaction,
            series_id,
            input.reminder_minutes_before,
            &now,
        )
        .await?;
        sqlx::query(
            "UPDATE reminders SET status = 'cancelled', updated_at_utc = ?, version = version + 1
             WHERE occurrence_series_id = ? AND status IN ('pending', 'leased')",
        )
        .bind(&now)
        .bind(series_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        update_selected_series_task(
            &mut transaction,
            input.task_id.as_deref(),
            input.expected_version,
            series_id,
            &occurrence_key.to_string(),
            &input,
            &task_mode,
            scheduled_local.as_deref(),
            scheduled_date.as_deref(),
            scheduled_utc.as_deref(),
            task_tzid.as_deref(),
            chosen_offset,
            adjusted,
            &now,
        )
        .await?;
        insert_recurrence_event(
            &mut transaction,
            series_id,
            "series_updated",
            serde_json::json!({"scope": "entire_series", "occurrenceKey": occurrence_key}),
            &now,
        )
        .await?;
        series_id
    } else {
        let split_local = selected_projection.original_local;
        let new_series_id = Uuid::now_v7();
        let new_rule = rule_for_split(
            &original_series.definition,
            split_local,
            &original_series.definition.rrule,
        )?;
        let definition = RecurrenceDefinition {
            series_id: new_series_id,
            start: requested_start,
            rrule: new_rule,
            rdates: original_series
                .definition
                .rdates
                .iter()
                .copied()
                .filter(|value| *value >= split_local)
                .collect(),
            exdates: original_series
                .definition
                .exdates
                .iter()
                .copied()
                .filter(|value| *value >= split_local)
                .collect(),
        };
        validate_recurrence_definition(&definition)?;
        let changed = sqlx::query(
            "UPDATE recurrence_series SET ends_before_occurrence_key = ?,
                    updated_at_utc = ?, series_version = series_version + 1
             WHERE id = ? AND series_version = ? AND deleted_at_utc IS NULL",
        )
        .bind(occurrence_key.to_string())
        .bind(&now)
        .bind(series_id.to_string())
        .bind(input.expected_series_version)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?
        .rows_affected();
        if changed != 1 {
            return Err("重复系列已在其他位置修改，请刷新后重试".to_owned());
        }
        let (mode, start_text, tzid) = series_columns(&definition.start);
        sqlx::query(
            "INSERT INTO recurrence_series(
                id, title, notes, priority, estimated_minutes, completion_criteria, time_mode,
                dtstart_local, tzid, rrule_text, rdates_json, exdates_json,
                created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_series_id.to_string())
        .bind(input.title.trim())
        .bind(clean_optional_text(input.notes.as_deref()))
        .bind(&input.priority)
        .bind(input.estimated_minutes)
        .bind(clean_optional_text(input.completion_criteria.as_deref()))
        .bind(mode)
        .bind(start_text)
        .bind(tzid)
        .bind(&definition.rrule)
        .bind(serde_json::to_string(&definition.rdates).map_err(json_error)?)
        .bind(serde_json::to_string(&definition.exdates).map_err(json_error)?)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        replace_series_reminder_rule(
            &mut transaction,
            new_series_id,
            input.reminder_minutes_before,
            &now,
        )
        .await?;
        cancel_series_reminders_from(&mut transaction, series_id, split_local, &now).await?;
        let first = expand_virtual(
            &definition,
            definition.start.canonical_date_for_product()? - Duration::seconds(1),
            definition.start.canonical_date_for_product()? + Duration::days(2),
            8,
        )
        .map_err(|error| error.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "拆分后的规则没有可用实例".to_owned())?;
        update_selected_series_task(
            &mut transaction,
            input.task_id.as_deref(),
            input.expected_version,
            new_series_id,
            &first.occurrence_ref.occurrence_key.to_string(),
            &input,
            &task_mode,
            scheduled_local.as_deref(),
            scheduled_date.as_deref(),
            scheduled_utc.as_deref(),
            task_tzid.as_deref(),
            chosen_offset,
            adjusted,
            &now,
        )
        .await?;
        insert_recurrence_event(
            &mut transaction,
            series_id,
            "series_split",
            serde_json::json!({
                "scope": "this_and_future",
                "occurrenceKey": occurrence_key,
                "newSeriesId": new_series_id
            }),
            &now,
        )
        .await?;
        insert_recurrence_event(
            &mut transaction,
            new_series_id,
            "series_created_from_split",
            serde_json::json!({"previousSeriesId": series_id}),
            &now,
        )
        .await?;
        new_series_id
    };

    transaction.commit().await.map_err(db_error)?;
    state.scheduler_wake.send_modify(|version| *version += 1);
    Ok(resulting_series_id.to_string())
}

#[tauri::command]
pub async fn delete_task(
    state: State<'_, AppState>,
    input: TaskTargetInput,
) -> Result<String, String> {
    let task_id = resolve_task_target(
        &state.store,
        input.task_id.as_deref(),
        input.series_id.as_deref(),
        input.occurrence_key.as_deref(),
    )
    .await?;
    let task = fetch_task(&state.store, task_id).await?;
    if input
        .expected_version
        .filter(|value| *value > 0)
        .is_some_and(|version| version != task.version)
    {
        return Err("任务已在其他位置修改，请刷新后重试".to_owned());
    }
    let now_value = Utc::now();
    let now = utc_text(now_value);
    let purge_after = utc_text(now_value + Duration::days(30));
    let trash_id = Uuid::now_v7();
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    let changed = sqlx::query(
        "UPDATE tasks SET deleted_at_utc = ?, updated_at_utc = ?, version = version + 1
         WHERE id = ? AND version = ? AND deleted_at_utc IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(task_id.to_string())
    .bind(task.version)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed != 1 {
        return Err("任务已在其他位置修改，请刷新后重试".to_owned());
    }
    sqlx::query(
        "INSERT INTO trash(id, entity_type, entity_id, snapshot_json, deleted_at_utc, purge_after_utc)
         VALUES (?, 'task', ?, ?, ?, ?)",
    )
    .bind(trash_id.to_string())
    .bind(task_id.to_string())
    .bind(serde_json::to_string(&task).map_err(json_error)?)
    .bind(&now)
    .bind(purge_after)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "UPDATE reminders SET status = 'cancelled', updated_at_utc = ?, version = version + 1
         WHERE task_id = ? AND status IN ('pending', 'leased')",
    )
    .bind(&now)
    .bind(task_id.to_string())
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    insert_task_event(
        &mut transaction,
        task_id,
        "task_deleted",
        serde_json::json!({"trashId": trash_id}),
        &now,
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(trash_id.to_string())
}

#[tauri::command]
pub async fn list_trash(state: State<'_, AppState>) -> Result<Vec<TrashItem>, String> {
    let rows = sqlx::query(
        "SELECT id, snapshot_json, deleted_at_utc, purge_after_utc
         FROM trash WHERE entity_type = 'task' AND restored_at_utc IS NULL
           AND julianday(purge_after_utc) > julianday('now')
         ORDER BY deleted_at_utc DESC",
    )
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            let snapshot: String = row.try_get("snapshot_json").map_err(db_error)?;
            Ok(TrashItem {
                id: row.try_get("id").map_err(db_error)?,
                deleted_at_utc: row.try_get("deleted_at_utc").map_err(db_error)?,
                purge_after_utc: row.try_get("purge_after_utc").map_err(db_error)?,
                task: serde_json::from_str(&snapshot).map_err(json_error)?,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn restore_task(
    state: State<'_, AppState>,
    trash_id: String,
) -> Result<TaskItem, String> {
    let trash_id = Uuid::parse_str(&trash_id).map_err(|_| "回收站标识无效".to_owned())?;
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    let row = sqlx::query(
        "SELECT entity_id FROM trash
         WHERE id = ? AND entity_type = 'task' AND restored_at_utc IS NULL",
    )
    .bind(trash_id.to_string())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "回收站项目不存在或已恢复".to_owned())?;
    let task_id_text: String = row.try_get("entity_id").map_err(db_error)?;
    let task_id = Uuid::parse_str(&task_id_text).map_err(|_| "任务标识无效".to_owned())?;
    sqlx::query(
        "UPDATE tasks SET deleted_at_utc = NULL, updated_at_utc = ?, version = version + 1
         WHERE id = ?",
    )
    .bind(&now)
    .bind(&task_id_text)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query("UPDATE trash SET restored_at_utc = ? WHERE id = ?")
        .bind(&now)
        .bind(trash_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    insert_task_event(
        &mut transaction,
        task_id,
        "task_restored",
        serde_json::json!({"trashId": trash_id}),
        &now,
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    fetch_task(&state.store, task_id).await
}

#[tauri::command]
pub async fn record_partial_progress(
    state: State<'_, AppState>,
    input: ReviewProgressInput,
) -> Result<TaskItem, String> {
    if input
        .progress_percent
        .is_some_and(|value| !(0..=100).contains(&value))
    {
        return Err("进度必须为 0 到 100".to_owned());
    }
    if input.progress_note.trim().is_empty() {
        return Err("请填写客观的进度说明".to_owned());
    }
    let task_id = resolve_task_target(
        &state.store,
        input.task_id.as_deref(),
        input.series_id.as_deref(),
        input.occurrence_key.as_deref(),
    )
    .await?;
    let task = fetch_task(&state.store, task_id).await?;
    if input
        .expected_version
        .filter(|value| *value > 0)
        .is_some_and(|value| value != task.version)
    {
        return Err("任务已在其他位置修改，请刷新后重试".to_owned());
    }
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    sqlx::query(
        "UPDATE tasks SET progress_percent = ?, progress_note = ?,
            status = CASE WHEN status = 'todo' THEN 'in_progress' ELSE status END,
            updated_at_utc = ?, version = version + 1
         WHERE id = ? AND version = ?",
    )
    .bind(input.progress_percent)
    .bind(input.progress_note.trim())
    .bind(&now)
    .bind(task_id.to_string())
    .bind(task.version)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    insert_task_event(
        &mut transaction,
        task_id,
        "review_outcome_partial",
        serde_json::json!({
            "progressPercent": input.progress_percent,
            "progressNote": input.progress_note.trim()
        }),
        &now,
    )
    .await?;
    insert_task_event(
        &mut transaction,
        task_id,
        "progress_updated",
        serde_json::json!({"progressPercent": input.progress_percent}),
        &now,
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    fetch_task(&state.store, task_id).await
}

#[tauri::command]
pub async fn record_not_started_review(
    state: State<'_, AppState>,
    input: ReviewOutcomeInput,
) -> Result<TaskItem, String> {
    let task_id = resolve_task_target(
        &state.store,
        input.task_id.as_deref(),
        input.series_id.as_deref(),
        input.occurrence_key.as_deref(),
    )
    .await?;
    let row = sqlx::query("SELECT version, series_id, occurrence_key FROM tasks WHERE id = ? AND deleted_at_utc IS NULL")
        .bind(task_id.to_string())
        .fetch_optional(&state.store.pool)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "任务不存在或已删除".to_owned())?;
    let version: i64 = row.try_get("version").map_err(db_error)?;
    if input
        .expected_version
        .filter(|value| *value > 0)
        .is_some_and(|expected| expected != version)
    {
        return Err("任务已在其他位置修改，请刷新后重试".to_owned());
    }
    let now = utc_text(Utc::now());
    sqlx::query(
        "INSERT INTO task_events(
            id, task_id, event_type, occurrence_series_id, occurrence_key,
            payload_json, created_at_utc
         ) VALUES (?, ?, 'review_outcome', ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(task_id.to_string())
    .bind(
        row.try_get::<Option<String>, _>("series_id")
            .map_err(db_error)?,
    )
    .bind(
        row.try_get::<Option<String>, _>("occurrence_key")
            .map_err(db_error)?,
    )
    .bind(serde_json::json!({"reviewOutcome": "not_started", "taskVersion": version}).to_string())
    .bind(now)
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?;
    fetch_task(&state.store, task_id).await
}

#[tauri::command]
pub async fn dashboard_summary(
    state: State<'_, AppState>,
    date: String,
) -> Result<DashboardSummary, String> {
    let start = parse_date(&date)?;
    let end = start + Duration::days(1);
    let tasks = list_tasks(
        state,
        TaskQuery {
            range_start: start.format("%Y-%m-%d").to_string(),
            range_end: end.format("%Y-%m-%d").to_string(),
            include_completed: true,
        },
    )
    .await?;
    let completed = tasks
        .iter()
        .filter(|task| task.status == "completed")
        .count();
    Ok(DashboardSummary {
        total: tasks.len(),
        completed,
        remaining: tasks.len() - completed,
        focus_minutes: tasks
            .iter()
            .filter(|task| task.status != "canceled")
            .filter_map(|task| task.estimated_minutes)
            .sum(),
    })
}

#[tauri::command]
pub fn runtime_info(state: State<'_, AppState>) -> RuntimeInfo {
    RuntimeInfo {
        app_version: env!("CARGO_PKG_VERSION"),
        data_directory: state.data_directory.clone(),
        database_engine: "SQLite (WAL, foreign_keys=ON)",
        recurrence_engine: "RRule.rs 0.14.0",
        timezone_database: "chrono-tz 0.10.4 / IANA",
        agent_bridge: probe_bundled_agent_bridge(),
    }
}

pub(crate) async fn purge_expired_trash(store: &CoreStore) -> Result<u64, String> {
    sqlx::query(
        "DELETE FROM trash WHERE restored_at_utc IS NULL
           AND julianday(purge_after_utc) <= julianday('now')",
    )
    .execute(&store.pool)
    .await
    .map(|result| result.rows_affected())
    .map_err(db_error)
}

fn probe_bundled_agent_bridge() -> String {
    let Ok(current) = std::env::current_exe() else {
        return "unavailable".to_owned();
    };
    let Some(directory) = current.parent() else {
        return "unavailable".to_owned();
    };
    let binary = directory.join("pga-agent-bridge.exe");
    if !binary.is_file() {
        return "development build (not bundled)".to_owned();
    }
    let mut command = std::process::Command::new(binary);
    command.args(["runtime", "probe"]).env_clear();
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    match command.output() {
        Ok(output) if output.status.success() => {
            serde_json::from_slice::<serde_json::Value>(&output.stdout)
                .ok()
                .and_then(|value| value.get("bridgeVersion")?.as_str().map(str::to_owned))
                .map(|version| format!("MCP SDK v2 bridge {version}"))
                .unwrap_or_else(|| "invalid probe response".to_owned())
        }
        _ => "unavailable".to_owned(),
    }
}

async fn replace_series_reminder_rule(
    transaction: &mut Transaction<'_, Sqlite>,
    series_id: Uuid,
    minutes_before: Option<i64>,
    now: &str,
) -> Result<(), String> {
    sqlx::query("UPDATE reminder_rules SET enabled = 0, updated_at_utc = ? WHERE series_id = ?")
        .bind(now)
        .bind(series_id.to_string())
        .execute(&mut **transaction)
        .await
        .map_err(db_error)?;
    if let Some(minutes_before) = minutes_before {
        sqlx::query(
            "INSERT INTO reminder_rules(
                id, series_id, minutes_before, enabled, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, 1, ?, ?)
             ON CONFLICT(series_id) DO UPDATE SET
                minutes_before = excluded.minutes_before,
                enabled = 1,
                updated_at_utc = excluded.updated_at_utc,
                version = reminder_rules.version + 1",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(series_id.to_string())
        .bind(minutes_before)
        .bind(now)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(db_error)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_selected_series_task(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: Option<&str>,
    expected_version: Option<i64>,
    target_series_id: Uuid,
    target_occurrence_key: &str,
    input: &UpdateRecurringSeriesInput,
    mode: &str,
    scheduled_local: Option<&str>,
    scheduled_date: Option<&str>,
    scheduled_utc: Option<&str>,
    tzid: Option<&str>,
    chosen_offset: Option<i32>,
    adjusted: bool,
    now: &str,
) -> Result<(), String> {
    let Some(task_id) = task_id else {
        return Ok(());
    };
    let task_id = Uuid::parse_str(task_id).map_err(|_| "任务标识无效".to_owned())?;
    let current = sqlx::query(
        "SELECT version, status, series_id FROM tasks
         WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(task_id.to_string())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "任务不存在或已删除".to_owned())?;
    let version: i64 = current.try_get("version").map_err(db_error)?;
    let status: String = current.try_get("status").map_err(db_error)?;
    let source_series_id: Option<String> = current.try_get("series_id").map_err(db_error)?;
    if source_series_id.as_deref() != Some(input.series_id.as_str()) {
        return Err("任务与重复系列不匹配".to_owned());
    }
    if expected_version
        .filter(|value| *value > 0)
        .is_some_and(|value| value != version)
    {
        return Err("任务已在其他位置修改，请刷新后重试".to_owned());
    }
    if matches!(status.as_str(), "completed" | "canceled") {
        return Ok(());
    }
    let changed = sqlx::query(
        "UPDATE tasks SET series_id = ?, occurrence_key = ?,
            title = ?, notes = ?, priority = ?, estimated_minutes = ?, completion_criteria = ?,
            time_mode = ?, scheduled_local = ?, scheduled_date = ?, scheduled_utc = ?,
            tzid = ?, chosen_offset_seconds = ?, dst_adjusted = ?,
            updated_at_utc = ?, version = version + 1
         WHERE id = ? AND version = ?",
    )
    .bind(target_series_id.to_string())
    .bind(target_occurrence_key)
    .bind(input.title.trim())
    .bind(clean_optional_text(input.notes.as_deref()))
    .bind(&input.priority)
    .bind(input.estimated_minutes)
    .bind(clean_optional_text(input.completion_criteria.as_deref()))
    .bind(mode)
    .bind(scheduled_local)
    .bind(scheduled_date)
    .bind(scheduled_utc)
    .bind(tzid)
    .bind(chosen_offset)
    .bind(i64::from(adjusted))
    .bind(now)
    .bind(task_id.to_string())
    .bind(version)
    .execute(&mut **transaction)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed != 1 {
        return Err("任务已在其他位置修改，请刷新后重试".to_owned());
    }
    insert_task_event(
        transaction,
        task_id,
        "task_updated_with_series",
        serde_json::json!({"scope": input.edit_scope}),
        now,
    )
    .await
}

async fn cancel_series_reminders_from(
    transaction: &mut Transaction<'_, Sqlite>,
    series_id: Uuid,
    cutoff: NaiveDateTime,
    now: &str,
) -> Result<(), String> {
    let rows = sqlx::query(
        "SELECT id, occurrence_key FROM reminders
         WHERE occurrence_series_id = ? AND status IN ('pending', 'leased')",
    )
    .bind(series_id.to_string())
    .fetch_all(&mut **transaction)
    .await
    .map_err(db_error)?;
    for row in rows {
        let key: String = row.try_get("occurrence_key").map_err(db_error)?;
        let key = key
            .parse::<OccurrenceKey>()
            .map_err(|error| error.to_string())?;
        if occurrence_original_local(&key) >= cutoff {
            let id: String = row.try_get("id").map_err(db_error)?;
            sqlx::query(
                "UPDATE reminders SET status = 'cancelled', updated_at_utc = ?,
                        version = version + 1 WHERE id = ?",
            )
            .bind(now)
            .bind(id)
            .execute(&mut **transaction)
            .await
            .map_err(db_error)?;
        }
    }
    Ok(())
}

async fn insert_recurrence_event(
    transaction: &mut Transaction<'_, Sqlite>,
    series_id: Uuid,
    event_type: &str,
    payload: serde_json::Value,
    now: &str,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO recurrence_events(id, series_id, event_type, payload_json, created_at_utc)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(series_id.to_string())
    .bind(event_type)
    .bind(serde_json::to_string(&payload).map_err(json_error)?)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(db_error)?;
    Ok(())
}

fn validate_recurrence_definition(definition: &RecurrenceDefinition) -> Result<(), String> {
    let start = definition.start.canonical_date_for_product()?;
    expand_virtual(
        definition,
        start - Duration::seconds(1),
        start + Duration::days(366),
        512,
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn rule_for_split(
    definition: &RecurrenceDefinition,
    split_local: NaiveDateTime,
    rule: &str,
) -> Result<String, String> {
    let count = rule.split(';').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        name.eq_ignore_ascii_case("COUNT").then_some(value)
    });
    let Some(count) = count else {
        return Ok(rule.to_owned());
    };
    let count = count
        .parse::<usize>()
        .map_err(|_| "重复规则中的 COUNT 无效".to_owned())?;
    if count > usize::from(u16::MAX) {
        return Err("COUNT 过大，无法安全拆分该系列".to_owned());
    }
    let mut count_definition = definition.clone();
    count_definition.rdates.clear();
    count_definition.exdates.clear();
    let start = definition.start.canonical_date_for_product()?;
    let before = expand_virtual(
        &count_definition,
        start - Duration::seconds(1),
        split_local,
        u16::MAX,
    )
    .map_err(|error| error.to_string())?
    .len();
    let remaining = count
        .checked_sub(before)
        .filter(|value| *value > 0)
        .ok_or_else(|| "拆分点已经超出 COUNT 规则范围".to_owned())?;
    Ok(rule
        .split(';')
        .map(|part| {
            if part
                .split_once('=')
                .is_some_and(|(name, _)| name.eq_ignore_ascii_case("COUNT"))
            {
                format!("COUNT={remaining}")
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(";"))
}

fn clean_optional_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(crate) async fn materialize_target(
    store: &CoreStore,
    input: &TaskStatusInput,
) -> Result<Uuid, String> {
    let series_id = input
        .series_id
        .as_deref()
        .ok_or_else(|| "缺少重复系列标识".to_owned())?;
    let series_id = Uuid::parse_str(series_id).map_err(|_| "重复系列标识无效".to_owned())?;
    let occurrence_key = input
        .occurrence_key
        .as_deref()
        .ok_or_else(|| "缺少重复实例标识".to_owned())?
        .parse::<OccurrenceKey>()
        .map_err(|error| error.to_string())?;
    let series = load_one_series(store, series_id).await?;
    let projection =
        project_from_key(&series.definition, &occurrence_key).map_err(|error| error.to_string())?;
    if !series.includes(projection.original_local) {
        return Err("该实例已不属于拆分后的旧系列".to_owned());
    }

    let original = projection.original_local;
    let members = expand_virtual(
        &series.definition,
        original - Duration::seconds(1),
        original + Duration::seconds(1),
        u16::MAX,
    )
    .map_err(|error| error.to_string())?;
    if !members
        .iter()
        .any(|member| member.occurrence_ref.occurrence_key == occurrence_key)
    {
        return Err("该实例不属于当前重复规则".to_owned());
    }
    let task = store
        .materialize_occurrence(MaterializationInput {
            projection,
            title: series.title,
            notes: series.notes,
            priority: series.priority,
            estimated_minutes: series.estimated_minutes,
            completion_criteria: series.completion_criteria,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(task.id)
}

async fn resolve_task_target(
    store: &CoreStore,
    task_id: Option<&str>,
    series_id: Option<&str>,
    occurrence_key: Option<&str>,
) -> Result<Uuid, String> {
    if let Some(task_id) = task_id {
        return Uuid::parse_str(task_id).map_err(|_| "任务标识无效".to_owned());
    }
    materialize_target(
        store,
        &TaskStatusInput {
            task_id: None,
            series_id: series_id.map(str::to_owned),
            occurrence_key: occurrence_key.map(str::to_owned),
            status: "todo".to_owned(),
            expected_version: None,
            blocked_reason: None,
            snoozed_minutes: None,
        },
    )
    .await
}

async fn load_materialized_refs(
    store: &CoreStore,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<HashSet<(String, String)>, String> {
    let rows = sqlx::query(
        "SELECT series_id, occurrence_key FROM tasks
         WHERE series_id IS NOT NULL AND occurrence_key IS NOT NULL
           AND COALESCE(scheduled_date, substr(scheduled_local, 1, 10)) >= ?
           AND COALESCE(scheduled_date, substr(scheduled_local, 1, 10)) < ?",
    )
    .bind(start.format("%Y-%m-%d").to_string())
    .bind(end.format("%Y-%m-%d").to_string())
    .fetch_all(&store.pool)
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("series_id").map_err(db_error)?,
                row.try_get("occurrence_key").map_err(db_error)?,
            ))
        })
        .collect()
}

async fn load_materialized_range(
    store: &CoreStore,
    start: NaiveDate,
    end: NaiveDate,
    include_completed: bool,
) -> Result<Vec<TaskItem>, String> {
    let rows = sqlx::query(
        "SELECT id, series_id, occurrence_key, title, notes, priority,
                estimated_minutes, completion_criteria, status, time_mode, scheduled_local,
                scheduled_date, scheduled_utc, tzid, dst_adjusted, version,
                progress_percent, progress_note, blocked_reason, snoozed_until_utc,
                (SELECT series_version FROM recurrence_series s WHERE s.id = tasks.series_id)
                    AS series_version
         FROM tasks
         WHERE deleted_at_utc IS NULL
           AND COALESCE(scheduled_date, substr(scheduled_local, 1, 10)) >= ?
           AND COALESCE(scheduled_date, substr(scheduled_local, 1, 10)) < ?
           AND (? = 1 OR status NOT IN ('completed', 'canceled'))",
    )
    .bind(start.format("%Y-%m-%d").to_string())
    .bind(end.format("%Y-%m-%d").to_string())
    .bind(i64::from(include_completed))
    .fetch_all(&store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(row_to_task_item).collect()
}

async fn fetch_task(store: &CoreStore, id: Uuid) -> Result<TaskItem, String> {
    let row = sqlx::query(
        "SELECT id, series_id, occurrence_key, title, notes, priority,
                estimated_minutes, completion_criteria, status, time_mode, scheduled_local,
                scheduled_date, scheduled_utc, tzid, dst_adjusted, version,
                progress_percent, progress_note, blocked_reason, snoozed_until_utc,
                (SELECT series_version FROM recurrence_series s WHERE s.id = tasks.series_id)
                    AS series_version
         FROM tasks WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(id.to_string())
    .fetch_optional(&store.pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "任务不存在".to_owned())?;
    row_to_task_item(&row)
}

fn row_to_task_item(row: &sqlx::sqlite::SqliteRow) -> Result<TaskItem, String> {
    Ok(TaskItem {
        id: Some(row.try_get("id").map_err(db_error)?),
        series_id: row.try_get("series_id").map_err(db_error)?,
        occurrence_key: row.try_get("occurrence_key").map_err(db_error)?,
        title: row.try_get("title").map_err(db_error)?,
        notes: row.try_get("notes").map_err(db_error)?,
        priority: row.try_get("priority").map_err(db_error)?,
        estimated_minutes: row.try_get("estimated_minutes").map_err(db_error)?,
        completion_criteria: row.try_get("completion_criteria").map_err(db_error)?,
        status: row.try_get("status").map_err(db_error)?,
        time_mode: row.try_get("time_mode").map_err(db_error)?,
        scheduled_local: row.try_get("scheduled_local").map_err(db_error)?,
        scheduled_date: row.try_get("scheduled_date").map_err(db_error)?,
        scheduled_utc: row.try_get("scheduled_utc").map_err(db_error)?,
        tzid: row.try_get("tzid").map_err(db_error)?,
        dst_adjusted: row.try_get::<i64, _>("dst_adjusted").map_err(db_error)? != 0,
        is_virtual: false,
        version: row.try_get("version").map_err(db_error)?,
        series_version: row.try_get("series_version").map_err(db_error)?,
        progress_percent: row.try_get("progress_percent").map_err(db_error)?,
        progress_note: row.try_get("progress_note").map_err(db_error)?,
        blocked_reason: row.try_get("blocked_reason").map_err(db_error)?,
        snoozed_until_utc: row.try_get("snoozed_until_utc").map_err(db_error)?,
    })
}

async fn load_series(store: &CoreStore) -> Result<Vec<StoredSeries>, String> {
    let rows = sqlx::query(
        "SELECT id, title, notes, priority, estimated_minutes, completion_criteria, time_mode,
                dtstart_local, tzid, rrule_text, rdates_json, exdates_json,
                series_version, ends_before_occurrence_key
         FROM recurrence_series WHERE deleted_at_utc IS NULL",
    )
    .fetch_all(&store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(row_to_series).collect()
}

async fn load_one_series(store: &CoreStore, series_id: Uuid) -> Result<StoredSeries, String> {
    let row = sqlx::query(
        "SELECT id, title, notes, priority, estimated_minutes, completion_criteria, time_mode,
                dtstart_local, tzid, rrule_text, rdates_json, exdates_json,
                series_version, ends_before_occurrence_key
         FROM recurrence_series WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(series_id.to_string())
    .fetch_optional(&store.pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "重复系列不存在或已删除".to_owned())?;
    row_to_series(&row)
}

fn row_to_series(row: &sqlx::sqlite::SqliteRow) -> Result<StoredSeries, String> {
    let id_text: String = row.try_get("id").map_err(db_error)?;
    let series_id = Uuid::parse_str(&id_text).map_err(|_| "数据库中的系列标识无效".to_owned())?;
    let mode: String = row.try_get("time_mode").map_err(db_error)?;
    let start_text: String = row.try_get("dtstart_local").map_err(db_error)?;
    let start = match mode.as_str() {
        "all_day" => SeriesStart::AllDay {
            date: parse_date(&start_text)?,
        },
        "floating" => SeriesStart::Floating {
            local: parse_db_local(&start_text)?,
        },
        "zoned" => SeriesStart::Zoned {
            local: parse_db_local(&start_text)?,
            tzid: row
                .try_get::<Option<String>, _>("tzid")
                .map_err(db_error)?
                .ok_or_else(|| "zoned 系列缺少时区".to_owned())?,
        },
        _ => return Err("数据库中的时间模式无效".to_owned()),
    };
    let rdates_text: String = row.try_get("rdates_json").map_err(db_error)?;
    let exdates_text: String = row.try_get("exdates_json").map_err(db_error)?;
    let ends_before_local = row
        .try_get::<Option<String>, _>("ends_before_occurrence_key")
        .map_err(db_error)?
        .map(|value| {
            value
                .parse::<OccurrenceKey>()
                .map(|key| occurrence_original_local(&key))
                .map_err(|error| error.to_string())
        })
        .transpose()?;
    Ok(StoredSeries {
        definition: RecurrenceDefinition {
            series_id,
            start,
            rrule: row.try_get("rrule_text").map_err(db_error)?,
            rdates: serde_json::from_str(&rdates_text).map_err(|_| "RDATE 数据无效".to_owned())?,
            exdates: serde_json::from_str(&exdates_text)
                .map_err(|_| "EXDATE 数据无效".to_owned())?,
        },
        title: row.try_get("title").map_err(db_error)?,
        notes: row.try_get("notes").map_err(db_error)?,
        priority: row.try_get("priority").map_err(db_error)?,
        estimated_minutes: row.try_get("estimated_minutes").map_err(db_error)?,
        completion_criteria: row.try_get("completion_criteria").map_err(db_error)?,
        series_version: row.try_get("series_version").map_err(db_error)?,
        ends_before_local,
    })
}

fn task_time_columns(
    date: NaiveDate,
    time: Option<&str>,
    mode: Option<&str>,
    tzid: Option<&str>,
) -> Result<TimeColumns, String> {
    let Some(time) = time.filter(|value| !value.is_empty()) else {
        return Ok((
            "all_day".to_owned(),
            None,
            Some(date.format("%Y-%m-%d").to_string()),
            None,
            None,
            None,
            false,
        ));
    };
    let local = date.and_time(parse_time(time)?);
    if mode == Some("floating") {
        let current_timezone = tzid.unwrap_or("Asia/Shanghai");
        let resolved =
            resolve_zoned_local(local, current_timezone).map_err(|error| error.to_string())?;
        return Ok((
            "floating".to_owned(),
            Some(format_local(local)),
            None,
            Some(utc_text(resolved.scheduled_utc)),
            None,
            Some(resolved.chosen_offset_seconds),
            resolved.dst_adjusted,
        ));
    }
    let tzid = tzid.unwrap_or("Asia/Shanghai");
    let resolved = resolve_zoned_local(local, tzid).map_err(|error| error.to_string())?;
    Ok((
        "zoned".to_owned(),
        Some(format_local(resolved.scheduled_local)),
        None,
        Some(utc_text(resolved.scheduled_utc)),
        Some(tzid.to_owned()),
        Some(resolved.chosen_offset_seconds),
        resolved.dst_adjusted,
    ))
}

type TimeColumns = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i32>,
    bool,
);

fn series_start(
    date: NaiveDate,
    time: Option<&str>,
    mode: Option<&str>,
    tzid: Option<&str>,
) -> Result<SeriesStart, String> {
    let Some(time) = time.filter(|value| !value.is_empty()) else {
        return Ok(SeriesStart::AllDay { date });
    };
    let local = date.and_time(parse_time(time)?);
    if mode == Some("floating") {
        Ok(SeriesStart::Floating { local })
    } else {
        let tzid = tzid.unwrap_or("Asia/Shanghai");
        resolve_zoned_local(local, tzid).map_err(|error| error.to_string())?;
        Ok(SeriesStart::Zoned {
            local,
            tzid: tzid.to_owned(),
        })
    }
}

fn series_columns(start: &SeriesStart) -> (&'static str, String, Option<&str>) {
    match start {
        SeriesStart::AllDay { date } => ("all_day", date.format("%Y-%m-%d").to_string(), None),
        SeriesStart::Floating { local } => ("floating", format_local(*local), None),
        SeriesStart::Zoned { local, tzid } => ("zoned", format_local(*local), Some(tzid)),
    }
}

trait ProductStartDate {
    fn canonical_date_for_product(&self) -> Result<NaiveDateTime, String>;
}

impl ProductStartDate for SeriesStart {
    fn canonical_date_for_product(&self) -> Result<NaiveDateTime, String> {
        Ok(match self {
            Self::AllDay { date } => date.and_time(NaiveTime::MIN),
            Self::Floating { local } | Self::Zoned { local, .. } => *local,
        })
    }
}

fn validate_title_priority(
    title: &str,
    priority: &str,
    estimated_minutes: Option<i64>,
) -> Result<(), String> {
    if title.trim().is_empty() || title.chars().count() > 200 {
        return Err("标题必须为 1 到 200 个字符".to_owned());
    }
    if !matches!(priority, "p0" | "p1" | "p2" | "p3") {
        return Err("优先级无效".to_owned());
    }
    if estimated_minutes.is_some_and(|minutes| !(1..=1440).contains(&minutes)) {
        return Err("预计时长必须为 1 到 1440 分钟".to_owned());
    }
    Ok(())
}

fn validate_reminder(minutes: Option<i64>, time: Option<&str>) -> Result<(), String> {
    if minutes.is_some_and(|value| !(0..=10_080).contains(&value)) {
        return Err("提醒提前量必须在 0 到 10080 分钟之间".to_owned());
    }
    if minutes.is_some() && time.is_none_or(str::is_empty) {
        return Err("全天任务需要先设置具体时间才能提醒".to_owned());
    }
    Ok(())
}

fn validate_completion_criteria(value: Option<&str>) -> Result<(), String> {
    if value.is_some_and(|value| value.trim().chars().count() > 500) {
        return Err("完成标准不能超过 500 个字符".to_owned());
    }
    Ok(())
}

async fn insert_task_reminder(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: Uuid,
    minutes_before: i64,
    scheduled_utc: &str,
    now: &str,
) -> Result<(), String> {
    let scheduled = DateTime::parse_from_rfc3339(scheduled_utc)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| "任务提醒时间无效".to_owned())?;
    let remind_at = scheduled - Duration::minutes(minutes_before);
    let proposed_rule_id = Uuid::now_v7();
    let reminder_id = Uuid::now_v7();
    let scheduled_text = utc_text(remind_at);
    sqlx::query(
        "INSERT INTO reminder_rules(
            id, task_id, minutes_before, enabled, created_at_utc, updated_at_utc
         ) VALUES (?, ?, ?, 1, ?, ?)
         ON CONFLICT(task_id) DO UPDATE SET
            minutes_before = excluded.minutes_before,
            enabled = 1,
            updated_at_utc = excluded.updated_at_utc,
            version = reminder_rules.version + 1",
    )
    .bind(proposed_rule_id.to_string())
    .bind(task_id.to_string())
    .bind(minutes_before)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(db_error)?;
    let rule_id: String = sqlx::query_scalar("SELECT id FROM reminder_rules WHERE task_id = ?")
        .bind(task_id.to_string())
        .fetch_one(&mut **transaction)
        .await
        .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO reminders(
            id, task_id, reminder_rule_id, remind_at_utc, status,
            created_at_utc, updated_at_utc
         ) VALUES (?, ?, ?, ?, 'pending', ?, ?)",
    )
    .bind(reminder_id.to_string())
    .bind(task_id.to_string())
    .bind(rule_id)
    .bind(&scheduled_text)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO jobs(
            id, kind, owner, business_key, scheduled_for_utc, attempt_class,
            run_at_utc, state, payload_json, created_at_utc, updated_at_utc
         ) VALUES (?, 'reminder', 'local', ?, ?, 'primary', ?, 'pending', ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(format!("reminder:{reminder_id}"))
    .bind(&scheduled_text)
    .bind(&scheduled_text)
    .bind(
        serde_json::json!({
            "reminderId": reminder_id,
            "taskId": task_id,
        })
        .to_string(),
    )
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn insert_task_event(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: Uuid,
    event_type: &str,
    payload: serde_json::Value,
    now: &str,
) -> Result<(), String> {
    let occurrence = sqlx::query("SELECT series_id, occurrence_key FROM tasks WHERE id = ?")
        .bind(task_id.to_string())
        .fetch_one(&mut **transaction)
        .await
        .map_err(db_error)?;
    let series_id: Option<String> = occurrence.try_get("series_id").map_err(db_error)?;
    let occurrence_key: Option<String> = occurrence.try_get("occurrence_key").map_err(db_error)?;
    sqlx::query(
        "INSERT INTO task_events(
            id, task_id, event_type, occurrence_series_id, occurrence_key,
            payload_json, created_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(task_id.to_string())
    .bind(event_type)
    .bind(series_id)
    .bind(occurrence_key)
    .bind(payload.to_string())
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(db_error)?;
    Ok(())
}

fn valid_status_transition(from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    match from {
        "todo" => matches!(
            to,
            "in_progress" | "snoozed" | "blocked" | "completed" | "canceled"
        ),
        "in_progress" => matches!(to, "snoozed" | "blocked" | "completed" | "canceled"),
        "snoozed" => matches!(
            to,
            "todo" | "in_progress" | "blocked" | "completed" | "canceled"
        ),
        "blocked" => matches!(to, "todo" | "in_progress" | "completed" | "canceled"),
        "completed" | "canceled" => to == "todo",
        _ => false,
    }
}

fn parse_date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "日期格式无效".to_owned())
}

fn parse_time(value: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(value, "%H:%M").map_err(|_| "时间格式无效".to_owned())
}

fn parse_db_local(value: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| "数据库中的本地时间无效".to_owned())
}

fn occurrence_original_local(key: &OccurrenceKey) -> NaiveDateTime {
    match key {
        OccurrenceKey::AllDay { original_date } => original_date.and_time(NaiveTime::MIN),
        OccurrenceKey::Floating { original_local }
        | OccurrenceKey::Zoned { original_local, .. } => *original_local,
    }
}

fn format_local(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S%.9f").to_string()
}

fn utc_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

const fn mode_text(mode: TimeMode) -> &'static str {
    match mode {
        TimeMode::AllDay => "all_day",
        TimeMode::Floating => "floating",
        TimeMode::Zoned => "zoned",
    }
}

fn task_sort_key(task: &TaskItem) -> String {
    format!(
        "{}|{}|{}",
        task.scheduled_date
            .as_deref()
            .or(task.scheduled_local.as_deref())
            .unwrap_or("9999"),
        priority_order(&task.priority),
        task.title
    )
}

const fn priority_order(priority: &str) -> u8 {
    match priority.as_bytes() {
        b"p0" => 0,
        b"p1" => 1,
        b"p2" => 2,
        _ => 3,
    }
}

fn db_error(error: sqlx::Error) -> String {
    format!("本地数据库操作失败：{error}")
}

fn json_error(error: serde_json::Error) -> String {
    format!("数据格式错误：{error}")
}

#[cfg(test)]
mod tests {
    use super::{rule_for_split, valid_status_transition};
    use crate::core::{RecurrenceDefinition, SeriesStart};
    use chrono::{NaiveDate, NaiveDateTime};
    use uuid::Uuid;

    #[test]
    fn task_state_machine_rejects_skipping_back_to_todo() {
        assert!(valid_status_transition("todo", "in_progress"));
        assert!(valid_status_transition("blocked", "todo"));
        assert!(valid_status_transition("completed", "todo"));
        assert!(!valid_status_transition("in_progress", "todo"));
        assert!(!valid_status_transition("completed", "blocked"));
    }

    #[test]
    fn splitting_a_counted_series_preserves_the_total_occurrence_budget() {
        let definition = RecurrenceDefinition {
            series_id: Uuid::now_v7(),
            start: SeriesStart::AllDay {
                date: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            },
            rrule: "FREQ=DAILY;COUNT=5".to_owned(),
            rdates: Vec::new(),
            exdates: Vec::new(),
        };
        let split =
            NaiveDateTime::parse_from_str("2026-09-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(
            rule_for_split(&definition, split, &definition.rrule).unwrap(),
            "FREQ=DAILY;COUNT=3"
        );
    }
}
