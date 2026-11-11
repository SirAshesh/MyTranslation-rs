use crate::deepseek::DeepSeekClient;
use crate::marian::MarianManager;
use crate::models::Bubble;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub enum Translator {
    DeepSeek(DeepSeekClient),
    Marian(Arc<Mutex<MarianManager>>),
}

impl Translator {
    pub async fn translate_bubbles(
        &self,
        bubbles: &[Bubble],
        source_lang: &str,
        target_lang: &str,
    ) -> Result<HashMap<usize, String>, String> {
        match self {
            Translator::DeepSeek(c) => c.translate_bubbles(bubbles, source_lang, target_lang).await,
            Translator::Marian(mgr) => {
                let guard = mgr.lock().await;
                guard.translate_bubbles(bubbles, source_lang, target_lang).await
            }
        }
    }
}
