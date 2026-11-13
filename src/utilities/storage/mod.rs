mod stats_store;

pub use stats_store::PersistedStats;

use hmac::Hmac;
use pbkdf2::pbkdf2;
use rand::rngs::OsRng;
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const PBKDF2_ROUNDS: u32 = 100_000;
const SESSION_TTL_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignupError {
    AlreadyExists,
    Invalid(String),
    Internal(String),
}

#[derive(Clone)]
pub struct StorageDb {
    pub(crate) conn: Arc<Mutex<Connection>>,
}

pub fn resolve_storage_path(config_path: &str) -> PathBuf {
    let trimmed = config_path.trim();
    if !trimmed.is_empty() {
        if let Some(parent) = Path::new(trimmed).parent() {
            if !parent.as_os_str().is_empty() {
                return parent.join("storage.db");
            }
        }
    }
    PathBuf::from("storage.db")
}

impl StorageDb {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create storage directory: {}", e))?;
            }
        }
        let conn = Connection::open(path)
            .map_err(|e| format!("Failed to open SQLite storage '{}': {}", path.display(), e))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;

             CREATE TABLE IF NOT EXISTS admin_users (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 username TEXT NOT NULL UNIQUE,
                 password_hash TEXT NOT NULL,
                 updated_at INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS admin_sessions (
                 token_hash TEXT PRIMARY KEY,
                 username TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 expires_at INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS stats_summary (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 total_processed INTEGER NOT NULL DEFAULT 0,
                 total_requests INTEGER NOT NULL DEFAULT 0,
                 total_success INTEGER NOT NULL DEFAULT 0,
                 total_errors INTEGER NOT NULL DEFAULT 0,
                 total_bubbles INTEGER NOT NULL DEFAULT 0,
                 ocr_ms_sum INTEGER NOT NULL DEFAULT 0,
                 translate_ms_sum INTEGER NOT NULL DEFAULT 0,
                 render_ms_sum INTEGER NOT NULL DEFAULT 0,
                 total_ms_sum INTEGER NOT NULL DEFAULT 0,
                 last_total_ms INTEGER NOT NULL DEFAULT 0
             );

             INSERT OR IGNORE INTO stats_summary (id) VALUES (1);

             CREATE TABLE IF NOT EXISTS stats_events (
                 seq INTEGER PRIMARY KEY AUTOINCREMENT,
                 timestamp_ms INTEGER NOT NULL,
                 endpoint TEXT NOT NULL,
                 provider_model TEXT NOT NULL,
                 success INTEGER NOT NULL,
                 bubbles INTEGER NOT NULL,
                 ocr_ms INTEGER NOT NULL,
                 translate_ms INTEGER NOT NULL,
                 render_ms INTEGER NOT NULL,
                 total_ms INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS stats_timeline (
                 minute_epoch INTEGER PRIMARY KEY,
                 requests INTEGER NOT NULL,
                 errors INTEGER NOT NULL,
                 bubbles INTEGER NOT NULL,
                 total_ms_sum INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS stats_breakdowns (
                 kind TEXT NOT NULL,
                 name TEXT NOT NULL,
                 requests INTEGER NOT NULL,
                 success INTEGER NOT NULL,
                 errors INTEGER NOT NULL,
                 bubbles INTEGER NOT NULL,
                 total_ms_sum INTEGER NOT NULL,
                 PRIMARY KEY (kind, name)
             );",
        )
        .map_err(|e| format!("Failed to initialize SQLite schema: {}", e))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn has_super_admin(&self) -> Result<bool, String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let existing: Option<i64> = conn
            .query_row("SELECT id FROM admin_users WHERE id = 1", [], |r| r.get(0))
            .optional()
            .map_err(|e| format!("Failed to query super admin state: {}", e))?;
        Ok(existing.is_some())
    }

    pub fn ensure_super_admin_from_env(&self) -> Result<bool, String> {
        let Ok(env_user) = std::env::var("MYTRANSLATION_ADMIN_USER") else {
            return Ok(false);
        };
        let Ok(env_pass) = std::env::var("MYTRANSLATION_ADMIN_PASS") else {
            return Ok(false);
        };
        let clean_user = env_user.trim().to_string();
        if clean_user.is_empty() || env_pass.is_empty() {
            return Ok(false);
        }

        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let existing: Option<i64> = conn
            .query_row("SELECT id FROM admin_users WHERE id = 1", [], |r| r.get(0))
            .optional()
            .map_err(|e| format!("Failed to query super admin user: {}", e))?;
        if existing.is_some() {
            return Ok(false);
        }

        let hash = hash_password(&env_pass)?;
        let now = current_unix_secs() as i64;
        conn.execute(
            "INSERT INTO admin_users (id, username, password_hash, updated_at) VALUES (1, ?1, ?2, ?3)",
            params![clean_user, hash, now],
        )
        .map_err(|e| format!("Failed to seed super admin user: {}", e))?;
        Ok(true)
    }

    pub fn create_initial_admin(
        &self,
        username: &str,
        password: &str,
        verify_password_input: &str,
    ) -> Result<String, SignupError> {
        let clean_user = username.trim();
        if clean_user.len() < 3 {
            return Err(SignupError::Invalid(
                "Username must be at least 3 characters.".to_string(),
            ));
        }
        if password.len() < 6 {
            return Err(SignupError::Invalid(
                "Password must be at least 6 characters.".to_string(),
            ));
        }
        if !constant_time_eq(password.as_bytes(), verify_password_input.as_bytes()) {
            return Err(SignupError::Invalid(
                "Password and verify password do not match.".to_string(),
            ));
        }

        let hash = hash_password(password).map_err(SignupError::Internal)?;
        let now = current_unix_secs() as i64;
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn
            .transaction()
            .map_err(|e| SignupError::Internal(format!("SQLite transaction error: {}", e)))?;

        let existing: Option<i64> = tx
            .query_row("SELECT id FROM admin_users WHERE id = 1", [], |r| r.get(0))
            .optional()
            .map_err(|e| SignupError::Internal(format!("Failed to check account state: {}", e)))?;
        if existing.is_some() {
            return Err(SignupError::AlreadyExists);
        }

        tx.execute(
            "INSERT INTO admin_users (id, username, password_hash, updated_at) VALUES (1, ?1, ?2, ?3)",
            params![clean_user, hash, now],
        )
        .map_err(|e| SignupError::Internal(format!("Failed to create account: {}", e)))?;

        let token = issue_session_in_conn(&tx, clean_user).map_err(SignupError::Internal)?;
        tx.commit()
            .map_err(|e| SignupError::Internal(format!("SQLite commit error: {}", e)))?;
        Ok(token)
    }

    pub fn set_super_admin_credentials(&self, username: &str, password: &str) -> Result<(), String> {
        let clean_user = username.trim();
        if clean_user.is_empty() || password.is_empty() {
            return Err("Username and password cannot be empty.".to_string());
        }
        let hash = hash_password(password)?;
        let now = current_unix_secs() as i64;
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn
            .transaction()
            .map_err(|e| format!("SQLite transaction error: {}", e))?;
        tx.execute(
            "INSERT INTO admin_users (id, username, password_hash, updated_at) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET username = excluded.username, password_hash = excluded.password_hash, updated_at = excluded.updated_at",
            params![clean_user, hash, now],
        )
        .map_err(|e| format!("Failed to update super admin user: {}", e))?;
        tx.execute("DELETE FROM admin_sessions", [])
            .map_err(|e| format!("Failed to clear old sessions: {}", e))?;
        tx.commit()
            .map_err(|e| format!("SQLite commit error: {}", e))?;
        Ok(())
    }

    pub fn verify_login(&self, username: &str, password: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT username, password_hash FROM admin_users WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("Failed to query admin credentials: {}", e))?;

        let Some((stored_user, stored_hash)) = row else {
            return Ok(None);
        };
        if !constant_time_eq(username.trim().as_bytes(), stored_user.as_bytes()) {
            return Ok(None);
        }
        if !verify_password(password, &stored_hash) {
            return Ok(None);
        }

        let token = issue_session_in_conn(&conn, &stored_user)?;
        Ok(Some(token))
    }

    pub fn validate_session(&self, raw_token: &str) -> bool {
        let trimmed = raw_token.trim();
        if trimmed.is_empty() {
            return false;
        }
        let token_hash = sha256_hex(trimmed.as_bytes());
        let now = current_unix_secs() as i64;
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let expires: Option<i64> = conn
            .query_row(
                "SELECT expires_at FROM admin_sessions WHERE token_hash = ?1",
                params![token_hash],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or(None);

        match expires {
            Some(exp) if exp >= now => true,
            Some(_) => {
                let _ = conn.execute(
                    "DELETE FROM admin_sessions WHERE token_hash = ?1",
                    params![token_hash],
                );
                false
            }
            None => false,
        }
    }

    pub fn revoke_session(&self, raw_token: &str) {
        let trimmed = raw_token.trim();
        if trimmed.is_empty() {
            return;
        }
        let token_hash = sha256_hex(trimmed.as_bytes());
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let _ = conn.execute(
            "DELETE FROM admin_sessions WHERE token_hash = ?1",
            params![token_hash],
        );
    }
}

fn issue_session_in_conn(conn: &Connection, username: &str) -> Result<String, String> {
    let token = generate_session_token();
    let token_hash = sha256_hex(token.as_bytes());
    let now = current_unix_secs();
    let expires_at = (now + SESSION_TTL_SECS) as i64;

    let _ = conn.execute(
        "DELETE FROM admin_sessions WHERE expires_at < ?1",
        params![now as i64],
    );
    conn.execute(
        "INSERT INTO admin_sessions (token_hash, username, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        params![token_hash, username, now as i64, expires_at],
    )
    .map_err(|e| format!("Failed to store session: {}", e))?;
    Ok(token)
}

fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill(&mut bytes);
    bytes_to_hex(&bytes)
}

fn hash_password(password: &str) -> Result<String, String> {
    let mut salt = [0u8; 16];
    OsRng.fill(&mut salt);
    let mut out = [0u8; 32];
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, PBKDF2_ROUNDS, &mut out)
        .map_err(|e| format!("Password hashing failed: {}", e))?;
    Ok(format!(
        "pbkdf2${}${}${}",
        PBKDF2_ROUNDS,
        bytes_to_hex(&salt),
        bytes_to_hex(&out)
    ))
}

fn verify_password(password: &str, encoded: &str) -> bool {
    let parts: Vec<&str> = encoded.split('$').collect();
    if parts.len() != 4 || parts[0] != "pbkdf2" {
        return false;
    }
    let Ok(rounds) = parts[1].parse::<u32>() else {
        return false;
    };
    let Some(salt) = hex_to_bytes(parts[2]) else {
        return false;
    };
    let Some(expected) = hex_to_bytes(parts[3]) else {
        return false;
    };
    let mut actual = vec![0u8; expected.len()];
    if pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, rounds, &mut actual).is_err() {
        return false;
    }
    constant_time_eq(&actual, &expected)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    bytes_to_hex(&hasher.finalize())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16).ok()?;
        out.push(byte);
    }
    Some(out)
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

