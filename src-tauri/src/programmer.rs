use std::{
    collections::HashMap,
    path::Path,
    sync::{LazyLock, Mutex},
    time::Instant,
};

use probe_rs::{
    Permissions,
    config::{Registry, TargetSelector},
    flashing::{
        BinLoader, BinOptions, DownloadOptions, ElfLoader, ElfOptions, FlashProgress, HexLoader,
        ImageLoader, ProgressEvent, ProgressOperation, Uf2Loader, download_file_with_options,
    },
    probe::{DebugProbeInfo, WireProtocol, list::Lister},
};
use tauri::ipc::Channel;

use crate::{
    error::AppError,
    models::{
        DetectRequest, DetectResult, FlashEvent, FlashRequest, FlashResult, ProbeInfo, TargetInfo,
    },
};

pub struct ProbeRsBackend;

static HARDWARE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

impl ProbeRsBackend {
    pub fn list_probes() -> Result<Vec<ProbeInfo>, AppError> {
        Ok(Lister::new()
            .list_all()
            .into_iter()
            .map(probe_info)
            .collect())
    }

    pub fn list_targets() -> Result<Vec<TargetInfo>, AppError> {
        let registry = Registry::from_builtin_families();
        let mut targets = registry
            .families()
            .iter()
            .flat_map(|family| {
                family.variants.iter().map(|chip| TargetInfo {
                    name: chip.name.clone(),
                    family: family.name.clone(),
                    architecture: chip
                        .cores
                        .first()
                        .map(|core| format!("{:?}", core.core_type))
                        .unwrap_or_else(|| "Unknown".into()),
                    aliases: chip.package_variants.clone(),
                })
            })
            .collect::<Vec<_>>();
        targets.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        targets.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
        Ok(targets)
    }

    pub fn detect_target(request: DetectRequest) -> Result<DetectResult, AppError> {
        let _hardware_guard = HARDWARE_LOCK
            .lock()
            .map_err(|_| AppError::new("HARDWARE_BUSY", "硬件服务状态异常，请重新启动应用"))?;
        let (probe, actual_speed_khz) =
            open_probe(&request.probe_id, &request.protocol, request.speed_khz)?;
        let registry = Registry::from_builtin_families();
        let session = if request.connect_under_reset {
            probe.attach_under_reset_with_registry(
                TargetSelector::Auto,
                Permissions::default(),
                &registry,
            )
        } else {
            probe.attach_with_registry(TargetSelector::Auto, Permissions::default(), &registry)
        }
        .map_err(|error| {
            AppError::new(
                "TARGET_AUTODETECT_FAILED",
                "无法自动识别目标芯片，请检查连接或手动选择型号",
            )
            .with_detail(error.to_string())
        })?;

        let target = session.target();
        Ok(DetectResult {
            target: TargetInfo {
                name: target.name.clone(),
                family: target.name.clone(),
                architecture: format!("{:?}", target.architecture()),
                aliases: Vec::new(),
            },
            actual_speed_khz,
        })
    }

    pub fn flash(
        request: FlashRequest,
        on_event: Channel<FlashEvent>,
    ) -> Result<FlashResult, AppError> {
        let _hardware_guard = HARDWARE_LOCK
            .lock()
            .map_err(|_| AppError::new("HARDWARE_BUSY", "硬件服务状态异常，请重新启动应用"))?;
        let started = Instant::now();
        send(&on_event, "connecting", 0, None, None);
        let (probe, _) = open_probe(&request.probe_id, &request.protocol, request.speed_khz)?;
        let registry = Registry::from_builtin_families();
        let target_selector = TargetSelector::Unspecified(request.target_name.clone());
        let mut session = if request.connect_under_reset {
            probe.attach_under_reset_with_registry(
                target_selector,
                Permissions::default(),
                &registry,
            )
        } else {
            probe.attach_with_registry(target_selector, Permissions::default(), &registry)
        }
        .map_err(|error| {
            AppError::new("TARGET_ATTACH_FAILED", "无法连接所选目标芯片")
                .with_detail(error.to_string())
        })?;

        let loader = firmware_loader(&request.firmware_path, request.base_address)?;
        let channel = on_event.clone();
        let mut totals = HashMap::<&'static str, u64>::new();
        let mut completed = HashMap::<&'static str, u64>::new();
        let progress = FlashProgress::new(move |event| match event {
            ProgressEvent::AddProgressBar { operation, total } => {
                let stage = operation_stage(operation);
                if let Some(total) = total {
                    totals.insert(stage, total);
                }
                completed.insert(stage, 0);
                send(&channel, stage, 0, total, None);
            }
            ProgressEvent::Started(operation) => {
                let stage = operation_stage(operation);
                send(
                    &channel,
                    stage,
                    *completed.get(stage).unwrap_or(&0),
                    totals.get(stage).copied(),
                    None,
                );
            }
            ProgressEvent::Progress {
                operation, size, ..
            } => {
                let stage = operation_stage(operation);
                let value = completed.entry(stage).or_default();
                *value += size;
                send(&channel, stage, *value, totals.get(stage).copied(), None);
            }
            ProgressEvent::DiagnosticMessage { message } => {
                send(&channel, "message", 0, None, Some(message))
            }
            ProgressEvent::Finished(operation) => {
                let stage = operation_stage(operation);
                let total = totals.get(stage).copied();
                send(
                    &channel,
                    stage,
                    total.unwrap_or(*completed.get(stage).unwrap_or(&0)),
                    total,
                    None,
                );
            }
            ProgressEvent::Failed(_) | ProgressEvent::FlashLayoutReady { .. } => {}
        });

        let mut options = DownloadOptions::default();
        options.progress = progress;
        options.verify = request.verify;
        options.do_chip_erase = request.chip_erase;
        download_file_with_options(&mut session, &request.firmware_path, loader, options).map_err(
            |error| AppError::new("FLASH_FAILED", "固件烧录失败").with_detail(error.to_string()),
        )?;

        if request.reset_after {
            send(&on_event, "resetting", 0, None, None);
            session
                .core(0)
                .and_then(|mut core| core.reset())
                .map_err(|error| {
                    AppError::new("RESET_FAILED", "固件已写入，但目标复位失败")
                        .with_detail(error.to_string())
                })?;
        }

        send(&on_event, "completed", 1, Some(1), None);
        Ok(FlashResult {
            target_name: request.target_name,
            elapsed_ms: started.elapsed().as_millis(),
        })
    }
}

