pub mod translate;

pub use translate::*;

use axum::extract::{Json, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use crate::frontend::render_dashboard_html;
use crate::model::*;
use crate::pipe::Pipeline;
use crate::utilities::{
    get_hardware_info, mask_secret, ImageQueue, IpRateLimiter, PipelineTimings, StatsSnapshot,
    StatsTracker, StorageDb,
};
use std::sync::Arc;

pub struct AppState {
    pub pipeline: Pipeline,
    pub stats: StatsTracker,
    pub storage: StorageDb,
    pub config_path: String,
    pub image_queue: ImageQueue,
    pub rate_limiter: IpRateLimiter,
    pub auth_rate_limiter: IpRateLimiter,
}

pub(crate) async fn record_translation_event(
    state: &AppState,
    endpoint: &str,
    success: bool,
    timings: PipelineTimings,
) {
    let provider_label = state.pipeline.translator.active_provider_label().await;
    state
        .stats
        .record(endpoint, &provider_label, success, timings);
}

pub async fn dashboard_handler() -> Html<String> {
    Html(render_dashboard_html())
}

async fn build_settings_response(
    state: &AppState,
    cfg: AppConfig,
    active_configured: bool,
    success: bool,
    message: String,
) -> SettingsResponse {
    #[cfg(feature = "free-mode")]
    let free = {
        let active_summary = state.pipeline.translator.free_active_summary(&cfg).await;
        cfg.free.to_response(active_summary)
    };
    #[cfg(not(feature = "free-mode"))]
    let _ = state;

    SettingsResponse {
        success,
        message,
        port: cfg.port,
        translation_mode: cfg.translation_mode,
        openai_key_configured: !cfg.deepseek_api_key.trim().is_empty(),
        openai_masked_key: mask_secret(&cfg.deepseek_api_key),
        openai_base_url: cfg.openai_base_url,
        openai_model: cfg.openai_model,
        claude_key_configured: !cfg.claude_api_key.trim().is_empty(),
        claude_masked_key: mask_secret(&cfg.claude_api_key),
        claude_base_url: cfg.claude_base_url,
        claude_model: cfg.claude_model,
        local_model: cfg.local_model,
        disable_batch_endpoint: cfg.disable_batch_endpoint,
        enable_queue: cfg.enable_queue,
        queue_max_concurrent: cfg.queue_max_concurrent,
        enable_rate_limit: cfg.enable_rate_limit,
        rate_limit_max_images: cfg.rate_limit_max_images,
        rate_limit_window_secs: cfg.rate_limit_window_secs,
        max_image_size_mb: cfg.max_image_size_mb,
        #[cfg(feature = "free-mode")]
        free,
        active_configured,
    }
}

pub async fn get_settings_handler(State(state): State<Arc<AppState>>) -> Json<SettingsResponse> {
    let cfg = state.pipeline.translator.get_config().await;
    let active_configured = state.pipeline.translator.is_active_configured().await;
    Json(
        build_settings_response(
            &state,
            cfg,
            active_configured,
            true,
            "Settings loaded.".to_string(),
        )
        .await,
    )
}

pub async fn save_settings_handler(
    State(state): State<Arc<AppState>>,
    Json(form): Json<SaveSettingsForm>,
) -> Json<SettingsResponse> {
    let updated_cfg = state.pipeline.translator.apply_settings(&form).await;
    let active_configured = state.pipeline.translator.is_active_configured().await;

    let write_result = serde_json::to_string_pretty(&updated_cfg)
        .map_err(|e| e.to_string())
        .and_then(|json_str| std::fs::write(&state.config_path, json_str).map_err(|e| e.to_string()));

    let (success, message) = match write_result {
        Ok(()) => (
            true,
            format!("Saved settings to config.json. Active mode: {}.", updated_cfg.translation_mode),
        ),
        Err(e) => (false, format!("Failed to write config.json: {}", e)),
    };
    Json(
        build_settings_response(&state, updated_cfg, active_configured, success, message).await,
    )
}

pub async fn hardware_handler(Query(query): Query<HardwareQuery>) -> Response {
    match tokio::task::spawn_blocking(move || get_hardware_info(query.refresh)).await {
        Ok(info) => Json(info).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"success": false, "message": format!("Hardware detection failed: {}", e)})),
        )
            .into_response(),
    }
}

pub async fn local_status_handler(State(state): State<Arc<AppState>>) -> Json<LocalStatusResponse> {
    Json(state.pipeline.translator.runtime.status().await)
}

pub async fn local_start_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LocalStartRequest>,
) -> Response {
    let runtime = &state.pipeline.translator.runtime;
    match runtime.start(&req.quant).await {
        Ok(()) => Json(runtime.status().await).into_response(),
        Err(message) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"success": false, "message": message})),
        )
            .into_response(),
    }
}

pub async fn local_stop_handler(State(state): State<Arc<AppState>>) -> Json<LocalStatusResponse> {
    let runtime = &state.pipeline.translator.runtime;
    runtime.stop().await;
    Json(runtime.status().await)
}

pub async fn test_key_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TestKeyRequest>,
) -> Json<TestKeyResponse> {
    match state.pipeline.translator.test_connection(&req).await {
        Ok(msg) => Json(TestKeyResponse {
            success: true,
            message: msg,
        }),
        Err(e) => Json(TestKeyResponse {
            success: false,
            message: e,
        }),
    }
}

#[cfg(feature = "free-mode")]
pub async fn free_models_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<crate::pipe::free_mode::FreeModelsRequest>,
) -> Json<crate::pipe::free_mode::FreeModelsResponse> {
    match state
        .pipeline
        .translator
        .fetch_free_models(&req.provider, &req.api_key)
        .await
    {
        Ok((models, evicted_count)) => {
            let suffix = if evicted_count > 0 {
                format!(" ({} evicted unavailable model(s) filtered out)", evicted_count)
            } else {
                String::new()
            };
            Json(crate::pipe::free_mode::FreeModelsResponse {
                success: true,
                provider: req.provider.clone(),
                message: format!("Loaded {} models for {}.{}", models.len(), req.provider, suffix),
                models,
            })
        }
        Err(message) => Json(crate::pipe::free_mode::FreeModelsResponse {
            success: false,
            provider: req.provider,
            models: Vec::new(),
            message,
        }),
    }
}

pub async fn stats_handler(State(state): State<Arc<AppState>>) -> Json<StatsSnapshot> {
    let configured = state.pipeline.translator.is_active_configured().await;
    let cfg = state.pipeline.translator.get_config().await;
    Json(state.stats.snapshot(&cfg.translation_mode, configured))
}

pub async fn reset_stats_handler(State(state): State<Arc<AppState>>) -> Json<StatsSnapshot> {
    state.stats.reset();
    let configured = state.pipeline.translator.is_active_configured().await;
    let cfg = state.pipeline.translator.get_config().await;
    Json(state.stats.snapshot(&cfg.translation_mode, configured))
}
