mod frontend;
mod handlers;
mod model;
mod pipe;
mod utilities;

use axum::http::{header, Method};
use axum::routing::{get, post};
use axum::Router;
use handlers::*;
use model::AppConfig;
use pipe::{ComicRenderer, OcrManager, Pipeline, Translator};
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

fn load_config() -> (AppConfig, String) {
    let config_path = if std::path::Path::new("config.json").exists() {
        "config.json"
    } else if std::path::Path::new("Server-rs/config.json").exists() {
        "Server-rs/config.json"
    } else {
        ""
    };

    if !config_path.is_empty() {
        if let Ok(data) = std::fs::read_to_string(config_path) {
            if let Ok(cfg) = serde_json::from_str::<AppConfig>(&data) {
                return (cfg, config_path.to_string());
            }
        }
    }

    if std::path::Path::new(".env").exists() {
        dotenvy::dotenv().ok();
    } else if std::path::Path::new("Server-rs/.env").exists() {
        dotenvy::from_path("Server-rs/.env").ok();
    }

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(5000);
    let key = std::env::var("DEEPSEEK_API").unwrap_or_default();

    let mut cfg = AppConfig::default();
    cfg.port = port;
    cfg.deepseek_api_key = key;

    (cfg, "config.json".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let (cfg, config_path) = load_config();
    let port = cfg.port;

    info!("Initializing MyTranslation server...");

    let font_path = if std::path::Path::new("fonts/comic_bold.ttf").exists() {
        "fonts/comic_bold.ttf"
    } else {
        "Server-rs/fonts/comic_bold.ttf"
    };
    info!("Loading comic font from {}...", font_path);
    let renderer = Arc::new(ComicRenderer::new(font_path)?);

    let ocr_mgr = OcrManager::new(51055)?;
    let ocr = Arc::new(Mutex::new(ocr_mgr));

    let translator = Translator::new(cfg);
    if translator.is_active_configured().await {
        info!("MyTranslation translator initialized with configured mode.");
    } else {
        info!(
            "Notice: Active translation mode is not configured. Open http://127.0.0.1:{} to configure.",
            port
        );
    }

    let pipeline = Pipeline::new(ocr, translator, renderer);

    let state = Arc::new(AppState {
        pipeline,
        total_processed: AtomicUsize::new(0),
        config_path,
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
        .route("/", get(dashboard_handler))
        .route("/settings", get(dashboard_handler))
        .route(
            "/api/settings",
            get(get_settings_handler).post(save_settings_handler),
        )
        .route("/api/test-key", post(test_key_handler))
        .route("/api/translate/text", post(translate_text_handler))
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
    info!("MyTranslation listening on http://127.0.0.1:{}", port);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
