use serde::{Deserialize, Serialize};

pub const LOCAL_SERVER_PORT: u16 = 51057;
pub const DEFAULT_LOCAL_QUANT: &str = "HY-MT1.5-1.8B-Q4_K_M";

pub fn local_server_url() -> String {
    format!("http://127.0.0.1:{}/v1", LOCAL_SERVER_PORT)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalModelFamily {
    HyMt15,
    Lfm3,
}

impl LocalModelFamily {
    pub fn key(self) -> &'static str {
        match self {
            LocalModelFamily::HyMt15 => "hy-mt1.5",
            LocalModelFamily::Lfm3 => "lfm3",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LocalModelSpec {
    pub quant: &'static str,
    pub family: LocalModelFamily,
    pub display_name: &'static str,
    pub quant_label: &'static str,
    pub repository: &'static str,
    pub file_name: &'static str,
    pub size_bytes: u64,
    pub min_vram_mb: u64,
    pub min_ram_gb: f64,
}

impl LocalModelSpec {
    pub fn file_name(&self) -> &'static str {
        self.file_name
    }

    pub fn label(&self) -> String {
        format!("{} ({})", self.display_name, self.quant_label)
    }

    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.repository,
            self.file_name
        )
    }
}

pub const LOCAL_MODEL_CATALOG: [LocalModelSpec; 6] = [
    LocalModelSpec {
        quant: "HY-MT1.5-1.8B-Q4_K_M",
        family: LocalModelFamily::HyMt15,
        display_name: "Tencent HY-MT1.5 1.8B",
        quant_label: "Q4_K_M",
        repository: "tencent/HY-MT1.5-1.8B-GGUF",
        file_name: "HY-MT1.5-1.8B-Q4_K_M.gguf",
        size_bytes: 1_133_080_512,
        min_vram_mb: 0,
        min_ram_gb: 0.0,
    },
    LocalModelSpec {
        quant: "HY-MT1.5-1.8B-Q6_K",
        family: LocalModelFamily::HyMt15,
        display_name: "Tencent HY-MT1.5 1.8B",
        quant_label: "Q6_K",
        repository: "tencent/HY-MT1.5-1.8B-GGUF",
        file_name: "HY-MT1.5-1.8B-Q6_K.gguf",
        size_bytes: 1_474_785_216,
        min_vram_mb: 2048,
        min_ram_gb: 10.0,
    },
    LocalModelSpec {
        quant: "HY-MT1.5-1.8B-Q8_0",
        family: LocalModelFamily::HyMt15,
        display_name: "Tencent HY-MT1.5 1.8B",
        quant_label: "Q8_0",
        repository: "tencent/HY-MT1.5-1.8B-GGUF",
        file_name: "HY-MT1.5-1.8B-Q8_0.gguf",
        size_bytes: 1_908_528_288,
        min_vram_mb: 4096,
        min_ram_gb: 14.0,
    },
    LocalModelSpec {
        quant: "Q4_K_M",
        family: LocalModelFamily::Lfm3,
        display_name: "LiquidAI LFM3 1.7B",
        quant_label: "Q4_K_M",
        repository: "LiquidAI/LFM3-1.7B-GGUF",
        file_name: "LFM3-1.7B-Q4_K_M.gguf",
        size_bytes: 1_099_627_808,
        min_vram_mb: 0,
        min_ram_gb: 0.0,
    },
    LocalModelSpec {
        quant: "Q6_K",
        family: LocalModelFamily::Lfm3,
        display_name: "LiquidAI LFM3 1.7B",
        quant_label: "Q6_K",
        repository: "LiquidAI/LFM3-1.7B-GGUF",
        file_name: "LFM3-1.7B-Q6_K.gguf",
        size_bytes: 1_427_638_304,
        min_vram_mb: 2048,
        min_ram_gb: 10.0,
    },
    LocalModelSpec {
        quant: "Q8_0",
        family: LocalModelFamily::Lfm3,
        display_name: "LiquidAI LFM3 1.7B",
        quant_label: "Q8_0",
        repository: "LiquidAI/LFM3-1.7B-GGUF",
        file_name: "LFM3-1.7B-Q8_0.gguf",
        size_bytes: 1_846_238_496,
        min_vram_mb: 4096,
        min_ram_gb: 14.0,
    },
];

