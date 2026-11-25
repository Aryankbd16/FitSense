use std::sync::Arc;

use serde::Serialize;
use tauri::{Manager, RunEvent, WebviewWindowBuilder};

use crate::config::WorkerConfig;
use crate::diagnostics::{self, Diagnostics, Event};
use crate::storage::{Mode, Storage};
use crate::wardrobe::commands;
use crate::worker::{Failure, Report, Snapshot, Status, Supervisor};

struct Desktop {
    supervisor: Option<Arc<Supervisor>>,
    logs: Arc<Diagnostics>,
    mode: Mode,
    schema_version: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemStatus {
    worker: Snapshot,
    storage_mode: Mode,
    schema_version: Option<u32>,
    private_storage_locked: bool,
    logs_available: bool,
}

#[tauri::command]
fn system_status(
    state: tauri::State<'_, Desktop>,
    wardrobe: tauri::State<'_, commands::Context>,
) -> SystemStatus {
    let worker = state.supervisor.as_ref().map_or(
        Snapshot {
            status: Status::Unavailable,
            failure: Some(Failure::Launch),
            latency_ms: None,
        },
        |worker| worker.snapshot(),
    );
    SystemStatus {
        worker,
        storage_mode: state.mode,
        schema_version: state.schema_version,
        logs_available: state.logs.available(),
        private_storage_locked: wardrobe.locked(),
    }
}

#[tauri::command]
fn retry_worker(state: tauri::State<'_, Desktop>) -> Result<(), Failure> {
    state
        .supervisor
        .as_ref()
        .ok_or(Failure::Unavailable)?
        .retry()
}

#[tauri::command]
async fn worker_diagnostics(state: tauri::State<'_, Desktop>) -> Result<Report, Failure> {
    let worker = Arc::clone(state.supervisor.as_ref().ok_or(Failure::Unavailable)?);
    tauri::async_runtime::spawn_blocking(move || worker.diagnose())
        .await
        .map_err(|_| Failure::Unavailable)?
}

pub fn run() -> tauri::Result<()> {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            system_status,
            retry_worker,
            worker_diagnostics,
            commands::wardrobe_categories,
            commands::wardrobe_state,
            commands::wardrobe_setup,
            commands::wardrobe_unlock,
            commands::wardrobe_change_passphrase,
            commands::wardrobe_list,
            commands::wardrobe_detail,
            commands::wardrobe_save,
            commands::wardrobe_delete,
            commands::wardrobe_image,
            commands::wardrobe_backups,
            commands::wardrobe_create_backup,
            commands::wardrobe_restore,
            commands::wardrobe_cancel_backup
        ])
        .setup(|app| {
            let storage = Storage::resolve(
                &std::env::current_exe()?,
                app.path().app_local_data_dir()?,
                cfg!(debug_assertions),
            )?;
            std::fs::create_dir_all(storage.root.join("webview"))?;
            let layout = storage.initialize_directories();
            let retention = std::env::var("FITSENSE_LOG_RETENTION_DAYS").ok();
            let configured = diagnostics::retention(retention.as_deref());
            let logs = Arc::new(Diagnostics::open(
                &storage.root.join("logs"),
                configured
                    .unwrap_or_else(|| diagnostics::retention(None).expect("default retention")),
            ));
            logs.record(Event::AppStarted, None);
            if configured.is_none() {
                logs.record(Event::AppConfigurationError, None);
            }
            let schema_version = if layout.is_ok() {
                storage.initialize_database().ok()
            } else {
                None
            };
            logs.record(
                if schema_version.is_some() {
                    Event::StorageReady
                } else {
                    Event::StorageFailed
                },
                None,
            );
            let supervisor = WorkerConfig::resolve(&app.path().resource_dir()?)
                .and_then(|config| Supervisor::start(config, Arc::clone(&logs)))
                .map(Arc::new)
                .ok();
            if supervisor.is_none() {
                logs.record(Event::WorkerLaunchFailed, None);
            }
            app.manage(commands::Context::new(
                storage.root.clone(),
                schema_version,
                Arc::clone(&logs),
            ));
            app.manage(Desktop {
                supervisor,
                logs,
                mode: storage.mode,
                schema_version,
            });
            WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .data_directory(storage.root.join("webview"))
                .incognito(true)
                .general_autofill_enabled(false)
                .on_navigation(|url| {
                    let host = url.host_str();
                    (((host == Some("tauri.localhost") && url.scheme() == "http")
                        || (url.scheme() == "tauri" && host == Some("localhost")))
                        && url.port().is_none())
                        || (cfg!(debug_assertions)
                            && url.scheme() == "http"
                            && host == Some("127.0.0.1")
                            && url.port() == Some(1420))
                })
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            let state = handle.state::<Desktop>();
            if let Some(worker) = &state.supervisor {
                worker.stop();
            }
            state.logs.record(Event::AppStopped, None);
        }
    });
    Ok(())
}
