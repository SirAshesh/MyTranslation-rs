use crate::model::{
    find_local_model, EngineBackend, LocalModelFamily, LocalModelSpec, LocalPhase,
    LocalRuntimeStatus, LocalStatusResponse, LOCAL_MODEL_CATALOG, LOCAL_SERVER_PORT,
};
use crate::utilities::{download_file, extract_archive, find_file_recursive, get_hardware_info};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};

const ENGINE_EXECUTABLE: &str = if cfg!(windows) {
    "llama-server.exe"
} else {
    "llama-server"
};
const RELEASES_URL: &str = "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10";
const READY_TIMEOUT: Duration = Duration::from_secs(180);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(500);
const LOG_TAIL_CHARS: usize = 800;
const CONTEXT_SIZE: &str = "4096";

#[derive(Deserialize)]
struct ReleaseInfo {
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Default)]
struct ProcessSlot {
    child: Option<Child>,
    task: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct LocalRuntime {
    root: PathBuf,
    client: reqwest::Client,
    status: Arc<StdMutex<LocalRuntimeStatus>>,
    process: Arc<Mutex<ProcessSlot>>,
}

impl LocalRuntime {
    pub fn new(root: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("MyTranslation")
            .connect_timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            root,
            client,
            status: Arc::new(StdMutex::new(LocalRuntimeStatus::stopped())),
            process: Arc::new(Mutex::new(ProcessSlot::default())),
        }
    }

    pub async fn status(&self) -> LocalStatusResponse {
        self.detect_crash().await;
        LocalStatusResponse {
            runtime: self.lock_status().clone(),
            installed_quants: self.installed_quants(),
        }
    }

    pub async fn is_running(&self) -> bool {
        self.detect_crash().await;
        self.lock_status().phase == LocalPhase::Running
    }

    pub async fn start(&self, quant: &str) -> Result<(), String> {
        let spec = find_local_model(quant).ok_or_else(|| format!("Unknown model variant: {}", quant))?;
        let mut slot = self.process.lock().await;
        if self.lock_status().phase.is_busy() {
            return Err("The local model is already starting or running. Stop it first.".to_string());
        }

        self.set_phase(LocalPhase::Starting, "Preparing local model...");
        let runtime = self.clone();
        slot.task = Some(tokio::spawn(async move {
            if let Err(message) = runtime.launch(spec).await {
                warn!("Local model launch failed: {}", message);
                runtime.mark_failed(message).await;
            }
        }));
        Ok(())
    }

    pub async fn stop(&self) {
        let mut slot = self.process.lock().await;
        if let Some(task) = slot.task.take() {
            task.abort();
        }
        Self::kill_child(&mut slot).await;
        *self.lock_status() = LocalRuntimeStatus::stopped();
    }

    async fn launch(&self, spec: &'static LocalModelSpec) -> Result<(), String> {
        let backend = tokio::task::spawn_blocking(|| get_hardware_info(false).backend)
            .await
            .map_err(|e| format!("Hardware detection failed: {}", e))?;

        let engine_path = self.ensure_engine(backend).await?;
        let model_path = self.ensure_model(spec).await?;
        self.spawn_server(&engine_path, &model_path, backend, spec.family)
            .await?;
        self.wait_until_ready().await?;

        let mut status = self.lock_status();
        status.phase = LocalPhase::Running;
        status.message = format!("Running {}", spec.label());
        status.active_quant = Some(spec.quant.to_string());
        info!("Local model server is ready with {}", spec.label());
        Ok(())
    }

    async fn ensure_engine(&self, backend: EngineBackend) -> Result<PathBuf, String> {
        let engine_dir = self.root.join("engine").join(backend.directory_name());
        if let Some(executable) = find_file_recursive(&engine_dir, ENGINE_EXECUTABLE) {
            return Ok(executable);
        }
        if !cfg!(any(all(windows, target_arch = "x86_64"), all(target_os = "linux", target_arch = "x86_64"))) {
            return Err("Automatic engine installation is only supported on x64 systems.".to_string());
        }

        self.set_phase(LocalPhase::InstallingEngine, "Downloading inference engine...");
        let staging_dir = engine_dir.with_extension("installing");
        if staging_dir.exists() {
            tokio::fs::remove_dir_all(&staging_dir)
                .await
                .map_err(|e| format!("Failed to clean {}: {}", staging_dir.display(), e))?;
        }

        for asset in self.resolve_engine_assets(backend).await? {
            let archive_path = staging_dir.join(&asset.name);
            download_file(&self.client, &asset.browser_download_url, &archive_path, |done, total| {
                self.report_progress(done, total)
            })
            .await?;
            self.set_phase(LocalPhase::InstallingEngine, "Extracting inference engine...");
            self.extract_and_remove(archive_path, staging_dir.clone()).await?;
        }

        tokio::fs::rename(&staging_dir, &engine_dir)
            .await
            .map_err(|e| format!("Failed to finalize {}: {}", engine_dir.display(), e))?;
        find_file_recursive(&engine_dir, ENGINE_EXECUTABLE)
            .ok_or_else(|| format!("{} was not found in the downloaded engine.", ENGINE_EXECUTABLE))
    }

    async fn extract_and_remove(&self, archive_path: PathBuf, destination: PathBuf) -> Result<(), String> {
        let archive_for_extraction = archive_path.clone();
        tokio::task::spawn_blocking(move || extract_archive(&archive_for_extraction, &destination))
            .await
            .map_err(|e| format!("Extraction task failed: {}", e))??;
        tokio::fs::remove_file(&archive_path)
            .await
            .map_err(|e| format!("Failed to remove {}: {}", archive_path.display(), e))
    }

