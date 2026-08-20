use std::{
    collections::HashMap,
    env,
    io::{self, Write},
    path::Path,
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

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
struct ConnectionOptions {
    probe_number: usize,
    protocol: WireProtocol,
    speed_khz: u32,
    connect_under_reset: bool,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            probe_number: 1,
            protocol: WireProtocol::Swd,
            speed_khz: 4_000,
            connect_under_reset: false,
        }
    }
}

struct FlashOptions {
    connection: ConnectionOptions,
    firmware: String,
    target: Option<String>,
    base_address: Option<u64>,
    verify: bool,
    chip_erase: bool,
    reset_after: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("\n错误：{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return interactive();
    }

    match args[0].as_str() {
        "-h" | "--help" | "help" => {
            print_help();
            Ok(())
        }
        "-V" | "--version" | "version" => {
            println!("uDAP Programmer CLI {VERSION}");
            Ok(())
        }
        "probes" => list_probes(),
        "targets" => list_targets(args.get(1).map(String::as_str)),
        "detect" => {
            let options = parse_connection_options(&args[1..])?;
            let (target, speed) = detect_target(&options)?;
            println!("目标芯片：{target}");
            println!("实际时钟：{speed} kHz");
            Ok(())
        }
        "flash" => {
            let options = parse_flash_options(&args[1..])?;
            flash(options)
        }
        unknown => Err(format!("未知命令：{unknown}。请使用 --help 查看帮助。")),
    }
}

fn print_help() {
    println!(
        "uDAP Programmer CLI {VERSION}\n\
         \n用法：\n\
         \x20 udap-cli.exe                     进入交互模式\n\
         \x20 udap-cli.exe probes              列出调试器\n\
         \x20 udap-cli.exe detect [选项]       自动识别目标\n\
         \x20 udap-cli.exe targets [关键字]    搜索支持的目标\n\
         \x20 udap-cli.exe flash <文件> [选项] 烧录固件\n\
         \n连接选项：\n\
         \x20 --probe <序号>                   调试器序号，默认 1\n\
         \x20 --protocol <swd|jtag>             接口协议，默认 swd\n\
         \x20 --speed <kHz>                     调试时钟，默认 4000\n\
         \x20 --connect-under-reset             复位下连接\n\
         \n烧录选项：\n\
         \x20 --target <型号>                   手动指定目标芯片\n\
         \x20 --base-address <地址>             BIN 烧录地址，例如 0x08000000\n\
         \x20 --chip-erase                      执行全片擦除\n\
         \x20 --no-verify                       不校验\n\
         \x20 --no-reset                        烧录后不复位"
    );
}

fn interactive() -> Result<(), String> {
    println!("uDAP Programmer CLI {VERSION}");
    println!("通用 CMSIS-DAP 烧录器（Windows 7 x64 兼容版）\n");

    let probes = available_probes()?;
    print_probe_list(&probes);
    let probe_number = if probes.len() == 1 {
        println!("自动选择调试器 1。\n");
        1
    } else {
        prompt_usize("请选择调试器", 1, probes.len())?
    };

    let protocol_text = prompt_default("接口协议 SWD/JTAG", "SWD")?;
    let protocol = parse_protocol(&protocol_text)?;
    let speed_khz = prompt_default("调试时钟 (kHz)", "4000")?
        .parse::<u32>()
        .map_err(|_| "调试时钟必须是正整数".to_string())?
        .max(1);
    let connect_under_reset = prompt_yes_no("是否复位下连接", false)?;
    let connection = ConnectionOptions {
        probe_number,
        protocol,
        speed_khz,
        connect_under_reset,
    };

    println!("\n正在自动识别目标……");
    let target = match detect_target(&connection) {
        Ok((target, actual_speed)) => {
            println!("识别成功：{target}（{actual_speed} kHz）");
            target
        }
        Err(error) => {
            println!("自动识别失败：{error}");
            choose_target_interactively()?
        }
    };

    let firmware = strip_path_quotes(&prompt_required("请输入固件路径")?);
    if !Path::new(&firmware).is_file() {
        return Err(format!("固件文件不存在：{firmware}"));
    }
    let base_address = if is_bin(&firmware) {
        Some(parse_address(&prompt_default(
            "BIN 烧录基地址",
            "0x08000000",
        )?)?)
    } else {
        None
    };
    let verify = prompt_yes_no("烧录后校验", true)?;
    let chip_erase = prompt_yes_no("执行全片擦除", false)?;
    let reset_after = prompt_yes_no("烧录后复位运行", true)?;

    println!("\n即将烧录：");
    println!("  目标：{target}");
    println!("  文件：{firmware}");
    if !prompt_yes_no("确认开始", true)? {
        println!("已取消。 ");
        return Ok(());
    }

    flash(FlashOptions {
        connection,
        firmware,
        target: Some(target),
        base_address,
        verify,
        chip_erase,
        reset_after,
    })
}

