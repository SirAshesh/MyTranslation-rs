use crate::model::{
    CpuInfo, EngineBackend, GpuInfo, HardwareResponse, LocalModelFamily, LocalModelOption,
    LocalModelSpec, LOCAL_MODEL_CATALOG,
};
use std::process::Command;
use std::sync::RwLock;
use tracing::warn;

const MIN_GPU_VRAM_MB: u64 = 2048;
const IGNORED_ADAPTER_KEYWORDS: [&str; 4] = ["virtual", "basic", "remote", "parsec"];

#[cfg(windows)]
const HARDWARE_PROBE_SCRIPT: &str = r#"
$classKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*'
Get-ItemProperty $classKey -ErrorAction SilentlyContinue | Where-Object { $_.DriverDesc } | ForEach-Object {
    $memory = $_.'HardwareInformation.qwMemorySize'
    if ($memory -is [byte[]]) { $memory = [BitConverter]::ToUInt64($memory, 0) }
    Write-Output ('GPU:::' + $_.DriverDesc + ':::' + $memory)
}
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
if ($cpu) { Write-Output ('CPU:::' + $cpu.Name.Trim() + ':::' + $cpu.NumberOfCores + ':::' + $cpu.NumberOfLogicalProcessors) }
$ram = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
if ($ram) { Write-Output ('RAM:::' + $ram) }
"#;

static CACHED_HARDWARE: RwLock<Option<HardwareResponse>> = RwLock::new(None);

struct ProbeResult {
    gpus: Vec<GpuInfo>,
    cpu: CpuInfo,
    ram_bytes: u64,
}

pub fn get_hardware_info(force_refresh: bool) -> HardwareResponse {
    if !force_refresh {
        if let Some(cached) = CACHED_HARDWARE.read().ok().and_then(|guard| guard.clone()) {
            return cached;
        }
    }

    let detected = build_hardware_response(probe_hardware());
    if let Ok(mut guard) = CACHED_HARDWARE.write() {
        *guard = Some(detected.clone());
    }
    detected
}

#[cfg(windows)]
fn probe_hardware() -> ProbeResult {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", HARDWARE_PROBE_SCRIPT])
        .output();

    match output {
        Ok(out) if out.status.success() => parse_probe_output(&String::from_utf8_lossy(&out.stdout)),
        Ok(out) => {
            warn!("Hardware probe exited with status {}", out.status);
            parse_probe_output("")
        }
        Err(err) => {
            warn!("Hardware probe could not start: {}", err);
            parse_probe_output("")
        }
    }
}

#[cfg(not(windows))]
fn probe_hardware() -> ProbeResult {
    let mut gpus = Vec::new();
    let mut cpu = CpuInfo {
        name: "Unknown CPU".to_string(),
        cores: 0,
        threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
    };
    let mut ram_bytes = 0u64;

    if let Ok(info) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in info.lines() {
            if line.starts_with("model name") && cpu.name == "Unknown CPU" {
                if let Some((_, val)) = line.split_once(':') {
                    cpu.name = val.trim().to_string();
                }
            } else if line.starts_with("cpu cores") && cpu.cores == 0 {
                if let Some((_, val)) = line.split_once(':') {
                    cpu.cores = val.trim().parse().unwrap_or(0);
                }
            }
        }
    }

    if let Ok(mem) = std::fs::read_to_string("/proc/meminfo") {
        for line in mem.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        ram_bytes = kb * 1024;
                        break;
                    }
                }
            }
        }
    }

    if let Ok(out) = Command::new("nvidia-smi")
        .args(["--query-gpu=gpu_name,memory.total", "--format=csv,noheader,nounits"])
        .output()
    {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Some((name, vram_str)) = line.split_once(',') {
                    let vram_mb = vram_str.trim().parse::<u64>().unwrap_or(0);
                    if let Some(gpu) = build_gpu_info(name, &(vram_mb * 1024 * 1024).to_string()) {
                        gpus.push(gpu);
                    }
                }
            }
        }
    }

    if gpus.is_empty() {
        if let Ok(out) = Command::new("lspci").output() {
            if out.status.success() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let lower = line.to_lowercase();
                    if lower.contains("vga compatible controller") || lower.contains("3d controller") {
                        if let Some((_, desc)) = line.split_once(':') {
                            if let Some(gpu) = build_gpu_info(desc, "0") {
                                gpus.push(gpu);
                            }
                        }
                    }
                }
            }
        }
    }

    ProbeResult { gpus, cpu, ram_bytes }
}