    async fn resolve_engine_assets(&self, backend: EngineBackend) -> Result<Vec<ReleaseAsset>, String> {
        let releases: Vec<ReleaseInfo> = self
            .client
            .get(RELEASES_URL)
            .send()
            .await
            .map_err(|e| format!("Failed to query llama.cpp releases: {}", e))?
            .error_for_status()
            .map_err(|e| format!("llama.cpp release query failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse llama.cpp releases: {}", e))?;

        releases
            .into_iter()
            .map(|release| {
                release
                    .assets
                    .into_iter()
                    .filter(|asset| backend.matches_asset(&asset.name))
                    .collect::<Vec<_>>()
            })
            .find(|assets| !assets.is_empty())
            .ok_or_else(|| "No llama.cpp release provides matching engine assets.".to_string())
    }

    async fn ensure_model(&self, spec: &LocalModelSpec) -> Result<PathBuf, String> {
        let model_path = self.model_path(spec);
        if model_path.exists() {
            return Ok(model_path);
        }

        self.set_phase(
            LocalPhase::DownloadingModel,
            format!("Downloading {}...", spec.label()),
        );
        download_file(&self.client, &spec.download_url(), &model_path, |done, total| {
            self.report_progress(done, total)
        })
        .await?;
        Ok(model_path)
    }

    async fn spawn_server(
        &self,
        engine_path: &Path,
        model_path: &Path,
        backend: EngineBackend,
        family: LocalModelFamily,
    ) -> Result<(), String> {
        let log_path = self.log_path();
        let log_file = std::fs::File::create(&log_path)
            .map_err(|e| format!("Failed to create {}: {}", log_path.display(), e))?;
        let error_log = log_file
            .try_clone()
            .map_err(|e| format!("Failed to duplicate log handle: {}", e))?;

        let mut command = Command::new(engine_path);
        command
            .arg("-m")
            .arg(model_path)
            .args(["--host", "127.0.0.1", "--port"])
            .arg(LOCAL_SERVER_PORT.to_string())
            .args(["-c", CONTEXT_SIZE])
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(error_log))
            .kill_on_drop(true);
        if family == LocalModelFamily::Lfm3 {
            command.args(["--chat-template-kwargs", r#"{"enable_thinking":false}"#]);
        }
        if backend.offloads_to_gpu() {
            command.args(["-ngl", "99"]);
        }
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);

        let child = command
            .spawn()
            .map_err(|e| format!("Failed to start {}: {}", engine_path.display(), e))?;
        self.process.lock().await.child = Some(child);
        Ok(())
    }

    async fn wait_until_ready(&self) -> Result<(), String> {
        self.set_phase(LocalPhase::Starting, "Loading model into memory...");
        let health_url = format!("http://127.0.0.1:{}/health", LOCAL_SERVER_PORT);
        let deadline = Instant::now() + READY_TIMEOUT;

        while Instant::now() < deadline {
            if let Some(exit_status) = self.child_exit_status().await {
                return Err(format!(
                    "Local model server exited ({}). {}",
                    exit_status,
                    self.log_tail()
                ));
            }
            let health = self.client.get(&health_url).timeout(Duration::from_secs(5)).send().await;
            if health.is_ok_and(|response| response.status().is_success()) {
                return Ok(());
            }
            tokio::time::sleep(READY_POLL_INTERVAL).await;
        }
        Err(format!("Timed out while loading the model. {}", self.log_tail()))
    }

    async fn child_exit_status(&self) -> Option<String> {
        let mut slot = self.process.lock().await;
        let exit_status = slot.child.as_mut()?.try_wait().ok().flatten()?;
        slot.child = None;
        Some(exit_status.to_string())
    }

    async fn detect_crash(&self) {
        if self.lock_status().phase != LocalPhase::Running {
            return;
        }
        if let Some(exit_status) = self.child_exit_status().await {
            let message = format!("Local model server exited ({}). {}", exit_status, self.log_tail());
            self.set_phase(LocalPhase::Failed, message);
        }
    }

    async fn mark_failed(&self, message: String) {
        let mut slot = self.process.lock().await;
        Self::kill_child(&mut slot).await;
        slot.task = None;
        self.set_phase(LocalPhase::Failed, message);
    }

    async fn kill_child(slot: &mut ProcessSlot) {
        if let Some(mut child) = slot.child.take() {
            if let Err(err) = child.kill().await {
                warn!("Failed to stop local model server: {}", err);
            }
        }
    }

    fn installed_quants(&self) -> Vec<String> {
        LOCAL_MODEL_CATALOG
            .iter()
            .filter(|spec| self.model_path(spec).exists())
            .map(|spec| spec.quant.to_string())
            .collect()
    }

    fn model_path(&self, spec: &LocalModelSpec) -> PathBuf {
        self.root.join("models").join(spec.file_name())
    }

    fn log_path(&self) -> PathBuf {
        self.root.join("llama-server.log")
    }

    fn log_tail(&self) -> String {
        let log = std::fs::read_to_string(self.log_path()).unwrap_or_default();
        let skipped = log.chars().count().saturating_sub(LOG_TAIL_CHARS);
        log.chars().skip(skipped).collect::<String>().trim().to_string()
    }

    fn lock_status(&self) -> MutexGuard<'_, LocalRuntimeStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_phase(&self, phase: LocalPhase, message: impl Into<String>) {
        let mut status = self.lock_status();
        status.phase = phase;
        status.message = message.into();
        status.downloaded_bytes = 0;
        status.total_bytes = 0;
        status.active_quant = None;
    }

    fn report_progress(&self, downloaded_bytes: u64, total_bytes: u64) {
        let mut status = self.lock_status();
        status.downloaded_bytes = downloaded_bytes;
        status.total_bytes = total_bytes;
    }
}
