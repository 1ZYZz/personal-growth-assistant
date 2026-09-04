use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Acquire, Row};
use uuid::Uuid;

use super::{CoreStore, StoreError};

const MAX_AUTOMATIC_RETRIES: i64 = 1;
const RETRY_BACKOFF_SECONDS: i64 = 30;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobSpec {
    pub kind: String,
    pub business_key: String,
    pub scheduled_for_utc: DateTime<Utc>,
    pub run_at_utc: DateTime<Utc>,
    #[serde(default = "local_owner")]
    pub owner: String,
    #[serde(default)]
    pub payload: Value,
}

fn local_owner() -> String {
    "local".to_owned()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub run_id: Uuid,
    pub kind: String,
    pub business_key: String,
    pub scheduled_for_utc: DateTime<Utc>,
    pub attempt_class: String,
    pub payload: Value,
    pub lease_token: Uuid,
    pub leased_until_utc: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobCompletion {
    Succeeded,
    RetryScheduled,
    PermanentlyFailed,
    AlreadyFinalized,
}

#[derive(Clone)]
pub struct SchedulerStore {
    core: CoreStore,
}

impl From<&CoreStore> for SchedulerStore {
    fn from(core: &CoreStore) -> Self {
        Self { core: core.clone() }
    }
}

impl SchedulerStore {
    pub async fn enqueue(&self, spec: &JobSpec) -> Result<Uuid, StoreError> {
        if !matches!(spec.owner.as_str(), "local" | "external") {
            return Err(StoreError::InvalidJobOwner(spec.owner.clone()));
        }
        if spec.kind.trim().is_empty() || spec.business_key.trim().is_empty() {
            return Err(StoreError::InvalidJobSpec);
        }

        let candidate_id = Uuid::now_v7();
        let now = utc_text(Utc::now());
        let scheduled = utc_text(spec.scheduled_for_utc);
        sqlx::query(
            "INSERT INTO jobs(
                id, kind, owner, business_key, scheduled_for_utc, attempt_class,
                run_at_utc, state, payload_json, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, ?, ?, 'primary', ?, 'pending', ?, ?, ?)
             ON CONFLICT(business_key, scheduled_for_utc, attempt_class) DO NOTHING",
        )
        .bind(candidate_id.to_string())
        .bind(spec.kind.trim())
        .bind(&spec.owner)
        .bind(spec.business_key.trim())
        .bind(&scheduled)
        .bind(utc_text(spec.run_at_utc))
        .bind(spec.payload.to_string())
        .bind(&now)
        .bind(&now)
        .execute(&self.core.pool)
        .await?;

        let row = sqlx::query(
            "SELECT id FROM jobs
             WHERE business_key = ? AND scheduled_for_utc = ? AND attempt_class = 'primary'",
        )
        .bind(spec.business_key.trim())
        .bind(scheduled)
        .fetch_one(&self.core.pool)
        .await?;
        parse_uuid("jobs.id", row.try_get::<String, _>("id")?)
    }

    pub async fn claim_due(
        &self,
        lease_owner: &str,
        now: DateTime<Utc>,
        lease_duration: StdDuration,
    ) -> Result<Option<ClaimedJob>, StoreError> {
        self.claim_due_filtered(lease_owner, now, lease_duration, true)
            .await
    }

    pub async fn claim_due_filtered(
        &self,
        lease_owner: &str,
        now: DateTime<Utc>,
        lease_duration: StdDuration,
        include_reminders: bool,
    ) -> Result<Option<ClaimedJob>, StoreError> {
        if lease_owner.trim().is_empty() {
            return Err(StoreError::InvalidLeaseOwner);
        }
        let mut connection = self.core.pool.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await?;
        recover_expired_in_transaction(&mut connection, now).await?;

        let row = sqlx::query(
            "SELECT id, kind, business_key, scheduled_for_utc, attempt_class, payload_json
             FROM jobs
             WHERE owner = 'local' AND state = 'pending' AND run_at_utc <= ?
               AND (? = 1 OR kind != 'reminder')
             ORDER BY run_at_utc, id
             LIMIT 1",
        )
        .bind(utc_text(now))
        .bind(i64::from(include_reminders))
        .fetch_optional(&mut *connection)
        .await?;

        let Some(row) = row else {
            sqlx::query("COMMIT").execute(&mut *connection).await?;
            return Ok(None);
        };

        let job_id_text: String = row.try_get("id")?;
        let job_id = parse_uuid("jobs.id", job_id_text.clone())?;
        let run_id = Uuid::now_v7();
        let lease_token = Uuid::now_v7();
        let attempt_class: String = row.try_get("attempt_class")?;
        let scheduled_text: String = row.try_get("scheduled_for_utc")?;
        let scheduled_for_utc = parse_utc("jobs.scheduled_for_utc", &scheduled_text)?;
        let lease_delta =
            Duration::from_std(lease_duration).map_err(|_| StoreError::InvalidLeaseDuration)?;
        let leased_until_utc = now + lease_delta;
        let now_text = utc_text(now);

        let updated = sqlx::query(
            "UPDATE jobs SET state = 'leased', updated_at_utc = ?, version = version + 1
             WHERE id = ? AND state = 'pending'",
        )
        .bind(&now_text)
        .bind(&job_id_text)
        .execute(&mut *connection)
        .await?;
        if updated.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await?;
            return Ok(None);
        }

        sqlx::query(
            "INSERT INTO job_runs(
                id, job_id, scheduled_for_utc, attempt_class, lease_token,
                outcome, started_at_utc
             ) VALUES (?, ?, ?, ?, ?, 'running', ?)",
        )
        .bind(run_id.to_string())
        .bind(&job_id_text)
        .bind(&scheduled_text)
        .bind(&attempt_class)
        .bind(lease_token.to_string())
        .bind(&now_text)
        .execute(&mut *connection)
        .await?;
        sqlx::query(
            "INSERT INTO job_leases(
                job_id, lease_owner, lease_token, run_id, leased_until_utc,
                acquired_at_utc, heartbeat_at_utc
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&job_id_text)
        .bind(lease_owner.trim())
        .bind(lease_token.to_string())
        .bind(run_id.to_string())
        .bind(utc_text(leased_until_utc))
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *connection)
        .await?;
        sqlx::query("COMMIT").execute(&mut *connection).await?;

        let payload_text: String = row.try_get("payload_json")?;
        let payload = serde_json::from_str(&payload_text)
            .map_err(|_| StoreError::InvalidStoredJson("jobs.payload_json"))?;
        Ok(Some(ClaimedJob {
            id: job_id,
            run_id,
            kind: row.try_get("kind")?,
            business_key: row.try_get("business_key")?,
            scheduled_for_utc,
            attempt_class,
            payload,
            lease_token,
            leased_until_utc,
        }))
    }

    pub async fn heartbeat(
        &self,
        job_id: Uuid,
        lease_token: Uuid,
        now: DateTime<Utc>,
        lease_duration: StdDuration,
    ) -> Result<bool, StoreError> {
        let lease_delta =
            Duration::from_std(lease_duration).map_err(|_| StoreError::InvalidLeaseDuration)?;
        let result = sqlx::query(
            "UPDATE job_leases
             SET heartbeat_at_utc = ?, leased_until_utc = ?
             WHERE job_id = ? AND lease_token = ?",
        )
        .bind(utc_text(now))
        .bind(utc_text(now + lease_delta))
        .bind(job_id.to_string())
        .bind(lease_token.to_string())
        .execute(&self.core.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn complete(
        &self,
        job: &ClaimedJob,
        succeeded: bool,
        error_message: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<JobCompletion, StoreError> {
        let mut connection = self.core.pool.acquire().await?;
        let mut transaction = connection.begin().await?;
        let lease =
            sqlx::query("SELECT run_id FROM job_leases WHERE job_id = ? AND lease_token = ?")
                .bind(job.id.to_string())
                .bind(job.lease_token.to_string())
                .fetch_optional(&mut *transaction)
                .await?;
        if lease.is_none() {
            transaction.rollback().await?;
            let previous = sqlx::query("SELECT outcome FROM job_runs WHERE id = ?")
                .bind(job.run_id.to_string())
                .fetch_optional(&self.core.pool)
                .await?;
            return if previous.is_some() {
                Ok(JobCompletion::AlreadyFinalized)
            } else {
                Err(StoreError::LeaseNotFound)
            };
        }

        let now_text = utc_text(now);
        let completion = if succeeded {
            sqlx::query(
                "UPDATE job_runs SET outcome = 'succeeded', finished_at_utc = ?
                 WHERE id = ? AND outcome = 'running'",
            )
            .bind(&now_text)
            .bind(job.run_id.to_string())
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE jobs SET state = 'succeeded', updated_at_utc = ?, version = version + 1
                 WHERE id = ?",
            )
            .bind(&now_text)
            .bind(job.id.to_string())
            .execute(&mut *transaction)
            .await?;
            JobCompletion::Succeeded
        } else if job.attempt_class == "primary" {
            sqlx::query(
                "UPDATE job_runs SET outcome = 'failed', error_message = ?, finished_at_utc = ?
                 WHERE id = ? AND outcome = 'running'",
            )
            .bind(error_message)
            .bind(&now_text)
            .bind(job.run_id.to_string())
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE jobs
                 SET state = 'pending', attempt_count = 1, attempt_class = 'retry',
                     run_at_utc = ?, updated_at_utc = ?, version = version + 1
                 WHERE id = ?",
            )
            .bind(utc_text(now + Duration::seconds(RETRY_BACKOFF_SECONDS)))
            .bind(&now_text)
            .bind(job.id.to_string())
            .execute(&mut *transaction)
            .await?;
            JobCompletion::RetryScheduled
        } else {
            sqlx::query(
                "UPDATE job_runs SET outcome = 'failed', error_message = ?, finished_at_utc = ?
                 WHERE id = ? AND outcome = 'running'",
            )
            .bind(error_message)
            .bind(&now_text)
            .bind(job.run_id.to_string())
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE jobs SET state = 'failed', updated_at_utc = ?, version = version + 1
                 WHERE id = ?",
            )
            .bind(&now_text)
            .bind(job.id.to_string())
            .execute(&mut *transaction)
            .await?;
            JobCompletion::PermanentlyFailed
        };
        sqlx::query("DELETE FROM job_leases WHERE job_id = ? AND lease_token = ?")
            .bind(job.id.to_string())
            .bind(job.lease_token.to_string())
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(completion)
    }

    pub async fn reconcile(&self, now: DateTime<Utc>) -> Result<u64, StoreError> {
        let mut connection = self.core.pool.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await?;
        let recovered = recover_expired_in_transaction(&mut connection, now).await?;
        sqlx::query("COMMIT").execute(&mut *connection).await?;
        Ok(recovered)
    }

    pub async fn next_run_at(&self) -> Result<Option<DateTime<Utc>>, StoreError> {
        let row = sqlx::query(
            "SELECT run_at_utc FROM jobs
             WHERE owner = 'local' AND state = 'pending'
             ORDER BY run_at_utc LIMIT 1",
        )
        .fetch_optional(&self.core.pool)
        .await?;
        row.map(|row| {
            let value: String = row.try_get("run_at_utc")?;
            parse_utc("jobs.run_at_utc", &value)
        })
        .transpose()
    }

    #[cfg(test)]
    async fn state_of(&self, job_id: Uuid) -> Result<String, StoreError> {
        let row = sqlx::query("SELECT state FROM jobs WHERE id = ?")
            .bind(job_id.to_string())
            .fetch_one(&self.core.pool)
            .await?;
        Ok(row.try_get("state")?)
    }
}

