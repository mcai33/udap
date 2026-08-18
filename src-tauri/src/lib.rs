mod error;
mod models;
mod programmer;

use models::{
    DetectRequest, DetectResult, FlashEvent, FlashRequest, FlashResult, ProbeInfo, TargetInfo,
};
use programmer::ProbeRsBackend;
use tauri::ipc::Channel;

#[tauri::command]
async fn list_probes() -> Result<Vec<ProbeInfo>, error::AppError> {
    tauri::async_runtime::spawn_blocking(ProbeRsBackend::list_probes)
        .await
        .map_err(error::AppError::from_join)?
}

#[tauri::command]
async fn list_targets() -> Result<Vec<TargetInfo>, error::AppError> {
    tauri::async_runtime::spawn_blocking(ProbeRsBackend::list_targets)
        .await
        .map_err(error::AppError::from_join)?
}

#[tauri::command]
async fn detect_target(request: DetectRequest) -> Result<DetectResult, error::AppError> {
    tauri::async_runtime::spawn_blocking(move || ProbeRsBackend::detect_target(request))
        .await
        .map_err(error::AppError::from_join)?
}

#[tauri::command]
async fn flash_firmware(
    request: FlashRequest,
    on_event: Channel<FlashEvent>,
) -> Result<FlashResult, error::AppError> {
    tauri::async_runtime::spawn_blocking(move || ProbeRsBackend::flash(request, on_event))
        .await
        .map_err(error::AppError::from_join)?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_probes,
            list_targets,
            detect_target,
            flash_firmware
        ])
        .run(tauri::generate_context!())
        .expect("failed to run uDAP");
}
