use super::StorageDb;
use crate::utilities::metrics::{
    PipelineTimings, RequestMetric, StatsSummaryTotals, TimelineBucket, UsageBreakdown,
};
use rusqlite::params;
use std::collections::{BTreeMap, VecDeque};

const MAX_STORED_EVENTS: usize = 500;
const MAX_STORED_TIMELINE: usize = 120;

#[derive(Default)]
pub struct PersistedStats {
    pub summary: StatsSummaryTotals,
    pub next_seq: usize,
    pub recent: VecDeque<RequestMetric>,
    pub timeline: VecDeque<TimelineBucket>,
    pub by_model: BTreeMap<String, UsageBreakdown>,
    pub by_endpoint: BTreeMap<String, UsageBreakdown>,
}

impl StorageDb {
    pub fn record_stats_event(
        &self,
        endpoint: &str,
        provider_model: &str,
        success: bool,
        timings: &PipelineTimings,
        timestamp_ms: u64,
    ) -> Result<usize, String> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn
            .transaction()
            .map_err(|e| format!("SQLite transaction error: {}", e))?;

        let processed_inc: i64 = if success && endpoint != "text" { 1 } else { 0 };
        let ok_inc: i64 = i64::from(success);
        let err_inc: i64 = i64::from(!success);

        tx.execute(
            "UPDATE stats_summary SET
                total_processed = total_processed + ?1,
                total_requests = total_requests + 1,
                total_success = total_success + ?2,
                total_errors = total_errors + ?3,
                total_bubbles = total_bubbles + ?4,
                ocr_ms_sum = ocr_ms_sum + ?5,
                translate_ms_sum = translate_ms_sum + ?6,
                render_ms_sum = render_ms_sum + ?7,
                total_ms_sum = total_ms_sum + ?8,
                last_total_ms = ?8,
                prompt_tokens_sum = prompt_tokens_sum + ?9,
                prompt_cached_tokens_sum = prompt_cached_tokens_sum + ?10,
                completion_tokens_sum = completion_tokens_sum + ?11,
                local_cache_hits = local_cache_hits + ?12,
                local_cache_misses = local_cache_misses + ?13
             WHERE id = 1",
            params![
                processed_inc,
                ok_inc,
                err_inc,
                timings.bubble_count as i64,
                timings.ocr_ms as i64,
                timings.translate_ms as i64,
                timings.render_ms as i64,
                timings.total_ms as i64,
                timings.usage.prompt_tokens as i64,
                timings.usage.prompt_cached_tokens as i64,
                timings.usage.completion_tokens as i64,
                timings.usage.local_cache_hits as i64,
                timings.usage.local_cache_misses as i64
            ],
        )
        .map_err(|e| format!("Failed to update stats_summary: {}", e))?;

        tx.execute(
            "INSERT INTO stats_events
                (timestamp_ms, endpoint, provider_model, success, bubbles, ocr_ms, translate_ms, render_ms, total_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                timestamp_ms as i64,
                endpoint,
                provider_model,
                ok_inc,
                timings.bubble_count as i64,
                timings.ocr_ms as i64,
                timings.translate_ms as i64,
                timings.render_ms as i64,
                timings.total_ms as i64
            ],
        )
        .map_err(|e| format!("Failed to insert stats_event: {}", e))?;

        let seq = tx.last_insert_rowid() as usize;

        tx.execute(
            "DELETE FROM stats_events WHERE seq NOT IN (SELECT seq FROM stats_events ORDER BY seq DESC LIMIT ?1)",
            params![MAX_STORED_EVENTS as i64],
        )
        .map_err(|e| format!("Failed to trim stats_events: {}", e))?;