async fn recover_expired_in_transaction(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    now: DateTime<Utc>,
) -> Result<u64, StoreError> {
    let rows = sqlx::query(
        "SELECT l.job_id, l.run_id, j.attempt_count
         FROM job_leases l JOIN jobs j ON j.id = l.job_id
         WHERE l.leased_until_utc <= ? AND j.state = 'leased'",
    )
    .bind(utc_text(now))
    .fetch_all(&mut **connection)
    .await?;
    let now_text = utc_text(now);

    for row in &rows {
        let job_id: String = row.try_get("job_id")?;
        let run_id: String = row.try_get("run_id")?;
        let attempts: i64 = row.try_get("attempt_count")?;
        sqlx::query(
            "UPDATE job_runs
             SET outcome = 'interrupted', error_message = 'lease expired', finished_at_utc = ?
             WHERE id = ? AND outcome = 'running'",
        )
        .bind(&now_text)
        .bind(run_id)
        .execute(&mut **connection)
        .await?;
        sqlx::query("DELETE FROM job_leases WHERE job_id = ?")
            .bind(&job_id)
            .execute(&mut **connection)
            .await?;

        if attempts < MAX_AUTOMATIC_RETRIES {
            sqlx::query(
                "UPDATE jobs
                 SET state = 'pending', attempt_count = attempt_count + 1,
                     attempt_class = 'retry', run_at_utc = ?, updated_at_utc = ?,
                     version = version + 1
                 WHERE id = ?",
            )
            .bind(utc_text(now + Duration::seconds(RETRY_BACKOFF_SECONDS)))
            .bind(&now_text)
            .bind(job_id)
            .execute(&mut **connection)
            .await?;
        } else {
            sqlx::query(
                "UPDATE jobs SET state = 'failed', updated_at_utc = ?, version = version + 1
                 WHERE id = ?",
            )
            .bind(&now_text)
            .bind(job_id)
            .execute(&mut **connection)
            .await?;
        }
    }
    Ok(rows.len() as u64)
}

