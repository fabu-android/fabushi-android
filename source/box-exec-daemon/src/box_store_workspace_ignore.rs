use std::{collections::HashMap, fs, path::Path};

pub const SAND_WORKSPACE_IGNORE_FILE_NAME: &str = ".sandignore";
pub const SAND_BOX_WORKSPACE_DEFAULT_IGNORE_PATTERNS: &[&str] = &[
    "node_modules/",
    ".next/",
    ".nuxt/",
    ".svelte-kit/",
    ".turbo/",
    ".parcel-cache/",
    ".cache/",
    "dist/",
    "build/",
    "out/",
    "coverage/",
    "__pycache__/",
    "*.pyc",
    "*.pyo",
    ".venv/",
    "venv/",
    ".pytest_cache/",
    ".mypy_cache/",
    ".ruff_cache/",
    ".tox/",
    ".ipynb_checkpoints/",
    "*.egg-info/",
    ".eggs/",
    "target/",
    ".gradle/",
    "core.[0-9]*",
    "*.core",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct NegationReach {
    everywhere: bool,
    literal_prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClassItem {
    Char(char),
    Range(char, char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Literal(char),
    Star,
    GlobStar,
    GlobStarDir,
    Question,
    Class { negated: bool, items: Vec<ClassItem> },
}

#[derive(Debug, Clone)]
struct Rule {
    negated: bool,
    anchored: bool,
    is_dir: bool,
    tokens: Vec<Token>,
    negation_reach: Option<NegationReach>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceIgnore {
    rules: Vec<Rule>,
}

impl WorkspaceIgnore {
    pub fn ignores(&self, rel_path: &str) -> bool {
        let mut ignored = false;
        for rule in &self.rules {
            if rule.matches_file(rel_path) {
                ignored = !rule.negated;
            }
        }
        ignored
    }

    pub fn can_prune_dir(&self, rel_dir: &str) -> bool {
        let positive_match = self
            .rules
            .iter()
            .any(|rule| !rule.negated && rule.matches_dir(rel_dir));
        positive_match
            && !self
                .rules
                .iter()
                .filter(|rule| rule.negated)
                .any(|rule| negation_reaches(rule.negation_reach.as_ref(), rel_dir))
    }

    #[cfg(test)]
    fn is_ignore_nothing(&self) -> bool {
        self.rules.is_empty()
    }
}

impl Rule {
    fn start_positions(&self, path: &[char]) -> Vec<usize> {
        if self.anchored {
            return vec![0];
        }
        let mut starts = vec![0];
        for (index, ch) in path.iter().enumerate() {
            if *ch == '/' {
                starts.push(index + 1);
            }
        }
        starts
    }

    fn base_matches_prefixes(&self, path: &[char], require_descendant: bool) -> bool {
        for start in self.start_positions(path) {
            let suffix = &path[start..];
            if !require_descendant && glob_match(&self.tokens, suffix) {
                return true;
            }
            for (offset, ch) in suffix.iter().enumerate() {
                if *ch == '/' && glob_match(&self.tokens, &suffix[..offset]) {
                    return true;
                }
            }
        }
        false
    }

    fn matches_file(&self, rel_path: &str) -> bool {
        let path = rel_path.chars().collect::<Vec<_>>();
        self.base_matches_prefixes(&path, self.is_dir)
    }

    fn matches_dir(&self, rel_dir: &str) -> bool {
        let path = rel_dir.chars().collect::<Vec<_>>();
        self.base_matches_prefixes(&path, false)
    }
}

pub fn parse_ignore_patterns(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

pub fn compile_workspace_ignore<I, S>(patterns: I) -> WorkspaceIgnore
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut rules = Vec::new();
    for pattern in patterns {
        let raw = pattern.as_ref();
        match compile_rule(raw) {
            Ok(Some(rule)) => rules.push(rule),
            Ok(None) => {}
            Err(()) if raw.starts_with('!') => return WorkspaceIgnore::default(),
            Err(()) => {}
        }
    }
    WorkspaceIgnore { rules }
}

pub fn load_workspace_ignore(workspace_dir: impl AsRef<Path>) -> WorkspaceIgnore {
    let mut patterns = SAND_BOX_WORKSPACE_DEFAULT_IGNORE_PATTERNS
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    if let Ok(text) = fs::read_to_string(
        workspace_dir
            .as_ref()
            .join(SAND_WORKSPACE_IGNORE_FILE_NAME),
    ) {
        patterns.extend(parse_ignore_patterns(&text));
    }
    compile_workspace_ignore(patterns)
}

fn compile_rule(raw_pattern: &str) -> Result<Option<Rule>, ()> {
    let mut pattern = raw_pattern.to_string();
    let mut negated = false;
    if let Some(rest) = pattern.strip_prefix('!') {
        negated = true;
        pattern = rest.to_string();
    } else if pattern.starts_with("\\!") || pattern.starts_with("\\#") {
        pattern.remove(0);
    }

    let mut is_dir = false;
    if pattern.ends_with('/') {
        is_dir = true;
        pattern.pop();
    }

    let mut anchored = false;
    if pattern.starts_with('/') {
        anchored = true;
        pattern.remove(0);
    }
    if pattern.is_empty() {
        return Ok(None);
    }
    if pattern.contains('/') {
        anchored = true;
    }

    let tokens = tokenize_glob(&pattern)?;
    Ok(Some(Rule {
        negated,
        anchored,
        is_dir,
        tokens,
        negation_reach: negated.then(|| compute_negation_reach(&pattern, anchored)),
    }))
}

fn compute_negation_reach(pattern: &str, anchored: bool) -> NegationReach {
    if !anchored {
        return NegationReach {
            everywhere: true,
            literal_prefix: String::new(),
        };
    }
    let mut literal = Vec::new();
    for segment in pattern.split('/') {
        if segment.contains('*') || segment.contains('?') || segment.contains('[') {
            break;
        }
        literal.push(segment);
    }
    if literal.is_empty() {
        NegationReach {
            everywhere: true,
            literal_prefix: String::new(),
        }
    } else {
        NegationReach {
            everywhere: false,
            literal_prefix: literal.join("/"),
        }
    }
}

fn negation_reaches(reach: Option<&NegationReach>, rel_dir: &str) -> bool {
    let Some(reach) = reach else {
        return false;
    };
    if reach.everywhere {
        return true;
    }
    let prefix = &reach.literal_prefix;
    prefix == rel_dir
        || prefix.starts_with(&format!("{rel_dir}/"))
        || rel_dir.starts_with(&format!("{prefix}/"))
}

fn tokenize_glob(glob: &str) -> Result<Vec<Token>, ()> {
    let chars = glob.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        match chars[index] {
            '*' => {
                let start = index;
                while index < chars.len() && chars[index] == '*' {
                    index += 1;
                }
                let double = index - start >= 2;
                let aligned = start == 0 || chars[start - 1] == '/';
                if double && aligned && chars.get(index) == Some(&'/') {
                    tokens.push(Token::GlobStarDir);
                    index += 1;
                } else if double && aligned {
                    tokens.push(Token::GlobStar);
                } else {
                    tokens.push(Token::Star);
                }
            }
            '?' => {
                tokens.push(Token::Question);
                index += 1;
            }
            '[' => {
                if let Some((token, next)) = parse_char_class(&chars, index)? {
                    tokens.push(token);
                    index = next;
                } else {
                    tokens.push(Token::Literal('['));
                    index += 1;
                }
            }
            ch => {
                tokens.push(Token::Literal(ch));
                index += 1;
            }
        }
    }
    Ok(tokens)
}

fn parse_char_class(chars: &[char], start: usize) -> Result<Option<(Token, usize)>, ()> {
    let mut cursor = start + 1;
    let mut negated = false;
    if matches!(chars.get(cursor), Some(&'!') | Some(&'^')) {
        negated = true;
        cursor += 1;
    }
    let content_start = cursor;
    if chars.get(cursor) == Some(&']') {
        cursor += 1;
    }
    while cursor < chars.len() && chars[cursor] != ']' {
        cursor += 1;
    }
    if cursor >= chars.len() {
        return Ok(None);
    }

    let content = &chars[content_start..cursor];
    let mut items = Vec::new();
    let mut index = 0usize;
    while index < content.len() {
        if index + 2 < content.len() && content[index + 1] == '-' {
            let start = content[index];
            let end = content[index + 2];
            if start > end {
                return Err(());
            }
            items.push(ClassItem::Range(start, end));
            index += 3;
        } else {
            items.push(ClassItem::Char(content[index]));
            index += 1;
        }
    }
    Ok(Some((Token::Class { negated, items }, cursor + 1)))
}

fn class_matches(items: &[ClassItem], ch: char) -> bool {
    items.iter().any(|item| match item {
        ClassItem::Char(value) => *value == ch,
        ClassItem::Range(start, end) => *start <= ch && ch <= *end,
    })
}

fn glob_match(tokens: &[Token], path: &[char]) -> bool {
    fn visit(
        tokens: &[Token],
        path: &[char],
        token_index: usize,
        path_index: usize,
        memo: &mut HashMap<(usize, usize), bool>,
    ) -> bool {
        if let Some(value) = memo.get(&(token_index, path_index)) {
            return *value;
        }
        let value = if token_index == tokens.len() {
            path_index == path.len()
        } else {
            match &tokens[token_index] {
                Token::Literal(expected) => {
                    path.get(path_index) == Some(expected)
                        && visit(tokens, path, token_index + 1, path_index + 1, memo)
                }
                Token::Question => {
                    path.get(path_index).is_some_and(|ch| *ch != '/')
                        && visit(tokens, path, token_index + 1, path_index + 1, memo)
                }
                Token::Star => {
                    if visit(tokens, path, token_index + 1, path_index, memo) {
                        true
                    } else {
                        let mut cursor = path_index;
                        let mut matched = false;
                        while cursor < path.len() && path[cursor] != '/' {
                            cursor += 1;
                            if visit(tokens, path, token_index + 1, cursor, memo) {
                                matched = true;
                                break;
                            }
                        }
                        matched
                    }
                }
                Token::GlobStar => (path_index..=path.len())
                    .any(|cursor| visit(tokens, path, token_index + 1, cursor, memo)),
                Token::GlobStarDir => {
                    if visit(tokens, path, token_index + 1, path_index, memo) {
                        true
                    } else {
                        (path_index..path.len()).any(|cursor| {
                            path[cursor] == '/'
                                && visit(tokens, path, token_index + 1, cursor + 1, memo)
                        })
                    }
                }
                Token::Class { negated, items } => path.get(path_index).is_some_and(|ch| {
                    let matched = class_matches(items, *ch);
                    matched != *negated
                        && visit(tokens, path, token_index + 1, path_index + 1, memo)
                }),
            }
        };
        memo.insert((token_index, path_index), value);
        value
    }

    visit(tokens, path, 0, 0, &mut HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ignore_file_lines_like_desktop() {
        assert_eq!(
            parse_ignore_patterns("node_modules/  \n# comment\n\n!node_modules/keep/\r\n"),
            vec!["node_modules/", "!node_modules/keep/"]
        );
    }

    #[test]
    fn defaults_ignore_nested_build_artifacts() {
        let ignore = compile_workspace_ignore(SAND_BOX_WORKSPACE_DEFAULT_IGNORE_PATTERNS);
        assert!(ignore.ignores("project/node_modules/pkg/index.js"));
        assert!(ignore.ignores("nested/app/target/debug/app"));
        assert!(ignore.ignores("src/cache.pyc"));
        assert!(ignore.ignores("core.123"));
        assert!(!ignore.ignores("src/main.rs"));
    }

    #[test]
    fn negation_prevents_unsafe_directory_pruning() {
        let ignore = compile_workspace_ignore(["dist/", "!dist/keep.txt", "build/"]);
        assert!(ignore.ignores("dist/drop.txt"));
        assert!(!ignore.ignores("dist/keep.txt"));
        assert!(!ignore.can_prune_dir("dist"));
        assert!(ignore.can_prune_dir("build"));

        let everywhere = compile_workspace_ignore(["target/", "!keep.txt"]);
        assert!(!everywhere.can_prune_dir("target"));
    }

    #[test]
    fn globstar_question_and_character_classes_match_desktop() {
        let ignore = compile_workspace_ignore([
            "**/cache/*.tmp",
            "logs/file?.txt",
            "core.[0-9]*",
        ]);
        assert!(ignore.ignores("cache/a.tmp"));
        assert!(ignore.ignores("a/b/cache/c.tmp"));
        assert!(ignore.ignores("logs/file1.txt"));
        assert!(!ignore.ignores("logs/file12.txt"));
        assert!(ignore.ignores("core.7dump"));
        assert!(!ignore.ignores("core.xdump"));
    }

    #[test]
    fn malformed_negated_regex_class_fails_open_to_ignore_nothing() {
        let ignore = compile_workspace_ignore(["dist/", "![z-a]"]);
        assert!(ignore.is_ignore_nothing());
        assert!(!ignore.ignores("dist/file"));
        assert!(!ignore.can_prune_dir("dist"));
    }

    #[test]
    fn load_merges_defaults_and_sandignore() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-workspace-ignore-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join(SAND_WORKSPACE_IGNORE_FILE_NAME),
            "generated/\n!generated/keep.txt\n",
        )
        .unwrap();

        let ignore = load_workspace_ignore(&root);
        assert!(ignore.ignores("node_modules/a.js"));
        assert!(ignore.ignores("generated/drop.txt"));
        assert!(!ignore.ignores("generated/keep.txt"));
        assert!(!ignore.can_prune_dir("generated"));

        let _ = fs::remove_dir_all(root);
    }
}