fn probe_key(info: &DebugProbeInfo) -> String {
    format!(
        "{:04x}:{:04x}:{}:{}",
        info.vendor_id,
        info.product_id,
        info.serial_number.as_deref().unwrap_or(""),
        info.interface
            .map(|value| value.to_string())
            .unwrap_or_default()
    )
}

fn probe_info(info: DebugProbeInfo) -> ProbeInfo {
    ProbeInfo {
        id: probe_key(&info),
        name: info.identifier,
        vendor_id: info.vendor_id,
        product_id: info.product_id,
        serial_number: info.serial_number,
        interface: info.interface,
        is_hid_interface: info.is_hid_interface,
    }
}

fn open_probe(
    probe_id: &str,
    protocol: &str,
    speed_khz: u32,
) -> Result<(probe_rs::probe::Probe, u32), AppError> {
    let info = Lister::new()
        .list_all()
        .into_iter()
        .find(|info| probe_key(info) == probe_id)
        .ok_or_else(|| AppError::new("PROBE_NOT_FOUND", "所选调试器已断开，请重新扫描"))?;
    let mut probe = info.open().map_err(|error| {
        AppError::new(
            "PROBE_OPEN_FAILED",
            "无法打开调试器，设备可能正被其他程序占用",
        )
        .with_detail(error.to_string())
    })?;
    let wire_protocol = match protocol.to_ascii_lowercase().as_str() {
        "swd" => WireProtocol::Swd,
        "jtag" => WireProtocol::Jtag,
        _ => return Err(AppError::new("INVALID_PROTOCOL", "仅支持 SWD 或 JTAG 协议")),
    };
    probe.select_protocol(wire_protocol).map_err(|error| {
        AppError::new("PROTOCOL_FAILED", "调试器不支持所选协议").with_detail(error.to_string())
    })?;
    let actual_speed = probe.set_speed(speed_khz.max(1)).map_err(|error| {
        AppError::new("SPEED_FAILED", "无法设置调试时钟").with_detail(error.to_string())
    })?;
    Ok((probe, actual_speed))
}

fn firmware_loader(
    path: &str,
    base_address: Option<u64>,
) -> Result<Box<dyn ImageLoader>, AppError> {
    if !Path::new(path).is_file() {
        return Err(AppError::new(
            "FIRMWARE_NOT_FOUND",
            "固件文件不存在或无法访问",
        ));
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "elf" | "axf" => Ok(Box::new(ElfLoader(ElfOptions::default()))),
        "hex" | "ihex" => Ok(Box::new(HexLoader)),
        "uf2" => Ok(Box::new(Uf2Loader)),
        "bin" => base_address
            .map(|address| {
                Box::new(BinLoader(BinOptions {
                    base_address: Some(address),
                    skip: 0,
                })) as Box<dyn ImageLoader>
            })
            .ok_or_else(|| AppError::new("BASE_ADDRESS_REQUIRED", "BIN 文件必须指定烧录基地址")),
        _ => Err(AppError::new(
            "FIRMWARE_FORMAT_UNSUPPORTED",
            "固件格式不受支持，请选择 ELF、HEX、BIN 或 UF2 文件",
        )),
    }
}

fn operation_stage(operation: ProgressOperation) -> &'static str {
    match operation {
        ProgressOperation::Erase => "erasing",
        ProgressOperation::Fill => "filling",
        ProgressOperation::Program => "programming",
        ProgressOperation::Verify => "verifying",
    }
}

fn send(
    channel: &Channel<FlashEvent>,
    stage: &'static str,
    completed: u64,
    total: Option<u64>,
    message: Option<String>,
) {
    let _ = channel.send(FlashEvent {
        stage,
        completed,
        total,
        message,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_target_list_is_available_and_sorted() {
        let targets = ProbeRsBackend::list_targets().unwrap();
        assert!(targets.len() > 100);
        assert!(
            targets
                .windows(2)
                .all(|pair| pair[0].name.to_ascii_lowercase() <= pair[1].name.to_ascii_lowercase())
        );
    }
}
