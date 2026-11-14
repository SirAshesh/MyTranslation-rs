use super::{record_translation_event, AppState};
use crate::model::*;
use crate::utilities::{
    extract_image_bytes, ClientIp, ImageExtractError, PipelineTimings, QueuePermit,
};
use axum::extract::{Form, Json, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use std::sync::Arc;
use tracing::{error, info};

fn max_image_bytes(cfg: &AppConfig) -> usize {
    cfg.max_image_size_mb.clamp(1, 100) * 1024 * 1024
}

fn check_rate_limit(
    state: &AppState,
    cfg: &AppConfig,
    client_ip: &str,
    image_count: usize,
) -> Result<(), (u64, String)> {
    if !cfg.enable_rate_limit {
        return Ok(());
    }
    state
        .rate_limiter
        .check_and_record(
            client_ip,
            image_count,
            cfg.rate_limit_max_images,
            cfg.rate_limit_window_secs,
        )
        .map_err(|retry_after| {
            (
                retry_after,
                format!(
                    "Rate limit exceeded: maximum {} image(s) per {}s per IP. Retry after {}s.",
                    cfg.rate_limit_max_images, cfg.rate_limit_window_secs, retry_after
                ),
            )
        })
}

async fn acquire_queue_if_enabled(state: &AppState, cfg: &AppConfig) -> Option<QueuePermit> {
    if cfg.enable_queue {
        Some(state.image_queue.acquire(cfg.queue_max_concurrent).await)
    } else {
        None
    }
}

fn image_extract_status(err: &ImageExtractError) -> StatusCode {
    match err {
        ImageExtractError::TooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
        ImageExtractError::Invalid(_) => StatusCode::BAD_REQUEST,
    }
}

fn rate_limited_response(retry_after: u64, body: Response) -> Response {
    let mut resp = body;
    if let Ok(val) = retry_after.to_string().parse() {
        resp.headers_mut().insert(header::RETRY_AFTER, val);
    }
    resp
}

fn is_valid_jsonp_callback(cb: &str) -> bool {
    if cb.is_empty() || cb.len() > 64 {
        return false;
    }
    cb.split('.').all(|segment| {
        let mut chars = segment.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first.is_ascii_alphabetic() || first == '_' || first == '$')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
    })
}

fn validate_callback(callback: Option<&str>) -> Result<(), Response> {
    if let Some(cb) = callback {
        if !cb.is_empty() && !is_valid_jsonp_callback(cb) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "message": "Invalid JSONP callback identifier."
                })),
            )
                .into_response());
        }
    }
    Ok(())
}