fn list_probes() -> Result<(), String> {
    let probes = available_probes()?;
    print_probe_list(&probes);
    Ok(())
}

fn available_probes() -> Result<Vec<DebugProbeInfo>, String> {
    let probes = Lister::new().list_all();
    if probes.is_empty() {
        Err("未发现 CMSIS-DAP 调试器，请检查 USB 连接和驱动".into())
    } else {
        Ok(probes)
    }
}

fn print_probe_list(probes: &[DebugProbeInfo]) {
    println!("发现 {} 个调试器：", probes.len());
    for (index, probe) in probes.iter().enumerate() {
        println!(
            "  {}. {} [{:04x}:{:04x}]{}",
            index + 1,
            probe.identifier,
            probe.vendor_id,
            probe.product_id,
            probe
                .serial_number
                .as_deref()
                .map(|serial| format!(" SN={serial}"))
                .unwrap_or_default()
        );
    }
}

fn list_targets(query: Option<&str>) -> Result<(), String> {
    let targets = matching_targets(query.unwrap_or(""));
    if targets.is_empty() {
        return Err("没有找到匹配的目标芯片".into());
    }
    for target in &targets {
        println!("{target}");
    }
    println!("\n共 {} 个目标", targets.len());
    Ok(())
}

fn matching_targets(query: &str) -> Vec<String> {
    let query = query.trim().to_ascii_lowercase();
    let registry = Registry::from_builtin_families();
    let mut targets = registry
        .families()
        .iter()
        .flat_map(|family| family.variants.iter().map(|chip| chip.name.clone()))
        .filter(|name| query.is_empty() || name.to_ascii_lowercase().contains(&query))
        .collect::<Vec<_>>();
    targets.sort_by_key(|name| name.to_ascii_lowercase());
    targets.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    targets
}

fn choose_target_interactively() -> Result<String, String> {
    loop {
        let query = prompt_required("请输入目标型号关键字")?;
        let targets = matching_targets(&query);
        if targets.is_empty() {
            println!("没有匹配结果，请换一个关键字。 ");
            continue;
        }
        let shown = targets.len().min(50);
        println!(
            "匹配结果{}：",
            if targets.len() > shown {
                "（仅显示前 50 个）"
            } else {
                ""
            }
        );
        for (index, target) in targets.iter().take(shown).enumerate() {
            println!("  {}. {target}", index + 1);
        }
        let selection = prompt_usize("请选择目标", 1, shown)?;
        return Ok(targets[selection - 1].clone());
    }
}

fn detect_target(options: &ConnectionOptions) -> Result<(String, u32), String> {
    let (probe, actual_speed) = open_probe(options)?;
    let registry = Registry::from_builtin_families();
    let session = if options.connect_under_reset {
        probe.attach_under_reset_with_registry(
            TargetSelector::Auto,
            Permissions::default(),
            &registry,
        )
    } else {
        probe.attach_with_registry(TargetSelector::Auto, Permissions::default(), &registry)
    }
    .map_err(|error| format!("无法自动识别目标芯片：{error}"))?;
    Ok((session.target().name.clone(), actual_speed))
}

fn open_probe(options: &ConnectionOptions) -> Result<(probe_rs::probe::Probe, u32), String> {
    let probes = available_probes()?;
    if options.probe_number == 0 || options.probe_number > probes.len() {
        return Err(format!(
            "调试器序号 {} 无效，当前共发现 {} 个",
            options.probe_number,
            probes.len()
        ));
    }
    let info = probes
        .into_iter()
        .nth(options.probe_number - 1)
        .ok_or_else(|| "调试器已断开".to_string())?;
    let mut probe = info
        .open()
        .map_err(|error| format!("无法打开调试器：{error}"))?;
    probe
        .select_protocol(options.protocol)
        .map_err(|error| format!("无法选择调试协议：{error}"))?;
    let actual_speed = probe
        .set_speed(options.speed_khz.max(1))
        .map_err(|error| format!("无法设置调试时钟：{error}"))?;
    Ok((probe, actual_speed))
}

