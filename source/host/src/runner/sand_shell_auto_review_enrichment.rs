use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SAND_SHELL_ENRICHMENT_MAX_CHARS: usize = 8_000;
pub const SAND_SHELL_ENRICHMENT_MAX_LINES: usize = 200;
pub const SAND_SHELL_APPROVAL_HASH_CHUNK_LINES: usize = 50;
pub const SAND_SHELL_APPROVAL_HASH_MAX_LINES: usize = 20_000;
pub const SAND_SHELL_APPROVAL_PARSE_MAX_CHARS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandShellEnrichmentCandidate {
    Executable {
        path: String,
    },
    PackageScript {
        package_json_path: String,
        script_name: String,
    },
    PackageScriptUnresolved {
        package_json_path: Option<String>,
        invocation: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellReadResult {
    pub content: String,
    pub total_lines: usize,
    pub truncated: bool,
}

pub trait SandShellReadAccessor {
    fn read(
        &self,
        path: &str,
        tool_call_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Option<ShellReadResult>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedText {
    pub content: String,
    pub total_lines: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullTextHash {
    pub definition_hash: String,
    pub full_content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandShellTargetEnrichment {
    BindingUnavailable {
        kind: &'static str,
        path: String,
        approval_binding_nonce: String,
    },
    Executable {
        path: String,
        definition: String,
        definition_hash: String,
        truncated: bool,
    },
    PackageScript {
        path: String,
        name: String,
        definition: String,
        definition_hash: String,
    },
}

pub fn unquote(token: &str) -> String {
    if token.len() >= 2
        && ((token.starts_with('"') && token.ends_with('"'))
            || (token.starts_with('\'') && token.ends_with('\'')))
    {
        token[1..token.len() - 1].to_string()
    } else {
        token.to_string()
    }
}

pub fn parse_sand_shell_enrichment_candidate(
    command: &str,
    working_directory: Option<&str>,
) -> Option<SandShellEnrichmentCandidate> {
    let mut candidate_command = command.trim().to_string();
    let mut candidate_working_directory = working_directory.map(PathBuf::from);

    for _ in 0..4 {
        if let Some(rest) = strip_environment_prefix(&candidate_command) {
            candidate_command = rest;
            continue;
        }
        if let Some((directory, remaining)) = strip_cd_prefix(&candidate_command) {
            candidate_working_directory = candidate_working_directory
                .as_ref()
                .map(|base| normalize_join(base, &unquote(&directory)));
            candidate_command = remaining;
            continue;
        }
        break;
    }

    let command_segments = candidate_command
        .split([';', '&', '|', '\n'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if command_segments.len() > 1
        && command_segments.iter().any(|segment| {
            parse_sand_shell_enrichment_candidate(
                segment,
                candidate_working_directory.as_deref().and_then(Path::to_str),
            )
            .is_some_and(|candidate| matches!(
                candidate,
                SandShellEnrichmentCandidate::PackageScript { .. }
                    | SandShellEnrichmentCandidate::PackageScriptUnresolved { .. }
            ))
        })
    {
        return Some(SandShellEnrichmentCandidate::PackageScriptUnresolved {
            package_json_path: candidate_working_directory
                .as_ref()
                .map(|base| normalize_join(base, "package.json").display().to_string()),
            invocation: candidate_command,
        });
    }

    let tokens = shellish_tokens(&candidate_command);
    let first = tokens.first().map(String::as_str).unwrap_or_default();
    if matches!(first, "npm" | "pnpm" | "yarn") {
        let script_index = if tokens.get(1).is_some_and(|value| value == "run") {
            2
        } else {
            1
        };
        if let Some(script_name) = tokens.get(script_index).filter(|value| valid_script_name(value)) {
            return Some(match candidate_working_directory.as_ref() {
                Some(base) => SandShellEnrichmentCandidate::PackageScript {
                    package_json_path: normalize_join(base, "package.json")
                        .display()
                        .to_string(),
                    script_name: script_name.clone(),
                },
                None => SandShellEnrichmentCandidate::PackageScriptUnresolved {
                    package_json_path: None,
                    invocation: tokens[..=script_index].join(" "),
                },
            });
        }
        return Some(SandShellEnrichmentCandidate::PackageScriptUnresolved {
            package_json_path: candidate_working_directory
                .as_ref()
                .map(|base| normalize_join(base, "package.json").display().to_string()),
            invocation: candidate_command,
        });
    }
    if candidate_command
        .split_whitespace()
        .any(|token| matches!(token, "npm" | "pnpm" | "yarn"))
    {
        return Some(SandShellEnrichmentCandidate::PackageScriptUnresolved {
            package_json_path: candidate_working_directory
                .as_ref()
                .map(|base| normalize_join(base, "package.json").display().to_string()),
            invocation: candidate_command,
        });
    }

    let token = if matches!(
        first,
        "python" | "python3" | "node" | "bash" | "sh" | "zsh"
    ) || first.starts_with("python3.")
    {
        tokens.get(1)
    } else {
        tokens.first().filter(|value| {
            value.starts_with('/') || value.starts_with("./") || value.starts_with("../")
        })
    }?;
    let executable_path = PathBuf::from(unquote(token));
    let resolved = if executable_path.is_absolute() {
        executable_path
    } else {
        normalize_join(candidate_working_directory.as_ref()?, &executable_path)
    };
    Some(SandShellEnrichmentCandidate::Executable {
        path: resolved.display().to_string(),
    })
}

pub fn read_bounded_text(
    resource_accessor: &dyn SandShellReadAccessor,
    path: &str,
    tool_call_id: &str,
) -> Result<Option<BoundedText>, String> {
    let Some(parsed) = resource_accessor.read(
        path,
        tool_call_id,
        1,
        SAND_SHELL_ENRICHMENT_MAX_LINES,
    )? else {
        return Ok(None);
    };
    let content = parsed
        .content
        .chars()
        .take(SAND_SHELL_ENRICHMENT_MAX_CHARS)
        .collect::<String>();
    Ok(Some(BoundedText {
        truncated: parsed.truncated
            || parsed.content.chars().count() > SAND_SHELL_ENRICHMENT_MAX_CHARS
            || parsed.total_lines > SAND_SHELL_ENRICHMENT_MAX_LINES,
        content,
        total_lines: parsed.total_lines,
    }))
}

pub fn hash_full_text(
    resource_accessor: &dyn SandShellReadAccessor,
    path: &str,
    tool_call_id: &str,
    total_lines: usize,
) -> Result<Option<FullTextHash>, String> {
    if total_lines > SAND_SHELL_APPROVAL_HASH_MAX_LINES {
        return Ok(None);
    }
    let mut hash = Sha256::new();
    let mut full_content = String::new();
    let mut can_retain_full_content = true;
    let max_lines = total_lines.max(1);
    let mut offset = 1usize;
    while offset <= max_lines {
        let Some(parsed) = resource_accessor.read(
            path,
            tool_call_id,
            offset,
            SAND_SHELL_APPROVAL_HASH_CHUNK_LINES,
        )? else {
            return Ok(None);
        };
        if parsed.truncated {
            return Ok(None);
        }
        hash.update(offset.to_string().as_bytes());
        hash.update([0]);
        hash.update(parsed.content.as_bytes());
        hash.update([0]);
        if can_retain_full_content
            && full_content.len().saturating_add(parsed.content.len())
                <= SAND_SHELL_APPROVAL_PARSE_MAX_CHARS
        {
            full_content.push_str(&parsed.content);
        } else {
            can_retain_full_content = false;
            full_content.clear();
        }
        offset = offset.saturating_add(SAND_SHELL_APPROVAL_HASH_CHUNK_LINES);
    }
    Ok(Some(FullTextHash {
        definition_hash: format!("{:x}", hash.finalize()),
        full_content: can_retain_full_content.then_some(full_content),
    }))
}

pub fn unavailable_binding(candidate: &SandShellEnrichmentCandidate) -> SandShellTargetEnrichment {
    let (kind, path) = match candidate {
        SandShellEnrichmentCandidate::Executable { path } => ("executable", path.clone()),
        SandShellEnrichmentCandidate::PackageScript {
            package_json_path, ..
        } => ("package_script", package_json_path.clone()),
        SandShellEnrichmentCandidate::PackageScriptUnresolved {
            package_json_path,
            invocation,
        } => (
            "package_script_unresolved",
            package_json_path.clone().unwrap_or_else(|| invocation.clone()),
        ),
    };
    SandShellTargetEnrichment::BindingUnavailable {
        kind,
        path,
        approval_binding_nonce: Uuid::new_v4().to_string(),
    }
}

pub fn build_sand_shell_auto_review_target_enrichment(
    command: &str,
    working_directory: Option<&str>,
    resource_accessor: &dyn SandShellReadAccessor,
    tool_call_id: &str,
) -> Option<SandShellTargetEnrichment> {
    let candidate = parse_sand_shell_enrichment_candidate(command, working_directory)?;
    if matches!(
        candidate,
        SandShellEnrichmentCandidate::PackageScriptUnresolved { .. }
    ) {
        return Some(unavailable_binding(&candidate));
    }
    let path = match &candidate {
        SandShellEnrichmentCandidate::Executable { path } => path.clone(),
        SandShellEnrichmentCandidate::PackageScript {
            package_json_path, ..
        } => package_json_path.clone(),
        SandShellEnrichmentCandidate::PackageScriptUnresolved { .. } => unreachable!(),
    };
    let read = match read_bounded_text(resource_accessor, &path, tool_call_id) {
        Ok(Some(read)) => read,
        _ => return Some(unavailable_binding(&candidate)),
    };
    let full = match hash_full_text(
        resource_accessor,
        &path,
        tool_call_id,
        read.total_lines,
    ) {
        Ok(Some(full)) => full,
        _ => return Some(unavailable_binding(&candidate)),
    };
    match candidate {
        SandShellEnrichmentCandidate::Executable { path } => {
            if read.truncated {
                return Some(unavailable_binding(
                    &SandShellEnrichmentCandidate::Executable { path },
                ));
            }
            Some(SandShellTargetEnrichment::Executable {
                path,
                definition: read.content,
                definition_hash: full.definition_hash,
                truncated: false,
            })
        }
        SandShellEnrichmentCandidate::PackageScript {
            package_json_path,
            script_name,
        } => {
            let source = full.full_content.as_deref().unwrap_or(&read.content);
            let parsed: Value = match serde_json::from_str(source) {
                Ok(parsed) => parsed,
                Err(_) => {
                    return Some(unavailable_binding(
                        &SandShellEnrichmentCandidate::PackageScript {
                            package_json_path,
                            script_name,
                        },
                    ))
                }
            };
            let Some(body) = parsed
                .get("scripts")
                .and_then(Value::as_object)
                .and_then(|scripts| scripts.get(&script_name))
                .and_then(Value::as_str)
                .filter(|body| body.chars().count() <= SAND_SHELL_ENRICHMENT_MAX_CHARS)
            else {
                return Some(unavailable_binding(
                    &SandShellEnrichmentCandidate::PackageScript {
                        package_json_path,
                        script_name,
                    },
                ));
            };
            Some(SandShellTargetEnrichment::PackageScript {
                path: package_json_path,
                name: script_name,
                definition: body.chars().take(SAND_SHELL_ENRICHMENT_MAX_CHARS).collect(),
                definition_hash: full.definition_hash,
            })
        }
        SandShellEnrichmentCandidate::PackageScriptUnresolved { .. } => unreachable!(),
    }
}

fn strip_environment_prefix(command: &str) -> Option<String> {
    let tokens = shellish_tokens(command);
    let mut index = usize::from(tokens.first().is_some_and(|value| value == "env"));
    let start = index;
    while let Some(token) = tokens.get(index) {
        let Some((name, _)) = token.split_once('=') else {
            break;
        };
        if !valid_env_name(name) {
            break;
        }
        index += 1;
    }
    if index == start {
        return None;
    }
    Some(tokens[index..].join(" "))
}

fn strip_cd_prefix(command: &str) -> Option<(String, String)> {
    let (left, right) = command.split_once("&&")?;
    let left_tokens = shellish_tokens(left.trim());
    if left_tokens.len() != 2 || left_tokens[0] != "cd" {
        return None;
    }
    Some((left_tokens[1].clone(), right.trim().to_string()))
}

fn shellish_tokens(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in command.chars() {
        match quote {
            Some(active) if ch == active => {
                current.push(ch);
                quote = None;
            }
            Some(_) => current.push(ch),
            None if matches!(ch, '"' | '\'') => {
                quote = Some(ch);
                current.push(ch);
            }
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn valid_script_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_alphanumeric())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '-'))
}

fn normalize_join(base: &Path, child: impl AsRef<Path>) -> PathBuf {
    let joined = base.join(child);
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
