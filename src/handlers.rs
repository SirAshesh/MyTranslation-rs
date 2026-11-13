use axum::extract::{Form, Json, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use crate::frontend::render_dashboard_html;
use crate::model::*;
use crate::pipe::Pipeline;
use crate::utilities::{
    extract_image_bytes, get_hardware_info, mask_secret, PipelineTimings, StatsSnapshot,
    StatsTracker, StorageDb,
};
use std::sync::Arc;
use tracing::{error, info};

pub struct AppState {
    pub pipeline: Pipeline,
    pub stats: StatsTracker,
    pub storage: StorageDb,
    pub config_path: String,
}

async fn record_translation_event(
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

pub async fn translate_text_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TextTranslateRequest>,
) -> Response {
    let t0 = std::time::Instant::now();
    let text = payload.text.trim();
    if text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(TextTranslateResponse {
                success: false,
                source: String::new(),
                target: String::new(),
                elapsed_ms: 0,
                message: Some("Text cannot be empty.".to_string()),
            }),
        )
            .into_response();
    }

    let bubble = Bubble {
        id: 1,
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        cx: 0.0,
        cy: 0.0,
        w: 0.0,
        h: 0.0,
        oriented_w: 0.0,
        oriented_h: 0.0,
        angle_rad: 0.0,
        is_left_aligned: false,
        source_text: text.to_string(),
        lines: vec![],
    };

    match state
        .pipeline
        .translator
        .translate_bubbles(&[bubble], &payload.source_lang, &payload.target_lang)
        .await
    {
        Ok(output) => {
            let elapsed_ms = t0.elapsed().as_millis();
            record_translation_event(
                &state,
                "text",
                true,
                PipelineTimings {
                    ocr_ms: 0,
                    translate_ms: elapsed_ms as u64,
                    render_ms: 0,
                    total_ms: elapsed_ms as u64,
                    bubble_count: 1,
                    usage: output.usage,
                },
            )
            .await;
            let target = output
                .translations
                .get(&1)
                .cloned()
                .unwrap_or_else(|| output.translations.values().next().cloned().unwrap_or_default());
            Json(TextTranslateResponse {
                success: true,
                source: text.to_string(),
                target,
                elapsed_ms,
                message: None,
            })
            .into_response()
        }
        Err(e) => {
            let elapsed_ms = t0.elapsed().as_millis();
            record_translation_event(
                &state,
                "text",
                false,
                PipelineTimings {
                    ocr_ms: 0,
                    translate_ms: elapsed_ms as u64,
                    render_ms: 0,
                    total_ms: elapsed_ms as u64,
                    bubble_count: 1,
                    ..PipelineTimings::default()
                },
            )
            .await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TextTranslateResponse {
                    success: false,
                    source: text.to_string(),
                    target: String::new(),
                    elapsed_ms,
                    message: Some(e),
                }),
            )
                .into_response()
        }
    }
}

pub async fn list_handler() -> Json<Vec<ListInstance>> {
    Json(vec![ListInstance {
        name: "MyTranslation".to_string(),
        display_name: "default".to_string(),
        running: false,
    }])
}

