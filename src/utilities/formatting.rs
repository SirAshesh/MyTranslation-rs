pub fn mask_secret(k: &str) -> String {
    let trimmed = k.trim();
    if trimmed.len() > 8 {
        format!("{}...{}", &trimmed[..4], &trimmed[trimmed.len() - 4..])
    } else if !trimmed.is_empty() {
        "****".to_string()
    } else {
        String::new()
    }
}

pub fn resolve_openai_url(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return "https://api.deepseek.com/chat/completions".to_string();
    }
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") || trimmed.contains("api.deepseek.com") {
        format!("{}/chat/completions", trimmed)
    } else {
        format!("{}/v1/chat/completions", trimmed)
    }
}

pub fn resolve_claude_url(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return "https://api.anthropic.com/v1/messages".to_string();
    }
    if trimmed.ends_with("/messages") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{}/messages", trimmed)
    } else {
        format!("{}/v1/messages", trimmed)
    }
}
