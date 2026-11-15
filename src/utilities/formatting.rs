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

pub fn redact_secrets(text: &str, secrets: &[&str]) -> String {
    let mut sanitized = text.to_string();
    for secret in secrets {
        let trimmed = secret.trim();
        if trimmed.len() >= 6 {
            sanitized = sanitized.replace(trimmed, "[REDACTED]");
        }
    }
    sanitized
}

pub fn resolve_openai_url(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return "https://api.deepseek.com/chat/completions".to_string();
    }
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1")
        || trimmed.ends_with("/openai")
        || trimmed.contains("api.deepseek.com")
    {
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

fn replace_endpoint_suffix(resolved_url: &str, suffix: &str, replacement: &str) -> String {
    match resolved_url.strip_suffix(suffix) {
        Some(prefix) => format!("{}{}", prefix, replacement),
        None => format!("{}{}", resolved_url.trim_end_matches('/'), replacement),
    }
}

pub fn resolve_openai_models_url(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    if trimmed.ends_with("/models") {
        return trimmed.to_string();
    }
    replace_endpoint_suffix(&resolve_openai_url(trimmed), "/chat/completions", "/models")
}

pub fn resolve_claude_models_url(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    if trimmed.ends_with("/models") {
        return trimmed.to_string();
    }
    replace_endpoint_suffix(&resolve_claude_url(trimmed), "/messages", "/models")
}

pub fn is_loopback_url(url: &str) -> bool {
    let without_scheme = url
        .trim()
        .strip_prefix("http://")
        .or_else(|| url.trim().strip_prefix("https://"))
        .unwrap_or(url.trim());
    let authority = without_scheme.split('/').next().unwrap_or("");
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    matches!(
        host.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}
