use std::{str::FromStr, time::Duration as StdDuration};

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Acquire, Row, Sqlite};
use tauri::{AppHandle, Emitter, Manager, State};
use url::Url;
use uuid::Uuid;

use crate::{
    core::{
        expand_virtual, project_from_key, CoreStore, OccurrenceKey, RecurrenceDefinition,
        SchedulerStore, SeriesStart, TimeMode,
    },
    product::AppState,
};

const LEASE_SECONDS: u64 = 45;
const RECONCILE_SECONDS: u64 = 60;
const MISSED_HIGH_PRIORITY_LIMIT: usize = 3;
const NONCE_LIFETIME_DAYS: i64 = 7;
const SNOOZE_MINUTES: i64 = 10;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationItem {
    pub id: String,
    pub reminder_id: String,
    pub task_id: String,
    pub title: String,
    pub scheduled_for_utc: String,
    pub delivered_at_utc: String,
    pub task_status: String,
    pub is_read: bool,
}

#[derive(Debug)]
struct PreparedToast {
    notification_id: Uuid,
    task_id: Uuid,
    title: String,
    body: String,
    complete_nonce: Uuid,
    snooze_nonce: Uuid,
    open_nonce: Uuid,
}

#[derive(Debug)]
enum DeliveryResult {
    Toast(PreparedToast),
    Skipped,
    AlreadyDelivered,
}

pub async fn scheduler_loop(
    app: AppHandle,
    store: CoreStore,
    mut wake: tokio::sync::watch::Receiver<u64>,
) {
    let scheduler = SchedulerStore::from(&store);
    let worker_id = format!("desktop-{}", std::process::id());
    loop {
        let now = Utc::now();
        let _ = resume_due_snoozed_tasks(&store, now).await;
        let _ = ensure_next_series_reminders(&store, now).await;
        let _ = scheduler.reconcile(now).await;
        let _ = crate::product::purge_expired_trash(&store).await;
        if let Err(error) = crate::supervision::ensure_automation_jobs(&store, now).await {
            crate::supervision::write_log(
                &store,
                "scheduler",
                "error",
                "automation_reconcile_failed",
                &Uuid::now_v7().to_string(),
                &error,
                serde_json::json!({}),
            )
            .await;
        }

        let notifications_enabled = crate::data::load_preferences(&store)
            .await
            .map(|preferences| preferences.notifications_enabled)
            .unwrap_or(true);
        let pause_until = notification_pause_until(&store).await.ok().flatten();
        let reminders_active =
            notifications_enabled && !pause_until.is_some_and(|until| until > now);

        let mut claimed_this_cycle = 0_usize;
        for _ in 0..MISSED_HIGH_PRIORITY_LIMIT {
            let claimed = match scheduler
                .claim_due_filtered(
                    &worker_id,
                    Utc::now(),
                    StdDuration::from_secs(LEASE_SECONDS),
                    reminders_active,
                )
                .await
            {
                Ok(Some(job)) => job,
                Ok(None) => break,
                Err(_) => break,
            };
            claimed_this_cycle += 1;
            let delivery = if claimed.kind == "reminder" {
                deliver_reminder(&store, &claimed.payload, Utc::now()).await
            } else {
                match crate::supervision::process_automation_job(
                    &store,
                    &claimed.kind,
                    &claimed.payload,
                )
                .await
                {
                    Ok(()) => {
                        let _ = app.emit(
                            "background-data-changed",
                            serde_json::json!({"kind": claimed.kind}),
                        );
                        Ok(DeliveryResult::Skipped)
                    }
                    Err(error) => Err(error),
                }
            };
            match delivery {
                Ok(DeliveryResult::Toast(toast)) => {
                    let native_delivered = show_windows_toast(&toast).is_ok();
                    crate::supervision::write_log(
                        &store,
                        "notification",
                        if native_delivered { "info" } else { "warning" },
                        if native_delivered {
                            "toast_delivered"
                        } else {
                            "toast_fallback_required"
                        },
                        &claimed.run_id.to_string(),
                        if native_delivered {
                            "Windows 原生提醒已投递"
                        } else {
                            "Windows 原生提醒不可用，已保留应用内通知"
                        },
                        serde_json::json!({"taskId": toast.task_id, "notificationId": toast.notification_id}),
                    )
                    .await;
                    let _ = app.emit(
                        "notification-delivered",
                        serde_json::json!({
                            "notificationId": toast.notification_id,
                            "taskId": toast.task_id,
                            "nativeDelivered": native_delivered,
                        }),
                    );
                    let _ = scheduler.complete(&claimed, true, None, Utc::now()).await;
                }
                Ok(DeliveryResult::Skipped | DeliveryResult::AlreadyDelivered) => {
                    let _ = scheduler.complete(&claimed, true, None, Utc::now()).await;
                }
                Err(error) => {
                    crate::supervision::write_log(
                        &store,
                        "scheduler",
                        "error",
                        "job_failed",
                        &claimed.run_id.to_string(),
                        &error,
                        serde_json::json!({"kind": claimed.kind, "jobId": claimed.id}),
                    )
                    .await;
                    let _ = scheduler
                        .complete(&claimed, false, Some(&error), Utc::now())
                        .await;
                }
            }
        }

        let wait = if !reminders_active || claimed_this_cycle >= MISSED_HIGH_PRIORITY_LIMIT {
            StdDuration::from_secs(RECONCILE_SECONDS)
        } else {
            match scheduler.next_run_at().await {
                Ok(Some(next)) => (next - Utc::now())
                    .to_std()
                    .unwrap_or_default()
                    .min(StdDuration::from_secs(RECONCILE_SECONDS)),
                _ => StdDuration::from_secs(RECONCILE_SECONDS),
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            changed = wake.changed() => {
                if changed.is_err() {
                    break;
                }
            }
        }
    }
}

#[tauri::command]
pub async fn pause_notifications(
    state: State<'_, AppState>,
    mode: String,
) -> Result<Option<String>, String> {
    let until = set_notification_pause(&state.store, &mode).await?;
    state.scheduler_wake.send_modify(|version| *version += 1);
    Ok(until.map(utc_text))
}

pub(crate) async fn set_notification_pause(
    store: &CoreStore,
    mode: &str,
) -> Result<Option<DateTime<Utc>>, String> {
    let now = Utc::now();
    let until = match mode {
        "one_hour" => Some(now + Duration::hours(1)),
        "today" => {
            let timezone = crate::data::load_preferences(store)
                .await?
                .app_timezone
                .parse::<chrono_tz::Tz>()
                .map_err(|_| "应用时区无效".to_owned())?;
            let next_date = now.with_timezone(&timezone).date_naive() + Duration::days(1);
            Some(
                crate::core::resolve_zoned_local(
                    next_date.and_time(NaiveTime::MIN),
                    timezone.name(),
                )
                .map_err(|error| error.to_string())?
                .scheduled_utc,
            )
        }
        "clear" => None,
        _ => return Err("提醒暂停方式无效".to_owned()),
    };
    let now_text = utc_text(now);
    let value = serde_json::to_string(&until.map(utc_text)).map_err(json_error)?;
    sqlx::query(
        "INSERT INTO settings(key, value_json, updated_at_utc, version)
         VALUES ('notification_pause_until', ?, ?, 1)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
            updated_at_utc = excluded.updated_at_utc, version = settings.version + 1",
    )
    .bind(value)
    .bind(now_text)
    .execute(&store.pool)
    .await
    .map_err(db_error)?;
    Ok(until)
}

async fn notification_pause_until(store: &CoreStore) -> Result<Option<DateTime<Utc>>, String> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT value_json FROM settings WHERE key = 'notification_pause_until'",
    )
    .fetch_optional(&store.pool)
    .await
    .map_err(db_error)?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: Option<String> = serde_json::from_str(&raw).map_err(json_error)?;
    value.map(|value| parse_utc(&value)).transpose()
}

