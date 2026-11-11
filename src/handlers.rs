use axum::extract::{Form, Json, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use crate::frontend::render_dashboard_html;
use crate::model::*;
use crate::pipe::Pipeline;
use crate::utilities::{extract_image_bytes, mask_secret};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::{error, info};

pub struct AppState {
    pub pipeline: Pipeline,
    pub total_processed: AtomicUsize,
    pub config_path: String,
}

pub async fn dashboard_handler() -> Html<String> {
    Html(render_dashboard_html())
}

pub async fn get_settings_handler(State(state): State<Arc<AppState>>) -> Json<SettingsResponse> {
    let cfg = state.pipeline.translator.get_config().await;
    let active_configured = state.pipeline.translator.is_active_configured().await;

    Json(SettingsResponse {
        success: true,
        message: "Settings loaded.".to_string(),
        port: cfg.port,
        translation_mode: cfg.translation_mode,
        openai_base_url: cfg.openai_base_url,
        openai_model: cfg.openai_model,
        openai_key_configured: !cfg.deepseek_api_key.trim().is_empty(),
        openai_masked_key: mask_secret(&cfg.deepseek_api_key),
        claude_base_url: cfg.claude_base_url,
        claude_model: cfg.claude_model,
        claude_key_configured: !cfg.claude_api_key.trim().is_empty(),
        claude_masked_key: mask_secret(&cfg.claude_api_key),
        active_configured,
    })
}

pub async fn save_settings_handler(
    State(state): State<Arc<AppState>>,
    Json(form): Json<SaveSettingsForm>,
) -> Json<SettingsResponse> {
    let updated_cfg = state.pipeline.translator.apply_settings(&form).await;
    let active_configured = state.pipeline.translator.is_active_configured().await;

    let save_res = serde_json::to_string_pretty(&updated_cfg)
        .map_err(|e| e.to_string())
        .and_then(|json_str| {
            std::fs::write(&state.config_path, json_str).map_err(|e| e.to_string())
        });

    match save_res {
        Ok(_) => Json(SettingsResponse {
            success: true,
            message: format!(
                "Saved settings to config.json. Active mode: {}.",
                updated_cfg.translation_mode
            ),
            port: updated_cfg.port,
            translation_mode: updated_cfg.translation_mode,
            openai_base_url: updated_cfg.openai_base_url,
            openai_model: updated_cfg.openai_model,
            openai_key_configured: !updated_cfg.deepseek_api_key.trim().is_empty(),
            openai_masked_key: mask_secret(&updated_cfg.deepseek_api_key),
            claude_base_url: updated_cfg.claude_base_url,
            claude_model: updated_cfg.claude_model,
            claude_key_configured: !updated_cfg.claude_api_key.trim().is_empty(),
            claude_masked_key: mask_secret(&updated_cfg.claude_api_key),
            active_configured,
        }),
        Err(e) => Json(SettingsResponse {
            success: false,
            message: format!("Failed to write config.json: {}", e),
            port: updated_cfg.port,
            translation_mode: updated_cfg.translation_mode,
            openai_base_url: updated_cfg.openai_base_url,
            openai_model: updated_cfg.openai_model,
            openai_key_configured: !updated_cfg.deepseek_api_key.trim().is_empty(),
            openai_masked_key: mask_secret(&updated_cfg.deepseek_api_key),
            claude_base_url: updated_cfg.claude_base_url,
            claude_model: updated_cfg.claude_model,
            claude_key_configured: !updated_cfg.claude_api_key.trim().is_empty(),
            claude_masked_key: mask_secret(&updated_cfg.claude_api_key),
            active_configured,
        }),
    }
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
        w: 0.0,
        h: 0.0,
        source_text: text.to_string(),
        lines: vec![],
    };

    match state
        .pipeline
        .translator
        .translate_bubbles(&[bubble], &payload.source_lang, &payload.target_lang)
        .await
    {
        Ok(map) => {
            let target = map
                .get(&1)
                .cloned()
                .unwrap_or_else(|| map.values().next().cloned().unwrap_or_default());
            Json(TextTranslateResponse {
                success: true,
                source: text.to_string(),
                target,
                elapsed_ms: t0.elapsed().as_millis(),
                message: None,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TextTranslateResponse {
                success: false,
                source: text.to_string(),
                target: String::new(),
                elapsed_ms: t0.elapsed().as_millis(),
                message: Some(e),
            }),
        )
            .into_response(),
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
        Ok((base64_img, img_map)) => {
            state.total_processed.fetch_add(1, Ordering::Relaxed);
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

    match state
        .pipeline
        .process_image(image_bytes, &form.source_lang, &form.target_lang, true)
        .await
    {
        Ok((_, img_map)) => {
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
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"success": false, "message": e})),
        )
            .into_response(),
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
            Ok((base64_img, img_map)) => {
                results.push(TranslateResponse {
                    success: true,
                    img: base64_img,
                    img_map: Some(img_map),
                    message: None,
                });
            }
            Err(e) => {
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
    state.total_processed.fetch_add(total, Ordering::Relaxed);

    Json(BatchResponse {
        success: true,
        total,
        elapsed_sec,
        results,
    })
    .into_response()
}

pub async fn stats_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let total = state.total_processed.load(Ordering::Relaxed);
    let configured = state.pipeline.translator.is_active_configured().await;
    let cfg = state.pipeline.translator.get_config().await;
    Json(serde_json::json!({
        "status": "online",
        "total_processed": total,
        "engine": "RapidOCR DirectML",
        "translation_mode": cfg.translation_mode,
        "api_key_configured": configured
    }))
}
