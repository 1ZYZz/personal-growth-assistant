use std::{path::Path, str::FromStr, time::Duration};

use chrono::{DateTime, NaiveDate, NaiveDateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Row, SqlitePool,
};
use uuid::Uuid;

use super::{OccurrenceProjection, RecurrenceDefinition, SeriesStart, TimeMode};

const CORE_SCHEMA_V1: &str = include_str!("../../migrations/0001_core.sql");
const PRODUCT_SCHEMA_V2: &str = include_str!("../../migrations/0002_product.sql");
const COMPLETE_SCHEMA_V3: &str = include_str!("../../migrations/0003_complete.sql");
const TASK_STATES_SCHEMA_V4: &str = include_str!("../../migrations/0004_task_states.sql");
const LEARNING_PATH_SCHEMA_V5: &str = include_str!("../../migrations/0005_learning_path.sql");
const AUTOMATION_SCHEMA_V6: &str = include_str!("../../migrations/0006_automation.sql");

#[derive(Clone)]
pub struct CoreStore {
    pub(crate) pool: SqlitePool,
}

#[derive(Clone, Debug)]
pub struct MaterializationInput {
    pub projection: OccurrenceProjection,
    pub title: String,
    pub notes: Option<String>,
    pub priority: String,
    pub estimated_minutes: Option<i64>,
    pub completion_criteria: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MaterializedTask {
    pub id: Uuid,
    pub series_id: Uuid,
    pub occurrence_key: String,
    pub title: String,
    pub status: String,
    pub time_mode: TimeMode,
    pub scheduled_local: Option<NaiveDateTime>,
    pub scheduled_date: Option<NaiveDate>,
    pub scheduled_utc: Option<DateTime<Utc>>,
    pub tzid: Option<String>,
    pub chosen_offset_seconds: Option<i32>,
    pub dst_adjusted: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid stored identifier in {field}: {value}")]
    InvalidUuid { field: &'static str, value: String },
    #[error("invalid stored timestamp in {field}: {value}")]
    InvalidTimestamp { field: &'static str, value: String },
    #[error("invalid stored date in {field}: {value}")]
    InvalidDate { field: &'static str, value: String },
    #[error("invalid stored time mode: {0}")]
    InvalidTimeMode(String),
    #[error("materialization title must not be blank")]
    BlankTitle,
    #[error("job kind and business key must not be blank")]
    InvalidJobSpec,
    #[error("invalid job owner: {0}")]
    InvalidJobOwner(String),
    #[error("lease owner must not be blank")]
    InvalidLeaseOwner,
    #[error("lease duration is outside the supported range")]
    InvalidLeaseDuration,
    #[error("job lease was not found")]
    LeaseNotFound,
    #[error("invalid JSON stored in {0}")]
    InvalidStoredJson(&'static str),
}

impl CoreStore {
    pub async fn connect(path: &Path) -> Result<Self, StoreError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(10));
        let pool = SqlitePoolOptions::new()
            .min_connections(1)
            .max_connections(8)
            .connect_with(options)
            .await?;
        sqlx::raw_sql(CORE_SCHEMA_V1).execute(&pool).await?;
        let product_schema_applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations WHERE version = 2")
                .fetch_one(&pool)
                .await?;
        if product_schema_applied == 0 {
            sqlx::raw_sql(PRODUCT_SCHEMA_V2).execute(&pool).await?;
        }
        let complete_schema_applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations WHERE version = 3")
                .fetch_one(&pool)
                .await?;
        if complete_schema_applied == 0 {
            sqlx::raw_sql(COMPLETE_SCHEMA_V3).execute(&pool).await?;
        }
        let task_states_schema_applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations WHERE version = 4")
                .fetch_one(&pool)
                .await?;
        if task_states_schema_applied == 0 {
            sqlx::raw_sql(TASK_STATES_SCHEMA_V4).execute(&pool).await?;
        }
        let learning_path_schema_applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations WHERE version = 5")
                .fetch_one(&pool)
                .await?;
        if learning_path_schema_applied == 0 {
            sqlx::raw_sql(LEARNING_PATH_SCHEMA_V5)
                .execute(&pool)
                .await?;
        }
        let automation_schema_applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations WHERE version = 6")
                .fetch_one(&pool)
                .await?;
        if automation_schema_applied == 0 {
            sqlx::raw_sql(AUTOMATION_SCHEMA_V6).execute(&pool).await?;
        }
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub async fn connect_for_test(path: &Path) -> Result<Self, StoreError> {
        Self::connect(path).await
    }

    pub async fn close(self) {
        self.pool.close().await;
    }

    pub async fn create_series(
        &self,
        definition: &RecurrenceDefinition,
        title: &str,
    ) -> Result<(), StoreError> {
        if title.trim().is_empty() {
            return Err(StoreError::BlankTitle);
        }
        let now = now_text();
        let (mode, dtstart_local, tzid) = match &definition.start {
            SeriesStart::AllDay { date } => ("all_day", date.format("%Y-%m-%d").to_string(), None),
            SeriesStart::Floating { local } => (
                "floating",
                local.format("%Y-%m-%dT%H:%M:%S%.9f").to_string(),
                None,
            ),
            SeriesStart::Zoned { local, tzid } => (
                "zoned",
                local.format("%Y-%m-%dT%H:%M:%S%.9f").to_string(),
                Some(tzid.as_str()),
            ),
        };
        let rdates = serde_json::to_string(&definition.rdates)
            .expect("serializing naive recurrence dates cannot fail");
        let exdates = serde_json::to_string(&definition.exdates)
            .expect("serializing naive recurrence dates cannot fail");

        sqlx::query(
            "INSERT INTO recurrence_series(
                id, title, time_mode, dtstart_local, tzid, rrule_text,
                rdates_json, exdates_json, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(definition.series_id.to_string())
        .bind(title.trim())
        .bind(mode)
        .bind(dtstart_local)
        .bind(tzid)
        .bind(&definition.rrule)
        .bind(rdates)
        .bind(exdates)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Materializes one virtual occurrence atomically. Competing callers may
    /// generate different candidate UUIDv7 values, but the unique occurrence
    /// reference and `ON CONFLICT DO NOTHING` guarantee a single durable row.
    pub async fn materialize_occurrence(
        &self,
        input: MaterializationInput,
    ) -> Result<MaterializedTask, StoreError> {
        if input.title.trim().is_empty() {
            return Err(StoreError::BlankTitle);
        }
        let candidate_id = Uuid::now_v7();
        let series_id = input.projection.occurrence_ref.series_id;
        let occurrence_key = input.projection.occurrence_ref.occurrence_key.to_string();
        let now = now_text();
        let (scheduled_local, scheduled_date, tzid) = projection_time_columns(&input.projection);

        sqlx::query(
            "INSERT INTO tasks(
                id, series_id, occurrence_key, title, notes, priority, estimated_minutes,
                completion_criteria, status, time_mode,
                scheduled_local, scheduled_date, scheduled_utc, tzid,
                chosen_offset_seconds, dst_adjusted, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'todo', ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(series_id, occurrence_key) DO NOTHING",
        )
        .bind(candidate_id.to_string())
        .bind(series_id.to_string())
        .bind(&occurrence_key)
        .bind(input.title.trim())
        .bind(input.notes)
        .bind(&input.priority)
        .bind(input.estimated_minutes)
        .bind(input.completion_criteria)
        .bind(mode_text(input.projection.time_mode))
        .bind(scheduled_local)
        .bind(scheduled_date)
        .bind(input.projection.scheduled_utc.map(format_utc))
        .bind(tzid)
        .bind(input.projection.chosen_offset_seconds)
        .bind(i64::from(input.projection.dst_adjusted))
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query(
            "SELECT id, series_id, occurrence_key, title, status, time_mode,
                    scheduled_local, scheduled_date, scheduled_utc, tzid,
                    chosen_offset_seconds, dst_adjusted
             FROM tasks WHERE series_id = ? AND occurrence_key = ?",
        )
        .bind(series_id.to_string())
        .bind(&occurrence_key)
        .fetch_one(&self.pool)
        .await?;
        row_to_task(&row)
    }

    pub async fn task_count(&self) -> Result<i64, StoreError> {
        let row = sqlx::query("SELECT COUNT(*) AS count FROM tasks")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get("count")?)
    }
}

fn projection_time_columns(
    projection: &OccurrenceProjection,
) -> (Option<String>, Option<String>, Option<String>) {
    match &projection.occurrence_ref.occurrence_key {
        super::OccurrenceKey::AllDay { original_date } => (
            None,
            Some(original_date.format("%Y-%m-%d").to_string()),
            None,
        ),
        super::OccurrenceKey::Floating { .. } => (
            Some(
                projection
                    .scheduled_local
                    .format("%Y-%m-%dT%H:%M:%S%.9f")
                    .to_string(),
            ),
            None,
            None,
        ),
        super::OccurrenceKey::Zoned { tzid, .. } => (
            Some(
                projection
                    .scheduled_local
                    .format("%Y-%m-%dT%H:%M:%S%.9f")
                    .to_string(),
            ),
            None,
            Some(tzid.clone()),
        ),
    }
}

fn row_to_task(row: &sqlx::sqlite::SqliteRow) -> Result<MaterializedTask, StoreError> {
    let id = required_uuid(row, "id")?;
    let series_id = required_uuid(row, "series_id")?;
    let time_mode_text: String = row.try_get("time_mode")?;
    let time_mode = parse_mode(&time_mode_text)?;
    let scheduled_local = optional_text(row, "scheduled_local")?
        .map(|value| parse_local("scheduled_local", &value))
        .transpose()?;
    let scheduled_date = optional_text(row, "scheduled_date")?
        .map(|value| {
            NaiveDate::parse_from_str(&value, "%Y-%m-%d").map_err(|_| StoreError::InvalidDate {
                field: "scheduled_date",
                value,
            })
        })
        .transpose()?;
    let scheduled_utc = optional_text(row, "scheduled_utc")?
        .map(|value| {
            DateTime::parse_from_rfc3339(&value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|_| StoreError::InvalidTimestamp {
                    field: "scheduled_utc",
                    value,
                })
        })
        .transpose()?;

    Ok(MaterializedTask {
        id,
        series_id,
        occurrence_key: row.try_get("occurrence_key")?,
        title: row.try_get("title")?,
        status: row.try_get("status")?,
        time_mode,
        scheduled_local,
        scheduled_date,
        scheduled_utc,
        tzid: optional_text(row, "tzid")?,
        chosen_offset_seconds: row.try_get("chosen_offset_seconds")?,
        dst_adjusted: row.try_get::<i64, _>("dst_adjusted")? != 0,
    })
}

fn required_uuid(row: &sqlx::sqlite::SqliteRow, field: &'static str) -> Result<Uuid, StoreError> {
    let value: String = row.try_get(field)?;
    Uuid::from_str(&value).map_err(|_| StoreError::InvalidUuid { field, value })
}

fn optional_text(
    row: &sqlx::sqlite::SqliteRow,
    field: &'static str,
) -> Result<Option<String>, StoreError> {
    Ok(row.try_get(field)?)
}

fn parse_local(field: &'static str, value: &str) -> Result<NaiveDateTime, StoreError> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.9f").map_err(|_| {
        StoreError::InvalidTimestamp {
            field,
            value: value.to_owned(),
        }
    })
}

fn parse_mode(value: &str) -> Result<TimeMode, StoreError> {
    match value {
        "all_day" => Ok(TimeMode::AllDay),
        "floating" => Ok(TimeMode::Floating),
        "zoned" => Ok(TimeMode::Zoned),
        _ => Err(StoreError::InvalidTimeMode(value.to_owned())),
    }
}

const fn mode_text(mode: TimeMode) -> &'static str {
    match mode {
        TimeMode::AllDay => "all_day",
        TimeMode::Floating => "floating",
        TimeMode::Zoned => "zoned",
    }
}

fn now_text() -> String {
    format_utc(Utc::now())
}

fn format_utc(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::NaiveDateTime;

    use crate::core::{expand_virtual, RecurrenceDefinition, SeriesStart};

    use super::*;

    fn local(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn test_path() -> PathBuf {
        std::env::temp_dir().join(format!("pga-core-{}.sqlite3", Uuid::now_v7()))
    }

    #[tokio::test]
    async fn virtual_reads_do_not_create_task_rows() {
        let path = test_path();
        let store = CoreStore::connect_for_test(&path).await.unwrap();
        let definition = RecurrenceDefinition {
            series_id: Uuid::now_v7(),
            start: SeriesStart::Floating {
                local: local("2026-01-01 09:00:00"),
            },
            rrule: "FREQ=DAILY;COUNT=365".to_owned(),
            rdates: Vec::new(),
            exdates: Vec::new(),
        };
        store.create_series(&definition, "Read").await.unwrap();
        assert_eq!(store.task_count().await.unwrap(), 0);
        let projected = expand_virtual(
            &definition,
            local("2026-01-01 00:00:00"),
            local("2027-01-01 00:00:00"),
            400,
        )
        .unwrap();
        assert_eq!(projected.len(), 365);
        assert_eq!(store.task_count().await.unwrap(), 0);

        store.close().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn twenty_concurrent_materializations_create_exactly_one_task() {
        let path = test_path();
        let store = CoreStore::connect_for_test(&path).await.unwrap();
        let definition = RecurrenceDefinition {
            series_id: Uuid::now_v7(),
            start: SeriesStart::Zoned {
                local: local("2026-09-03 09:00:00"),
                tzid: "Asia/Shanghai".to_owned(),
            },
            rrule: "FREQ=DAILY;COUNT=2".to_owned(),
            rdates: Vec::new(),
            exdates: Vec::new(),
        };
        store.create_series(&definition, "Focus").await.unwrap();
        let projection = expand_virtual(
            &definition,
            local("2026-09-03 00:00:00"),
            local("2026-09-04 00:00:00"),
            10,
        )
        .unwrap()
        .remove(0);

        let mut handles = Vec::new();
        for _ in 0..20 {
            let store = store.clone();
            let input = MaterializationInput {
                projection: projection.clone(),
                title: "Focus".to_owned(),
                notes: None,
                priority: "p1".to_owned(),
                estimated_minutes: Some(25),
                completion_criteria: None,
            };
            handles.push(tokio::spawn(async move {
                store.materialize_occurrence(input).await.unwrap()
            }));
        }
        let mut ids = Vec::new();
        for handle in handles {
            ids.push(handle.await.unwrap().id);
        }
        assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(store.task_count().await.unwrap(), 1);

        store.close().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn version_three_database_upgrades_without_losing_tasks_or_foreign_keys() {
        let path = test_path();
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::raw_sql(CORE_SCHEMA_V1).execute(&pool).await.unwrap();
        sqlx::raw_sql(PRODUCT_SCHEMA_V2)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(COMPLETE_SCHEMA_V3)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO tasks(
                id, title, status, time_mode, scheduled_date, version,
                created_at_utc, updated_at_utc, progress_percent
             ) VALUES ('legacy-task', 'Legacy task', 'cancelled', 'all_day',
                       '2026-09-03', 3, '2026-09-03T00:00:00Z',
                       '2026-09-03T00:00:00Z', 25)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let store = CoreStore::connect_for_test(&path).await.unwrap();
        let migrated: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT status, version, snoozed_until_utc FROM tasks WHERE id = 'legacy-task'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(migrated, ("canceled".to_owned(), 3, None));
        let migration_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations WHERE version = 5")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(migration_count, 1);
        let learning_columns: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('knowledge_nodes')
             WHERE name IN ('stage_outcome', 'estimated_minutes')",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(learning_columns, 2);
        let foreign_key_violations: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(foreign_key_violations, 0);

        store.close().await;
        let _ = std::fs::remove_file(path);
    }
}