fn parse_probe_output(text: &str) -> ProbeResult {
    let mut gpus = Vec::new();
    let mut cpu = CpuInfo {
        name: "Unknown CPU".to_string(),
        cores: 0,
        threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
    };
    let mut ram_bytes = 0u64;

    for line in text.lines() {
        let parts: Vec<&str> = line.trim().split(":::").collect();
        match parts.as_slice() {
            ["GPU", name, vram_bytes] => gpus.extend(build_gpu_info(name, vram_bytes)),
            ["CPU", name, cores, threads] => {
                cpu = CpuInfo {
                    name: name.trim().to_string(),
                    cores: cores.trim().parse().unwrap_or(0),
                    threads: threads.trim().parse().unwrap_or(cpu.threads),
                }
            }
            ["RAM", bytes] => ram_bytes = bytes.trim().parse().unwrap_or(0),
            _ => {}
        }
    }

    ProbeResult { gpus, cpu, ram_bytes }
}

fn build_gpu_info(name: &str, vram_bytes: &str) -> Option<GpuInfo> {
    let trimmed_name = name.trim();
    let lowered = trimmed_name.to_lowercase();
    if trimmed_name.is_empty() || IGNORED_ADAPTER_KEYWORDS.iter().any(|k| lowered.contains(k)) {
        return None;
    }

    Some(GpuInfo {
        name: trimmed_name.to_string(),
        vendor: detect_gpu_vendor(&lowered).to_string(),
        vram_mb: vram_bytes.trim().parse::<u64>().unwrap_or(0) / (1024 * 1024),
    })
}

fn detect_gpu_vendor(lowered_name: &str) -> &'static str {
    let contains_any = |keywords: &[&str]| keywords.iter().any(|k| lowered_name.contains(k));
    if contains_any(&["nvidia", "geforce", "rtx", "gtx", "quadro"]) {
        "NVIDIA"
    } else if contains_any(&["amd", "radeon"]) {
        "AMD"
    } else if contains_any(&["intel", "arc"]) {
        "Intel"
    } else {
        "Other"
    }
}

fn select_backend(gpu: Option<&GpuInfo>) -> EngineBackend {
    match gpu {
        Some(gpu) if gpu.vram_mb >= MIN_GPU_VRAM_MB && gpu.vendor == "NVIDIA" => EngineBackend::Cuda,
        Some(gpu) if gpu.vram_mb >= MIN_GPU_VRAM_MB => EngineBackend::Vulkan,
        _ => EngineBackend::Cpu,
    }
}

fn fits_hardware(
    spec: &LocalModelSpec,
    backend: EngineBackend,
    vram_mb: u64,
    ram_gb: f64,
) -> bool {
    if backend.offloads_to_gpu() {
        return spec.min_vram_mb <= vram_mb;
    }
    spec.min_ram_gb <= ram_gb
}

fn pick_recommended_quant(
    backend: EngineBackend,
    vram_mb: u64,
    ram_gb: f64,
) -> Option<&'static str> {
    LOCAL_MODEL_CATALOG
        .iter()
        .rev()
        .filter(|spec| spec.family == LocalModelFamily::HyMt15)
        .find(|spec| fits_hardware(spec, backend, vram_mb, ram_gb))
        .or_else(|| {
            LOCAL_MODEL_CATALOG
                .iter()
                .rev()
                .find(|spec| fits_hardware(spec, backend, vram_mb, ram_gb))
        })
        .map(|spec| spec.quant)
}

