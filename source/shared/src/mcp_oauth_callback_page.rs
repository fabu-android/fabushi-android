fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn render_callback_page(title: &str, message: &str, hint: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title><style>html,body{{margin:0;min-height:100%;}}body{{font-family:ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,\"Segoe UI\",sans-serif;background:#0c0c0d;color:#f4f4f5;}}.page{{display:flex;min-height:100vh;flex:1;align-items:center;justify-content:center;}}.content{{text-align:center;}}.message{{font-size:1.125rem;line-height:1.75rem;margin:0;font-weight:400;}}.hint{{margin:0.5rem 0 0;font-size:1rem;line-height:1.5rem;color:#a1a1aa;}}</style></head><body><main class=\"page\"><div class=\"content\"><p class=\"message\">{}</p><p class=\"hint\">{}</p></div></main></body></html>",
        escape_html(title),
        escape_html(message),
        escape_html(hint)
    )
}

pub fn render_mcp_oauth_success_page(server_name: Option<&str>) -> String {
    let name = server_name.map(str::trim).filter(|value| !value.is_empty());
    let title = name
        .map(|value| format!("{value} connected"))
        .unwrap_or_else(|| "Authentication complete".to_string());
    render_callback_page(
        &title,
        "Authorization complete!",
        "You can close this tab.",
    )
}

pub fn render_mcp_oauth_error_page(server_name: Option<&str>) -> String {
    let name = server_name.map(str::trim).filter(|value| !value.is_empty());
    let title = name
        .map(|value| format!("{value} — Authentication failed"))
        .unwrap_or_else(|| "Authentication failed".to_string());
    render_callback_page(
        &title,
        "OAuth callback failed.",
        "Close this tab and try connecting again.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_pages_escape_untrusted_server_names() {
        let html = render_mcp_oauth_success_page(Some("<script>alert(1)</script>"));
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("Authorization complete!"));
    }
}