fn flash(mut options: FlashOptions) -> Result<(), String> {
    if !Path::new(&options.firmware).is_file() {
        return Err(format!("固件文件不存在：{}", options.firmware));
    }
    if is_bin(&options.firmware) && options.base_address.is_none() {
        return Err("BIN 文件必须通过 --base-address 指定烧录地址".into());
    }
    let target = match options.target.take() {
        Some(target) => target,
        None => {
            println!("正在自动识别目标……");
            let (target, speed) = detect_target(&options.connection)?;
            println!("识别成功：{target}（{speed} kHz）");
            target
        }
    };

    println!("正在连接 {target}……");
    let (probe, _) = open_probe(&options.connection)?;
    let registry = Registry::from_builtin_families();
    let selector = TargetSelector::Unspecified(target.clone());
    let mut session = if options.connection.connect_under_reset {
        probe.attach_under_reset_with_registry(selector, Permissions::default(), &registry)
    } else {
        probe.attach_with_registry(selector, Permissions::default(), &registry)
    }
    .map_err(|error| format!("无法连接目标芯片 {target}：{error}"))?;

    let loader = firmware_loader(&options.firmware, options.base_address)?;
    let started = Instant::now();
    let mut totals = HashMap::<&'static str, u64>::new();
    let mut completed = HashMap::<&'static str, u64>::new();
    let mut displayed_percent = HashMap::<&'static str, u64>::new();
    let progress = FlashProgress::new(move |event| match event {
        ProgressEvent::AddProgressBar { operation, total } => {
            let stage = operation_stage(operation);
            if let Some(total) = total {
                totals.insert(stage, total);
            }
            completed.insert(stage, 0);
            displayed_percent.insert(stage, 0);
            print_progress(stage, 0, total);
        }
        ProgressEvent::Started(operation) => {
            let stage = operation_stage(operation);
            print_progress(
                stage,
                *completed.get(stage).unwrap_or(&0),
                totals.get(stage).copied(),
            );
        }
        ProgressEvent::Progress {
            operation, size, ..
        } => {
            let stage = operation_stage(operation);
            let value = completed.entry(stage).or_default();
            *value += size;
            let total = totals.get(stage).copied();
            let percent = progress_percent(*value, total);
            if displayed_percent.get(stage).copied() != percent {
                print_progress(stage, *value, total);
                if let Some(percent) = percent {
                    displayed_percent.insert(stage, percent);
                }
            }
        }
        ProgressEvent::DiagnosticMessage { message } => println!("  {message}"),
        ProgressEvent::Finished(operation) => {
            let stage = operation_stage(operation);
            let total = totals.get(stage).copied();
            let value = total.unwrap_or(*completed.get(stage).unwrap_or(&0));
            if displayed_percent.get(stage).copied() != Some(100) {
                print_progress(stage, value, total);
                displayed_percent.insert(stage, 100);
            }
        }
        ProgressEvent::Failed(operation) => eprintln!("{}失败", operation_stage(operation)),
        ProgressEvent::FlashLayoutReady { .. } => {}
    });

    let mut download = DownloadOptions::default();
    download.progress = progress;
    download.verify = options.verify;
    download.do_chip_erase = options.chip_erase;
    download_file_with_options(&mut session, &options.firmware, loader, download)
        .map_err(|error| format!("固件烧录失败：{error}"))?;

    if options.reset_after {
        println!("正在复位目标……");
        session
            .core(0)
            .and_then(|mut core| core.reset())
            .map_err(|error| format!("固件已写入，但目标复位失败：{error}"))?;
    }
    println!("烧录成功，用时 {:.2} 秒", started.elapsed().as_secs_f64());
    Ok(())
}

fn firmware_loader(path: &str, base_address: Option<u64>) -> Result<Box<dyn ImageLoader>, String> {
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
            .ok_or_else(|| "BIN 文件必须指定烧录基地址".to_string()),
        _ => Err("不支持的固件格式，仅支持 ELF、AXF、HEX、BIN 和 UF2".into()),
    }
}

