#[cfg(feature = "free-mode")]
mod free;
mod stats;

fn render_auth_shell(title: &str, body: &str) -> String {
    let base_css = include_str!("css/base.css");
    let comp_css = include_str!("css/components.css");

    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>
{}
{}
.login-shell {{
    min-height: 100dvh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px 16px;
}}
.login-card {{
    max-width: 400px;
    width: 100%;
    padding: 28px 24px;
}}
.login-brand {{
    padding-bottom: 16px;
    margin-bottom: 20px;
    border-bottom: 1px solid var(--rule);
}}
.login-brand h1 {{
    font-family: var(--font-mono);
    font-size: 1.15rem;
    font-weight: 600;
}}
.login-brand p {{
    font-size: 0.8125rem;
    color: var(--muted);
    margin-top: 2px;
}}
.signup-warn {{
    margin-top: 0;
    margin-bottom: 18px;
    font-family: var(--font-sans);
    font-size: 0.8125rem;
    line-height: 1.45;
}}
.login-submit {{
    width: 100%;
    margin-top: 8px;
}}
    </style>
</head>
<body>
{}
</body>
</html>"#,
        title, base_css, comp_css, body
    )
}

pub fn render_login_html() -> String {
    render_auth_shell("Login - MyTranslation", include_str!("html/login.html"))
}

pub fn render_signup_html() -> String {
    render_auth_shell("Create Account - MyTranslation", include_str!("html/signup.html"))
}

pub fn render_dashboard_html() -> String {
    let base_css = include_str!("css/base.css");
    let comp_css = include_str!("css/components.css");
    let body_template = include_str!("html/index.html");
    let app_js = include_str!("js/app.js");
    let workspace_js = include_str!("js/workspace.js");
    let local_js = include_str!("js/local.js");

    #[cfg(feature = "free-mode")]
    let (free_css, free_tab_html, free_section_html, free_js) = (
        free::FREE_CSS,
        free::FREE_TAB_HTML,
        free::FREE_SECTION_HTML,
        free::FREE_JS,
    );

    #[cfg(not(feature = "free-mode"))]
    let (free_css, free_tab_html, free_section_html, free_js) = ("", "", "", "");

    let body_html = body_template
        .replace("<!-- STATS_TAB_BUTTON -->", stats::STATS_TAB_HTML)
        .replace("<!-- STATS_PANEL_SECTION -->", stats::STATS_PANEL_HTML)
        .replace("<!-- FREE_MODE_BUTTON -->", free_tab_html)
        .replace("<!-- FREE_MODE_SECTION -->", free_section_html);

    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>MyTranslation</title>
    <style>
{}
{}
{}
{}
    </style>
</head>
<body>
{}
    <script>
{}
{}
{}
{}
{}
    </script>
</body>
</html>"#,
        base_css,
        comp_css,
        stats::STATS_CSS,
        free_css,
        body_html,
        app_js,
        workspace_js,
        local_js,
        free_js,
        stats::STATS_JS
    )
}
