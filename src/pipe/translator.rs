use crate::model::{AppConfig, Bubble, SaveSettingsForm, TestKeyRequest};
use super::llm_client::{LlmClient, LlmTranslationOutput};
use super::local_runtime::LocalRuntime;
use super::prompts::{
    normalize_bubble_text, resolve_source_language_label, resolve_target_language_label,
};
use crate::utilities::LlmUsageStats;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

const MAX_TRANSLATION_CACHE_ENTRIES: usize = 2048;

#[derive(Default)]
struct TranslationCache {
    entries: HashMap<String, String>,
    order: VecDeque<String>,
}

impl TranslationCache {
    fn get(&self, key: &str) -> Option<&String> {
        self.entries.get(key)
    }

    fn insert(&mut self, key: String, value: String) {
        if self.entries.contains_key(&key) {
            self.entries.insert(key, value);
            return;
        }
        if self.order.len() >= MAX_TRANSLATION_CACHE_ENTRIES {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
    }
}

fn is_non_lexical_bubble(text: &str) -> bool {
    !text.chars().any(|c| c.is_alphanumeric())
}

fn cache_namespace(cfg: &AppConfig, source_lang: &str, target_lang: &str) -> String {
    let model = match cfg.translation_mode.as_str() {
        "claude" => cfg.claude_model.trim(),
        "local" => cfg.local_model.trim(),
        #[cfg(feature = "free-mode")]
        "free" => cfg.free.free_provider.trim(),
        _ => cfg.openai_model.trim(),
    };
    format!(
        "{}:{}:{}:{}",
        cfg.translation_mode.trim(),
        model,
        resolve_source_language_label(source_lang),
        resolve_target_language_label(target_lang)
    )
}

#[derive(Clone)]
pub struct Translator {
    client: LlmClient,
    config: Arc<RwLock<AppConfig>>,
    pub runtime: LocalRuntime,
    translation_cache: Arc<Mutex<TranslationCache>>,
    #[cfg(feature = "free-mode")]
    free_cache: crate::pipe::free_mode::FreeRouterCache,
}

impl Translator {
    pub fn new(initial_config: AppConfig, local_data_dir: PathBuf) -> Self {
        Self {
            client: LlmClient::new(),
            config: Arc::new(RwLock::new(initial_config)),
            runtime: LocalRuntime::new(local_data_dir),
            translation_cache: Arc::new(Mutex::new(TranslationCache::default())),
            #[cfg(feature = "free-mode")]
            free_cache: crate::pipe::free_mode::FreeRouterCache::new(),
        }
    }

    pub async fn get_config(&self) -> AppConfig {
        self.config.read().await.clone()
    }

    pub async fn apply_settings(&self, form: &SaveSettingsForm) -> AppConfig {
        let mut guard = self.config.write().await;
        let cfg = &mut *guard;
        let mode = form.translation_mode.trim().to_lowercase();
        let is_valid_mode = matches!(mode.as_str(), "openai" | "claude" | "local")
            || (cfg!(feature = "free-mode") && mode == "free");
        if is_valid_mode {
            cfg.translation_mode = mode;
        }

        let text_settings = [
            (&form.openai_base_url, &mut cfg.openai_base_url),
            (&form.openai_model, &mut cfg.openai_model),
            (&form.deepseek_api_key, &mut cfg.deepseek_api_key),
            (&form.claude_base_url, &mut cfg.claude_base_url),
            (&form.claude_model, &mut cfg.claude_model),
            (&form.claude_api_key, &mut cfg.claude_api_key),
            (&form.local_model, &mut cfg.local_model),
        ];
        for (submitted, stored) in text_settings {
            if !submitted.trim().is_empty() {
                *stored = submitted.trim().to_string();
            }
        }
        #[cfg(feature = "free-mode")]
        cfg.free.apply_form(&form.free);

        cfg.clone()
    }

    pub async fn is_active_configured(&self) -> bool {
        let cfg = self.get_config().await;
        match cfg.translation_mode.as_str() {
            "claude" => !cfg.claude_api_key.trim().is_empty(),
            "local" => self.runtime.is_running().await,
            #[cfg(feature = "free-mode")]
            "free" => cfg.free.is_configured(),
            _ => !cfg.deepseek_api_key.trim().is_empty(),
        }
    }

    pub async fn test_connection(&self, req: &TestKeyRequest) -> Result<String, String> {
        let mut temp_cfg = self.get_config().await;

        let overrides = [
            (&req.translation_mode, &mut temp_cfg.translation_mode),
            (&req.openai_base_url, &mut temp_cfg.openai_base_url),
            (&req.openai_model, &mut temp_cfg.openai_model),
            (&req.api_key, &mut temp_cfg.deepseek_api_key),
            (&req.claude_base_url, &mut temp_cfg.claude_base_url),
            (&req.claude_model, &mut temp_cfg.claude_model),
            (&req.claude_api_key, &mut temp_cfg.claude_api_key),
            (&req.local_model, &mut temp_cfg.local_model),
        ];
        for (submitted, stored) in overrides {
            if !submitted.trim().is_empty() {
                *stored = submitted.trim().to_string();
            }
        }
        #[cfg(feature = "free-mode")]
        temp_cfg.free.apply_form(&req.free);
        temp_cfg.translation_mode = temp_cfg.translation_mode.to_lowercase();

        match temp_cfg.translation_mode.as_str() {
            "claude" => self.client.test_connection_claude(&temp_cfg).await,
            #[cfg(feature = "free-mode")]
            "free" => {
                crate::pipe::free_mode::verify_free_connection(
                    &self.client,
                    &self.free_cache,
                    &temp_cfg.free,
                )
                .await
            }
            _ => self.client.test_connection_chat(&temp_cfg).await,
        }
    }