pub async fn translate_handler(
    State(state): State<Arc<AppState>>,
    Form(form): Form<TranslateForm>,
) -> Response {
    let t0 = std::time::Instant::now();

    let image_bytes = match extract_image_bytes(&form.src).await {
        Ok(b) => b,
        Err(e) => {
            return Json(TranslateResponse {
                success: false,
                img: None,
                img_map: None,
                message: Some(format!("Failed to parse image data: {}", e)),
            })
            .into_response();
        }
    };

    let without_image = form
        .without_image
        .as_deref()
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    match state
        .pipeline
        .process_image(
            image_bytes,
            &form.source_lang,
            &form.target_lang,
            without_image,
        )
        .await
    {
        Ok((base64_img, img_map, timings)) => {
            record_translation_event(&state, "page", true, timings).await;
            info!("Finished /translate request in {:?}", t0.elapsed());

            if form.r#type.as_deref() == Some("html") {
                if let Some(b64) = base64_img {
                    return Html(format!("<img src=\"data:image/webp;base64,{}\">", b64))
                        .into_response();
                }
            }

            let resp = TranslateResponse {
                success: true,
                img: base64_img,
                img_map: Some(img_map),
                message: None,
            };

            if let Some(cb) = form.callback {
                let json_str = serde_json::to_string(&resp).unwrap_or_default();
                let js_resp = format!("{}({})", cb, json_str);
                return (
                    [(header::CONTENT_TYPE, "application/javascript")],
                    js_resp,
                )
                    .into_response();
            }

            Json(resp).into_response()
        }
        Err(e) => {
            let total_ms = t0.elapsed().as_millis() as u64;
            record_translation_event(
                &state,
                "page",
                false,
                PipelineTimings {
                    total_ms,
                    ..PipelineTimings::default()
                },
            )
            .await;
            error!("Translation pipeline failed: {}", e);
            Json(TranslateResponse {
                success: false,
                img: None,
                img_map: None,
                message: Some(e),
            })
            .into_response()
        }
    }
}

pub async fn translate_region_handler(
    State(state): State<Arc<AppState>>,
    Form(form): Form<TranslateRegionForm>,
) -> Response {
    let image_bytes = match extract_image_bytes(&form.base64).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"success": false, "message": e})),
            )
                .into_response();
        }
    };

    let t0 = std::time::Instant::now();
    match state
        .pipeline
        .process_image(image_bytes, &form.source_lang, &form.target_lang, true)
        .await
    {
        Ok((_, img_map, timings)) => {
            record_translation_event(&state, "region", true, timings).await;
            let (source_text, target_text) = if let Some(first_box) = img_map.boxes.first() {
                (first_box.text.clone(), first_box.target.clone())
            } else {
                ("".to_string(), "".to_string())
            };

            let resp = RegionMapResponse {
                success: true,
                region_map: RegionMapData {
                    source: source_text,
                    target: vec![TargetEngine {
                        engine: "default".to_string(),
                        text: target_text,
                    }],
                },
            };

            if let Some(cb) = form.callback {
                let json_str = serde_json::to_string(&resp).unwrap_or_default();
                let js_resp = format!("{}({})", cb, json_str);
                return (
                    [(header::CONTENT_TYPE, "application/javascript")],
                    js_resp,
                )
                    .into_response();
            }

            Json(resp).into_response()
        }
        Err(e) => {
            record_translation_event(
                &state,
                "region",
                false,
                PipelineTimings {
                    total_ms: t0.elapsed().as_millis() as u64,
                    ..PipelineTimings::default()
                },
            )
            .await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"success": false, "message": e})),
            )
                .into_response()
        }
    }
}

pub async fn batch_translate_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<BatchRequest>,
) -> Response {
    let t0 = std::time::Instant::now();
    let total = payload.images.len();
    let mut results = Vec::with_capacity(total);

    for img_str in payload.images {
        let image_bytes = match extract_image_bytes(&img_str).await {
            Ok(b) => b,
            Err(e) => {
                results.push(TranslateResponse {
                    success: false,
                    img: None,
                    img_map: None,
                    message: Some(format!("Failed to parse image: {}", e)),
                });
                continue;
            }
        };

        match state
            .pipeline
            .process_image(
                image_bytes,
                &payload.source_lang,
                &payload.target_lang,
                false,
            )
            .await
        {
            Ok((base64_img, img_map, timings)) => {
                record_translation_event(&state, "batch", true, timings).await;
                results.push(TranslateResponse {
                    success: true,
                    img: base64_img,
                    img_map: Some(img_map),
                    message: None,
                });
            }
            Err(e) => {
                record_translation_event(&state, "batch", false, PipelineTimings::default()).await;
                results.push(TranslateResponse {
                    success: false,
                    img: None,
                    img_map: None,
                    message: Some(e),
                });
            }
        }
    }

    let elapsed_sec = t0.elapsed().as_secs_f64();

    Json(BatchResponse {
        success: true,
        total,
        elapsed_sec,
        results,
    })
    .into_response()
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
