pub fn render_dashboard_html() -> String {
    let base_css = include_str!("base.css");
    let comp_css = include_str!("components.css");
    let body_html = include_str!("index.html");
    let app_js = include_str!("app.js");
    let local_js = include_str!("local.js");

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
    </style>
</head>
<body>
{}
    <script>
{}
{}
    </script>
</body>
</html>"#,
        base_css, comp_css, body_html, app_js, local_js
    )
}
