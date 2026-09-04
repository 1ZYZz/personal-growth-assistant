pub mod core;
mod data;
mod diagnostics;
mod intelligence;
mod learning;
mod product;
mod reminders;
mod runtime;
mod supervision;

use std::{
    fs, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};

use crate::{
    core::CoreStore,
    data::{
        apply_pending_restore, create_backup, create_daily_backup, export_data, get_preferences,
        get_startup_enabled, list_backups, load_preferences, restore_backup, save_preferences,
        set_startup_enabled, third_party_notices,
    },
    intelligence::{
        create_watch_field, create_watch_source, delete_watch_field, delete_watch_source,
        list_news, list_watch_fields, list_watch_sources, refresh_intelligence,
        reorder_watch_fields, reorder_watch_sources, set_news_feedback, set_watch_field_enabled,
        set_watch_source_enabled,
    },
    learning::{
        create_knowledge_node, create_learning_goal, create_learning_resource, create_quiz_item,
        list_knowledge_nodes, list_learning_goals, list_learning_resources, list_quiz_items,
        submit_quiz,
    },
    product::{
        create_recurring_task, create_task, dashboard_summary, delete_task, list_tasks, list_trash,
        record_not_started_review, record_partial_progress, restore_task, runtime_info,
        set_task_status, update_recurring_series, update_task, AppState,
    },
    reminders::{
        handle_activation_arguments, list_notifications, pause_notifications,
        run_notification_action, scheduler_loop, set_notification_pause, show_main_window,
    },
    runtime::{
        cancel_agent_run, generate_morning_brief, get_agent_budget, logout_agent_runtime,
        probe_agent_runtime, save_agent_budget, start_agent_login,
    },
    supervision::{
        apply_proposal, generate_daily_lesson, generate_digest, get_automation_settings,
        list_daily_lessons, list_digests, list_recent_failures, propose_lesson_tasks,
        reject_proposal, save_automation_settings,
    },
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FoundationStatus {
    app_version: &'static str,
    canonical_store: &'static str,
    milestone: &'static str,
}

#[tauri::command]
fn foundation_status() -> FoundationStatus {
    FoundationStatus {
        app_version: env!("CARGO_PKG_VERSION"),
        canonical_store: "sqlite",
        milestone: "release-1.0",
    }
}

fn set_pause_from_tray(app: &AppHandle, mode: &'static str) {
    let state = app.state::<AppState>();
    let store = state.store.clone();
    let wake = state.scheduler_wake.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(until) = set_notification_pause(&store, mode).await {
            wake.send_modify(|version| *version += 1);
            let _ = handle.emit(
                "notification-pause-changed",
                until.map(|value| value.to_rfc3339()),
            );
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            |app, arguments, _cwd| {
                handle_activation_arguments(app, &arguments);
            },
        ))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_directory = app.path().app_data_dir()?;
            fs::create_dir_all(&data_directory)?;
            apply_pending_restore(&data_directory).map_err(io::Error::other)?;
            let database_path = data_directory.join("personal-growth.sqlite3");
            let store = tauri::async_runtime::block_on(CoreStore::connect(&database_path))
                .map_err(io::Error::other)?;
            tauri::async_runtime::block_on(create_daily_backup(&store, &data_directory))
                .map_err(io::Error::other)?;
            let preferences = tauri::async_runtime::block_on(load_preferences(&store))
                .map_err(io::Error::other)?;
            let (scheduler_wake, scheduler_receiver) = tokio::sync::watch::channel(0_u64);
            app.manage(AppState {
                store: store.clone(),
                data_directory: data_directory.to_string_lossy().into_owned(),
                scheduler_wake,
                close_to_tray: Arc::new(AtomicBool::new(preferences.close_to_tray)),
                agent_cancel: tokio::sync::Mutex::new(None),
            });

            let show_item =
                MenuItem::with_id(app, "show_today", "今日 / Today", true, None::<&str>)?;
            let quick_add_item =
                MenuItem::with_id(app, "quick_add", "快速新增 / Quick add", true, None::<&str>)?;
            let pause_item = MenuItem::with_id(
                app,
                "pause_hour",
                "暂停提醒 1 小时 / Pause 1 hour",
                true,
                None::<&str>,
            )?;
            let quiet_today_item = MenuItem::with_id(
                app,
                "quiet_today",
                "今日免打扰 / Quiet today",
                true,
                None::<&str>,
            )?;
            let resume_item = MenuItem::with_id(
                app,
                "resume_reminders",
                "恢复提醒 / Resume reminders",
                true,
                None::<&str>,
            )?;
            let quit_item =
                MenuItem::with_id(app, "quit", "退出（提醒将暂停）/ Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(
                app,
                &[
                    &show_item,
                    &quick_add_item,
                    &pause_item,
                    &quiet_today_item,
                    &resume_item,
                    &quit_item,
                ],
            )?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| io::Error::other("application icon is unavailable"))?;
            TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .tooltip("个人成长助手")
                .build(app)?;

            let app_handle = app.handle().clone();
            let startup_store = store.clone();
            tauri::async_runtime::spawn(async move {
                crate::supervision::write_log(
                    &startup_store,
                    "app",
                    "info",
                    "application_started",
                    &uuid::Uuid::now_v7().to_string(),
                    "应用已启动，SQLite 数据库可用",
                    serde_json::json!({"schemaVersion": 6}),
                )
                .await;
                crate::supervision::write_log(
                    &startup_store,
                    "database",
                    "info",
                    "database_ready",
                    &uuid::Uuid::now_v7().to_string(),
                    "数据库迁移、WAL 与外键检查已完成",
                    serde_json::json!({"journalMode": "WAL", "foreignKeys": true}),
                )
                .await;
            });
            tauri::async_runtime::spawn(scheduler_loop(app_handle, store, scheduler_receiver));
            let initial_arguments = std::env::args().collect::<Vec<_>>();
            if initial_arguments
                .iter()
                .any(|argument| argument.starts_with("pga://"))
            {
                handle_activation_arguments(app.handle(), &initial_arguments);
            }
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_today" => {
                show_main_window(app);
                let _ = app.emit("tray-show-today", ());
            }
            "quick_add" => {
                show_main_window(app);
                let _ = app.emit("tray-quick-add", ());
            }
            "pause_hour" => set_pause_from_tray(app, "one_hour"),
            "quiet_today" => set_pause_from_tray(app, "today"),
            "resume_reminders" => set_pause_from_tray(app, "clear"),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|app, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show_main_window(app);
            }
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if window
                        .state::<AppState>()
                        .close_to_tray
                        .load(Ordering::Relaxed)
                    {
                        let _ = window.hide();
                    } else {
                        window.app_handle().exit(0);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            foundation_status,
            create_task,
            create_recurring_task,
            list_tasks,
            set_task_status,
            update_task,
            update_recurring_series,
            delete_task,
            list_trash,
            restore_task,
            record_partial_progress,
            record_not_started_review,
            dashboard_summary,
            runtime_info,
            list_notifications,
            run_notification_action,
            pause_notifications,
            create_backup,
            list_backups,
            restore_backup,
            export_data,
            get_preferences,
            save_preferences,
            get_startup_enabled,
            set_startup_enabled,
            third_party_notices,
            list_watch_fields,
            create_watch_field,
            set_watch_field_enabled,
            delete_watch_field,
            reorder_watch_fields,
            list_watch_sources,
            create_watch_source,
            set_watch_source_enabled,
            delete_watch_source,
            reorder_watch_sources,
            refresh_intelligence,
            list_news,
            set_news_feedback,
            list_learning_goals,
            create_learning_goal,
            list_knowledge_nodes,
            create_knowledge_node,
            list_learning_resources,
            create_learning_resource,
            create_quiz_item,
            list_quiz_items,
            submit_quiz,
            probe_agent_runtime,
            start_agent_login,
            logout_agent_runtime,
            generate_morning_brief,
            cancel_agent_run,
            get_agent_budget,
            save_agent_budget,
            get_automation_settings,
            save_automation_settings,
            generate_digest,
            list_digests,
            generate_daily_lesson,
            list_daily_lessons,
            propose_lesson_tasks,
            apply_proposal,
            reject_proposal,
            list_recent_failures,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Personal Growth Assistant");
}

pub use diagnostics::write_diagnostic_report;

#[cfg(test)]
mod tests {
    use super::foundation_status;

    #[test]
    fn foundation_status_keeps_sqlite_as_the_canonical_store() {
        let status = foundation_status();
        assert_eq!(status.canonical_store, "sqlite");
        assert_eq!(status.milestone, "release-1.0");
    }
}
