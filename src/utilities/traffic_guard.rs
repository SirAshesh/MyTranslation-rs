use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

const MAX_TRACKED_IPS: usize = 4096;

#[derive(Default)]
struct QueueState {
    active: usize,
    max_concurrent: usize,
    waiters: VecDeque<oneshot::Sender<()>>,
}

impl QueueState {
    fn wake_waiters(&mut self) {
        while self.active < self.max_concurrent {
            let Some(tx) = self.waiters.pop_front() else {
                break;
            };
            if tx.send(()).is_ok() {
                self.active += 1;
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct ImageQueue {
    inner: Arc<Mutex<QueueState>>,
}

pub struct QueuePermit {
    inner: Arc<Mutex<QueueState>>,
}

impl Drop for QueuePermit {
    fn drop(&mut self) {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        state.active = state.active.saturating_sub(1);
        state.wake_waiters();
    }
}

impl ImageQueue {
    pub async fn acquire(&self, max_concurrent: usize) -> QueuePermit {
        let limit = max_concurrent.max(1);
        let rx_opt = {
            let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            state.max_concurrent = limit;
            state.wake_waiters();

            if state.active < limit && state.waiters.is_empty() {
                state.active += 1;
                None
            } else {
                let (tx, rx) = oneshot::channel();
                state.waiters.push_back(tx);
                Some(rx)
            }
        };

        if let Some(rx) = rx_opt {
            if rx.await.is_err() {
                let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                state.active += 1;
            }
        }

        QueuePermit {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Clone, Default)]
pub struct IpRateLimiter {
    buckets: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
}

impl IpRateLimiter {
    pub fn check_and_record(
        &self,
        client_ip: &str,
        image_count: usize,
        max_images: usize,
        window_secs: u64,
    ) -> Result<(), u64> {
        let max_allowed = max_images.max(1);
        let window_duration = Duration::from_secs(window_secs.max(1));
        if image_count > max_allowed {
            return Err(window_secs.max(1));
        }

        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() > MAX_TRACKED_IPS {
            buckets.retain(|_, entries| {
                while let Some(&front) = entries.front() {
                    if now.duration_since(front) >= window_duration {
                        entries.pop_front();
                    } else {
                        break;
                    }
                }
                !entries.is_empty()
            });
        }

        let entries = buckets.entry(client_ip.to_string()).or_default();
        while let Some(&front) = entries.front() {
            if now.duration_since(front) >= window_duration {
                entries.pop_front();
            } else {
                break;
            }
        }

        if entries.len().saturating_add(image_count) > max_allowed {
            let retry_after = entries
                .front()
                .map(|&oldest| {
                    window_duration
                        .saturating_sub(now.duration_since(oldest))
                        .as_secs()
                        .max(1)
                })
                .unwrap_or_else(|| window_secs.max(1));
            return Err(retry_after);
        }

        for _ in 0..image_count {
            entries.push_back(now);
        }
        Ok(())
    }

    pub fn reset(&self, client_ip: &str) {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        buckets.remove(client_ip);
    }
}

fn parse_header_ip(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(name)?.to_str().ok()?;
    let candidate = raw.split(',').next()?.trim();
    candidate.parse::<IpAddr>().ok().map(|ip| ip.to_string())
}

pub fn extract_client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    for header_name in ["cf-connecting-ip", "x-real-ip", "x-forwarded-for"] {
        if let Some(ip) = parse_header_ip(headers, header_name) {
            return ip;
        }
    }
    peer.map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub struct ClientIp(pub String);

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0);
        Ok(Self(extract_client_ip(&parts.headers, peer)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn rate_limiter_enforces_ten_images_per_window_per_ip() {
        let limiter = IpRateLimiter::default();
        for _ in 0..10 {
            assert!(limiter.check_and_record("1.2.3.4", 1, 10, 10).is_ok());
        }
        assert!(limiter.check_and_record("1.2.3.4", 1, 10, 10).is_err());
        assert!(limiter.check_and_record("5.6.7.8", 1, 10, 10).is_ok());
    }

    #[test]
    fn rate_limiter_counts_batch_images_against_quota() {
        let limiter = IpRateLimiter::default();
        assert!(limiter.check_and_record("1.2.3.4", 6, 10, 10).is_ok());
        assert!(limiter.check_and_record("1.2.3.4", 5, 10, 10).is_err());
        assert!(limiter.check_and_record("1.2.3.4", 4, 10, 10).is_ok());
    }

    #[tokio::test]
    async fn image_queue_bounds_concurrency_and_wakes_waiters() {
        let queue = ImageQueue::default();
        let active_count = Arc::new(AtomicUsize::new(0));
        let max_observed = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..6 {
            let q = queue.clone();
            let active = active_count.clone();
            let max_obs = max_observed.clone();
            handles.push(tokio::spawn(async move {
                let _permit = q.acquire(2).await;
                let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_obs.fetch_max(now_active, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }

        for h in handles {
            let _ = h.await;
        }
        assert_eq!(max_observed.load(Ordering::SeqCst), 2);
        assert_eq!(active_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn extract_client_ip_prefers_valid_proxy_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        let peer = Some("127.0.0.1:5000".parse().unwrap());
        assert_eq!(extract_client_ip(&headers, peer), "203.0.113.9");

        headers.insert("cf-connecting-ip", "198.51.100.2".parse().unwrap());
        assert_eq!(extract_client_ip(&headers, peer), "198.51.100.2");
    }
}
