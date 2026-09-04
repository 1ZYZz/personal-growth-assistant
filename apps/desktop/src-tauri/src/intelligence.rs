use std::{collections::HashSet, net::IpAddr, time::Duration as StdDuration};

use chrono::{DateTime, SecondsFormat, Utc};
use feed_rs::parser;
use reqwest::{header, redirect::Policy, Client};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Acquire, Row};
use tauri::State;
use url::Url;
use uuid::Uuid;

use crate::{core::CoreStore, product::AppState};

const MAX_FEED_BYTES: usize = 2 * 1024 * 1024;
const MAX_ITEMS_PER_SOURCE: usize = 100;

fn default_languages() -> Vec<String> {
    vec!["zh-CN".to_owned(), "en".to_owned()]
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchFieldItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub include_terms: Vec<String>,
    pub exclude_terms: Vec<String>,
    pub regions: Vec<String>,
    pub languages: Vec<String>,
    pub max_items: i64,
    pub reading_minutes: i64,
    pub relevance_weight: f64,
    pub recency_weight: f64,
    pub authority_weight: f64,
    pub heat_weight: f64,
    pub breaking_alerts: bool,
    pub enabled: bool,
    pub source_count: i64,
    pub last_success_at_utc: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchSourceItem {
    pub id: String,
    pub field_id: String,
    pub name: String,
    pub url: String,
    pub source_type: String,
    pub selector: Option<String>,
    pub authority: f64,
    pub enabled: bool,
    pub last_success_at_utc: Option<String>,
    pub last_error: Option<String>,
    pub sort_order: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    pub id: String,
    pub field_id: String,
    pub source_id: String,
    pub source_name: String,
    pub canonical_url: String,
    pub title: String,
    pub summary: String,
    pub published_at_utc: Option<String>,
    pub information_kind: String,
    pub score: f64,
    pub is_read: bool,
    pub is_saved: bool,
    pub feedback: Option<String>,
    pub uncertainty: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWatchFieldInput {
    pub name: String,
    pub description: String,
    pub include_terms: Vec<String>,
    pub exclude_terms: Vec<String>,
    #[serde(default)]
    pub regions: Vec<String>,
    #[serde(default = "default_languages")]
    pub languages: Vec<String>,
    pub max_items: Option<i64>,
    pub reading_minutes: Option<i64>,
    pub relevance_weight: Option<f64>,
    pub recency_weight: Option<f64>,
    pub authority_weight: Option<f64>,
    pub heat_weight: Option<f64>,
    #[serde(default)]
    pub breaking_alerts: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWatchSourceInput {
    pub field_id: String,
    pub name: String,
    pub url: String,
    pub source_type: Option<String>,
    pub selector: Option<String>,
    pub authority: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsFeedbackInput {
    pub news_id: String,
    pub action: String,
    pub value: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub source_count: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub new_items: usize,
}

#[tauri::command]
pub async fn list_watch_fields(state: State<'_, AppState>) -> Result<Vec<WatchFieldItem>, String> {
    let rows = sqlx::query(
        "SELECT f.id, f.name, f.description, f.include_terms_json, f.exclude_terms_json,
                f.regions_json, f.languages_json, f.max_items, f.reading_minutes,
                f.relevance_weight, f.recency_weight, f.authority_weight, f.heat_weight,
                f.breaking_alerts, f.enabled,
                COUNT(s.id) AS source_count, MAX(s.last_success_at_utc) AS last_success_at_utc
         FROM watch_fields f
         LEFT JOIN watch_sources s ON s.field_id = f.id AND s.deleted_at_utc IS NULL
         WHERE f.deleted_at_utc IS NULL
         GROUP BY f.id
         ORDER BY f.sort_order, f.created_at_utc",
    )
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(row_to_field).collect()
}

#[tauri::command]
pub async fn create_watch_field(
    state: State<'_, AppState>,
    input: CreateWatchFieldInput,
) -> Result<WatchFieldItem, String> {
    let name = clean_required(&input.name, 120, "领域名称")?;
    let description = clean_optional(&input.description, 1000, "领域说明")?;
    let include_terms = clean_terms(input.include_terms)?;
    let exclude_terms = clean_terms(input.exclude_terms)?;
    let regions = clean_terms(input.regions)?;
    let languages = clean_terms(input.languages)?;
    let max_items = input.max_items.unwrap_or(5);
    let reading_minutes = input.reading_minutes.unwrap_or(10);
    if !(1..=20).contains(&max_items) || !(1..=120).contains(&reading_minutes) {
        return Err("领域的条数或阅读时长超出允许范围".to_owned());
    }
    if languages.is_empty() {
        return Err("关注领域至少需要一种语言".to_owned());
    }
    let relevance_weight = input.relevance_weight.unwrap_or(0.35);
    let recency_weight = input.recency_weight.unwrap_or(0.20);
    let authority_weight = input.authority_weight.unwrap_or(0.25);
    let heat_weight = input.heat_weight.unwrap_or(0.10);
    if [
        relevance_weight,
        recency_weight,
        authority_weight,
        heat_weight,
    ]
    .iter()
    .any(|value| !(0.0..=1.0).contains(value))
        || authority_weight <= 0.0
    {
        return Err("排序权重必须在 0 到 1 之间，且来源权威性不能为零".to_owned());
    }
    let id = Uuid::now_v7();
    let now = utc_text(Utc::now());
    let sort_order: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM watch_fields")
            .fetch_one(&state.store.pool)
            .await
            .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO watch_fields(
            id, name, description, include_terms_json, exclude_terms_json,
            regions_json, languages_json, max_items, reading_minutes,
            relevance_weight, recency_weight, authority_weight, heat_weight,
            breaking_alerts, sort_order, created_at_utc, updated_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(description)
    .bind(serde_json::to_string(&include_terms).map_err(json_error)?)
    .bind(serde_json::to_string(&exclude_terms).map_err(json_error)?)
    .bind(serde_json::to_string(&regions).map_err(json_error)?)
    .bind(serde_json::to_string(&languages).map_err(json_error)?)
    .bind(max_items)
    .bind(reading_minutes)
    .bind(relevance_weight)
    .bind(recency_weight)
    .bind(authority_weight)
    .bind(heat_weight)
    .bind(i64::from(input.breaking_alerts))
    .bind(sort_order)
    .bind(&now)
    .bind(&now)
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?;
    fetch_field(&state.store.pool, id).await
}

#[tauri::command]
pub async fn set_watch_field_enabled(
    state: State<'_, AppState>,
    field_id: String,
    enabled: bool,
) -> Result<(), String> {
    let field_id = parse_uuid(&field_id, "领域标识")?;
    let now = utc_text(Utc::now());
    let changed = sqlx::query(
        "UPDATE watch_fields SET enabled = ?, updated_at_utc = ?, version = version + 1
         WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(i64::from(enabled))
    .bind(now)
    .bind(field_id.to_string())
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?
    .rows_affected();
    (changed == 1)
        .then_some(())
        .ok_or_else(|| "关注领域不存在".to_owned())
}

#[tauri::command]
pub async fn delete_watch_field(
    state: State<'_, AppState>,
    field_id: String,
) -> Result<(), String> {
    let field_id = parse_uuid(&field_id, "领域标识")?;
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    sqlx::query(
        "UPDATE watch_fields SET deleted_at_utc = ?, enabled = 0,
            updated_at_utc = ?, version = version + 1 WHERE id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(field_id.to_string())
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "UPDATE watch_sources SET deleted_at_utc = ?, enabled = 0,
            updated_at_utc = ?, version = version + 1
         WHERE field_id = ? AND deleted_at_utc IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(field_id.to_string())
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    transaction.commit().await.map_err(db_error)
}

#[tauri::command]
pub async fn reorder_watch_fields(
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let current: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM watch_fields WHERE deleted_at_utc IS NULL ORDER BY sort_order, created_at_utc",
    )
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    validate_complete_order(&ordered_ids, &current)?;
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    for (index, id) in ordered_ids.iter().enumerate() {
        sqlx::query(
            "UPDATE watch_fields SET sort_order = ?, updated_at_utc = ?, version = version + 1
             WHERE id = ? AND deleted_at_utc IS NULL",
        )
        .bind(index as i64)
        .bind(&now)
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    }
    transaction.commit().await.map_err(db_error)
}

#[tauri::command]
pub async fn list_watch_sources(
    state: State<'_, AppState>,
    field_id: String,
) -> Result<Vec<WatchSourceItem>, String> {
    let field_id = parse_uuid(&field_id, "领域标识")?;
    let rows = sqlx::query(
        "SELECT id, field_id, name, url, source_type, selector, authority, enabled,
                last_success_at_utc, last_error, sort_order
         FROM watch_sources
         WHERE field_id = ? AND deleted_at_utc IS NULL
         ORDER BY sort_order, created_at_utc",
    )
    .bind(field_id.to_string())
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(row_to_source).collect()
}

#[tauri::command]
pub async fn reorder_watch_sources(
    state: State<'_, AppState>,
    field_id: String,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let field_id = parse_uuid(&field_id, "领域标识")?;
    let current: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM watch_sources WHERE field_id = ? AND deleted_at_utc IS NULL
         ORDER BY sort_order, created_at_utc",
    )
    .bind(field_id.to_string())
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    validate_complete_order(&ordered_ids, &current)?;
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    for (index, id) in ordered_ids.iter().enumerate() {
        sqlx::query(
            "UPDATE watch_sources SET sort_order = ?, updated_at_utc = ?, version = version + 1
             WHERE id = ? AND field_id = ? AND deleted_at_utc IS NULL",
        )
        .bind(index as i64)
        .bind(&now)
        .bind(id)
        .bind(field_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    }
    transaction.commit().await.map_err(db_error)
}

#[tauri::command]
pub async fn create_watch_source(
    state: State<'_, AppState>,
    input: CreateWatchSourceInput,
) -> Result<WatchSourceItem, String> {
    let field_id = parse_uuid(&input.field_id, "领域标识")?;
    let name = clean_required(&input.name, 120, "来源名称")?;
    let url = validate_public_feed_url(&input.url)?.to_string();
    let source_type = input.source_type.unwrap_or_else(|| "rss".to_owned());
    if !matches!(source_type.as_str(), "rss" | "atom" | "api" | "page") {
        return Err("来源类型必须是 RSS、Atom、JSON API 或页面监控".to_owned());
    }
    let selector = input
        .selector
        .as_deref()
        .map(|value| clean_optional(value, 200, "页面选择器"))
        .transpose()?
        .filter(|value| !value.is_empty());
    if source_type != "page" && selector.is_some() {
        return Err("只有页面监控来源可以设置选择器".to_owned());
    }
    let authority = input.authority.unwrap_or(0.8);
    if !(0.1..=1.0).contains(&authority) {
        return Err("来源权威性必须在 0.1 到 1.0 之间".to_owned());
    }
    let field_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM watch_fields WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(field_id.to_string())
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    if field_exists != 1 {
        return Err("关注领域不存在".to_owned());
    }
    let sort_order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM watch_sources WHERE field_id = ?",
    )
    .bind(field_id.to_string())
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let id = Uuid::now_v7();
    let now = utc_text(Utc::now());
    sqlx::query(
        "INSERT INTO watch_sources(
            id, field_id, name, url, source_type, selector, authority, sort_order,
            created_at_utc, updated_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(field_id.to_string())
    .bind(name)
    .bind(url)
    .bind(source_type)
    .bind(selector)
    .bind(authority)
    .bind(sort_order)
    .bind(&now)
    .bind(&now)
    .execute(&state.store.pool)
    .await
    .map_err(|error| {
        if error.to_string().contains("UNIQUE") {
            "该领域已经添加了这个来源".to_owned()
        } else {
            db_error(error)
        }
    })?;
    fetch_source(&state.store.pool, id).await
}

#[tauri::command]
pub async fn set_watch_source_enabled(
    state: State<'_, AppState>,
    source_id: String,
    enabled: bool,
) -> Result<(), String> {
    let source_id = parse_uuid(&source_id, "来源标识")?;
    let now = utc_text(Utc::now());
    let changed = sqlx::query(
        "UPDATE watch_sources SET enabled = ?, updated_at_utc = ?, version = version + 1
         WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(i64::from(enabled))
    .bind(now)
    .bind(source_id.to_string())
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?
    .rows_affected();
    (changed == 1)
        .then_some(())
        .ok_or_else(|| "来源不存在".to_owned())
}

#[tauri::command]
pub async fn delete_watch_source(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<(), String> {
    let source_id = parse_uuid(&source_id, "来源标识")?;
    let now = utc_text(Utc::now());
    sqlx::query(
        "UPDATE watch_sources SET deleted_at_utc = ?, enabled = 0,
            updated_at_utc = ?, version = version + 1 WHERE id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(source_id.to_string())
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?;
    Ok(())
}

#[tauri::command]
pub async fn refresh_intelligence(
    state: State<'_, AppState>,
    field_id: Option<String>,
) -> Result<RefreshResult, String> {
    refresh_intelligence_for_store(&state.store, field_id, "manual").await
}

pub(crate) async fn refresh_intelligence_for_store(
    store: &CoreStore,
    field_id: Option<String>,
    trigger_kind: &str,
) -> Result<RefreshResult, String> {
    if !matches!(trigger_kind, "manual" | "scheduled") {
        return Err("采集触发方式无效".to_owned());
    }
    let parsed_field = field_id
        .as_deref()
        .map(|value| parse_uuid(value, "领域标识"))
        .transpose()?;
    let rows = sqlx::query(
        "SELECT s.id, s.field_id, s.name, s.url, s.source_type, s.selector,
                s.authority, s.etag, s.last_modified,
                f.include_terms_json, f.exclude_terms_json,
                f.relevance_weight, f.recency_weight, f.authority_weight, f.heat_weight
         FROM watch_sources s JOIN watch_fields f ON f.id = s.field_id
         WHERE s.enabled = 1 AND s.deleted_at_utc IS NULL
           AND f.enabled = 1 AND f.deleted_at_utc IS NULL
           AND (? IS NULL OR s.field_id = ?)
         ORDER BY s.sort_order, s.created_at_utc",
    )
    .bind(parsed_field.map(|value| value.to_string()))
    .bind(parsed_field.map(|value| value.to_string()))
    .fetch_all(&store.pool)
    .await
    .map_err(db_error)?;
    let run_id = Uuid::now_v7();
    let started = Utc::now();
    sqlx::query(
        "INSERT INTO ingestion_runs(
            id, trigger_kind, field_id, status, source_count, started_at_utc
         ) VALUES (?, ?, ?, 'running', ?, ?)",
    )
    .bind(run_id.to_string())
    .bind(trigger_kind)
    .bind(parsed_field.map(|value| value.to_string()))
    .bind(rows.len() as i64)
    .bind(utc_text(started))
    .execute(&store.pool)
    .await
    .map_err(db_error)?;
    let mut result = RefreshResult {
        source_count: rows.len(),
        succeeded: 0,
        failed: 0,
        new_items: 0,
    };
    for row in rows {
        match refresh_source(&store.pool, &row).await {
            Ok(count) => {
                result.succeeded += 1;
                result.new_items += count;
            }
            Err(error) => {
                result.failed += 1;
                let source_id: String = row.try_get("id").map_err(db_error)?;
                let now = utc_text(Utc::now());
                let _ = sqlx::query(
                    "UPDATE watch_sources SET last_error = ?, updated_at_utc = ? WHERE id = ?",
                )
                .bind(error.chars().take(500).collect::<String>())
                .bind(now)
                .bind(source_id)
                .execute(&store.pool)
                .await;
            }
        }
    }
    let finished = Utc::now();
    let status = if result.failed == 0 {
        "succeeded"
    } else if result.succeeded > 0 {
        "partial"
    } else {
        "failed"
    };
    sqlx::query(
        "UPDATE ingestion_runs SET status = ?, succeeded_count = ?, failed_count = ?,
            new_item_count = ?, finished_at_utc = ? WHERE id = ?",
    )
    .bind(status)
    .bind(result.succeeded as i64)
    .bind(result.failed as i64)
    .bind(result.new_items as i64)
    .bind(utc_text(finished))
    .bind(run_id.to_string())
    .execute(&store.pool)
    .await
    .map_err(db_error)?;
    for stage in [
        "fetch",
        "normalize",
        "deduplicate",
        "rank",
        "verify",
        "store",
    ] {
        sqlx::query(
            "INSERT INTO ingestion_stages(
                id, run_id, stage, input_count, output_count, duration_ms, error_message, created_at_utc
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(run_id.to_string())
        .bind(stage)
        .bind(result.source_count as i64)
        .bind(if stage == "store" { result.new_items as i64 } else { result.succeeded as i64 })
        .bind((finished - started).num_milliseconds())
        .bind((result.failed > 0).then(|| format!("{} 个来源失败", result.failed)))
        .bind(utc_text(finished))
        .execute(&store.pool)
        .await
        .map_err(db_error)?;
    }
    crate::supervision::write_log(
        store,
        "ingestion",
        if result.failed == 0 {
            "info"
        } else {
            "warning"
        },
        "ingestion_completed",
        &run_id.to_string(),
        &format!(
            "采集完成：新增 {} 条，失败 {} 个来源",
            result.new_items, result.failed
        ),
        serde_json::json!({"trigger": trigger_kind, "result": result}),
    )
    .await;
    Ok(result)
}

#[tauri::command]
pub async fn list_news(
    state: State<'_, AppState>,
    field_id: Option<String>,
    filter: Option<String>,
) -> Result<Vec<NewsItem>, String> {
    let parsed_field = field_id
        .as_deref()
        .map(|value| parse_uuid(value, "领域标识"))
        .transpose()?;
    let filter = filter.unwrap_or_else(|| "latest".to_owned());
    if !matches!(filter.as_str(), "latest" | "saved" | "unread") {
        return Err("资讯筛选条件无效".to_owned());
    }
    let rows = sqlx::query(
        "SELECT n.id, n.field_id, n.source_id, s.name AS source_name,
                n.canonical_url, n.title, n.summary, n.published_at_utc,
                n.information_kind, n.score, n.is_read, n.is_saved,
                n.feedback, n.uncertainty
         FROM news_items n JOIN watch_sources s ON s.id = n.source_id
         WHERE (? IS NULL OR n.field_id = ?)
           AND (? != 'saved' OR n.is_saved = 1)
           AND (? != 'unread' OR n.is_read = 0)
           AND n.feedback IS NULL
         ORDER BY n.score DESC, COALESCE(n.published_at_utc, n.created_at_utc) DESC
         LIMIT 500",
    )
    .bind(parsed_field.map(|value| value.to_string()))
    .bind(parsed_field.map(|value| value.to_string()))
    .bind(&filter)
    .bind(&filter)
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(row_to_news).collect()
}

#[tauri::command]
pub async fn set_news_feedback(
    state: State<'_, AppState>,
    input: NewsFeedbackInput,
) -> Result<(), String> {
    let news_id = parse_uuid(&input.news_id, "资讯标识")?;
    let now = utc_text(Utc::now());
    match input.action.as_str() {
        "read" => {
            sqlx::query("UPDATE news_items SET is_read = ?, updated_at_utc = ? WHERE id = ?")
                .bind(i64::from(input.value))
                .bind(now)
                .bind(news_id.to_string())
                .execute(&state.store.pool)
                .await
                .map_err(db_error)?;
        }
        "saved" => {
            sqlx::query("UPDATE news_items SET is_saved = ?, updated_at_utc = ? WHERE id = ?")
                .bind(i64::from(input.value))
                .bind(now)
                .bind(news_id.to_string())
                .execute(&state.store.pool)
                .await
                .map_err(db_error)?;
        }
        "not_interested" => {
            sqlx::query("UPDATE news_items SET feedback = ?, updated_at_utc = ? WHERE id = ?")
                .bind(input.value.then_some("not_interested"))
                .bind(now)
                .bind(news_id.to_string())
                .execute(&state.store.pool)
                .await
                .map_err(db_error)?;
        }
        _ => return Err("资讯操作无效".to_owned()),
    }
    Ok(())
}

async fn refresh_source(
    pool: &sqlx::SqlitePool,
    source: &sqlx::sqlite::SqliteRow,
) -> Result<usize, String> {
    let source_id: String = source.try_get("id").map_err(db_error)?;
    let field_id: String = source.try_get("field_id").map_err(db_error)?;
    let raw_url: String = source.try_get("url").map_err(db_error)?;
    let source_url = validate_public_feed_url(&raw_url)?;
    let client = pinned_public_client(&source_url).await?;
    let authority: f64 = source.try_get("authority").map_err(db_error)?;
    let relevance_weight: f64 = source.try_get("relevance_weight").map_err(db_error)?;
    let recency_weight: f64 = source.try_get("recency_weight").map_err(db_error)?;
    let authority_weight: f64 = source.try_get("authority_weight").map_err(db_error)?;
    let heat_weight: f64 = source.try_get("heat_weight").map_err(db_error)?;
    let include_terms = parse_terms(source, "include_terms_json")?;
    let exclude_terms = parse_terms(source, "exclude_terms_json")?;
    let started = Utc::now();
    let fetch_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO source_fetches(id, source_id, status, started_at_utc)
         VALUES (?, ?, 'running', ?)",
    )
    .bind(fetch_id.to_string())
    .bind(&source_id)
    .bind(utc_text(started))
    .execute(pool)
    .await
    .map_err(db_error)?;
    let mut request = client.get(source_url.clone());
    if let Some(etag) = source
        .try_get::<Option<String>, _>("etag")
        .map_err(db_error)?
    {
        request = request.header(header::IF_NONE_MATCH, etag);
    }
    if let Some(last_modified) = source
        .try_get::<Option<String>, _>("last_modified")
        .map_err(db_error)?
    {
        request = request.header(header::IF_MODIFIED_SINCE, last_modified);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => return finish_fetch_error(pool, &fetch_id, &http_error(error)).await,
    };
    if response.status().is_redirection() {
        return finish_fetch_error(pool, &fetch_id, "来源发生未验证的重定向").await;
    }
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        finish_fetch(pool, &fetch_id, "not_modified", 304, 0, 0, started).await?;
        return Ok(0);
    }
    let status = response.status();
    if !status.is_success() {
        return finish_fetch_error(pool, &fetch_id, &format!("HTTP {}", status.as_u16())).await;
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_FEED_BYTES as u64)
    {
        return finish_fetch_error(pool, &fetch_id, "订阅源超过 2 MB 限制").await;
    }
    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let last_modified = response
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = match response.bytes().await {
        Ok(body) => body,
        Err(error) => return finish_fetch_error(pool, &fetch_id, &http_error(error)).await,
    };
    if body.len() > MAX_FEED_BYTES {
        return finish_fetch_error(pool, &fetch_id, "订阅源超过 2 MB 限制").await;
    }
    let source_type: String = source.try_get("source_type").map_err(db_error)?;
    let body_hash = content_hash(&String::from_utf8_lossy(&body));
    let snapshot_added = sqlx::query(
        "INSERT INTO source_snapshots(id, source_id, fetch_id, content_hash, captured_at_utc)
         VALUES (?, ?, ?, ?, ?) ON CONFLICT(source_id, content_hash) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&source_id)
    .bind(fetch_id.to_string())
    .bind(&body_hash)
    .bind(utc_text(Utc::now()))
    .execute(pool)
    .await
    .map_err(db_error)?
    .rows_affected();
    if source_type == "page" && snapshot_added == 0 {
        finish_fetch(
            pool,
            &fetch_id,
            "not_modified",
            status.as_u16() as i64,
            1,
            0,
            started,
        )
        .await?;
        return Ok(0);
    }
    let candidates = match source_type.as_str() {
        "rss" | "atom" => parse_feed_candidates(&body)?,
        "api" => parse_api_candidates(&body)?,
        "page" => {
            let source_name: String = source.try_get("name").map_err(db_error)?;
            let selector: Option<String> = source.try_get("selector").map_err(db_error)?;
            page_candidate(&source_name, &source_url, &body, selector.as_deref())?
                .into_iter()
                .collect()
        }
        _ => return finish_fetch_error(pool, &fetch_id, "来源类型无效").await,
    };
    let input_count = candidates.len();
    let mut inserted = 0_usize;
    for candidate in candidates.into_iter().take(MAX_ITEMS_PER_SOURCE) {
        let title = candidate.title;
        let link = candidate.link;
        let summary = candidate.summary;
        let searchable = format!("{} {}", title.to_lowercase(), summary.to_lowercase());
        if exclude_terms
            .iter()
            .any(|term| searchable.contains(&term.to_lowercase()))
        {
            continue;
        }
        let matches = include_terms
            .iter()
            .filter(|term| searchable.contains(&term.to_lowercase()))
            .count();
        let relevance = if include_terms.is_empty() {
            0.65
        } else {
            (matches as f64 / include_terms.len() as f64).clamp(0.0, 1.0)
        };
        let published = candidate.published;
        let age_days = published
            .map(|date| (Utc::now() - date).num_days().max(0))
            .unwrap_or(30);
        let recency = (1.0 - age_days as f64 / 30.0).clamp(0.0, 1.0);
        let score = relevance_weight * relevance
            + authority_weight * authority
            + recency_weight * recency
            + heat_weight * 0.5
            + 0.10;
        let event_date = published
            .map(|value| value.date_naive().to_string())
            .unwrap_or_default();
        let fingerprint = content_hash(&format!("{}|{}", title.to_lowercase(), event_date));
        let already_known: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM news_items WHERE field_id = ? AND content_hash = ?",
        )
        .bind(&field_id)
        .bind(&fingerprint)
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
        if already_known > 0 {
            continue;
        }
        let now = utc_text(Utc::now());
        let changed = sqlx::query(
            "INSERT INTO news_items(
                id, field_id, source_id, external_id, canonical_url, title, summary,
                published_at_utc, event_date, information_kind, relevance_score,
                authority_score, recency_score, novelty_score, corroboration_score,
                score, uncertainty, content_hash, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'media_report', ?, ?, ?, 1, 0, ?, ?, ?, ?, ?)
             ON CONFLICT(source_id, content_hash) DO NOTHING",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(&field_id)
        .bind(&source_id)
        .bind(candidate.external_id)
        .bind(link)
        .bind(title)
        .bind(summary)
        .bind(published.map(utc_text))
        .bind(published.map(|value| value.date_naive().to_string()))
        .bind(relevance)
        .bind(authority)
        .bind(recency)
        .bind(score)
        .bind(published.is_none().then_some("发布日期未知"))
        .bind(fingerprint)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(db_error)?
        .rows_affected();
        inserted += changed as usize;
    }
    finish_fetch(
        pool,
        &fetch_id,
        "succeeded",
        status.as_u16() as i64,
        input_count as i64,
        inserted as i64,
        started,
    )
    .await?;
    let now = utc_text(Utc::now());
    sqlx::query(
        "UPDATE watch_sources SET etag = ?, last_modified = ?,
            last_success_at_utc = ?, last_error = NULL, updated_at_utc = ? WHERE id = ?",
    )
    .bind(etag)
    .bind(last_modified)
    .bind(&now)
    .bind(&now)
    .bind(source_id)
    .execute(pool)
    .await
    .map_err(db_error)?;
    Ok(inserted)
}

#[derive(Debug)]
struct SourceCandidate {
    external_id: String,
    title: String,
    link: String,
    summary: String,
    published: Option<DateTime<Utc>>,
}

fn parse_feed_candidates(body: &[u8]) -> Result<Vec<SourceCandidate>, String> {
    let feed = parser::parse(body).map_err(|error| format!("无法解析 RSS/Atom：{error}"))?;
    Ok(feed
        .entries
        .into_iter()
        .filter_map(|entry| {
            let title = entry
                .title
                .as_ref()
                .map(|value| collapse_whitespace(&strip_html(&value.content)))
                .filter(|value| !value.is_empty())?;
            let link = entry.links.iter().find_map(|link| {
                validate_public_article_url(&link.href)
                    .ok()
                    .map(|value| canonicalize_url(value).to_string())
            })?;
            let summary = entry
                .summary
                .as_ref()
                .map(|value| value.content.clone())
                .or_else(|| {
                    entry
                        .content
                        .as_ref()
                        .and_then(|content| content.body.clone())
                })
                .map(|value| truncate_chars(&collapse_whitespace(&strip_html(&value)), 600))
                .unwrap_or_default();
            Some(SourceCandidate {
                external_id: entry.id,
                title,
                link,
                summary,
                published: entry.published.or(entry.updated),
            })
        })
        .collect())
}

fn parse_api_candidates(body: &[u8]) -> Result<Vec<SourceCandidate>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|error| format!("无法解析 JSON API：{error}"))?;
    let items = value
        .as_array()
        .or_else(|| value.get("items").and_then(serde_json::Value::as_array))
        .or_else(|| value.get("data").and_then(serde_json::Value::as_array))
        .or_else(|| value.get("results").and_then(serde_json::Value::as_array))
        .ok_or_else(|| "JSON API 必须返回数组，或包含 items/data/results 数组".to_owned())?;
    let mut candidates = Vec::new();
    for (index, item) in items.iter().take(MAX_ITEMS_PER_SOURCE).enumerate() {
        let title = ["title", "name"]
            .iter()
            .find_map(|key| item.get(key).and_then(serde_json::Value::as_str))
            .map(collapse_whitespace)
            .filter(|value| !value.is_empty());
        let raw_link = ["url", "link", "html_url"]
            .iter()
            .find_map(|key| item.get(key).and_then(serde_json::Value::as_str));
        let (Some(title), Some(raw_link)) = (title, raw_link) else {
            continue;
        };
        let Ok(link) = validate_public_article_url(raw_link) else {
            continue;
        };
        let summary = ["summary", "description", "body"]
            .iter()
            .find_map(|key| item.get(key).and_then(serde_json::Value::as_str))
            .map(|value| truncate_chars(&collapse_whitespace(&strip_html(value)), 600))
            .unwrap_or_default();
        let published = [
            "published_at",
            "publishedAt",
            "date",
            "updated_at",
            "updatedAt",
        ]
        .iter()
        .find_map(|key| item.get(key).and_then(serde_json::Value::as_str))
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
        let external_id = item
            .get("id")
            .and_then(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| value.as_i64().map(|v| v.to_string()))
            })
            .unwrap_or_else(|| index.to_string());
        candidates.push(SourceCandidate {
            external_id,
            title,
            link: canonicalize_url(link).to_string(),
            summary,
            published,
        });
    }
    Ok(candidates)
}

fn page_candidate(
    source_name: &str,
    source_url: &Url,
    body: &[u8],
    selector: Option<&str>,
) -> Result<Option<SourceCandidate>, String> {
    let html = std::str::from_utf8(body).map_err(|_| "页面不是有效 UTF-8 文本".to_owned())?;
    let selected = selector
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "body")
        .map(|value| select_html_fragment(html, value))
        .transpose()?
        .unwrap_or(html);
    let normalized = truncate_chars(&collapse_whitespace(&strip_html(selected)), 600);
    if normalized.is_empty() {
        return Ok(None);
    }
    Ok(Some(SourceCandidate {
        external_id: content_hash(selected),
        title: format!("{source_name} 页面更新"),
        link: canonicalize_url(source_url.clone()).to_string(),
        summary: normalized,
        published: Some(Utc::now()),
    }))
}

fn select_html_fragment<'a>(html: &'a str, selector: &str) -> Result<&'a str, String> {
    if selector.chars().count() > 200
        || selector.contains(['>', '[', ']', ':', ',', '*'])
        || selector.split_whitespace().count() != 1
    {
        return Err("基础页面监控只支持单个标签、#id 或 .class 选择器".to_owned());
    }
    let lower = html.to_ascii_lowercase();
    let needle = if let Some(id) = selector.strip_prefix('#') {
        format!("id=\"{}\"", id.to_ascii_lowercase())
    } else if let Some(class) = selector.strip_prefix('.') {
        format!("class=\"{}", class.to_ascii_lowercase())
    } else {
        format!("<{}", selector.to_ascii_lowercase())
    };
    let found = lower
        .find(&needle)
        .or_else(|| lower.find(&needle.replace('"', "'")))
        .ok_or_else(|| format!("页面中未找到选择器 {selector}"))?;
    let start = lower[..=found].rfind('<').unwrap_or(found);
    let tag_start = start + 1;
    let tag_end = lower[tag_start..]
        .find(|character: char| character.is_whitespace() || character == '>')
        .map(|offset| tag_start + offset)
        .ok_or_else(|| "无法识别选择器对应的 HTML 标签".to_owned())?;
    let tag = &lower[tag_start..tag_end];
    let close = format!("</{tag}>");
    let end = lower[start..]
        .find(&close)
        .map(|offset| start + offset + close.len())
        .or_else(|| lower[start..].find('>').map(|offset| start + offset + 1))
        .ok_or_else(|| "选择器对应的 HTML 片段不完整".to_owned())?;
    Ok(&html[start..end])
}