        let minute_epoch = (timestamp_ms / 60_000) as i64;
        tx.execute(
            "INSERT INTO stats_timeline (minute_epoch, requests, errors, bubbles, total_ms_sum)
             VALUES (?1, 1, ?2, ?3, ?4)
             ON CONFLICT(minute_epoch) DO UPDATE SET
                requests = requests + 1,
                errors = errors + excluded.errors,
                bubbles = bubbles + excluded.bubbles,
                total_ms_sum = total_ms_sum + excluded.total_ms_sum",
            params![minute_epoch, err_inc, timings.bubble_count as i64, timings.total_ms as i64],
        )
        .map_err(|e| format!("Failed to upsert stats_timeline: {}", e))?;

        tx.execute(
            "DELETE FROM stats_timeline WHERE minute_epoch NOT IN (SELECT minute_epoch FROM stats_timeline ORDER BY minute_epoch DESC LIMIT ?1)",
            params![MAX_STORED_TIMELINE as i64],
        )
        .map_err(|e| format!("Failed to trim stats_timeline: {}", e))?;

        for (kind, name) in [("model", provider_model), ("endpoint", endpoint)] {
            tx.execute(
                "INSERT INTO stats_breakdowns (kind, name, requests, success, errors, bubbles, total_ms_sum)
                 VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)
                 ON CONFLICT(kind, name) DO UPDATE SET
                    requests = requests + 1,
                    success = success + excluded.success,
                    errors = errors + excluded.errors,
                    bubbles = bubbles + excluded.bubbles,
                    total_ms_sum = total_ms_sum + excluded.total_ms_sum",
                params![kind, name, ok_inc, err_inc, timings.bubble_count as i64, timings.total_ms as i64],
            )
            .map_err(|e| format!("Failed to upsert stats_breakdowns: {}", e))?;
        }

        tx.commit()
            .map_err(|e| format!("SQLite commit error: {}", e))?;
        Ok(seq)
    }

    pub fn load_stats(&self, recent_limit: usize, timeline_limit: usize) -> Result<PersistedStats, String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stats = PersistedStats::default();

        stats.summary = conn
            .query_row(
                "SELECT total_processed, total_requests, total_success, total_errors, total_bubbles,
                        ocr_ms_sum, translate_ms_sum, render_ms_sum, total_ms_sum, last_total_ms,
                        prompt_tokens_sum, prompt_cached_tokens_sum, completion_tokens_sum,
                        local_cache_hits, local_cache_misses
                 FROM stats_summary WHERE id = 1",
                [],
                |r| {
                    Ok(StatsSummaryTotals {
                        total_processed: r.get::<_, i64>(0)?.max(0) as usize,
                        total_requests: r.get::<_, i64>(1)?.max(0) as usize,
                        total_success: r.get::<_, i64>(2)?.max(0) as usize,
                        total_errors: r.get::<_, i64>(3)?.max(0) as usize,
                        total_bubbles: r.get::<_, i64>(4)?.max(0) as usize,
                        ocr_ms_sum: r.get::<_, i64>(5)?.max(0) as u64,
                        translate_ms_sum: r.get::<_, i64>(6)?.max(0) as u64,
                        render_ms_sum: r.get::<_, i64>(7)?.max(0) as u64,
                        total_ms_sum: r.get::<_, i64>(8)?.max(0) as u64,
                        last_total_ms: r.get::<_, i64>(9)?.max(0) as u64,
                        prompt_tokens_sum: r.get::<_, i64>(10)?.max(0) as u64,
                        prompt_cached_tokens_sum: r.get::<_, i64>(11)?.max(0) as u64,
                        completion_tokens_sum: r.get::<_, i64>(12)?.max(0) as u64,
                        local_cache_hits: r.get::<_, i64>(13)?.max(0) as u64,
                        local_cache_misses: r.get::<_, i64>(14)?.max(0) as u64,
                    })
                },
            )
            .map_err(|e| format!("Failed to read stats_summary: {}", e))?;

        let mut evt_stmt = conn
            .prepare(
                "SELECT seq, timestamp_ms, endpoint, provider_model, success, bubbles, ocr_ms, translate_ms, render_ms, total_ms
                 FROM stats_events ORDER BY seq DESC LIMIT ?1",
            )
            .map_err(|e| format!("Failed to prepare stats_events query: {}", e))?;
        let mut rev_events: Vec<RequestMetric> = evt_stmt
            .query_map(params![recent_limit as i64], |r| {
                Ok(RequestMetric {
                    seq: r.get::<_, i64>(0)?.max(0) as usize,
                    timestamp_ms: r.get::<_, i64>(1)?.max(0) as u64,
                    endpoint: r.get(2)?,
                    provider_model: r.get(3)?,
                    success: r.get::<_, i64>(4)? != 0,
                    bubbles: r.get::<_, i64>(5)?.max(0) as usize,
                    ocr_ms: r.get::<_, i64>(6)?.max(0) as u64,
                    translate_ms: r.get::<_, i64>(7)?.max(0) as u64,
                    render_ms: r.get::<_, i64>(8)?.max(0) as u64,
                    total_ms: r.get::<_, i64>(9)?.max(0) as u64,
                })
            })
            .map_err(|e| format!("Failed to query stats_events: {}", e))?
            .filter_map(|res| res.ok())
            .collect();
        rev_events.reverse();
        stats.next_seq = rev_events.last().map(|e| e.seq + 1).unwrap_or(1);
        stats.recent = VecDeque::from(rev_events);

        let mut tl_stmt = conn
            .prepare(
                "SELECT minute_epoch, requests, errors, bubbles, total_ms_sum
                 FROM stats_timeline ORDER BY minute_epoch DESC LIMIT ?1",
            )
            .map_err(|e| format!("Failed to prepare stats_timeline query: {}", e))?;
        let mut rev_tl: Vec<TimelineBucket> = tl_stmt
            .query_map(params![timeline_limit as i64], |r| {
                let requests = r.get::<_, i64>(1)?.max(0) as usize;
                let total_ms_sum = r.get::<_, i64>(4)?.max(0) as u64;
                Ok(TimelineBucket {
                    minute_epoch: r.get::<_, i64>(0)?.max(0) as u64,
                    requests,
                    errors: r.get::<_, i64>(2)?.max(0) as usize,
                    bubbles: r.get::<_, i64>(3)?.max(0) as usize,
                    total_ms_sum,
                    avg_ms: total_ms_sum / (requests.max(1) as u64),
                })
            })
            .map_err(|e| format!("Failed to query stats_timeline: {}", e))?
            .filter_map(|res| res.ok())
            .collect();
        rev_tl.reverse();
        stats.timeline = VecDeque::from(rev_tl);

        let mut bd_stmt = conn
            .prepare(
                "SELECT kind, name, requests, success, errors, bubbles, total_ms_sum FROM stats_breakdowns",
            )
            .map_err(|e| format!("Failed to prepare stats_breakdowns query: {}", e))?;
        let rows = bd_stmt
            .query_map([], |r| {
                let kind: String = r.get(0)?;
                let name: String = r.get(1)?;
                let requests = r.get::<_, i64>(2)?.max(0) as usize;
                let total_ms_sum = r.get::<_, i64>(6)?.max(0) as u64;
                Ok((
                    kind,
                    UsageBreakdown {
                        name,
                        requests,
                        success: r.get::<_, i64>(3)?.max(0) as usize,
                        errors: r.get::<_, i64>(4)?.max(0) as usize,
                        bubbles: r.get::<_, i64>(5)?.max(0) as usize,
                        total_ms_sum,
                        avg_ms: total_ms_sum / (requests.max(1) as u64),
                    },
                ))
            })
            .map_err(|e| format!("Failed to query stats_breakdowns: {}", e))?;

        for row in rows.flatten() {
            if row.0 == "model" {
                stats.by_model.insert(row.1.name.clone(), row.1);
            } else if row.0 == "endpoint" {
                stats.by_endpoint.insert(row.1.name.clone(), row.1);
            }
        }

        Ok(stats)
    }

    pub fn reset_stats(&self) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn
            .transaction()
            .map_err(|e| format!("SQLite transaction error: {}", e))?;
        tx.execute_batch(
            "UPDATE stats_summary SET
                total_processed = 0, total_requests = 0, total_success = 0, total_errors = 0,
                total_bubbles = 0, ocr_ms_sum = 0, translate_ms_sum = 0, render_ms_sum = 0,
                total_ms_sum = 0, last_total_ms = 0, prompt_tokens_sum = 0,
                prompt_cached_tokens_sum = 0, completion_tokens_sum = 0,
                local_cache_hits = 0, local_cache_misses = 0
             WHERE id = 1;
             DELETE FROM stats_events;
             DELETE FROM stats_timeline;
             DELETE FROM stats_breakdowns;",
        )
        .map_err(|e| format!("Failed to reset stats in SQLite: {}", e))?;
        tx.commit()
            .map_err(|e| format!("SQLite commit error: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db_path(name: &str) -> PathBuf {
        let pid = std::process::id();
        std::env::temp_dir().join(format!("mt_test_{}_{}.db", name, pid))
    }

    #[test]
    fn single_super_admin_login_session_and_logout_roundtrip() {
        let path = temp_db_path("auth");
        let _ = std::fs::remove_file(&path);
        let db = StorageDb::open(&path).expect("storage db should open");
        db.set_super_admin_credentials("superroot", "S3cretPass!")
            .expect("admin should seed");

        assert!(db.verify_login("superroot", "wrong").expect("query ok").is_none());
        let token = db
            .verify_login("superroot", "S3cretPass!")
            .expect("query ok")
            .expect("login should succeed");
        assert!(db.validate_session(&token));

        db.revoke_session(&token);
        assert!(!db.validate_session(&token));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn first_run_signup_requires_matching_password_and_locks_after_one_user() {
        use crate::utilities::storage::SignupError;

        let path = temp_db_path("signup");
        let _ = std::fs::remove_file(&path);
        let db = StorageDb::open(&path).expect("open db");
        assert!(!db.has_super_admin().expect("query ok"));

        let mismatch = db.create_initial_admin("owner", "secret123", "other123");
        assert!(matches!(mismatch, Err(SignupError::Invalid(_))));

        let token = db
            .create_initial_admin("owner", "secret123", "secret123")
            .expect("initial signup succeeds");
        assert!(db.has_super_admin().expect("admin exists"));
        assert!(db.validate_session(&token));

        let second = db.create_initial_admin("intruder", "password123", "password123");
        assert_eq!(second, Err(SignupError::AlreadyExists));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stats_persist_across_reopen_in_sqlite() {
        let path = temp_db_path("stats");
        let _ = std::fs::remove_file(&path);
        {
            let db = StorageDb::open(&path).expect("open db");
            db.record_stats_event(
                "page",
                "Groq (llama-3.3-70b-versatile)",
                true,
                &PipelineTimings {
                    ocr_ms: 100,
                    translate_ms: 250,
                    render_ms: 80,
                    total_ms: 430,
                    bubble_count: 5,
                    usage: crate::utilities::LlmUsageStats {
                        prompt_tokens: 400,
                        prompt_cached_tokens: 300,
                        completion_tokens: 90,
                        local_cache_hits: 2,
                        local_cache_misses: 3,
                    },
                },
                1_700_000_060_000,
            )
            .expect("record ok");
        }
        let db2 = StorageDb::open(&path).expect("reopen db");
        let loaded = db2.load_stats(24, 20).expect("load stats");
        assert_eq!(loaded.summary.total_requests, 1);
        assert_eq!(loaded.summary.total_bubbles, 5);
        assert_eq!(loaded.summary.prompt_tokens_sum, 400);
        assert_eq!(loaded.summary.prompt_cached_tokens_sum, 300);
        assert_eq!(loaded.summary.local_cache_hits, 2);
        assert_eq!(loaded.recent.len(), 1);
        assert_eq!(loaded.by_model.len(), 1);
        let _ = std::fs::remove_file(&path);
    }
}

