mod clustering;
mod deepseek;
mod inpainting;
mod marian;
mod models;
mod ocr;
mod pipeline;
mod renderer;
mod translator;

use axum::extract::{Form, Json, State};
use axum::http::{header, Method, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use dotenvy::dotenv;
use models::{
    BatchRequest, BatchResponse, ListInstance, RegionMapData, RegionMapResponse, TargetEngine,
    TranslateForm, TranslateRegionForm, TranslateResponse,
};
use ocr::OcrManager;
use pipeline::Pipeline;
use renderer::ComicRenderer;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;
use translator::Translator;

struct AppState {
    pipeline: Pipeline,
    total_processed: AtomicUsize,
    engine_name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    dotenv().ok();
    if std::path::Path::new("Server-rs/.env").exists() {
        let _ = dotenvy::from_path("Server-rs/.env");
    }

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(5000);

    info!("Initializing Server-rs (Rust High-Performance Comic Translator)...");

    let font_path = if std::path::Path::new("fonts/comic_bold.ttf").exists() {
        "fonts/comic_bold.ttf"
    } else {
        "Server-rs/fonts/comic_bold.ttf"
    };
    info!("Loading comic font from {}...", font_path);
    let renderer = Arc::new(ComicRenderer::new(font_path)?);

    let ocr_mgr = OcrManager::new(51055)?;
    let ocr = Arc::new(Mutex::new(ocr_mgr));

    let local_trans = std::env::var("LOCAL_TRANSLATION")
        .unwrap_or_default()
        .to_lowercase();
    let use_local = local_trans == "true" || local_trans == "1";

    let (translator, engine_name) = if use_local {
        info!("Using local MarianMT offline translation engine...");
        let marian_port = 51056;
        let mut marian_mgr = marian::MarianManager::new(marian_port);
        marian_mgr.ensure_started().await?;
        (
            Translator::Marian(Arc::new(Mutex::new(marian_mgr))),
            "Native Rust DirectML OCR + MarianMT Local + Rust Inpainting".to_string(),
        )
    } else {
        info!("Using cloud DeepSeek V3 translation engine...");
        let deepseek_api_key = std::env::var("DEEPSEEK_API")
            .expect("DEEPSEEK_API environment variable must be set in .env when LOCAL_TRANSLATION is false");
        let client = deepseek::DeepSeekClient::new(deepseek_api_key);
        (
            Translator::DeepSeek(client),
            "Native Rust DirectML OCR + DeepSeek V3 + Rust Inpainting".to_string(),
        )
    };

    let pipeline = Pipeline::new(ocr, translator, renderer);

    let state = Arc::new(AppState {
        pipeline,
        total_processed: AtomicUsize::new(0),
        engine_name,
    });

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::OPTIONS,
            Method::HEAD,
            Method::PUT,
        ])
        .allow_headers(tower_http::cors::Any)
        .expose_headers([header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/list", get(list_handler))
        .route("/translate", post(translate_handler))
        .route("/translateRegion", post(translate_region_handler))
        .route("/api/translate/batch", post(batch_translate_handler))
        .route("/batch", post(batch_translate_handler))
        .route("/stats", get(stats_handler))
        .layer(axum::extract::DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Server-rs is running and listening on http://127.0.0.1:{}", port);
    info!("Compatible with ImageTrans extension at http://localhost:{} or http://127.0.0.1:{}", port, port);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index_handler() -> Html<&'static str> {
    Html(r#"<!DOCTYPE html>
<html>
<head><title>Server-rs - ImageTrans High Performance Server</title></head>
<body style="font-family: sans-serif; padding: 40px; background: #0f172a; color: #f8fafc;">
    <h1>⚡ Server-rs is Online</h1>
    <p>High-performance Rust implementation for comic, manga, and webtoon translation.</p>
    <ul>
        <li><b>POST /translate</b>: ImageTrans compatible translation endpoint</li>
        <li><b>POST /translateRegion</b>: Region OCR and translation</li>
        <li><b>POST /batch</b>: Bulk translation of multiple images</li>
        <li><b>GET /list</b>: ImageTrans server discovery</li>
        <li><b>GET /stats</b>: Performance statistics</li>
    </ul>
</body>
</html>"#)
}

async fn list_handler() -> Json<Vec<ListInstance>> {
    Json(vec![ListInstance {
        name: "Server-rs".to_string(),
        display_name: "default".to_string(),
        running: false,
    }])
}

async fn translate_handler(
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

async fn translate_region_handler(
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

async fn batch_translate_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<BatchRequest>,
) -> Json<BatchResponse> {
    let t0 = std::time::Instant::now();
    let total = payload.images.len();

    let mut handles = Vec::new();
    for img_data in payload.images {
        let state_clone = state.clone();
        let src_lang = payload.source_lang.clone();
        let tgt_lang = payload.target_lang.clone();

        handles.push(tokio::spawn(async move {
            match extract_image_bytes(&img_data).await {
                Ok(bytes) => {
                    match state_clone
                        .pipeline
                        .process_image(bytes, &src_lang, &tgt_lang, false)
                        .await
                    {
                        Ok((base64_img, img_map)) => TranslateResponse {
                            success: true,
                            img: base64_img,
                            img_map: Some(img_map),
                            message: None,
                        },
                        Err(e) => TranslateResponse {
                            success: false,
                            img: None,
                            img_map: None,
                            message: Some(e),
                        },
                    }
                }
                Err(e) => TranslateResponse {
                    success: false,
                    img: None,
                    img_map: None,
                    message: Some(e),
                },
            }
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        if let Ok(res) = h.await {
            results.push(res);
        }
    }

    let elapsed = t0.elapsed().as_secs_f64();
    info!("Batch translated {} images in {:.2}s", total, elapsed);

    Json(BatchResponse {
        success: true,
        total,
        elapsed_sec: elapsed,
        results,
    })
}

async fn stats_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let count = state.total_processed.load(Ordering::Relaxed);
    Json(serde_json::json!({
        "server": "Server-rs",
        "version": "1.0.0",
        "total_images_processed": count,
        "engine": state.engine_name
    }))
}

async fn extract_image_bytes(src: &str) -> Result<Vec<u8>, String> {
    let trimmed = src.trim();

    if trimmed.is_empty() {
        return Err("src is empty".to_string());
    }

    if let Some(pos) = trimmed.find(";base64,") {
        let b64_part = &trimmed[pos + 8..];
        return BASE64
            .decode(b64_part)
            .map_err(|e| format!("Base64 decode error: {}", e));
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        let resp = client
            .get(trimmed)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch image URL: {}", e))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read image bytes from URL: {}", e))?;
        return Ok(bytes.to_vec());
    }

    if std::path::Path::new(trimmed).exists() {
        return std::fs::read(trimmed)
            .map_err(|e| format!("Failed to read local image file: {}", e));
    }

    match BASE64.decode(trimmed) {
        Ok(bytes) => Ok(bytes),
        Err(_) => Err("Invalid image format (not a valid data URI, URL, file, or base64)".to_string()),
    }
}