fn build_model_options(
    backend: EngineBackend,
    vram_mb: u64,
    ram_gb: f64,
) -> Vec<LocalModelOption> {
    let recommended_quant = pick_recommended_quant(backend, vram_mb, ram_gb);

    LOCAL_MODEL_CATALOG
        .iter()
        .map(|spec| LocalModelOption {
            quant: spec.quant.to_string(),
            family: spec.family.key().to_string(),
            name: spec.label(),
            size_mb: spec.size_bytes / (1024 * 1024),
            recommended: Some(spec.quant) == recommended_quant,
        })
        .collect()
}

pub fn recommended_local_model() -> Option<String> {
    get_hardware_info(false)
        .models
        .into_iter()
        .find(|model| model.recommended)
        .map(|model| model.quant)
}

fn build_hardware_response(probe: ProbeResult) -> HardwareResponse {
    let gpu = probe.gpus.into_iter().max_by_key(|gpu| gpu.vram_mb);
    let backend = select_backend(gpu.as_ref());
    let vram_mb = gpu.as_ref().map_or(0, |gpu| gpu.vram_mb);
    let ram_gb = (probe.ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0) * 10.0).round() / 10.0;

    HardwareResponse {
        success: true,
        cpu: probe.cpu,
        gpu,
        ram_gb,
        backend,
        models: build_model_options(backend, vram_mb, ram_gb),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(name: &str, vram_mb: u64) -> GpuInfo {
        build_gpu_info(name, &(vram_mb * 1024 * 1024).to_string()).expect("gpu should be accepted")
    }

    fn recommended_quant(options: &[LocalModelOption]) -> Option<&str> {
        options.iter().find(|o| o.recommended).map(|o| o.quant.as_str())
    }

    #[test]
    fn parses_probe_output_and_skips_virtual_adapters() {
        let text = "GPU:::Parsec Virtual Display Adapter:::\nGPU:::AMD Radeon RX 6600:::8573157376\nCPU:::AMD Ryzen 5 5500GT:::6:::12\nRAM:::16530931712";
        let probe = parse_probe_output(text);
        assert_eq!(probe.gpus.len(), 1);
        assert_eq!(probe.gpus[0].vendor, "AMD");
        assert_eq!(probe.gpus[0].vram_mb, 8176);
        assert_eq!(probe.cpu.cores, 6);
        assert_eq!(probe.ram_bytes, 16530931712);
    }

    #[test]
    fn selects_backend_by_vendor_and_vram() {
        assert_eq!(select_backend(Some(&gpu("NVIDIA GeForce RTX 3060", 12288))), EngineBackend::Cuda);
        assert_eq!(select_backend(Some(&gpu("AMD Radeon RX 6600", 8176))), EngineBackend::Vulkan);
        assert_eq!(select_backend(Some(&gpu("AMD Radeon Graphics", 512))), EngineBackend::Cpu);
        assert_eq!(select_backend(None), EngineBackend::Cpu);
    }

    #[test]
    fn recommends_quant_that_fits_available_memory() {
        assert_eq!(
            recommended_quant(&build_model_options(EngineBackend::Vulkan, 8176, 16.0)),
            Some("HY-MT1.5-1.8B-Q8_0")
        );
        assert_eq!(
            recommended_quant(&build_model_options(EngineBackend::Vulkan, 3072, 8.0)),
            Some("HY-MT1.5-1.8B-Q6_K")
        );
        assert_eq!(
            recommended_quant(&build_model_options(EngineBackend::Cpu, 0, 5.0)),
            Some("HY-MT1.5-1.8B-Q4_K_M")
        );
        assert_eq!(
            recommended_quant(&build_model_options(EngineBackend::Cpu, 0, 12.0)),
            Some("HY-MT1.5-1.8B-Q6_K")
        );
        assert_eq!(
            recommended_quant(&build_model_options(EngineBackend::Cpu, 0, 16.0)),
            Some("HY-MT1.5-1.8B-Q8_0")
        );
    }
}
