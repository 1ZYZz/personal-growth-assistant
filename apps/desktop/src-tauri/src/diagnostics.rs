use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use sqlx::Row;

use crate::core::CoreStore;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticReport {
    app_version: &'static str,
    database_path: String,
    database_bytes: u64,
    schema_versions: Vec<i64>,
    integrity_check: String,
    foreign_key_violations: i64,
    task_count: i64,
    sidecar_probe: String,
}

pub fn write_diagnostic_report(output: &Path) -> Result<(), String> {
    let app_data = PathBuf::from(
        std::env::var_os("APPDATA").ok_or_else(|| "APPDATA is unavailable".to_owned())?,
    );
    let database_path = app_data
        .join("org.personal-growth-assistant.desktop")
        .join("personal-growth.sqlite3");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let report = runtime.block_on(async {
        let store = CoreStore::connect(&database_path)
            .await
            .map_err(|error| error.to_string())?;
        let schema_versions: Vec<i64> =
            sqlx::query_scalar("SELECT version FROM schema_migrations ORDER BY version")
                .fetch_all(&store.pool)
                .await
                .map_err(|error| error.to_string())?;
        let integrity_check: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&store.pool)
            .await
            .map_err(|error| error.to_string())?;
        let foreign_key_violations = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&store.pool)
            .await
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|row| row.try_get::<String, _>("table").is_ok())
            .count() as i64;
        let task_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks")
            .fetch_one(&store.pool)
            .await
            .map_err(|error| error.to_string())?;
        store.close().await;
        Ok::<_, String>((
            schema_versions,
            integrity_check,
            foreign_key_violations,
            task_count,
        ))
    })?;
    let sidecar_probe = probe_installed_sidecar();
    let diagnostic = DiagnosticReport {
        app_version: env!("CARGO_PKG_VERSION"),
        database_path: database_path.to_string_lossy().into_owned(),
        database_bytes: fs::metadata(&database_path)
            .map_err(|error| error.to_string())?
            .len(),
        schema_versions: report.0,
        integrity_check: report.1,
        foreign_key_violations: report.2,
        task_count: report.3,
        sidecar_probe,
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        output,
        serde_json::to_vec_pretty(&diagnostic).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn probe_installed_sidecar() -> String {
    let Some(directory) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return "unavailable".to_owned();
    };
    let executable = directory.join("pga-agent-bridge.exe");
    if !executable.is_file() {
        return "unavailable".to_owned();
    }
    let mut command = Command::new(executable);
    command.arg("probe").env_clear();
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    command
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_owned())
        .unwrap_or_else(|| "unavailable".to_owned())
}