pub fn find_local_model(quant: &str) -> Option<&'static LocalModelSpec> {
    let trimmed = quant.trim();
    LOCAL_MODEL_CATALOG
        .iter()
        .find(|spec| spec.quant.eq_ignore_ascii_case(trimmed))
}

pub fn is_hymt_model(model_id: &str) -> bool {
    if let Some(spec) = find_local_model(model_id) {
        return spec.family == LocalModelFamily::HyMt15;
    }
    model_id.trim().to_ascii_lowercase().starts_with("hy-mt")
}

pub fn format_local_model_label(model_id: &str) -> String {
    match find_local_model(model_id) {
        Some(spec) => spec.label(),
        None => format!("Local ({})", model_id.trim()),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EngineBackend {
    Cuda,
    Vulkan,
    Cpu,
}

impl EngineBackend {
    pub fn matches_asset(self, name: &str) -> bool {
        if cfg!(windows) {
            match self {
                EngineBackend::Cuda => {
                    (name.contains("win-cuda") && name.ends_with("-x64.zip"))
                        || name.ends_with("bin-win-cuda-12.4-x64.zip")
                }
                EngineBackend::Vulkan => name.ends_with("bin-win-vulkan-x64.zip"),
                EngineBackend::Cpu => name.ends_with("bin-win-cpu-x64.zip"),
            }
        } else {
            match self {
                EngineBackend::Cuda => {
                    (name.contains("ubuntu-cuda") && name.ends_with("-x64.tar.gz") && !name.starts_with("cudart-"))
                        || name.ends_with("bin-ubuntu-cuda-12.8-x64.tar.gz")
                }
                EngineBackend::Vulkan => name.ends_with("bin-ubuntu-vulkan-x64.tar.gz"),
                EngineBackend::Cpu => name.ends_with("bin-ubuntu-x64.tar.gz"),
            }
        }
    }

    pub fn directory_name(self) -> &'static str {
        match self {
            EngineBackend::Cuda => "cuda",
            EngineBackend::Vulkan => "vulkan",
            EngineBackend::Cpu => "cpu",
        }
    }

    pub fn offloads_to_gpu(self) -> bool {
        self != EngineBackend::Cpu
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuInfo {
    pub name: String,
    pub cores: usize,
    pub threads: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub vram_mb: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalModelOption {
    pub quant: String,
    pub family: String,
    pub name: String,
    pub size_mb: u64,
    pub recommended: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HardwareResponse {
    pub success: bool,
    pub cpu: CpuInfo,
    pub gpu: Option<GpuInfo>,
    pub ram_gb: f64,
    pub backend: EngineBackend,
    pub models: Vec<LocalModelOption>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalPhase {
    Stopped,
    InstallingEngine,
    DownloadingModel,
    Starting,
    Running,
    Failed,
}

impl LocalPhase {
    pub fn is_busy(self) -> bool {
        matches!(
            self,
            LocalPhase::InstallingEngine
                | LocalPhase::DownloadingModel
                | LocalPhase::Starting
                | LocalPhase::Running
        )
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct LocalRuntimeStatus {
    pub phase: LocalPhase,
    pub message: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub active_quant: Option<String>,
}

impl LocalRuntimeStatus {
    pub fn stopped() -> Self {
        Self {
            phase: LocalPhase::Stopped,
            message: "Stopped".to_string(),
            downloaded_bytes: 0,
            total_bytes: 0,
            active_quant: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LocalStatusResponse {
    #[serde(flatten)]
    pub runtime: LocalRuntimeStatus,
    pub installed_quants: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LocalStartRequest {
    pub quant: String,
}

#[derive(Debug, Deserialize)]
pub struct HardwareQuery {
    #[serde(default)]
    pub refresh: bool,
}