    #[cfg(feature = "free-mode")]
    pub async fn free_active_summary(&self, cfg: &AppConfig) -> Option<String> {
        if cfg.translation_mode == "free" {
            self.free_cache.current_winner_label(&cfg.free).await
        } else {
            None
        }
    }

    pub async fn active_provider_label(&self) -> String {
        let cfg = self.get_config().await;
        match cfg.translation_mode.as_str() {
            "claude" => format!("Claude ({})", cfg.claude_model),
            "local" => format!("Local ({})", cfg.local_model),
            #[cfg(feature = "free-mode")]
            "free" => self
                .free_active_summary(&cfg)
                .await
                .unwrap_or_else(|| format!("Free ({})", cfg.free.free_provider)),
            _ => format!("OpenAI ({})", cfg.openai_model),
        }
    }

    #[cfg(feature = "free-mode")]
    pub async fn fetch_free_models(
        &self,
        provider: &str,
        submitted_key: &str,
    ) -> Result<(Vec<String>, usize), String> {
        let cfg = self.get_config().await;
        let effective_key = if submitted_key.trim().is_empty() {
            cfg.free.stored_key_for(provider).to_string()
        } else {
            submitted_key.trim().to_string()
        };
        crate::pipe::free_mode::fetch_provider_models(
            self.client.http_client(),
            &self.free_cache,
            provider,
            &effective_key,
        )
        .await
    }

    pub async fn translate_bubbles(
        &self,
        bubbles: &[Bubble],
        source_lang: &str,
        target_lang: &str,
    ) -> Result<LlmTranslationOutput, String> {
        let cfg = self.get_config().await;
        let namespace = cache_namespace(&cfg, source_lang, target_lang);
        let mut resolved = HashMap::with_capacity(bubbles.len());
        let mut uncached = Vec::new();
        let mut uncached_keys = Vec::new();
        let mut local_hits: u64 = 0;

        {
            let cache_guard = self
                .translation_cache
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for bubble in bubbles {
                let normalized = normalize_bubble_text(&bubble.source_text);
                if normalized.is_empty() || is_non_lexical_bubble(&normalized) {
                    resolved.insert(bubble.id, bubble.source_text.clone());
                    local_hits += 1;
                    continue;
                }
                let cache_key = format!("{}|{}", namespace, normalized);
                if let Some(cached) = cache_guard.get(&cache_key) {
                    resolved.insert(bubble.id, cached.clone());
                    local_hits += 1;
                } else {
                    uncached.push(bubble.clone());
                    uncached_keys.push((bubble.id, cache_key));
                }
            }
        }

        let local_misses = uncached.len() as u64;
        if uncached.is_empty() {
            return Ok(LlmTranslationOutput {
                translations: resolved,
                usage: LlmUsageStats {
                    local_cache_hits: local_hits,
                    local_cache_misses: 0,
                    ..LlmUsageStats::default()
                },
            });
        }

        let mut llm_output = match cfg.translation_mode.as_str() {
            "claude" => {
                self.client
                    .translate_claude(&cfg, &uncached, source_lang, target_lang)
                    .await?
            }
            #[cfg(feature = "free-mode")]
            "free" => {
                crate::pipe::free_mode::translate_free(
                    &self.client,
                    &self.free_cache,
                    &cfg.free,
                    &uncached,
                    source_lang,
                    target_lang,
                )
                .await?
            }
            _ => {
                self.client
                    .translate_chat(&cfg, &uncached, source_lang, target_lang)
                    .await?
            }
        };

        {
            let mut cache_guard = self
                .translation_cache
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for (bubble_id, cache_key) in uncached_keys {
                if let Some(translated) = llm_output.translations.remove(&bubble_id) {
                    let trimmed = translated.trim();
                    if !trimmed.is_empty() {
                        cache_guard.insert(cache_key, translated.clone());
                    }
                    resolved.insert(bubble_id, translated);
                }
            }
        }

        llm_output.usage.local_cache_hits = local_hits;
        llm_output.usage.local_cache_misses = local_misses;

        Ok(LlmTranslationOutput {
            translations: resolved,
            usage: llm_output.usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_lexical_and_cache_eviction_work_cleanly() {
        assert!(is_non_lexical_bubble("..."));
        assert!(is_non_lexical_bubble("?!"));
        assert!(!is_non_lexical_bubble("Wait?!"));

        let mut cache = TranslationCache::default();
        cache.insert("k1".to_string(), "v1".to_string());
        assert_eq!(cache.get("k1").map(String::as_str), Some("v1"));
    }
}
