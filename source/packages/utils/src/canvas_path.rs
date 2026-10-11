pub fn normalize_canvas_path(value: &str) -> String {
    let normalized = value.replace('\\', "/");
    let mut segments: Vec<&str> = Vec::new();
    for segment in normalized.split('/') {
        match segment {
            "" | "." => {}
            ".." => { segments.pop(); }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

pub fn is_managed_canvas_path(value: &str) -> bool {
    let normalized = normalize_canvas_path(value);
    let segments: Vec<&str> = normalized.split('/').collect();
    if segments.len() < 5 { return false; }
    let start = segments.len() - 5;
    let filename = segments[start + 4].to_ascii_lowercase();
    segments[start].eq_ignore_ascii_case(".cursor")
        && segments[start + 1].eq_ignore_ascii_case("projects")
        && !segments[start + 2].is_empty()
        && segments[start + 3].eq_ignore_ascii_case("canvases")
        && filename.ends_with(".canvas.tsx")
        && filename.len() > ".canvas.tsx".len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_recognizes_only_managed_canvas_suffix() {
        assert_eq!(
            normalize_canvas_path(r#"root\project\..\.cursor\projects\p\canvases\a.canvas.tsx"#),
            "root/.cursor/projects/p/canvases/a.canvas.tsx"
        );
        assert!(is_managed_canvas_path("root/.cursor/projects/p/canvases/a.canvas.tsx"));
        assert!(is_managed_canvas_path(".CURSOR/projects/p/CANVASES/a.CANVAS.TSX"));
        assert!(!is_managed_canvas_path(".cursor/projects/p/a.canvas.tsx"));
        assert!(!is_managed_canvas_path(".cursor/projects/p/canvases/.canvas.tsx"));
    }
}