fn json_or_jsonp_response<T: serde::Serialize>(payload: &T, callback: Option<&str>) -> Response {
    if let Some(cb) = callback {
        if !cb.is_empty() && is_valid_jsonp_callback(cb) {
            let safe_json = serde_json::to_string(payload)
                .unwrap_or_else(|_| "{}".to_string())
                .replace('<', "\\u003c")
                .replace('>', "\\u003e")
                .replace('\u{2028}', "\\u2028")
                .replace('\u{2029}', "\\u2029");
            let js_resp = format!("/**/ {}({});", cb, safe_json);
            return (
                [
                    (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
                    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                ],
                js_resp,
            )
                .into_response();
        }
    }
    Json(payload).into_response()
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
    ClientIp(client_ip): ClientIp,
    Form(form): Form<TranslateForm>,
) -> Response {
    if let Err(resp) = validate_callback(form.callback.as_deref()) {
        return resp;
    }

    let t0 = std::time::Instant::now();
    let cfg = state.pipeline.translator.get_config().await;

    if let Err((retry_after, message)) = check_rate_limit(&state, &cfg, &client_ip, 1) {
        let resp = (
            StatusCode::TOO_MANY_REQUESTS,
            Json(TranslateResponse {
                success: false,
                img: None,
                img_map: None,
                message: Some(message),
            }),
        )
            .into_response();
        return rate_limited_response(retry_after, resp);
    }

    let image_bytes = match extract_image_bytes(&form.src, max_image_bytes(&cfg)).await {
        Ok(b) => b,
        Err(err) => {
            return (
                image_extract_status(&err),
                Json(TranslateResponse {
                    success: false,
                    img: None,
                    img_map: None,
                    message: Some(format!("Failed to parse image data: {}", err.message())),
                }),
            )
                .into_response();
        }
    };

    let without_image = form
        .without_image
        .as_deref()
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let _queue_permit = acquire_queue_if_enabled(&state, &cfg).await;

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
                if let Some(ref b64) = base64_img {
                    if b64
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
                    {
                        return (
                            [
                                (
                                    header::CONTENT_SECURITY_POLICY,
                                    "default-src 'none'; img-src data:",
                                ),
                                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                            ],
                            Html(format!("<img src=\"data:image/webp;base64,{}\">", b64)),
                        )
                            .into_response();
                    }
                }
            }

            let resp = TranslateResponse {
                success: true,
                img: base64_img,
                img_map: Some(img_map),
                message: None,
            };

            json_or_jsonp_response(&resp, form.callback.as_deref())
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
    ClientIp(client_ip): ClientIp,
    Form(form): Form<TranslateRegionForm>,
) -> Response {
    if let Err(resp) = validate_callback(form.callback.as_deref()) {
        return resp;
    }

    let cfg = state.pipeline.translator.get_config().await;

    if let Err((retry_after, message)) = check_rate_limit(&state, &cfg, &client_ip, 1) {
        let resp = (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"success": false, "message": message})),
        )
            .into_response();
        return rate_limited_response(retry_after, resp);
    }

    let image_bytes = match extract_image_bytes(&form.base64, max_image_bytes(&cfg)).await {
        Ok(b) => b,
        Err(err) => {
            return (
                image_extract_status(&err),
                Json(serde_json::json!({"success": false, "message": err.message()})),
            )
                .into_response();
        }
    };

    let _queue_permit = acquire_queue_if_enabled(&state, &cfg).await;

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

            json_or_jsonp_response(&resp, form.callback.as_deref())
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
    ClientIp(client_ip): ClientIp,
    Json(payload): Json<BatchRequest>,
) -> Response {
    let cfg = state.pipeline.translator.get_config().await;

    if cfg.disable_batch_endpoint {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "success": false,
                "total": 0,
                "elapsed_sec": 0.0,
                "results": [],
                "message": "The /batch endpoint is disabled."
            })),
        )
            .into_response();
    }

    let total = payload.images.len();
    if let Err((retry_after, message)) = check_rate_limit(&state, &cfg, &client_ip, total.max(1)) {
        let resp = (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "success": false,
                "total": 0,
                "elapsed_sec": 0.0,
                "results": [],
                "message": message
            })),
        )
            .into_response();
        return rate_limited_response(retry_after, resp);
    }

    let max_bytes = max_image_bytes(&cfg);
    let t0 = std::time::Instant::now();
    let mut results = Vec::with_capacity(total);

    for img_str in payload.images {
        let image_bytes = match extract_image_bytes(&img_str, max_bytes).await {
            Ok(b) => b,
            Err(err) => {
                results.push(TranslateResponse {
                    success: false,
                    img: None,
                    img_map: None,
                    message: Some(format!("Failed to parse image: {}", err.message())),
                });
                continue;
            }
        };

        let _queue_permit = acquire_queue_if_enabled(&state, &cfg).await;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonp_callback_validation_blocks_injection() {
        assert!(is_valid_jsonp_callback("cb"));
        assert!(is_valid_jsonp_callback("jQuery_123.done"));
        assert!(!is_valid_jsonp_callback("alert(1)//"));
        assert!(!is_valid_jsonp_callback("foo;bar"));
        assert!(!is_valid_jsonp_callback("1bad"));
        assert!(!is_valid_jsonp_callback("a..b"));
    }
}
