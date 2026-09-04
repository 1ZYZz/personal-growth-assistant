use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Acquire, Row};
use tauri::State;
use uuid::Uuid;

use crate::{
    core::{resolve_zoned_local, CoreStore},
    product::{list_tasks_for_store, AppState, TaskItem, TaskQuery},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutomationSettings {
    pub morning_enabled: bool,
    pub morning_time: String,
    pub evening_enabled: bool,
    pub evening_time: String,
    pub weekly_enabled: bool,
    pub weekly_weekday: u32,
    pub weekly_time: String,
    pub intelligence_enabled: bool,
    pub intelligence_interval_hours: u32,
    pub lesson_enabled: bool,
    pub lesson_time: String,
}

impl Default for AutomationSettings {
    fn default() -> Self {
        Self {
            morning_enabled: true,
            morning_time: "08:00".to_owned(),
            evening_enabled: true,
            evening_time: "21:30".to_owned(),
            weekly_enabled: true,
            weekly_weekday: 6,
            weekly_time: "20:30".to_owned(),
            intelligence_enabled: true,
            intelligence_interval_hours: 6,
            lesson_enabled: true,
            lesson_time: "07:45".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestItem {
    pub id: String,
    pub digest_type: String,
    pub period_start: String,
    pub period_end: String,
    pub content: serde_json::Value,
    pub generator: String,
    pub created_at_utc: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonPlanItem {
    pub id: String,
    pub node_id: Option<String>,
    pub item_kind: String,
    pub title: String,
    pub minutes: i64,
    pub sort_order: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonPlan {
    pub id: String,
    pub plan_date: String,
    pub goal_id: String,
    pub goal_name: String,
    pub budget_minutes: i64,
    pub planned_minutes: i64,
    pub task_load: String,
    pub status: String,
    pub version: i64,
    pub items: Vec<LessonPlanItem>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalPreview {
    pub id: String,
    pub proposal_type: String,
    pub diff: serde_json::Value,
    pub status: String,
    pub entity_version: Option<i64>,
    pub expires_at_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_token: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogItem {
    pub id: String,
    pub category: String,
    pub level: String,
    pub event_name: String,
    pub correlation_id: String,
    pub message: String,
    pub created_at_utc: String,
}

#[tauri::command]
pub async fn get_automation_settings(
    state: State<'_, AppState>,
) -> Result<AutomationSettings, String> {
    load_automation_settings(&state.store).await
}

#[tauri::command]
pub async fn save_automation_settings(
    state: State<'_, AppState>,
    input: AutomationSettings,
) -> Result<AutomationSettings, String> {
    validate_automation_settings(&input)?;
    let now = utc_text(Utc::now());
    let value = serde_json::to_string(&input).map_err(json_error)?;
    sqlx::query(
        "INSERT INTO settings(key, value_json, updated_at_utc, version)
         VALUES ('automation_settings', ?, ?, 1)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
             updated_at_utc = excluded.updated_at_utc, version = settings.version + 1",
    )
    .bind(value)
    .bind(&now)
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "UPDATE jobs SET state = 'cancelled', updated_at_utc = ?, version = version + 1
         WHERE kind IN ('morning_digest', 'evening_digest', 'weekly_digest',
                        'intelligence_refresh', 'lesson_plan')
           AND state = 'pending'",
    )
    .bind(&now)
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?;
    ensure_automation_jobs(&state.store, Utc::now()).await?;
    state.scheduler_wake.send_modify(|version| *version += 1);
    Ok(input)
}

#[tauri::command]
pub async fn generate_digest(
    state: State<'_, AppState>,
    digest_type: String,
    date: String,
) -> Result<DigestItem, String> {
    let date = parse_date(&date)?;
    generate_digest_for_store(&state.store, &digest_type, date).await
}

#[tauri::command]
pub async fn list_digests(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<DigestItem>, String> {
    let rows = sqlx::query(
        "SELECT id, digest_type, period_start, period_end, content_json, generator, created_at_utc
         FROM digests ORDER BY period_end DESC, created_at_utc DESC LIMIT ?",
    )
    .bind(limit.unwrap_or(30).clamp(1, 200))
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(row_to_digest).collect()
}

#[tauri::command]
pub async fn generate_daily_lesson(
    state: State<'_, AppState>,
    date: String,
) -> Result<Vec<LessonPlan>, String> {
    let date = parse_date(&date)?;
    generate_lesson_plans_for_store(&state.store, date).await?;
    list_lesson_plans_for_store(&state.store, date).await
}

#[tauri::command]
pub async fn list_daily_lessons(
    state: State<'_, AppState>,
    date: String,
) -> Result<Vec<LessonPlan>, String> {
    list_lesson_plans_for_store(&state.store, parse_date(&date)?).await
}

#[tauri::command]
pub async fn propose_lesson_tasks(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<ProposalPreview, String> {
    let plan_id = parse_uuid(&plan_id, "课程计划标识")?;
    let plan = sqlx::query(
        "SELECT p.plan_date, p.version, p.status, g.name AS goal_name
         FROM lesson_plans p JOIN learning_goals g ON g.id = p.goal_id WHERE p.id = ?",
    )
    .bind(plan_id.to_string())
    .fetch_optional(&state.store.pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "课程计划不存在".to_owned())?;
    let version: i64 = plan.try_get("version").map_err(db_error)?;
    let plan_date: String = plan.try_get("plan_date").map_err(db_error)?;
    let goal_name: String = plan.try_get("goal_name").map_err(db_error)?;
    let rows = sqlx::query(
        "SELECT title, minutes, item_kind FROM lesson_plan_items
         WHERE plan_id = ? ORDER BY sort_order",
    )
    .bind(plan_id.to_string())
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    if rows.is_empty() {
        return Err("课程计划没有可转换的项目".to_owned());
    }
    let tasks = rows
        .iter()
        .map(|row| {
            let title: String = row.try_get("title").map_err(db_error)?;
            let minutes: i64 = row.try_get("minutes").map_err(db_error)?;
            let kind: String = row.try_get("item_kind").map_err(db_error)?;
            Ok(serde_json::json!({
                "title": format!("{goal_name} · {title}"),
                "date": plan_date,
                "estimatedMinutes": minutes,
                "priority": "p2",
                "notes": format!("由每日课程建议转换（{kind}）")
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let proposal_id = Uuid::now_v7();
    let raw_token = format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple());
    let token_hash = hex_hash(raw_token.as_bytes());
    let now = Utc::now();
    let expires = now + Duration::minutes(5);
    let diff = serde_json::json!({"planId": plan_id, "tasks": tasks});
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    sqlx::query(
        "INSERT INTO proposals(id, proposal_type, diff_json, entity_version, created_at_utc, expires_at_utc)
         VALUES (?, 'lesson_to_tasks', ?, ?, ?, ?)",
    )
    .bind(proposal_id.to_string())
    .bind(diff.to_string())
    .bind(version)
    .bind(utc_text(now))
    .bind(utc_text(expires))
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO approvals(id, proposal_id, token_hash, expires_at_utc, created_at_utc)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(proposal_id.to_string())
    .bind(token_hash)
    .bind(utc_text(expires))
    .bind(utc_text(now))
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    Ok(ProposalPreview {
        id: proposal_id.to_string(),
        proposal_type: "lesson_to_tasks".to_owned(),
        diff,
        status: "pending".to_owned(),
        entity_version: Some(version),
        expires_at_utc: utc_text(expires),
        approval_token: Some(raw_token),
    })
}

#[tauri::command]
pub async fn apply_proposal(
    state: State<'_, AppState>,
    proposal_id: String,
    approval_token: String,
) -> Result<Vec<String>, String> {
    let proposal_id = parse_uuid(&proposal_id, "提案标识")?;
    if approval_token.len() != 64 || !approval_token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("APPROVAL_REQUIRED: 审批令牌无效".to_owned());
    }
    let now = Utc::now();
    let now_text = utc_text(now);
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    let proposal = sqlx::query(
        "SELECT proposal_type, diff_json, status, entity_version, expires_at_utc
         FROM proposals WHERE id = ?",
    )
    .bind(proposal_id.to_string())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "NOT_FOUND: 提案不存在".to_owned())?;
    let status: String = proposal.try_get("status").map_err(db_error)?;
    if status == "applied" {
        return sqlx::query_scalar(
            "SELECT task_id FROM proposal_applications WHERE proposal_id = ? ORDER BY task_id",
        )
        .bind(proposal_id.to_string())
        .fetch_all(&mut *transaction)
        .await
        .map_err(db_error);
    }
    if status != "pending" {
        return Err("APPROVAL_REQUIRED: 提案已不再等待审批".to_owned());
    }
    let expires: String = proposal.try_get("expires_at_utc").map_err(db_error)?;
    if parse_utc(&expires)? <= now {
        sqlx::query("UPDATE proposals SET status = 'expired' WHERE id = ?")
            .bind(proposal_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        transaction.commit().await.map_err(db_error)?;
        return Err("APPROVAL_REQUIRED: 提案已过期，请重新预览".to_owned());
    }
    let token_hash = hex_hash(approval_token.as_bytes());
    let consumed = sqlx::query(
        "UPDATE approvals SET consumed_at_utc = ?
         WHERE proposal_id = ? AND token_hash = ? AND consumed_at_utc IS NULL AND expires_at_utc > ?",
    )
    .bind(&now_text)
    .bind(proposal_id.to_string())
    .bind(token_hash)
    .bind(&now_text)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?
    .rows_affected();
    if consumed != 1 {
        return Err("APPROVAL_REQUIRED: 审批令牌缺失、已使用或不匹配".to_owned());
    }
    let proposal_type: String = proposal.try_get("proposal_type").map_err(db_error)?;
    if proposal_type != "lesson_to_tasks" {
        return Err("INVALID_INPUT: 不支持的提案类型".to_owned());
    }
    let raw_diff: String = proposal.try_get("diff_json").map_err(db_error)?;
    let diff: serde_json::Value = serde_json::from_str(&raw_diff).map_err(json_error)?;
    let plan_id = diff
        .get("planId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "INVALID_INPUT: 提案内容不完整".to_owned())?;
    let expected_version: i64 = proposal
        .try_get::<Option<i64>, _>("entity_version")
        .map_err(db_error)?
        .ok_or_else(|| "INVALID_INPUT: 提案缺少版本".to_owned())?;
    let current_version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM lesson_plans WHERE id = ?")
            .bind(plan_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(db_error)?;
    if current_version != Some(expected_version) {
        return Err("VERSION_CONFLICT: 课程计划已被修改，请重新预览".to_owned());
    }
    let tasks = diff
        .get("tasks")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "INVALID_INPUT: 提案任务列表无效".to_owned())?;
    let mut created = Vec::with_capacity(tasks.len());
    for task in tasks {
        let title = task
            .get("title")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "INVALID_INPUT: 提案任务标题无效".to_owned())?;
        let date = task
            .get("date")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "INVALID_INPUT: 提案任务日期无效".to_owned())?;
        parse_date(date)?;
        let minutes = task
            .get("estimatedMinutes")
            .and_then(serde_json::Value::as_i64)
            .filter(|value| (1..=1440).contains(value))
            .ok_or_else(|| "INVALID_INPUT: 提案任务时长无效".to_owned())?;
        let task_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO tasks(
                id, title, notes, priority, estimated_minutes, status, time_mode,
                scheduled_date, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, 'p2', ?, 'todo', 'all_day', ?, ?, ?)",
        )
        .bind(task_id.to_string())
        .bind(title.trim())
        .bind(task.get("notes").and_then(serde_json::Value::as_str))
        .bind(minutes)
        .bind(date)
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        sqlx::query(
            "INSERT INTO task_events(id, task_id, event_type, payload_json, created_at_utc)
             VALUES (?, ?, 'created_from_approved_proposal', ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(task_id.to_string())
        .bind(serde_json::json!({"proposalId": proposal_id}).to_string())
        .bind(&now_text)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        sqlx::query(
            "INSERT INTO proposal_applications(proposal_id, task_id, created_at_utc)
             VALUES (?, ?, ?)",
        )
        .bind(proposal_id.to_string())
        .bind(task_id.to_string())
        .bind(&now_text)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        created.push(task_id.to_string());
    }
    sqlx::query("UPDATE proposals SET status = 'applied' WHERE id = ?")
        .bind(proposal_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    sqlx::query(
        "UPDATE lesson_plans SET status = 'approved', updated_at_utc = ?, version = version + 1
         WHERE id = ? AND version = ?",
    )
    .bind(&now_text)
    .bind(plan_id)
    .bind(expected_version)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO audit_events(id, actor, action, entity_type, entity_id, payload_json, created_at_utc)
         VALUES (?, 'user', 'proposal_applied', 'proposal', ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(proposal_id.to_string())
    .bind(serde_json::json!({"createdTaskIds": created}).to_string())
    .bind(&now_text)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    state.scheduler_wake.send_modify(|version| *version += 1);
    Ok(created)
}

#[tauri::command]
pub async fn reject_proposal(
    state: State<'_, AppState>,
    proposal_id: String,
) -> Result<(), String> {
    let proposal_id = parse_uuid(&proposal_id, "提案标识")?;
    let changed =
        sqlx::query("UPDATE proposals SET status = 'rejected' WHERE id = ? AND status = 'pending'")
            .bind(proposal_id.to_string())
            .execute(&state.store.pool)
            .await
            .map_err(db_error)?
            .rows_affected();
    (changed == 1)
        .then_some(())
        .ok_or_else(|| "提案不存在或已处理".to_owned())
}

#[tauri::command]
pub async fn list_recent_failures(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<LogItem>, String> {
    let rows = sqlx::query(
        "SELECT id, category, level, event_name, correlation_id, message, created_at_utc
         FROM app_logs WHERE level IN ('warning', 'error')
         ORDER BY created_at_utc DESC LIMIT ?",
    )
    .bind(limit.unwrap_or(50).clamp(1, 200))
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok(LogItem {
                id: row.try_get("id").map_err(db_error)?,
                category: row.try_get("category").map_err(db_error)?,
                level: row.try_get("level").map_err(db_error)?,
                event_name: row.try_get("event_name").map_err(db_error)?,
                correlation_id: row.try_get("correlation_id").map_err(db_error)?,
                message: row.try_get("message").map_err(db_error)?,
                created_at_utc: row.try_get("created_at_utc").map_err(db_error)?,
            })
        })
        .collect()
}

pub(crate) async fn load_automation_settings(
    store: &CoreStore,
) -> Result<AutomationSettings, String> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT value_json FROM settings WHERE key = 'automation_settings'")
            .fetch_optional(&store.pool)
            .await
            .map_err(db_error)?;
    let value = raw
        .map(|raw| serde_json::from_str(&raw).map_err(json_error))
        .unwrap_or_else(|| Ok(AutomationSettings::default()))?;
    validate_automation_settings(&value)?;
    Ok(value)
}

pub(crate) async fn ensure_automation_jobs(
    store: &CoreStore,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let settings = load_automation_settings(store).await?;
    let timezone = crate::data::load_preferences(store).await?.app_timezone;
    let tz = timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| "应用时区无效".to_owned())?;
    let today = now.with_timezone(&tz).date_naive();
    let current_hour = now
        .with_timezone(&tz)
        .format("%H")
        .to_string()
        .parse::<u32>()
        .unwrap_or(0);
    for offset in 0..=7 {
        let date = today + Duration::days(offset);
        if settings.morning_enabled {
            enqueue_local_job(
                store,
                "morning_digest",
                date,
                &settings.morning_time,
                &timezone,
            )
            .await?;
        }
        if settings.evening_enabled {
            enqueue_local_job(
                store,
                "evening_digest",
                date,
                &settings.evening_time,
                &timezone,
            )
            .await?;
        }
        if settings.lesson_enabled {
            enqueue_local_job(store, "lesson_plan", date, &settings.lesson_time, &timezone).await?;
        }
        if settings.weekly_enabled
            && date.weekday().num_days_from_monday() == settings.weekly_weekday
        {
            enqueue_local_job(
                store,
                "weekly_digest",
                date,
                &settings.weekly_time,
                &timezone,
            )
            .await?;
        }
        if settings.intelligence_enabled {
            let step = settings.intelligence_interval_hours.max(1) as usize;
            for hour in (0..24).step_by(step) {
                if offset == 0
                    && (hour as u32 + settings.intelligence_interval_hours) <= current_hour
                {
                    continue;
                }
                enqueue_local_job(
                    store,
                    "intelligence_refresh",
                    date,
                    &format!("{hour:02}:00"),
                    &timezone,
                )
                .await?;
            }
        }
    }
    sqlx::query(
        "DELETE FROM app_logs WHERE julianday(created_at_utc) < julianday('now', '-14 days')",
    )
    .execute(&store.pool)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "DELETE FROM app_logs WHERE id IN (
            SELECT id FROM (
                SELECT id,
                    SUM(length(message) + length(details_json) + 256)
                    OVER (ORDER BY created_at_utc DESC, id DESC) AS retained_bytes
                FROM app_logs
            ) WHERE retained_bytes > 104857600
         )",
    )
    .execute(&store.pool)
    .await
    .map_err(db_error)?;
    Ok(())
}

pub(crate) async fn process_automation_job(
    store: &CoreStore,
    kind: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let date = payload
        .get("date")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "自动作业缺少日期".to_owned())?;
    let date = parse_date(date)?;
    match kind {
        "morning_digest" => {
            generate_digest_for_store(store, "morning", date).await?;
        }
        "evening_digest" => {
            generate_digest_for_store(store, "evening", date).await?;
        }
        "weekly_digest" => {
            generate_digest_for_store(store, "weekly", date).await?;
        }
        "lesson_plan" => generate_lesson_plans_for_store(store, date).await?,
        "intelligence_refresh" => {
            crate::intelligence::refresh_intelligence_for_store(store, None, "scheduled").await?;
        }
        _ => return Err(format!("未知自动作业类型：{kind}")),
    }
    Ok(())
}

pub(crate) async fn write_log(
    store: &CoreStore,
    category: &str,
    level: &str,
    event_name: &str,
    correlation_id: &str,
    message: &str,
    details: serde_json::Value,
) {
    if !matches!(
        category,
        "app" | "scheduler" | "database" | "agent" | "ingestion" | "notification"
    ) || !matches!(level, "info" | "warning" | "error")
    {
        return;
    }
    let _ = sqlx::query(
        "INSERT INTO app_logs(
            id, category, level, event_name, correlation_id, message, details_json, created_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(category)
    .bind(level)
    .bind(event_name)
    .bind(correlation_id)
    .bind(message.chars().take(1000).collect::<String>())
    .bind(details.to_string())
    .bind(utc_text(Utc::now()))
    .execute(&store.pool)
    .await;
}

async fn enqueue_local_job(
    store: &CoreStore,
    kind: &str,
    date: NaiveDate,
    time: &str,
    timezone: &str,
) -> Result<(), String> {
    let local_time = parse_time(time)?;
    let scheduled = resolve_zoned_local(date.and_time(local_time), timezone)
        .map_err(|error| error.to_string())?
        .scheduled_utc;
    let scheduled_text = utc_text(scheduled);
    let business_key = format!(
        "automation:{kind}:{}:{time}:{timezone}",
        date.format("%Y-%m-%d")
    );
    let now = utc_text(Utc::now());
    sqlx::query(
        "INSERT INTO jobs(
            id, kind, owner, business_key, scheduled_for_utc, attempt_class,
            run_at_utc, state, payload_json, created_at_utc, updated_at_utc
         ) VALUES (?, ?, 'local', ?, ?, 'primary', ?, 'pending', ?, ?, ?)
         ON CONFLICT(business_key, scheduled_for_utc, attempt_class) DO UPDATE SET
             state = 'pending', run_at_utc = excluded.run_at_utc,
             updated_at_utc = excluded.updated_at_utc, version = jobs.version + 1
         WHERE jobs.state = 'cancelled'",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(kind)
    .bind(business_key)
    .bind(&scheduled_text)
    .bind(&scheduled_text)
    .bind(serde_json::json!({"date": date.format("%Y-%m-%d").to_string()}).to_string())
    .bind(&now)
    .bind(&now)
    .execute(&store.pool)
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn generate_digest_for_store(
    store: &CoreStore,
    digest_type: &str,
    date: NaiveDate,
) -> Result<DigestItem, String> {
    if !matches!(digest_type, "morning" | "evening" | "weekly" | "industry") {
        return Err("简报类型无效".to_owned());
    }
    let (period_start, period_end) = if digest_type == "weekly" {
        let start = date - Duration::days(date.weekday().num_days_from_monday() as i64);
        (start, start + Duration::days(6))
    } else {
        (date, date)
    };
    let task_end = period_end + Duration::days(1);
    let tasks = list_tasks_for_store(
        store,
        TaskQuery {
            range_start: period_start.format("%Y-%m-%d").to_string(),
            range_end: task_end.format("%Y-%m-%d").to_string(),
            include_completed: true,
        },
    )
    .await?;
    let active = tasks
        .iter()
        .filter(|task| !matches!(task.status.as_str(), "completed" | "canceled"))
        .collect::<Vec<_>>();
    let completed = tasks
        .iter()
        .filter(|task| task.status == "completed")
        .count();
    let blocked = tasks.iter().filter(|task| task.status == "blocked").count();
    let canceled = tasks
        .iter()
        .filter(|task| task.status == "canceled")
        .count();
    let mut priorities = active.clone();
    priorities.sort_by_key(|task| {
        (
            priority_rank(&task.priority),
            task.estimated_minutes.unwrap_or(i64::MAX),
        )
    });
    priorities.truncate(3);
    let priority_json = priorities.iter().map(task_summary).collect::<Vec<_>>();
    let news = sqlx::query(
        "SELECT n.title, n.summary, n.published_at_utc, n.score, n.source_id, s.name AS source_name
         FROM news_items n JOIN watch_sources s ON s.id = n.source_id
         WHERE n.feedback IS NULL AND COALESCE(n.published_at_utc, n.created_at_utc) >= ?
         ORDER BY n.score DESC LIMIT 5",
    )
    .bind(utc_text(Utc::now() - Duration::days(7)))
    .fetch_all(&store.pool)
    .await
    .map_err(db_error)?;
    let news_json = news
        .iter()
        .map(|row| {
            Ok(serde_json::json!({
                "title": row.try_get::<String, _>("title").map_err(db_error)?,
                "summary": row.try_get::<String, _>("summary").map_err(db_error)?,
                "publishedAtUtc": row.try_get::<Option<String>, _>("published_at_utc").map_err(db_error)?,
                "score": row.try_get::<f64, _>("score").map_err(db_error)?,
                "sourceId": row.try_get::<String, _>("source_id").map_err(db_error)?,
                "sourceName": row.try_get::<String, _>("source_name").map_err(db_error)?
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let due_reviews: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fsrs_cards c
         JOIN quiz_items q ON q.id = c.quiz_item_id
         JOIN knowledge_nodes n ON n.id = q.node_id
         JOIN learning_goals g ON g.id = n.goal_id
         WHERE c.due_utc <= ? AND n.deleted_at_utc IS NULL AND g.deleted_at_utc IS NULL",
    )
    .bind(utc_text(Utc::now()))
    .fetch_one(&store.pool)
    .await
    .map_err(db_error)?;
    let content = match digest_type {
        "morning" => serde_json::json!({
            "headline": priorities.first().map(|task| format!("今天先完成：{}", task.title)).unwrap_or_else(|| "今天没有已安排的核心任务".to_owned()),
            "priorities": priority_json,
            "risks": active.iter().filter(|task| task.status == "blocked").map(task_summary).collect::<Vec<_>>(),
            "deferCandidates": active.iter().filter(|task| task.priority == "p3").map(task_summary).collect::<Vec<_>>(),
            "industry": news_json,
            "missingInformation": if tasks.is_empty() { vec!["尚未安排今日任务"] } else { Vec::<&str>::new() }
        }),
        "evening" => serde_json::json!({
            "headline": if tasks.is_empty() { "今天没有可复盘的任务" } else { "请核对尚未明确结论的任务" },
            "statistics": {"planned": tasks.len(), "completed": completed, "blocked": blocked, "canceled": canceled},
            "needsReview": active.iter().filter(|task| matches!(task.priority.as_str(), "p0" | "p1")).map(task_summary).collect::<Vec<_>>(),
            "tone": "factual-supportive"
        }),
        "weekly" => serde_json::json!({
            "headline": "本周事实复盘",
            "statistics": {"planned": tasks.len(), "completed": completed, "blocked": blocked, "canceled": canceled},
            "completionRate": if tasks.is_empty() { serde_json::Value::Null } else { serde_json::json!(completed as f64 / tasks.len() as f64) },
            "plannedMinutes": tasks.iter().filter_map(|task| task.estimated_minutes).sum::<i64>(),
            "industryTrends": news_json.into_iter().take(3).collect::<Vec<_>>(),
            "learning": {"dueReviews": due_reviews},
            "nextWeekCandidates": priority_json
        }),
        _ => {
            serde_json::json!({"headline": if news_json.is_empty() { "没有重要更新" } else { "行业动态" }, "items": news_json})
        }
    };
    let id = Uuid::now_v7();
    let now = utc_text(Utc::now());
    sqlx::query(
        "INSERT INTO digests(id, digest_type, period_start, period_end, content_json, generator, created_at_utc)
         VALUES (?, ?, ?, ?, ?, 'local-rules-v1', ?)
         ON CONFLICT(digest_type, period_start, period_end) DO UPDATE SET
             content_json = excluded.content_json, generator = excluded.generator,
             created_at_utc = excluded.created_at_utc",
    )
    .bind(id.to_string())
    .bind(digest_type)
    .bind(period_start.format("%Y-%m-%d").to_string())
    .bind(period_end.format("%Y-%m-%d").to_string())
    .bind(content.to_string())
    .bind(&now)
    .execute(&store.pool)
    .await
    .map_err(db_error)?;
    let row = sqlx::query(
        "SELECT id, digest_type, period_start, period_end, content_json, generator, created_at_utc
         FROM digests WHERE digest_type = ? AND period_start = ? AND period_end = ?",
    )
    .bind(digest_type)
    .bind(period_start.format("%Y-%m-%d").to_string())
    .bind(period_end.format("%Y-%m-%d").to_string())
    .fetch_one(&store.pool)
    .await
    .map_err(db_error)?;
    row_to_digest(&row)
}

async fn generate_lesson_plans_for_store(store: &CoreStore, date: NaiveDate) -> Result<(), String> {
    let task_end = date + Duration::days(1);
    let tasks = list_tasks_for_store(
        store,
        TaskQuery {
            range_start: date.format("%Y-%m-%d").to_string(),
            range_end: task_end.format("%Y-%m-%d").to_string(),
            include_completed: false,
        },
    )
    .await?;
    let p0 = tasks.iter().filter(|task| task.priority == "p0").count();
    let p1 = tasks.iter().filter(|task| task.priority == "p1").count();
    let task_load = if p0 > 0 || p1 >= 2 {
        "heavy"
    } else if p1 == 1 {
        "normal"
    } else {
        "light"
    };
    let goals = sqlx::query(
        "SELECT id, name, daily_minutes FROM learning_goals
         WHERE status = 'active' AND deleted_at_utc IS NULL ORDER BY created_at_utc",
    )
    .fetch_all(&store.pool)
    .await
    .map_err(db_error)?;
    for goal in goals {
        let goal_id: String = goal.try_get("id").map_err(db_error)?;
        let daily: i64 = goal.try_get("daily_minutes").map_err(db_error)?;
        let budget = match task_load {
            "heavy" => (daily / 2).max(10),
            "normal" => (daily * 3 / 4).max(10),
            _ => daily,
        };
        let candidates = sqlx::query(
            "SELECT n.id, n.name, n.status, n.estimated_minutes,
                    COUNT(DISTINCT CASE WHEN c.due_utc <= ? THEN c.id END) AS due_count
             FROM knowledge_nodes n
             LEFT JOIN quiz_items q ON q.node_id = n.id
             LEFT JOIN fsrs_cards c ON c.quiz_item_id = q.id
             WHERE n.goal_id = ? AND n.deleted_at_utc IS NULL AND n.status != 'mastered'
               AND NOT EXISTS (
                   SELECT 1 FROM knowledge_edges e JOIN knowledge_nodes p ON p.id = e.prerequisite_node_id
                   WHERE e.dependent_node_id = n.id AND p.status NOT IN ('basic', 'mastered')
               )
             GROUP BY n.id
             ORDER BY CASE WHEN due_count > 0 THEN 0 ELSE 1 END, n.sort_order, n.created_at_utc",
        )
        .bind(utc_text(Utc::now()))
        .bind(&goal_id)
        .fetch_all(&store.pool)
        .await
        .map_err(db_error)?;
        let plan_id: String =
            sqlx::query_scalar("SELECT id FROM lesson_plans WHERE plan_date = ? AND goal_id = ?")
                .bind(date.format("%Y-%m-%d").to_string())
                .bind(&goal_id)
                .fetch_optional(&store.pool)
                .await
                .map_err(db_error)?
                .unwrap_or_else(|| Uuid::now_v7().to_string());
        let mut items: Vec<(Option<String>, &str, String, i64)> = Vec::new();
        let mut remaining = budget;
        if let Some(node) = candidates
            .iter()
            .find(|row| row.try_get::<i64, _>("due_count").unwrap_or(0) > 0)
        {
            let minutes = remaining.min(10);
            if minutes >= 5 {
                let node_id: String = node.try_get("id").map_err(db_error)?;
                let name: String = node.try_get("name").map_err(db_error)?;
                items.push((Some(node_id), "review", format!("复习：{name}"), minutes));
                remaining -= minutes;
            }
        }
        if let Some(node) = candidates.first() {
            let node_id: String = node.try_get("id").map_err(db_error)?;
            let name: String = node.try_get("name").map_err(db_error)?;
            let estimated: Option<i64> = node.try_get("estimated_minutes").map_err(db_error)?;
            let minutes = remaining.min(estimated.unwrap_or(30).clamp(10, 40));
            if minutes >= 10 {
                items.push((
                    Some(node_id.clone()),
                    "learn",
                    format!("学习：{name}"),
                    minutes,
                ));
                remaining -= minutes;
            }
            let practice = remaining.min(20);
            if practice >= 10 {
                items.push((
                    Some(node_id.clone()),
                    "practice",
                    format!("练习：{name}"),
                    practice,
                ));
                remaining -= practice;
            }
            let quiz = remaining.min(5);
            if quiz >= 3 {
                items.push((Some(node_id), "quiz", format!("快速测验：{name}"), quiz));
            }
        }
        let planned: i64 = items.iter().map(|item| item.3).sum();
        let now = utc_text(Utc::now());
        let mut connection = store.pool.acquire().await.map_err(db_error)?;
        let mut transaction = connection.begin().await.map_err(db_error)?;
        sqlx::query(
            "INSERT INTO lesson_plans(
                id, plan_date, goal_id, budget_minutes, planned_minutes, task_load,
                status, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, ?, ?, ?, 'suggested', ?, ?)
             ON CONFLICT(plan_date, goal_id) DO UPDATE SET
                budget_minutes = excluded.budget_minutes,
                planned_minutes = excluded.planned_minutes,
                task_load = excluded.task_load,
                updated_at_utc = excluded.updated_at_utc,
                version = lesson_plans.version + 1",
        )
        .bind(&plan_id)
        .bind(date.format("%Y-%m-%d").to_string())
        .bind(&goal_id)
        .bind(budget)
        .bind(planned)
        .bind(task_load)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        sqlx::query("DELETE FROM lesson_plan_items WHERE plan_id = ?")
            .bind(&plan_id)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        for (index, (node_id, kind, title, minutes)) in items.into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO lesson_plan_items(
                    id, plan_id, node_id, item_kind, title, minutes, sort_order, created_at_utc
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind(&plan_id)
            .bind(node_id)
            .bind(kind)
            .bind(title)
            .bind(minutes)
            .bind(index as i64)
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        }
        transaction.commit().await.map_err(db_error)?;
    }
    Ok(())
}

async fn list_lesson_plans_for_store(
    store: &CoreStore,
    date: NaiveDate,
) -> Result<Vec<LessonPlan>, String> {
    let rows = sqlx::query(
        "SELECT p.id, p.plan_date, p.goal_id, g.name AS goal_name, p.budget_minutes,
                p.planned_minutes, p.task_load, p.status, p.version
         FROM lesson_plans p JOIN learning_goals g ON g.id = p.goal_id
         WHERE p.plan_date = ? ORDER BY g.created_at_utc",
    )
    .bind(date.format("%Y-%m-%d").to_string())
    .fetch_all(&store.pool)
    .await
    .map_err(db_error)?;
    let mut plans = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id").map_err(db_error)?;
        let item_rows = sqlx::query(
            "SELECT id, node_id, item_kind, title, minutes, sort_order
             FROM lesson_plan_items WHERE plan_id = ? ORDER BY sort_order",
        )
        .bind(&id)
        .fetch_all(&store.pool)
        .await
        .map_err(db_error)?;
        let items = item_rows
            .iter()
            .map(|item| {
                Ok(LessonPlanItem {
                    id: item.try_get("id").map_err(db_error)?,
                    node_id: item.try_get("node_id").map_err(db_error)?,
                    item_kind: item.try_get("item_kind").map_err(db_error)?,
                    title: item.try_get("title").map_err(db_error)?,
                    minutes: item.try_get("minutes").map_err(db_error)?,
                    sort_order: item.try_get("sort_order").map_err(db_error)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        plans.push(LessonPlan {
            id,
            plan_date: row.try_get("plan_date").map_err(db_error)?,
            goal_id: row.try_get("goal_id").map_err(db_error)?,
            goal_name: row.try_get("goal_name").map_err(db_error)?,
            budget_minutes: row.try_get("budget_minutes").map_err(db_error)?,
            planned_minutes: row.try_get("planned_minutes").map_err(db_error)?,
            task_load: row.try_get("task_load").map_err(db_error)?,
            status: row.try_get("status").map_err(db_error)?,
            version: row.try_get("version").map_err(db_error)?,
            items,
        });
    }
    Ok(plans)
}

fn task_summary(task: &&TaskItem) -> serde_json::Value {
    serde_json::json!({
        "taskId": task.id,
        "seriesId": task.series_id,
        "occurrenceKey": task.occurrence_key,
        "title": task.title,
        "priority": task.priority,
        "status": task.status,
        "estimatedMinutes": task.estimated_minutes,
        "completionCriteria": task.completion_criteria,
        "firstStep": "用 5 分钟打开所需材料并写下下一步"
    })
}

fn row_to_digest(row: &sqlx::sqlite::SqliteRow) -> Result<DigestItem, String> {
    let raw: String = row.try_get("content_json").map_err(db_error)?;
    Ok(DigestItem {
        id: row.try_get("id").map_err(db_error)?,
        digest_type: row.try_get("digest_type").map_err(db_error)?,
        period_start: row.try_get("period_start").map_err(db_error)?,
        period_end: row.try_get("period_end").map_err(db_error)?,
        content: serde_json::from_str(&raw).map_err(json_error)?,
        generator: row.try_get("generator").map_err(db_error)?,
        created_at_utc: row.try_get("created_at_utc").map_err(db_error)?,
    })
}

fn validate_automation_settings(value: &AutomationSettings) -> Result<(), String> {
    parse_time(&value.morning_time)?;
    parse_time(&value.evening_time)?;
    parse_time(&value.weekly_time)?;
    parse_time(&value.lesson_time)?;
    if value.weekly_weekday > 6 || !(1..=24).contains(&value.intelligence_interval_hours) {
        return Err("自动作业设置无效".to_owned());
    }
    Ok(())
}

fn parse_time(value: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(value, "%H:%M").map_err(|_| "自动作业时间无效".to_owned())
}

fn parse_date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "日期格式无效".to_owned())
}

fn parse_uuid(value: &str, label: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| format!("{label}无效"))
}

const fn priority_rank(priority: &str) -> u8 {
    match priority.as_bytes() {
        b"p0" => 0,
        b"p1" => 1,
        b"p2" => 2,
        _ => 3,
    }
}

fn hex_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_utc(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| "UTC 时间无效".to_owned())
}

fn utc_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn db_error(error: sqlx::Error) -> String {
    format!("本地数据库操作失败：{error}")
}

fn json_error(error: serde_json::Error) -> String {
    format!("本地结构化数据无效：{error}")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn test_path() -> PathBuf {
        std::env::temp_dir().join(format!("pga-supervision-{}.sqlite3", Uuid::now_v7()))
    }

    #[test]
    fn automation_defaults_are_valid_and_bounded() {
        let defaults = AutomationSettings::default();
        assert!(validate_automation_settings(&defaults).is_ok());
        assert_eq!(defaults.intelligence_interval_hours, 6);
    }

    #[test]
    fn approval_tokens_are_256_bit_hex_values() {
        let token = format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple());
        assert_eq!(token.len(), 64);
        assert_eq!(hex_hash(token.as_bytes()).len(), 64);
    }

    #[tokio::test]
    async fn automation_jobs_are_idempotent_for_the_same_schedule() {
        let path = test_path();
        let store = CoreStore::connect_for_test(&path).await.unwrap();
        let now = DateTime::parse_from_rfc3339("2026-09-04T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        ensure_automation_jobs(&store, now).await.unwrap();
        let first: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE business_key LIKE 'automation:%'")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        ensure_automation_jobs(&store, now).await.unwrap();
        let second: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE business_key LIKE 'automation:%'")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert!(first > 0);
        assert_eq!(first, second);
        store.close().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn a_p0_day_reduces_the_lesson_plan_without_exceeding_budget() {
        let path = test_path();
        let store = CoreStore::connect_for_test(&path).await.unwrap();
        let now = "2026-09-04T00:00:00.000Z";
        sqlx::query(
            "INSERT INTO tasks(id, title, priority, status, time_mode, scheduled_date, created_at_utc, updated_at_utc)
             VALUES (?, 'Critical delivery', 'p0', 'todo', 'all_day', '2026-09-04', ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(now)
        .bind(now)
        .execute(&store.pool)
        .await
        .unwrap();
        let goal_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO learning_goals(id, name, daily_minutes, created_at_utc, updated_at_utc)
             VALUES (?, 'Rust', 60, ?, ?)",
        )
        .bind(&goal_id)
        .bind(now)
        .bind(now)
        .execute(&store.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO knowledge_nodes(id, goal_id, name, estimated_minutes, created_at_utc, updated_at_utc)
             VALUES (?, ?, 'Ownership', 40, ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(&goal_id)
        .bind(now)
        .bind(now)
        .execute(&store.pool)
        .await
        .unwrap();
        generate_lesson_plans_for_store(&store, NaiveDate::from_ymd_opt(2026, 9, 4).unwrap())
            .await
            .unwrap();
        let (budget, planned, load): (i64, i64, String) = sqlx::query_as(
            "SELECT budget_minutes, planned_minutes, task_load FROM lesson_plans WHERE goal_id = ?",
        )
        .bind(&goal_id)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(load, "heavy");
        assert_eq!(budget, 30);
        assert!(planned <= budget);
        store.close().await;
        let _ = std::fs::remove_file(path);
    }
}
