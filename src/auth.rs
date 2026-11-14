use crate::handlers::AppState;
use crate::utilities::storage::SignupError;
use crate::utilities::ClientIp;
use axum::extract::{Json, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use std::sync::Arc;

const SESSION_COOKIE_NAME: &str = "mt_session";
const SESSION_MAX_AGE_SECS: u64 = 7 * 24 * 60 * 60;
const AUTH_MAX_ATTEMPTS: usize = 5;
const AUTH_WINDOW_SECS: u64 = 60;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub verify_password: String,
}

pub fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    let raw_cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw_cookie.split(';') {
        let mut parts = pair.trim().splitn(2, '=');
        let name = parts.next()?.trim();
        let val = parts.next()?.trim();
        if name == SESSION_COOKIE_NAME && !val.is_empty() {
            return Some(val.to_string());
        }
    }
    None
}

fn is_authenticated(state: &AppState, headers: &HeaderMap) -> bool {
    extract_session_token(headers)
        .map(|token| state.storage.validate_session(&token))
        .unwrap_or(false)
}

fn session_cookie_header(token: &str, max_age_secs: u64) -> String {
    format!(
        "{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
        SESSION_COOKIE_NAME, token, max_age_secs
    )
}

pub async fn require_admin_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    if is_authenticated(&state, req.headers()) {
        return next.run(req).await;
    }

    let path = req.uri().path();
    if path == "/" || path == "/settings" {
        let target = if state.storage.has_super_admin().unwrap_or(false) {
            "/login"
        } else {
            "/signup"
        };
        return Redirect::to(target).into_response();
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "success": false,
            "message": "Authentication required."
        })),
    )
        .into_response()
}

pub async fn login_page_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if is_authenticated(&state, &headers) {
        return Redirect::to("/").into_response();
    }
    if !state.storage.has_super_admin().unwrap_or(false) {
        return Redirect::to("/signup").into_response();
    }
    Html(crate::frontend::render_login_html()).into_response()
}

pub async fn signup_page_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if is_authenticated(&state, &headers) {
        return Redirect::to("/").into_response();
    }
    if state.storage.has_super_admin().unwrap_or(false) {
        return Redirect::to("/login").into_response();
    }
    Html(crate::frontend::render_signup_html()).into_response()
}

fn check_auth_rate_limit(state: &AppState, client_ip: &str) -> Result<(), Response> {
    if let Err(retry_after) =
        state
            .auth_rate_limiter
            .check_and_record(client_ip, 1, AUTH_MAX_ATTEMPTS, AUTH_WINDOW_SECS)
    {
        let mut resp = (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "success": false,
                "message": format!("Too many authentication attempts. Try again in {}s.", retry_after)
            })),
        )
            .into_response();
        if let Ok(val) = retry_after.to_string().parse() {
            resp.headers_mut().insert(header::RETRY_AFTER, val);
        }
        return Err(resp);
    }
    Ok(())
}

pub async fn signup_submit_handler(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    Json(payload): Json<SignupRequest>,
) -> Response {
    if let Err(resp) = check_auth_rate_limit(&state, &client_ip) {
        return resp;
    }

    match state.storage.create_initial_admin(
        &payload.username,
        &payload.password,
        &payload.verify_password,
    ) {
        Ok(token) => {
            state.auth_rate_limiter.reset(&client_ip);
            (
                StatusCode::OK,
                [(header::SET_COOKIE, session_cookie_header(&token, SESSION_MAX_AGE_SECS))],
                Json(serde_json::json!({
                    "success": true,
                    "message": "Account created."
                })),
            )
                .into_response()
        }
        Err(SignupError::AlreadyExists) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "success": false,
                "message": "An account already exists. Please sign in."
            })),
        )
            .into_response(),
        Err(SignupError::Invalid(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "message": msg
            })),
        )
            .into_response(),
        Err(SignupError::Internal(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "message": format!("Sign up error: {}", err)
            })),
        )
            .into_response(),
    }
}

pub async fn login_submit_handler(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    Json(payload): Json<LoginRequest>,
) -> Response {
    if let Err(resp) = check_auth_rate_limit(&state, &client_ip) {
        return resp;
    }

    match state
        .storage
        .verify_login(&payload.username, &payload.password)
    {
        Ok(Some(token)) => {
            state.auth_rate_limiter.reset(&client_ip);
            (
                StatusCode::OK,
                [(header::SET_COOKIE, session_cookie_header(&token, SESSION_MAX_AGE_SECS))],
                Json(serde_json::json!({
                    "success": true,
                    "message": "Signed in."
                })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "success": false,
                "message": "Invalid username or password."
            })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "message": format!("Login error: {}", err)
            })),
        )
            .into_response(),
    }
}

pub async fn logout_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Some(token) = extract_session_token(&headers) {
        state.storage.revoke_session(&token);
    }
    (
        StatusCode::OK,
        [(header::SET_COOKIE, session_cookie_header("", 0))],
        Json(serde_json::json!({
            "success": true,
            "message": "Logged out."
        })),
    )
        .into_response()
}