#[tauri::command]
pub async fn list_notifications(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<NotificationItem>, String> {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let rows = sqlx::query(
        "SELECT n.id, n.reminder_id, n.created_at_utc, n.payload_json,
                r.task_id, r.remind_at_utc, t.title, t.status
         FROM notification_events n
         JOIN reminders r ON r.id = n.reminder_id
         JOIN tasks t ON t.id = r.task_id
         WHERE n.action = 'displayed'
         ORDER BY n.created_at_utc DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;

    rows.iter()
        .map(|row| {
            let payload_text: String = row.try_get("payload_json").map_err(db_error)?;
            let payload: serde_json::Value =
                serde_json::from_str(&payload_text).map_err(|_| "通知数据已损坏".to_owned())?;
            Ok(NotificationItem {
                id: row.try_get("id").map_err(db_error)?,
                reminder_id: row.try_get("reminder_id").map_err(db_error)?,
                task_id: row.try_get("task_id").map_err(db_error)?,
                title: row.try_get("title").map_err(db_error)?,
                scheduled_for_utc: row.try_get("remind_at_utc").map_err(db_error)?,
                delivered_at_utc: row.try_get("created_at_utc").map_err(db_error)?,
                task_status: row.try_get("status").map_err(db_error)?,
                is_read: payload
                    .get("isRead")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

#[tauri::command]
pub async fn run_notification_action(
    app: AppHandle,
    state: State<'_, AppState>,
    notification_id: String,
    action: String,
    nonce: String,
) -> Result<(), String> {
    let notification_id =
        Uuid::parse_str(&notification_id).map_err(|_| "notification_id 无效".to_owned())?;
    let nonce = Uuid::parse_str(&nonce).map_err(|_| "nonce 无效".to_owned())?;
    process_notification_action(&app, &state.store, notification_id, &action, nonce).await
}

pub fn handle_activation_arguments(app: &AppHandle, arguments: &[String]) {
    let Some(raw_url) = arguments
        .iter()
        .find(|argument| argument.starts_with("pga://"))
    else {
        show_main_window(app);
        return;
    };
    let Ok(parsed) = parse_action_url(raw_url) else {
        show_main_window(app);
        return;
    };
    let app_handle = app.clone();
    let store = app.state::<AppState>().store.clone();
    tauri::async_runtime::spawn(async move {
        let _ =
            process_notification_action(&app_handle, &store, parsed.0, &parsed.1, parsed.2).await;
    });
}

fn parse_action_url(value: &str) -> Result<(Uuid, String, Uuid), String> {
    let url = Url::parse(value).map_err(|_| "通知链接无效".to_owned())?;
    if url.scheme() != "pga" || url.host_str() != Some("notification") {
        return Err("通知链接范围无效".to_owned());
    }
    let id = url
        .path_segments()
        .and_then(|mut segments| segments.next())
        .ok_or_else(|| "通知链接缺少标识".to_owned())?;
    let notification_id = Uuid::parse_str(id).map_err(|_| "通知标识无效".to_owned())?;
    let mut action = None;
    let mut nonce = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "action" if action.is_none() => action = Some(value.into_owned()),
            "nonce" if nonce.is_none() => {
                nonce = Some(Uuid::parse_str(&value).map_err(|_| "通知 nonce 无效".to_owned())?);
            }
            _ => return Err("通知链接包含未知参数".to_owned()),
        }
    }
    let action = action.ok_or_else(|| "通知链接缺少动作".to_owned())?;
    if !matches!(action.as_str(), "complete" | "snooze" | "open") {
        return Err("通知动作无效".to_owned());
    }
    Ok((
        notification_id,
        action,
        nonce.ok_or_else(|| "通知链接缺少 nonce".to_owned())?,
    ))
}

async fn process_notification_action(
    app: &AppHandle,
    store: &CoreStore,
    notification_id: Uuid,
    action: &str,
    nonce: Uuid,
) -> Result<(), String> {
    if !matches!(action, "complete" | "snooze" | "open") {
        return Err("通知动作无效".to_owned());
    }
    let mut connection = store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    let now = Utc::now();
    let now_text = utc_text(now);
    let hash = nonce_hash(nonce);
    let consumed = sqlx::query(
        "UPDATE notification_nonces
         SET consumed_at_utc = ?
         WHERE notification_id = ? AND action = ? AND nonce_hash = ?
           AND consumed_at_utc IS NULL AND expires_at_utc > ?",
    )
    .bind(&now_text)
    .bind(notification_id.to_string())
    .bind(action)
    .bind(hash)
    .bind(&now_text)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    if consumed.rows_affected() == 0 {
        let existed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notification_nonces
             WHERE notification_id = ? AND action = ? AND nonce_hash = ?",
        )
        .bind(notification_id.to_string())
        .bind(action)
        .bind(nonce_hash(nonce))
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
        transaction.rollback().await.map_err(db_error)?;
        return if existed > 0 {
            Ok(())
        } else {
            Err("通知动作已失效".to_owned())
        };
    }

    let row = sqlx::query(
        "SELECT r.id AS reminder_id, r.task_id
         FROM notification_events n JOIN reminders r ON r.id = n.reminder_id
         WHERE n.id = ?",
    )
    .bind(notification_id.to_string())
    .fetch_one(&mut *transaction)
    .await
    .map_err(db_error)?;
    let reminder_id: String = row.try_get("reminder_id").map_err(db_error)?;
    let task_id: String = row.try_get("task_id").map_err(db_error)?;

    match action {
        "complete" => {
            sqlx::query(
                "UPDATE tasks
                 SET status = 'completed', completed_at_utc = ?, updated_at_utc = ?, version = version + 1
                 WHERE id = ? AND status NOT IN ('completed', 'canceled') AND deleted_at_utc IS NULL",
            )
            .bind(&now_text)
            .bind(&now_text)
            .bind(&task_id)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        }
        "snooze" => {
            let snoozed_until = utc_text(now + Duration::minutes(SNOOZE_MINUTES));
            sqlx::query(
                "UPDATE reminders SET status = 'pending', remind_at_utc = ?, updated_at_utc = ?, version = version + 1
                 WHERE id = ?",
            )
            .bind(&snoozed_until)
            .bind(&now_text)
            .bind(&reminder_id)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
            sqlx::query(
                "INSERT INTO jobs(
                    id, kind, owner, business_key, scheduled_for_utc, attempt_class,
                    run_at_utc, state, payload_json, created_at_utc, updated_at_utc
                 ) VALUES (?, 'reminder', 'local', ?, ?, 'primary', ?, 'pending', ?, ?, ?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind(format!("reminder:{reminder_id}:snooze:{snoozed_until}"))
            .bind(&snoozed_until)
            .bind(&snoozed_until)
            .bind(serde_json::json!({"reminderId": reminder_id}).to_string())
            .bind(&now_text)
            .bind(&now_text)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        }
        "open" => {}
        _ => unreachable!("action was validated"),
    }
    transaction.commit().await.map_err(db_error)?;
    show_main_window(app);
    let _ = app.emit(
        "notification-action",
        serde_json::json!({"notificationId": notification_id, "taskId": task_id, "action": action}),
    );
    Ok(())
}

async fn resume_due_snoozed_tasks(store: &CoreStore, now: DateTime<Utc>) -> Result<(), String> {
    let now_text = utc_text(now);
    let mut connection = store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    let rows = sqlx::query(
        "SELECT id, series_id, occurrence_key, version FROM tasks
         WHERE status = 'snoozed' AND snoozed_until_utc <= ? AND deleted_at_utc IS NULL
         LIMIT 200",
    )
    .bind(&now_text)
    .fetch_all(&mut *transaction)
    .await
    .map_err(db_error)?;
    for row in rows {
        let id: String = row.try_get("id").map_err(db_error)?;
        let version: i64 = row.try_get("version").map_err(db_error)?;
        let changed = sqlx::query(
            "UPDATE tasks SET status = 'todo', snoozed_until_utc = NULL,
                updated_at_utc = ?, version = version + 1
             WHERE id = ? AND version = ? AND status = 'snoozed'",
        )
        .bind(&now_text)
        .bind(&id)
        .bind(version)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?
        .rows_affected();
        if changed == 1 {
            let series_id: Option<String> = row.try_get("series_id").map_err(db_error)?;
            let occurrence_key: Option<String> = row.try_get("occurrence_key").map_err(db_error)?;
            sqlx::query(
                "INSERT INTO task_events(
                    id, task_id, event_type, occurrence_series_id, occurrence_key,
                    payload_json, created_at_utc
                 ) VALUES (?, ?, 'status_todo', ?, ?, ?, ?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind(id)
            .bind(series_id)
            .bind(occurrence_key)
            .bind(serde_json::json!({"from": "snoozed", "reason": "snooze_expired"}).to_string())
            .bind(&now_text)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        }
    }
    transaction.commit().await.map_err(db_error)
}

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

async fn ensure_next_series_reminders(store: &CoreStore, now: DateTime<Utc>) -> Result<(), String> {
    let app_timezone = crate::data::load_preferences(store).await?.app_timezone;
    let rows = sqlx::query(
        "SELECT rr.id AS rule_id, rr.minutes_before,
                s.id, s.title, s.notes, s.priority, s.estimated_minutes,
                s.time_mode, s.dtstart_local, s.tzid, s.rrule_text,
                s.rdates_json, s.exdates_json, s.ends_before_occurrence_key
         FROM reminder_rules rr
         JOIN recurrence_series s ON s.id = rr.series_id
         WHERE rr.enabled = 1 AND s.deleted_at_utc IS NULL",
    )
    .fetch_all(&store.pool)
    .await
    .map_err(db_error)?;

    for row in rows {
        let definition = series_from_row(&row)?;
        let cutoff = row
            .try_get::<Option<String>, _>("ends_before_occurrence_key")
            .map_err(db_error)?
            .map(|value| {
                OccurrenceKey::from_str(&value)
                    .map(|key| occurrence_original_local(&key))
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
        let start_local =
            local_now_for_series(&definition.start, now, &app_timezone)? - Duration::days(1);
        let projections = expand_virtual(
            &definition,
            start_local,
            start_local + Duration::days(370),
            20_000,
        )
        .map_err(|error| error.to_string())?;
        let minutes_before: i64 = row.try_get("minutes_before").map_err(db_error)?;
        let next = projections
            .into_iter()
            .filter(|projection| cutoff.is_none_or(|value| projection.original_local < value))
            .find_map(|projection| {
                let scheduled_utc = projection.scheduled_utc.or_else(|| {
                    (projection.time_mode == TimeMode::Floating)
                        .then(|| {
                            crate::core::resolve_zoned_local(
                                projection.scheduled_local,
                                &app_timezone,
                            )
                            .ok()
                            .map(|resolution| resolution.scheduled_utc)
                        })
                        .flatten()
                })?;
                let remind_at = scheduled_utc - Duration::minutes(minutes_before);
                (remind_at >= now - Duration::minutes(15)).then_some((projection, remind_at))
            });
        let Some((projection, remind_at)) = next else {
            continue;
        };
        let rule_id: String = row.try_get("rule_id").map_err(db_error)?;
        let reminder_id = Uuid::now_v7();
        let occurrence_key = projection.occurrence_ref.occurrence_key.to_string();
        let now_text = utc_text(now);
        sqlx::query(
            "INSERT INTO reminders(
                id, reminder_rule_id, occurrence_series_id, occurrence_key,
                remind_at_utc, status, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, ?, ?, 'pending', ?, ?)
             ON CONFLICT(reminder_rule_id, occurrence_series_id, occurrence_key)
             WHERE reminder_rule_id IS NOT NULL AND occurrence_series_id IS NOT NULL
                   AND occurrence_key IS NOT NULL
             DO UPDATE SET
                remind_at_utc = CASE WHEN reminders.status = 'delivered'
                    THEN reminders.remind_at_utc ELSE excluded.remind_at_utc END,
                status = CASE WHEN reminders.status = 'delivered'
                    THEN 'delivered' ELSE 'pending' END,
                updated_at_utc = excluded.updated_at_utc,
                version = reminders.version + 1",
        )
        .bind(reminder_id.to_string())
        .bind(&rule_id)
        .bind(definition.series_id.to_string())
        .bind(&occurrence_key)
        .bind(utc_text(remind_at))
        .bind(&now_text)
        .bind(&now_text)
        .execute(&store.pool)
        .await
        .map_err(db_error)?;
        let stored_id: String = sqlx::query_scalar(
            "SELECT id FROM reminders
             WHERE reminder_rule_id = ? AND occurrence_series_id = ? AND occurrence_key = ?",
        )
        .bind(&rule_id)
        .bind(definition.series_id.to_string())
        .bind(&occurrence_key)
        .fetch_one(&store.pool)
        .await
        .map_err(db_error)?;
        let scheduled = utc_text(remind_at);
        sqlx::query(
            "UPDATE jobs SET state = 'cancelled', updated_at_utc = ?
             WHERE business_key = ? AND state = 'pending' AND scheduled_for_utc != ?",
        )
        .bind(&now_text)
        .bind(format!("reminder:{stored_id}"))
        .bind(&scheduled)
        .execute(&store.pool)
        .await
        .map_err(db_error)?;
        sqlx::query(
            "INSERT INTO jobs(
                id, kind, owner, business_key, scheduled_for_utc, attempt_class,
                run_at_utc, state, payload_json, created_at_utc, updated_at_utc
             ) VALUES (?, 'reminder', 'local', ?, ?, 'primary', ?, 'pending', ?, ?, ?)
             ON CONFLICT(business_key, scheduled_for_utc, attempt_class) DO NOTHING",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(format!("reminder:{stored_id}"))
        .bind(&scheduled)
        .bind(&scheduled)
        .bind(serde_json::json!({"reminderId": stored_id}).to_string())
        .bind(&now_text)
        .bind(&now_text)
        .execute(&store.pool)
        .await
        .map_err(db_error)?;
    }
    Ok(())
}

async fn deliver_reminder(
    store: &CoreStore,
    payload: &serde_json::Value,
    now: DateTime<Utc>,
) -> Result<DeliveryResult, String> {
    let reminder_id = payload
        .get("reminderId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "提醒作业缺少 reminderId".to_owned())?;
    let reminder_id = Uuid::parse_str(reminder_id).map_err(|_| "提醒标识无效".to_owned())?;
    let mut connection = store.pool.acquire().await.map_err(db_error)?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await
        .map_err(db_error)?;
    let result = deliver_in_transaction(&mut connection, reminder_id, now).await;
    match result {
        Ok(delivery) => {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(db_error)?;
            Ok(delivery)
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            Err(error)
        }
    }
}

async fn deliver_in_transaction(
    connection: &mut sqlx::pool::PoolConnection<Sqlite>,
    reminder_id: Uuid,
    now: DateTime<Utc>,
) -> Result<DeliveryResult, String> {
    let row = sqlx::query(
        "SELECT r.id, r.task_id, r.occurrence_series_id, r.occurrence_key,
                r.remind_at_utc, r.status,
                s.title AS series_title, s.notes AS series_notes,
                s.priority AS series_priority, s.estimated_minutes AS series_minutes,
                s.time_mode AS series_time_mode, s.dtstart_local, s.tzid AS series_tzid,
                s.rrule_text, s.rdates_json, s.exdates_json
         FROM reminders r
         LEFT JOIN recurrence_series s ON s.id = r.occurrence_series_id
         WHERE r.id = ?",
    )
    .bind(reminder_id.to_string())
    .fetch_optional(&mut **connection)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "提醒不存在".to_owned())?;
    let status: String = row.try_get("status").map_err(db_error)?;
    if status == "delivered" || status == "cancelled" {
        return Ok(DeliveryResult::AlreadyDelivered);
    }
    let remind_at_text: String = row.try_get("remind_at_utc").map_err(db_error)?;
    let remind_at = parse_utc(&remind_at_text)?;
    if remind_at > now + Duration::seconds(2) {
        return Ok(DeliveryResult::Skipped);
    }

    let task_id = match row
        .try_get::<Option<String>, _>("task_id")
        .map_err(db_error)?
    {
        Some(id) => Uuid::parse_str(&id).map_err(|_| "任务标识无效".to_owned())?,
        None => materialize_reminder_occurrence(connection, &row, now).await?,
    };
    let task = sqlx::query(
        "SELECT title, status, priority FROM tasks WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(task_id.to_string())
    .fetch_optional(&mut **connection)
    .await
    .map_err(db_error)?;
    let Some(task) = task else {
        sqlx::query("UPDATE reminders SET status = 'cancelled', updated_at_utc = ? WHERE id = ?")
            .bind(utc_text(now))
            .bind(reminder_id.to_string())
            .execute(&mut **connection)
            .await
            .map_err(db_error)?;
        return Ok(DeliveryResult::Skipped);
    };
    let task_status: String = task.try_get("status").map_err(db_error)?;
    if task_status != "todo" {
        sqlx::query("UPDATE reminders SET status = 'cancelled', updated_at_utc = ? WHERE id = ?")
            .bind(utc_text(now))
            .bind(reminder_id.to_string())
            .execute(&mut **connection)
            .await
            .map_err(db_error)?;
        return Ok(DeliveryResult::Skipped);
    }

    let title: String = task.try_get("title").map_err(db_error)?;
    let overdue_minutes = (now - remind_at).num_minutes().max(0);
    let body = if overdue_minutes > 0 {
        format!(
            "原定提醒：{}，已逾期 {} 分钟",
            remind_at.format("%H:%M"),
            overdue_minutes
        )
    } else {
        "现在可以开始了".to_owned()
    };
    let notification_id = Uuid::now_v7();
    let idempotency_key = format!("reminder:{reminder_id}:{remind_at_text}");
    let now_text = utc_text(now);
    let inserted = sqlx::query(
        "INSERT INTO notification_events(
            id, reminder_id, idempotency_key, action, payload_json, created_at_utc
         ) VALUES (?, ?, ?, 'displayed', ?, ?)
         ON CONFLICT(idempotency_key) DO NOTHING",
    )
    .bind(notification_id.to_string())
    .bind(reminder_id.to_string())
    .bind(idempotency_key)
    .bind(serde_json::json!({"taskId": task_id, "isRead": false}).to_string())
    .bind(&now_text)
    .execute(&mut **connection)
    .await
    .map_err(db_error)?;
    if inserted.rows_affected() == 0 {
        return Ok(DeliveryResult::AlreadyDelivered);
    }
    sqlx::query(
        "UPDATE reminders SET status = 'delivered', task_id = ?, updated_at_utc = ?, version = version + 1
         WHERE id = ?",
    )
    .bind(task_id.to_string())
    .bind(&now_text)
    .bind(reminder_id.to_string())
    .execute(&mut **connection)
    .await
    .map_err(db_error)?;

    let complete_nonce = Uuid::now_v7();
    let snooze_nonce = Uuid::now_v7();
    let open_nonce = Uuid::now_v7();
    for (action, nonce) in [
        ("complete", complete_nonce),
        ("snooze", snooze_nonce),
        ("open", open_nonce),
    ] {
        sqlx::query(
            "INSERT INTO notification_nonces(
                id, notification_id, action, nonce_hash, expires_at_utc, created_at_utc
             ) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(notification_id.to_string())
        .bind(action)
        .bind(nonce_hash(nonce))
        .bind(utc_text(now + Duration::days(NONCE_LIFETIME_DAYS)))
        .bind(&now_text)
        .execute(&mut **connection)
        .await
        .map_err(db_error)?;
    }
    Ok(DeliveryResult::Toast(PreparedToast {
        notification_id,
        task_id,
        title,
        body,
        complete_nonce,
        snooze_nonce,
        open_nonce,
    }))
}

async fn materialize_reminder_occurrence(
    connection: &mut sqlx::pool::PoolConnection<Sqlite>,
    reminder: &sqlx::sqlite::SqliteRow,
    now: DateTime<Utc>,
) -> Result<Uuid, String> {
    let series_id_text: String = reminder.try_get("occurrence_series_id").map_err(db_error)?;
    let series_id = Uuid::parse_str(&series_id_text).map_err(|_| "系列标识无效".to_owned())?;
    let occurrence_key_text: String = reminder.try_get("occurrence_key").map_err(db_error)?;
    let occurrence_key =
        OccurrenceKey::from_str(&occurrence_key_text).map_err(|error| error.to_string())?;
    let definition = series_from_reminder_row(reminder, series_id)?;
    let projection =
        project_from_key(&definition, &occurrence_key).map_err(|error| error.to_string())?;
    let candidate = Uuid::now_v7();
    let (scheduled_local, scheduled_date, scheduled_utc, tzid) = match &occurrence_key {
        OccurrenceKey::AllDay { original_date } => (
            None,
            Some(original_date.format("%Y-%m-%d").to_string()),
            None,
            None,
        ),
        OccurrenceKey::Floating { .. } => {
            let app_timezone = current_app_timezone(connection).await?;
            let utc = crate::core::resolve_zoned_local(projection.scheduled_local, &app_timezone)
                .map_err(|error| error.to_string())?
                .scheduled_utc;
            (
                Some(local_text(projection.scheduled_local)),
                None,
                Some(utc_text(utc)),
                None,
            )
        }
        OccurrenceKey::Zoned { tzid, .. } => (
            Some(local_text(projection.scheduled_local)),
            None,
            projection.scheduled_utc.map(utc_text),
            Some(tzid.clone()),
        ),
    };
    let title: String = reminder.try_get("series_title").map_err(db_error)?;
    let notes: Option<String> = reminder.try_get("series_notes").map_err(db_error)?;
    let priority: String = reminder.try_get("series_priority").map_err(db_error)?;
    let minutes: Option<i64> = reminder.try_get("series_minutes").map_err(db_error)?;
    let now_text = utc_text(now);
    sqlx::query(
        "INSERT INTO tasks(
            id, series_id, occurrence_key, title, notes, priority, estimated_minutes,
            status, time_mode, scheduled_local, scheduled_date, scheduled_utc, tzid,
            chosen_offset_seconds, dst_adjusted, created_at_utc, updated_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?, 'todo', ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(series_id, occurrence_key) DO NOTHING",
    )
    .bind(candidate.to_string())
    .bind(&series_id_text)
    .bind(&occurrence_key_text)
    .bind(title)
    .bind(notes)
    .bind(priority)
    .bind(minutes)
    .bind(mode_text(projection.time_mode))
    .bind(scheduled_local)
    .bind(scheduled_date)
    .bind(scheduled_utc)
    .bind(tzid)
    .bind(projection.chosen_offset_seconds)
    .bind(i64::from(projection.dst_adjusted))
    .bind(&now_text)
    .bind(&now_text)
    .execute(&mut **connection)
    .await
    .map_err(db_error)?;
    let task_id_text: String =
        sqlx::query_scalar("SELECT id FROM tasks WHERE series_id = ? AND occurrence_key = ?")
            .bind(series_id.to_string())
            .bind(occurrence_key_text)
            .fetch_one(&mut **connection)
            .await
            .map_err(db_error)?;
    Uuid::parse_str(&task_id_text).map_err(|_| "任务标识无效".to_owned())
}

fn series_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<RecurrenceDefinition, String> {
    let id: String = row.try_get("id").map_err(db_error)?;
    let series_id = Uuid::parse_str(&id).map_err(|_| "系列标识无效".to_owned())?;
    series_from_columns(
        series_id,
        row.try_get("time_mode").map_err(db_error)?,
        row.try_get("dtstart_local").map_err(db_error)?,
        row.try_get("tzid").map_err(db_error)?,
        row.try_get("rrule_text").map_err(db_error)?,
        row.try_get("rdates_json").map_err(db_error)?,
        row.try_get("exdates_json").map_err(db_error)?,
    )
}

fn series_from_reminder_row(
    row: &sqlx::sqlite::SqliteRow,
    series_id: Uuid,
) -> Result<RecurrenceDefinition, String> {
    series_from_columns(
        series_id,
        row.try_get("series_time_mode").map_err(db_error)?,
        row.try_get("dtstart_local").map_err(db_error)?,
        row.try_get("series_tzid").map_err(db_error)?,
        row.try_get("rrule_text").map_err(db_error)?,
        row.try_get("rdates_json").map_err(db_error)?,
        row.try_get("exdates_json").map_err(db_error)?,
    )
}

fn series_from_columns(
    series_id: Uuid,
    mode: String,
    start_text: String,
    tzid: Option<String>,
    rrule: String,
    rdates: String,
    exdates: String,
) -> Result<RecurrenceDefinition, String> {
    let start = match mode.as_str() {
        "all_day" => SeriesStart::AllDay {
            date: NaiveDate::parse_from_str(&start_text, "%Y-%m-%d")
                .map_err(|_| "系列日期无效".to_owned())?,
        },
        "floating" => SeriesStart::Floating {
            local: parse_local(&start_text)?,
        },
        "zoned" => SeriesStart::Zoned {
            local: parse_local(&start_text)?,
            tzid: tzid.ok_or_else(|| "系列缺少时区".to_owned())?,
        },
        _ => return Err("系列时间模式无效".to_owned()),
    };
    Ok(RecurrenceDefinition {
        series_id,
        start,
        rrule,
        rdates: serde_json::from_str(&rdates).map_err(|_| "RDATE 无效".to_owned())?,
        exdates: serde_json::from_str(&exdates).map_err(|_| "EXDATE 无效".to_owned())?,
    })
}

fn local_now_for_series(
    start: &SeriesStart,
    now: DateTime<Utc>,
    app_timezone: &str,
) -> Result<NaiveDateTime, String> {
    match start {
        SeriesStart::AllDay { .. } => Ok(now.date_naive().and_time(NaiveTime::MIN)),
        SeriesStart::Floating { .. } => app_timezone
            .parse::<chrono_tz::Tz>()
            .map(|timezone| now.with_timezone(&timezone).naive_local())
            .map_err(|_| "应用时区无效".to_owned()),
        SeriesStart::Zoned { tzid, .. } => {
            let timezone = tzid
                .parse::<chrono_tz::Tz>()
                .map_err(|_| "系列时区无效".to_owned())?;
            Ok(now.with_timezone(&timezone).naive_local())
        }
    }
}

async fn current_app_timezone(
    connection: &mut sqlx::pool::PoolConnection<Sqlite>,
) -> Result<String, String> {
    let timezone: Option<String> = sqlx::query_scalar(
        "SELECT json_extract(value_json, '$.appTimezone')
         FROM settings WHERE key = 'preferences'",
    )
    .fetch_optional(&mut **connection)
    .await
    .map_err(db_error)?
    .flatten();
    let timezone = timezone.unwrap_or_else(|| "Asia/Shanghai".to_owned());
    timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| "应用时区无效".to_owned())?;
    Ok(timezone)
}

fn occurrence_original_local(key: &OccurrenceKey) -> NaiveDateTime {
    match key {
        OccurrenceKey::AllDay { original_date } => original_date.and_time(NaiveTime::MIN),
        OccurrenceKey::Floating { original_local }
        | OccurrenceKey::Zoned { original_local, .. } => *original_local,
    }
}

fn nonce_hash(nonce: Uuid) -> String {
    let bytes = Sha256::digest(nonce.as_bytes());
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(windows)]
fn show_windows_toast(toast: &PreparedToast) -> Result<(), String> {
    use windows::{
        core::HSTRING,
        Data::Xml::Dom::XmlDocument,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
    };

    let link = |action: &str, nonce: Uuid| {
        format!(
            "pga://notification/{}?action={action}&amp;nonce={nonce}",
            toast.notification_id
        )
    };
    let xml = format!(
        "<toast launch=\"{}\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual><actions><action content=\"完成\" activationType=\"protocol\" arguments=\"{}\"/><action content=\"稍后 10 分钟\" activationType=\"protocol\" arguments=\"{}\"/><action content=\"打开任务\" activationType=\"protocol\" arguments=\"{}\"/></actions></toast>",
        link("open", toast.open_nonce),
        escape_xml(&toast.title),
        escape_xml(&toast.body),
        link("complete", toast.complete_nonce),
        link("snooze", toast.snooze_nonce),
        link("open", toast.open_nonce),
    );
    let document = XmlDocument::new().map_err(|error| error.to_string())?;
    document
        .LoadXml(&HSTRING::from(xml))
        .map_err(|error| error.to_string())?;
    let notification =
        ToastNotification::CreateToastNotification(&document).map_err(|error| error.to_string())?;
    notification
        .SetTag(&HSTRING::from(
            toast.notification_id.to_string()[..16].to_owned(),
        ))
        .map_err(|error| error.to_string())?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
        "org.personal-growth-assistant.desktop",
    ))
    .map_err(|error| error.to_string())?;
    notifier
        .Show(&notification)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn show_windows_toast(_toast: &PreparedToast) -> Result<(), String> {
    Err("native Windows notifications are unavailable".to_owned())
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn parse_local(value: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| "本地时间无效".to_owned())
}

fn local_text(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S%.9f").to_string()
}

fn parse_utc(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| "UTC 时间无效".to_owned())
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

fn db_error(error: sqlx::Error) -> String {
    format!("本地数据库操作失败：{error}")
}

fn json_error(error: serde_json::Error) -> String {
    format!("提醒设置格式无效：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_url_parser_rejects_unknown_parameters_and_actions() {
        let notification = Uuid::now_v7();
        let nonce = Uuid::now_v7();
        assert!(parse_action_url(&format!(
            "pga://notification/{notification}?action=complete&nonce={nonce}"
        ))
        .is_ok());
        assert!(parse_action_url(&format!(
            "pga://notification/{notification}?action=erase&nonce={nonce}"
        ))
        .is_err());
        assert!(parse_action_url(&format!(
            "pga://notification/{notification}?action=open&nonce={nonce}&url=https://evil.test"
        ))
        .is_err());
    }

    #[test]
    fn xml_escaping_never_allows_task_text_to_inject_actions() {
        assert_eq!(
            escape_xml("</text><action content=\"x\">"),
            "&lt;/text&gt;&lt;action content=&quot;x&quot;&gt;"
        );
    }

    #[tokio::test]
    async fn notification_pause_can_be_set_and_cleared_without_losing_jobs() {
        let path = std::env::temp_dir().join(format!("pga-reminders-{}.sqlite3", Uuid::now_v7()));
        let store = CoreStore::connect_for_test(&path).await.unwrap();
        let until = set_notification_pause(&store, "one_hour")
            .await
            .unwrap()
            .unwrap();
        assert!(until > Utc::now() + Duration::minutes(59));
        assert!(notification_pause_until(&store).await.unwrap().is_some());
        set_notification_pause(&store, "clear").await.unwrap();
        assert!(notification_pause_until(&store).await.unwrap().is_none());
        store.close().await;
        let _ = std::fs::remove_file(path);
    }
}
