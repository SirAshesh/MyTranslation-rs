use crate::utilities::storage::StorageDb;
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::warn;

const MAX_RECENT_EVENTS: usize = 24;
const MAX_TIMELINE_BUCKETS: usize = 20;

#[derive(Clone, Debug, Default, Serialize)]
pub struct PipelineTimings {
    pub ocr_ms: u64,
    pub translate_ms: u64,
    pub render_ms: u64,
    pub total_ms: u64,
    pub bubble_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct RequestMetric {
    pub seq: usize,
    pub timestamp_ms: u64,
    pub endpoint: String,
    pub provider_model: String,
    pub success: bool,
    pub bubbles: usize,
    pub ocr_ms: u64,
    pub translate_ms: u64,
    pub render_ms: u64,
    pub total_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TimelineBucket {
    pub minute_epoch: u64,
    pub requests: usize,
    pub errors: usize,
    pub bubbles: usize,
    #[serde(skip_serializing)]
    pub total_ms_sum: u64,
    pub avg_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsageBreakdown {
    pub name: String,
    pub requests: usize,
    pub success: usize,
    pub errors: usize,
    pub bubbles: usize,
    #[serde(skip_serializing)]
    pub total_ms_sum: u64,
    pub avg_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct StageAverages {
    pub ocr_ms: u64,
    pub translate_ms: u64,
    pub render_ms: u64,
    pub total_ms: u64,
}

#[derive(Clone, Debug, Default)]
pub struct StatsSummaryTotals {
    pub total_processed: usize,
    pub total_requests: usize,
    pub total_success: usize,
    pub total_errors: usize,
    pub total_bubbles: usize,
    pub ocr_ms_sum: u64,
    pub translate_ms_sum: u64,
    pub render_ms_sum: u64,
    pub total_ms_sum: u64,
    pub last_total_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatsSnapshot {
    pub status: &'static str,
    pub total_processed: usize,
    pub engine: &'static str,
    pub translation_mode: String,
    pub api_key_configured: bool,
    pub uptime_secs: u64,
    pub total_requests: usize,
    pub total_success: usize,
    pub total_errors: usize,
    pub total_bubbles: usize,
    pub avg_total_ms: u64,
    pub last_total_ms: u64,
    pub stage_averages: StageAverages,
    pub recent_requests: Vec<RequestMetric>,
    pub timeline: Vec<TimelineBucket>,
    pub by_model: Vec<UsageBreakdown>,
    pub by_endpoint: Vec<UsageBreakdown>,
}

struct StatsInner {
    started_at: Instant,
    next_seq: usize,
    summary: StatsSummaryTotals,
    recent: VecDeque<RequestMetric>,
    timeline: VecDeque<TimelineBucket>,
    by_model: BTreeMap<String, UsageBreakdown>,
    by_endpoint: BTreeMap<String, UsageBreakdown>,
}

impl Default for StatsInner {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            next_seq: 1,
            summary: StatsSummaryTotals::default(),
            recent: VecDeque::with_capacity(MAX_RECENT_EVENTS),
            timeline: VecDeque::with_capacity(MAX_TIMELINE_BUCKETS),
            by_model: BTreeMap::new(),
            by_endpoint: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Default)]
pub struct StatsTracker {
    inner: Arc<Mutex<StatsInner>>,
    storage: Option<StorageDb>,
}

impl StatsTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_storage(storage: StorageDb) -> Self {
        let mut inner = StatsInner::default();
        match storage.load_stats(MAX_RECENT_EVENTS, MAX_TIMELINE_BUCKETS) {
            Ok(persisted) => {
                inner.next_seq = persisted.next_seq;
                inner.summary = persisted.summary;
                inner.recent = persisted.recent;
                inner.timeline = persisted.timeline;
                inner.by_model = persisted.by_model;
                inner.by_endpoint = persisted.by_endpoint;
            }
            Err(err) => {
                warn!("Failed to load persisted statistics from SQLite: {}", err);
            }
        }
        Self {
            inner: Arc::new(Mutex::new(inner)),
            storage: Some(storage),
        }
    }

    pub fn record(&self, endpoint: &str, provider_model: &str, success: bool, timings: PipelineTimings) {
        let now_ms = current_unix_ms();
        self.record_at(endpoint, provider_model, success, timings, now_ms);
    }

    pub fn record_at(
        &self,
        endpoint: &str,
        provider_model: &str,
        success: bool,
        timings: PipelineTimings,
        timestamp_ms: u64,
    ) {
        let persisted_seq = self.storage.as_ref().and_then(|db| {
            match db.record_stats_event(endpoint, provider_model, success, &timings, timestamp_ms) {
                Ok(seq) => Some(seq),
                Err(err) => {
                    warn!("Failed to persist stat event to SQLite: {}", err);
                    None
                }
            }
        });

        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let seq = persisted_seq.unwrap_or(guard.next_seq);
        guard.next_seq = seq + 1;

        guard.summary.total_requests += 1;
        if success {
            guard.summary.total_success += 1;
            if endpoint != "text" {
                guard.summary.total_processed += 1;
            }
        } else {
            guard.summary.total_errors += 1;
        }

        guard.summary.total_bubbles += timings.bubble_count;
        guard.summary.ocr_ms_sum += timings.ocr_ms;
        guard.summary.translate_ms_sum += timings.translate_ms;
        guard.summary.render_ms_sum += timings.render_ms;
        guard.summary.total_ms_sum += timings.total_ms;
        guard.summary.last_total_ms = timings.total_ms;

        if guard.recent.len() >= MAX_RECENT_EVENTS {
            guard.recent.pop_front();
        }
        guard.recent.push_back(RequestMetric {
            seq,
            timestamp_ms,
            endpoint: endpoint.to_string(),
            provider_model: provider_model.to_string(),
            success,
            bubbles: timings.bubble_count,
            ocr_ms: timings.ocr_ms,
            translate_ms: timings.translate_ms,
            render_ms: timings.render_ms,
            total_ms: timings.total_ms,
        });

        let minute_epoch = timestamp_ms / 60_000;
        if let Some(last) = guard.timeline.back_mut().filter(|b| b.minute_epoch == minute_epoch) {
            last.requests += 1;
            if !success {
                last.errors += 1;
            }
            last.bubbles += timings.bubble_count;
            last.total_ms_sum += timings.total_ms;
            last.avg_ms = last.total_ms_sum / (last.requests.max(1) as u64);
        } else {
            if guard.timeline.len() >= MAX_TIMELINE_BUCKETS {
                guard.timeline.pop_front();
            }
            guard.timeline.push_back(TimelineBucket {
                minute_epoch,
                requests: 1,
                errors: usize::from(!success),
                bubbles: timings.bubble_count,
                total_ms_sum: timings.total_ms,
                avg_ms: timings.total_ms,
            });
        }

        upsert_breakdown(
            &mut guard.by_model,
            provider_model,
            success,
            timings.bubble_count,
            timings.total_ms,
        );
        upsert_breakdown(
            &mut guard.by_endpoint,
            endpoint,
            success,
            timings.bubble_count,
            timings.total_ms,
        );
    }

    pub fn reset(&self) {
        if let Some(db) = &self.storage {
            if let Err(err) = db.reset_stats() {
                warn!("Failed to reset statistics in SQLite: {}", err);
            }
        }
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let started_at = guard.started_at;
        *guard = StatsInner {
            started_at,
            ..StatsInner::default()
        };
    }

    pub fn snapshot(&self, translation_mode: &str, api_key_configured: bool) -> StatsSnapshot {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let s = &guard.summary;
        let denom = s.total_requests.max(1) as u64;

        let stage_averages = if s.total_requests == 0 {
            StageAverages::default()
        } else {
            StageAverages {
                ocr_ms: s.ocr_ms_sum / denom,
                translate_ms: s.translate_ms_sum / denom,
                render_ms: s.render_ms_sum / denom,
                total_ms: s.total_ms_sum / denom,
            }
        };

        StatsSnapshot {
            status: "online",
            total_processed: s.total_processed,
            engine: "RapidOCR DirectML",
            translation_mode: translation_mode.to_string(),
            api_key_configured,
            uptime_secs: guard.started_at.elapsed().as_secs(),
            total_requests: s.total_requests,
            total_success: s.total_success,
            total_errors: s.total_errors,
            total_bubbles: s.total_bubbles,
            avg_total_ms: stage_averages.total_ms,
            last_total_ms: s.last_total_ms,
            stage_averages,
            recent_requests: guard.recent.iter().cloned().collect(),
            timeline: guard.timeline.iter().cloned().collect(),
            by_model: sort_breakdown(&guard.by_model),
            by_endpoint: sort_breakdown(&guard.by_endpoint),
        }
    }
}

fn upsert_breakdown(
    map: &mut BTreeMap<String, UsageBreakdown>,
    name: &str,
    success: bool,
    bubbles: usize,
    total_ms: u64,
) {
    let entry = map.entry(name.to_string()).or_insert_with(|| UsageBreakdown {
        name: name.to_string(),
        requests: 0,
        success: 0,
        errors: 0,
        bubbles: 0,
        total_ms_sum: 0,
        avg_ms: 0,
    });
    entry.requests += 1;
    if success {
        entry.success += 1;
    } else {
        entry.errors += 1;
    }
    entry.bubbles += bubbles;
    entry.total_ms_sum += total_ms;
    entry.avg_ms = entry.total_ms_sum / (entry.requests.max(1) as u64);
}

fn sort_breakdown(map: &BTreeMap<String, UsageBreakdown>) -> Vec<UsageBreakdown> {
    let mut items: Vec<UsageBreakdown> = map.values().cloned().collect();
    items.sort_by(|a, b| b.requests.cmp(&a.requests).then_with(|| a.name.cmp(&b.name)));
    items
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_timings_breakdowns_and_resets_cleanly() {
        let tracker = StatsTracker::new();
        tracker.record_at(
            "page",
            "Groq (llama-3.3-70b-versatile)",
            true,
            PipelineTimings {
                ocr_ms: 120,
                translate_ms: 380,
                render_ms: 100,
                total_ms: 600,
                bubble_count: 4,
            },
            1_700_000_040_000,
        );
        tracker.record_at(
            "text",
            "OpenAI (deepseek-chat)",
            false,
            PipelineTimings {
                ocr_ms: 0,
                translate_ms: 200,
                render_ms: 0,
                total_ms: 200,
                bubble_count: 1,
            },
            1_700_000_050_000,
        );

        let snap = tracker.snapshot("free", true);
        assert_eq!(snap.total_processed, 1);
        assert_eq!(snap.total_requests, 2);
        assert_eq!(snap.total_success, 1);
        assert_eq!(snap.total_errors, 1);
        assert_eq!(snap.total_bubbles, 5);
        assert_eq!(snap.avg_total_ms, 400);
        assert_eq!(snap.stage_averages.ocr_ms, 60);
        assert_eq!(snap.timeline.len(), 1);
        assert_eq!(snap.timeline[0].requests, 2);
        assert_eq!(snap.timeline[0].errors, 1);
        assert_eq!(snap.by_model.len(), 2);

        tracker.reset();
        let after = tracker.snapshot("free", true);
        assert_eq!(after.total_requests, 0);
        assert!(after.recent_requests.is_empty());
    }
}