fn utc_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_utc(field: &'static str, value: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| StoreError::InvalidTimestamp {
            field,
            value: value.to_owned(),
        })
}

fn parse_uuid(field: &'static str, value: String) -> Result<Uuid, StoreError> {
    Uuid::parse_str(&value).map_err(|_| StoreError::InvalidUuid { field, value })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn path() -> PathBuf {
        std::env::temp_dir().join(format!("pga-scheduler-{}.sqlite3", Uuid::now_v7()))
    }

    fn spec(now: DateTime<Utc>) -> JobSpec {
        JobSpec {
            kind: "reminder".to_owned(),
            business_key: "occurrence/reminder-10m".to_owned(),
            scheduled_for_utc: now,
            run_at_utc: now,
            owner: "local".to_owned(),
            payload: serde_json::json!({"priority": "p1"}),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_claimers_receive_one_lease() {
        let path = path();
        let core = CoreStore::connect_for_test(&path).await.unwrap();
        let scheduler = SchedulerStore::from(&core);
        let now = Utc::now();
        scheduler.enqueue(&spec(now)).await.unwrap();

        let mut handles = Vec::new();
        for index in 0..20 {
            let scheduler = scheduler.clone();
            handles.push(tokio::spawn(async move {
                scheduler
                    .claim_due(&format!("worker-{index}"), now, StdDuration::from_secs(30))
                    .await
                    .unwrap()
            }));
        }
        let mut claimed = Vec::new();
        for handle in handles {
            if let Some(job) = handle.await.unwrap() {
                claimed.push(job);
            }
        }
        assert_eq!(claimed.len(), 1);
        assert!(scheduler
            .heartbeat(
                claimed[0].id,
                claimed[0].lease_token,
                now,
                StdDuration::from_secs(60),
            )
            .await
            .unwrap());

        core.close().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn expired_lease_is_interrupted_and_retried_only_once() {
        let path = path();
        let core = CoreStore::connect_for_test(&path).await.unwrap();
        let scheduler = SchedulerStore::from(&core);
        let now = Utc::now();
        let job_id = scheduler.enqueue(&spec(now)).await.unwrap();
        scheduler
            .claim_due("worker-a", now, StdDuration::from_secs(10))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            scheduler
                .reconcile(now + Duration::seconds(11))
                .await
                .unwrap(),
            1
        );
        assert_eq!(scheduler.state_of(job_id).await.unwrap(), "pending");
        let retry = scheduler
            .claim_due(
                "worker-b",
                now + Duration::seconds(45),
                StdDuration::from_secs(10),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retry.attempt_class, "retry");
        assert_eq!(
            scheduler
                .complete(
                    &retry,
                    false,
                    Some("still unavailable"),
                    now + Duration::seconds(46),
                )
                .await
                .unwrap(),
            JobCompletion::PermanentlyFailed
        );
        assert_eq!(scheduler.state_of(job_id).await.unwrap(), "failed");
        assert_eq!(
            scheduler
                .complete(
                    &retry,
                    false,
                    Some("duplicate callback"),
                    now + Duration::seconds(47),
                )
                .await
                .unwrap(),
            JobCompletion::AlreadyFinalized
        );

        core.close().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn enqueue_is_idempotent_for_the_business_schedule() {
        let path = path();
        let core = CoreStore::connect_for_test(&path).await.unwrap();
        let scheduler = SchedulerStore::from(&core);
        let now = Utc::now();
        let first = scheduler.enqueue(&spec(now)).await.unwrap();
        let second = scheduler.enqueue(&spec(now)).await.unwrap();
        assert_eq!(first, second);

        core.close().await;
        let _ = std::fs::remove_file(path);
    }
}