async fn finish_fetch(
    pool: &sqlx::SqlitePool,
    fetch_id: &Uuid,
    status: &str,
    http_status: i64,
    input_count: i64,
    output_count: i64,
    started: DateTime<Utc>,
) -> Result<(), String> {
    let finished = Utc::now();
    sqlx::query(
        "UPDATE source_fetches SET status = ?, http_status = ?, input_count = ?,
            output_count = ?, duration_ms = ?, finished_at_utc = ? WHERE id = ?",
    )
    .bind(status)
    .bind(http_status)
    .bind(input_count)
    .bind(output_count)
    .bind((finished - started).num_milliseconds())
    .bind(utc_text(finished))
    .bind(fetch_id.to_string())
    .execute(pool)
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn finish_fetch_error<T>(
    pool: &sqlx::SqlitePool,
    fetch_id: &Uuid,
    message: &str,
) -> Result<T, String> {
    let now = utc_text(Utc::now());
    let _ = sqlx::query(
        "UPDATE source_fetches SET status = 'failed', error_message = ?,
            finished_at_utc = ? WHERE id = ?",
    )
    .bind(message)
    .bind(now)
    .bind(fetch_id.to_string())
    .execute(pool)
    .await;
    Err(message.to_owned())
}

async fn fetch_field(pool: &sqlx::SqlitePool, id: Uuid) -> Result<WatchFieldItem, String> {
    let row = sqlx::query(
        "SELECT f.id, f.name, f.description, f.include_terms_json, f.exclude_terms_json,
                f.regions_json, f.languages_json, f.max_items, f.reading_minutes,
                f.relevance_weight, f.recency_weight, f.authority_weight, f.heat_weight,
                f.breaking_alerts, f.enabled,
                COUNT(s.id) AS source_count, MAX(s.last_success_at_utc) AS last_success_at_utc
         FROM watch_fields f
         LEFT JOIN watch_sources s ON s.field_id = f.id AND s.deleted_at_utc IS NULL
         WHERE f.id = ? AND f.deleted_at_utc IS NULL GROUP BY f.id",
    )
    .bind(id.to_string())
    .fetch_one(pool)
    .await
    .map_err(db_error)?;
    row_to_field(&row)
}

async fn fetch_source(pool: &sqlx::SqlitePool, id: Uuid) -> Result<WatchSourceItem, String> {
    let row = sqlx::query(
        "SELECT id, field_id, name, url, source_type, selector, authority, enabled,
                last_success_at_utc, last_error, sort_order
         FROM watch_sources WHERE id = ? AND deleted_at_utc IS NULL",
    )
    .bind(id.to_string())
    .fetch_one(pool)
    .await
    .map_err(db_error)?;
    row_to_source(&row)
}

fn row_to_field(row: &sqlx::sqlite::SqliteRow) -> Result<WatchFieldItem, String> {
    Ok(WatchFieldItem {
        id: row.try_get("id").map_err(db_error)?,
        name: row.try_get("name").map_err(db_error)?,
        description: row.try_get("description").map_err(db_error)?,
        include_terms: parse_terms(row, "include_terms_json")?,
        exclude_terms: parse_terms(row, "exclude_terms_json")?,
        regions: parse_terms(row, "regions_json")?,
        languages: parse_terms(row, "languages_json")?,
        max_items: row.try_get("max_items").map_err(db_error)?,
        reading_minutes: row.try_get("reading_minutes").map_err(db_error)?,
        relevance_weight: row.try_get("relevance_weight").map_err(db_error)?,
        recency_weight: row.try_get("recency_weight").map_err(db_error)?,
        authority_weight: row.try_get("authority_weight").map_err(db_error)?,
        heat_weight: row.try_get("heat_weight").map_err(db_error)?,
        breaking_alerts: row.try_get::<i64, _>("breaking_alerts").map_err(db_error)? != 0,
        enabled: row.try_get::<i64, _>("enabled").map_err(db_error)? != 0,
        source_count: row.try_get("source_count").map_err(db_error)?,
        last_success_at_utc: row.try_get("last_success_at_utc").map_err(db_error)?,
    })
}

fn row_to_source(row: &sqlx::sqlite::SqliteRow) -> Result<WatchSourceItem, String> {
    Ok(WatchSourceItem {
        id: row.try_get("id").map_err(db_error)?,
        field_id: row.try_get("field_id").map_err(db_error)?,
        name: row.try_get("name").map_err(db_error)?,
        url: row.try_get("url").map_err(db_error)?,
        source_type: row.try_get("source_type").map_err(db_error)?,
        selector: row.try_get("selector").map_err(db_error)?,
        authority: row.try_get("authority").map_err(db_error)?,
        enabled: row.try_get::<i64, _>("enabled").map_err(db_error)? != 0,
        last_success_at_utc: row.try_get("last_success_at_utc").map_err(db_error)?,
        last_error: row.try_get("last_error").map_err(db_error)?,
        sort_order: row.try_get("sort_order").map_err(db_error)?,
    })
}

fn row_to_news(row: &sqlx::sqlite::SqliteRow) -> Result<NewsItem, String> {
    Ok(NewsItem {
        id: row.try_get("id").map_err(db_error)?,
        field_id: row.try_get("field_id").map_err(db_error)?,
        source_id: row.try_get("source_id").map_err(db_error)?,
        source_name: row.try_get("source_name").map_err(db_error)?,
        canonical_url: row.try_get("canonical_url").map_err(db_error)?,
        title: row.try_get("title").map_err(db_error)?,
        summary: row.try_get("summary").map_err(db_error)?,
        published_at_utc: row.try_get("published_at_utc").map_err(db_error)?,
        information_kind: row.try_get("information_kind").map_err(db_error)?,
        score: row.try_get("score").map_err(db_error)?,
        is_read: row.try_get::<i64, _>("is_read").map_err(db_error)? != 0,
        is_saved: row.try_get::<i64, _>("is_saved").map_err(db_error)? != 0,
        feedback: row.try_get("feedback").map_err(db_error)?,
        uncertainty: row.try_get("uncertainty").map_err(db_error)?,
    })
}

fn validate_public_feed_url(raw: &str) -> Result<Url, String> {
    validate_public_https_url(raw, "RSS/Atom 来源")
}

fn validate_public_article_url(raw: &str) -> Result<Url, String> {
    validate_public_https_url(raw, "资讯链接")
}

fn validate_public_https_url(raw: &str, label: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|_| format!("{label}不是有效网址"))?;
    if url.scheme() != "https" || url.username() != "" || url.password().is_some() {
        return Err(format!("{label}必须使用不含凭据的 HTTPS 地址"));
    }
    let host = url.host_str().ok_or_else(|| format!("{label}缺少主机名"))?;
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.parse::<IpAddr>().is_ok_and(is_private_ip)
    {
        return Err(format!("{label}不能指向本机或私有网络"));
    }
    Ok(url)
}