fn parse_connection_options(args: &[String]) -> Result<ConnectionOptions, String> {
    let mut options = ConnectionOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--probe" => {
                options.probe_number = next_value(args, &mut index, "--probe")?
                    .parse()
                    .map_err(|_| "--probe 必须是数字".to_string())?;
            }
            "--protocol" => {
                options.protocol = parse_protocol(next_value(args, &mut index, "--protocol")?)?;
            }
            "--speed" => {
                options.speed_khz = next_value(args, &mut index, "--speed")?
                    .parse::<u32>()
                    .map_err(|_| "--speed 必须是正整数".to_string())?
                    .max(1);
            }
            "--connect-under-reset" => options.connect_under_reset = true,
            value => return Err(format!("未知选项：{value}")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_flash_options(args: &[String]) -> Result<FlashOptions, String> {
    let firmware = args
        .first()
        .filter(|value| !value.starts_with('-'))
        .map(|value| strip_path_quotes(value))
        .ok_or_else(|| "flash 命令需要固件文件路径".to_string())?;
    let mut connection = ConnectionOptions::default();
    let mut target = None;
    let mut base_address = None;
    let mut verify = true;
    let mut chip_erase = false;
    let mut reset_after = true;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--probe" => {
                connection.probe_number = next_value(args, &mut index, "--probe")?
                    .parse()
                    .map_err(|_| "--probe 必须是数字".to_string())?;
            }
            "--protocol" => {
                connection.protocol = parse_protocol(next_value(args, &mut index, "--protocol")?)?;
            }
            "--speed" => {
                connection.speed_khz = next_value(args, &mut index, "--speed")?
                    .parse::<u32>()
                    .map_err(|_| "--speed 必须是正整数".to_string())?
                    .max(1);
            }
            "--connect-under-reset" => connection.connect_under_reset = true,
            "--target" => target = Some(next_value(args, &mut index, "--target")?.to_string()),
            "--base-address" => {
                base_address = Some(parse_address(next_value(
                    args,
                    &mut index,
                    "--base-address",
                )?)?)
            }
            "--chip-erase" => chip_erase = true,
            "--no-verify" => verify = false,
            "--no-reset" => reset_after = false,
            value => return Err(format!("未知选项：{value}")),
        }
        index += 1;
    }
    Ok(FlashOptions {
        connection,
        firmware,
        target,
        base_address,
        verify,
        chip_erase,
        reset_after,
    })
}

fn next_value<'a>(args: &'a [String], index: &mut usize, name: &str) -> Result<&'a str, String> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} 缺少参数"))
}

fn parse_protocol(value: &str) -> Result<WireProtocol, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "swd" => Ok(WireProtocol::Swd),
        "jtag" => Ok(WireProtocol::Jtag),
        _ => Err("协议仅支持 SWD 或 JTAG".into()),
    }
}

fn parse_address(value: &str) -> Result<u64, String> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|_| format!("无效地址：{value}"))
    } else {
        value
            .parse::<u64>()
            .map_err(|_| format!("无效地址：{value}"))
    }
}

fn is_bin(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bin"))
}

fn strip_path_quotes(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn operation_stage(operation: ProgressOperation) -> &'static str {
    match operation {
        ProgressOperation::Erase => "擦除",
        ProgressOperation::Fill => "准备",
        ProgressOperation::Program => "写入",
        ProgressOperation::Verify => "校验",
    }
}

fn print_progress(stage: &str, completed: u64, total: Option<u64>) {
    if let Some(percent) = progress_percent(completed, total) {
        println!("{stage:<4} {percent:>3}%");
    } else {
        println!("{stage}……");
    }
}

fn progress_percent(completed: u64, total: Option<u64>) -> Option<u64> {
    total
        .filter(|total| *total > 0)
        .map(|total| completed.saturating_mul(100).saturating_div(total).min(100))
}

fn prompt(label: &str) -> Result<String, String> {
    print!("{label}：");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|error| format!("读取输入失败：{error}"))?;
    Ok(value.trim().to_string())
}

fn prompt_required(label: &str) -> Result<String, String> {
    loop {
        let value = prompt(label)?;
        if !value.is_empty() {
            return Ok(value);
        }
        println!("该项不能为空。 ");
    }
}

fn prompt_default(label: &str, default: &str) -> Result<String, String> {
    let value = prompt(&format!("{label} [{default}]"))?;
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value
    })
}

fn prompt_yes_no(label: &str, default: bool) -> Result<bool, String> {
    let suffix = if default { "Y/n" } else { "y/N" };
    loop {
        let value = prompt(&format!("{label} [{suffix}]"))?;
        match value.to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" | "是" => return Ok(true),
            "n" | "no" | "否" => return Ok(false),
            _ => println!("请输入 y 或 n。 "),
        }
    }
}

fn prompt_usize(label: &str, min: usize, max: usize) -> Result<usize, String> {
    loop {
        let value = prompt_default(label, &min.to_string())?;
        if let Ok(number) = value.parse::<usize>()
            && (min..=max).contains(&number)
        {
            return Ok(number);
        }
        println!("请输入 {min} 到 {max} 之间的数字。 ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_decimal_addresses() {
        assert_eq!(parse_address("0x08000000").unwrap(), 0x0800_0000);
        assert_eq!(parse_address("134217728").unwrap(), 0x0800_0000);
        assert!(parse_address("0xXYZ").is_err());
    }

    #[test]
    fn target_search_is_available_and_sorted() {
        let targets = matching_targets("STM32F103");
        assert!(!targets.is_empty());
        assert!(targets.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn calculates_bounded_progress() {
        assert_eq!(progress_percent(5, Some(10)), Some(50));
        assert_eq!(progress_percent(20, Some(10)), Some(100));
        assert_eq!(progress_percent(1, None), None);
    }
}