async fn pinned_public_client(url: &Url) -> Result<Client, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "RSS/Atom 来源缺少主机名".to_owned())?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("无法解析订阅源主机：{error}"))?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| is_private_ip(address.ip())) {
        return Err("订阅源解析到本机、保留地址或私有网络，已拒绝访问".to_owned());
    }
    Client::builder()
        .user_agent(concat!(
            "PersonalGrowthAssistant/",
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(StdDuration::from_secs(8))
        .timeout(StdDuration::from_secs(20))
        .redirect(Policy::none())
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(http_error)
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(value) => {
            value.is_private()
                || value.is_loopback()
                || value.is_link_local()
                || value.is_broadcast()
                || value.is_unspecified()
                || value.octets()[0] == 0
        }
        IpAddr::V6(value) => {
            value.is_loopback()
                || value.is_unspecified()
                || (value.segments()[0] & 0xfe00) == 0xfc00
                || (value.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

fn canonicalize_url(mut url: Url) -> Url {
    let tracking = [
        "utm_source",
        "utm_medium",
        "utm_campaign",
        "utm_term",
        "utm_content",
        "fbclid",
        "gclid",
    ]
    .into_iter()
    .collect::<HashSet<_>>();
    let pairs = url
        .query_pairs()
        .filter(|(key, _)| !tracking.contains(key.as_ref()))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    url.set_fragment(None);
    url
}

fn parse_terms(row: &sqlx::sqlite::SqliteRow, field: &str) -> Result<Vec<String>, String> {
    let raw: String = row.try_get(field).map_err(db_error)?;
    serde_json::from_str(&raw).map_err(json_error)
}

fn validate_complete_order(ordered_ids: &[String], current_ids: &[String]) -> Result<(), String> {
    if ordered_ids.iter().any(|id| Uuid::parse_str(id).is_err()) {
        return Err("排序列表包含无效标识".to_owned());
    }
    let ordered = ordered_ids.iter().collect::<HashSet<_>>();
    let current = current_ids.iter().collect::<HashSet<_>>();
    if ordered.len() != ordered_ids.len() || ordered != current {
        return Err("排序列表必须且只能包含当前全部项目".to_owned());
    }
    Ok(())
}

fn clean_terms(terms: Vec<String>) -> Result<Vec<String>, String> {
    if terms.len() > 100 {
        return Err("关键词数量不能超过 100".to_owned());
    }
    let mut result = Vec::new();
    for term in terms {
        let value = clean_required(&term, 80, "关键词")?;
        if !result.contains(&value) {
            result.push(value);
        }
    }
    Ok(result)
}

fn clean_required(value: &str, max: usize, label: &str) -> Result<String, String> {
    let value = collapse_whitespace(value.trim());
    if value.is_empty() || value.chars().count() > max {
        return Err(format!("{label}不能为空且不能超过 {max} 个字符"));
    }
    Ok(value)
}

fn clean_optional(value: &str, max: usize, label: &str) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.chars().count() > max {
        return Err(format!("{label}不能超过 {max} 个字符"));
    }
    Ok(value)
}

fn strip_html(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_owned()
    } else {
        format!("{}…", value.chars().take(max).collect::<String>())
    }
}

fn content_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn parse_uuid(value: &str, label: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| format!("{label}无效"))
}

fn utc_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn db_error(error: sqlx::Error) -> String {
    format!("本地数据库操作失败：{error}")
}

fn json_error(error: serde_json::Error) -> String {
    format!("数据格式错误：{error}")
}

fn http_error(error: reqwest::Error) -> String {
    format!("来源请求失败：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_urls_and_credentials_are_rejected() {
        for value in [
            "http://example.com/feed",
            "https://localhost/feed",
            "https://127.0.0.1/feed",
            "https://10.0.0.1/feed",
            "https://user:password@example.com/feed",
        ] {
            assert!(validate_public_feed_url(value).is_err(), "{value}");
        }
        assert!(validate_public_feed_url("https://example.com/feed.xml").is_ok());
    }

    #[test]
    fn tracking_parameters_do_not_change_canonical_identity() {
        let url = Url::parse("https://example.com/a?utm_source=x&id=7#top").unwrap();
        assert_eq!(canonicalize_url(url).as_str(), "https://example.com/a?id=7");
    }

    #[test]
    fn html_is_reduced_to_a_short_plain_text_excerpt() {
        let input = "<p>Hello <strong>world</strong></p><script>bad()</script>";
        assert_eq!(collapse_whitespace(&strip_html(input)), "Hello world bad()");
    }

    #[test]
    fn complete_order_rejects_duplicates_and_missing_items() {
        let first = Uuid::now_v7().to_string();
        let second = Uuid::now_v7().to_string();
        assert!(validate_complete_order(
            &[second.clone(), first.clone()],
            &[first.clone(), second.clone()]
        )
        .is_ok());
        assert!(
            validate_complete_order(&[first.clone(), first], &[second.clone(), second]).is_err()
        );
    }

    #[test]
    fn json_api_parser_accepts_common_envelopes_and_rejects_private_links() {
        let body = br#"{"items":[
          {"id":"one","title":"Public update","url":"https://example.com/news/1","summary":"Verified summary","published_at":"2026-09-04T08:00:00Z"},
          {"id":"two","title":"Private update","url":"https://127.0.0.1/secret"}
        ]}"#;
        let parsed = parse_api_candidates(body).expect("valid JSON API payload");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].external_id, "one");
        assert_eq!(parsed[0].link, "https://example.com/news/1");
    }

    #[test]
    fn basic_page_selector_returns_only_the_authorized_fragment() {
        let html = r#"<html><body><div id="other">ignore</div><section id="watch"><p>Keep this</p></section></body></html>"#;
        let selected = select_html_fragment(html, "#watch").expect("selector should match");
        assert!(selected.contains("Keep this"));
        assert!(!selected.contains("ignore"));
        assert!(select_html_fragment(html, "body > section").is_err());
    }
}
