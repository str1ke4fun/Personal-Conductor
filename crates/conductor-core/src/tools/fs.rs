use crate::proposals::RiskLevel;
use anyhow::bail;

use super::registry::{
    ToolExecutionResult, ToolPermission, ToolProviderKind, ToolRegistry, ToolSpec,
};
use super::{current_workspace_root, display_path, resolve_workspace_path};
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn glob_to_regex(pattern: &str) -> Result<regex::Regex, regex::Error> {
    let mut re = String::from("^");
    for c in pattern.chars() {
        match c {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                re.push('\\');
                re.push(c);
            }
            _ => re.push(c),
        }
    }
    re.push('$');
    regex::Regex::new(&re)
}

fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext,
        "exe"
            | "dll"
            | "so"
            | "dylib"
            | "bin"
            | "o"
            | "obj"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "bmp"
            | "ico"
            | "mp3"
            | "mp4"
            | "wav"
            | "avi"
            | "mkv"
            | "zip"
            | "tar"
            | "gz"
            | "7z"
            | "rar"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "ppt"
            | "pptx"
    )
}

fn suggest_claude_skills_path(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return None;
    }

    for skill_root in path.ancestors().skip(1) {
        let Some(skill_name) = skill_root.file_name() else {
            continue;
        };
        let Some(parent) = skill_root.parent() else {
            continue;
        };
        let candidate_root = parent.join(".claude").join("skills").join(skill_name);
        if !candidate_root.exists() {
            continue;
        }
        let Ok(relative_tail) = path.strip_prefix(skill_root) else {
            continue;
        };
        let candidate = candidate_root.join(relative_tail);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn file_read_error(path: &Path, err: std::io::Error) -> anyhow::Error {
    if err.kind() == std::io::ErrorKind::NotFound {
        if let Some(suggestion) = suggest_claude_skills_path(path) {
            return anyhow::anyhow!(
                "file not found: {}; did you mean {}?",
                display_path(path),
                display_path(&suggestion)
            );
        }
    }
    anyhow::Error::new(err)
}

fn execute_file_glob(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let pattern = input
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing pattern"))?;
    let root = input
        .get("path")
        .and_then(|v| v.as_str())
        .map(resolve_workspace_path)
        .transpose()?
        .unwrap_or_else(current_workspace_root);

    if pattern.starts_with('/') || pattern.starts_with('\\') || pattern.contains(':') {
        bail!("absolute glob patterns are not allowed outside the active workspace");
    }

    let glob_pattern = root.join(pattern).to_string_lossy().to_string();

    let mut matches = Vec::new();
    for entry in glob::glob(&glob_pattern)? {
        if let Ok(path) = entry {
            matches.push(path.display().to_string());
        }
        if matches.len() >= 500 {
            break;
        }
    }
    matches.sort();
    let count = matches.len();
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "matches": matches, "count": count }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_file_grep(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let pattern = input
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing pattern"))?;
    let root = input
        .get("path")
        .and_then(|v| v.as_str())
        .map(resolve_workspace_path)
        .transpose()?
        .unwrap_or_else(current_workspace_root);
    let case_insensitive = input.get("-i").and_then(|v| v.as_bool()).unwrap_or(false);
    let glob_filter = input.get("glob").and_then(|v| v.as_str());
    let head_limit = input
        .get("head_limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as usize;

    let re = if case_insensitive {
        regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()?
    } else {
        regex::Regex::new(pattern)?
    };

    let glob_regex = glob_filter.and_then(|g| glob_to_regex(g).ok());

    let mut results = Vec::new();
    for entry in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if results.len() >= head_limit {
            break;
        }
        let path = entry.path();

        // Skip binary files by extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if is_binary_extension(ext) {
                continue;
            }
        }

        if let Some(ref re) = glob_regex {
            let fname = path.file_name().unwrap_or_default().to_string_lossy();
            if !re.is_match(&fname) {
                continue;
            }
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_idx, line) in content.lines().enumerate() {
            if results.len() >= head_limit {
                break;
            }
            if re.is_match(line) {
                results.push(serde_json::json!({
                    "path": path.display().to_string(),
                    "line": line_idx + 1,
                    "text": line,
                }));
            }
        }
    }

    let count = results.len();
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "matches": results, "count": count }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_file_read(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let path = input
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing file_path"))?;
    let path = resolve_workspace_path(path)?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" => {
            // Image files: return metadata only (Phase 3 will add base64)
            let metadata = std::fs::metadata(&path).map_err(|err| file_read_error(&path, err))?;
            Ok(ToolExecutionResult {
                success: true,
                output: serde_json::json!({
                    "type": "image",
                    "path": display_path(&path),
                    "size": metadata.len(),
                    "format": ext,
                    "note": "图片内容暂不支持直接读取，仅返回元数据",
                }),
                error: None,
                duration_ms: 0,
            })
        }
        _ => {
            let offset = input.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;

            let content =
                std::fs::read_to_string(&path).map_err(|err| file_read_error(&path, err))?;
            let lines: Vec<&str> = content.lines().collect();
            let start = offset.min(lines.len());
            let end = (start + limit).min(lines.len());
            let selected = &lines[start..end];

            let text = selected
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{}: {}", start + i + 1, line))
                .collect::<Vec<_>>()
                .join("\n");

            Ok(ToolExecutionResult {
                success: true,
                output: serde_json::json!({
                    "type": "text",
                    "text": text,
                    "total_lines": lines.len(),
                    "offset": start,
                    "limit": limit,
                }),
                error: None,
                duration_ms: 0,
            })
        }
    }
}

fn execute_file_write(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let path = input
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing file_path"))?;
    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing content"))?;

    // Resolve relative paths against workspace root
    let resolved = resolve_workspace_path(path)?;

    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&resolved, content)?;

    // Return the display-friendly path (relative to workspace root if possible)
    let display = display_path(&resolved);
    let workspace_root = current_workspace_root();

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "path": display,
            "absolute_path": resolved.display().to_string(),
            "workspace_root": workspace_root.display().to_string(),
            "bytes": content.len()
        }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_file_append(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let path = input
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing file_path"))?;
    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing content"))?;

    let resolved = resolve_workspace_path(path)?;

    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&resolved)?;
    file.write_all(content.as_bytes())?;

    let display = display_path(&resolved);
    let workspace_root = current_workspace_root();
    let size = std::fs::metadata(&resolved)
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "path": display,
            "absolute_path": resolved.display().to_string(),
            "workspace_root": workspace_root.display().to_string(),
            "bytes_appended": content.len(),
            "size": size
        }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_file_edit(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let path = input
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing file_path"))?;
    let resolved = resolve_workspace_path(path)?;
    let old = input
        .get("old_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing old_string"))?;
    let new = input
        .get("new_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing new_string"))?;
    let replace_all = input
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let content = std::fs::read_to_string(&resolved)?;

    let (new_content, count) = if replace_all {
        (content.replace(old, new), content.matches(old).count())
    } else {
        match content.find(old) {
            Some(pos) => {
                let new_c = format!("{}{}{}", &content[..pos], new, &content[pos + old.len()..]);
                (new_c, 1)
            }
            None => {
                return Ok(ToolExecutionResult {
                    success: false,
                    output: serde_json::json!({}),
                    error: Some("未在文件中找到匹配的原文".to_string()),
                    duration_ms: 0,
                });
            }
        }
    };

    std::fs::write(&resolved, &new_content)?;

    let workspace_root = current_workspace_root();
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "success": true,
            "replacements": count,
            "path": display_path(&resolved),
            "absolute_path": resolved.display().to_string(),
            "workspace_root": workspace_root.display().to_string(),
        }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_file_stat(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let path = input
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing file_path"))?;
    let resolved = resolve_workspace_path(path)?;
    let metadata = std::fs::metadata(&resolved)?;

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "path": display_path(&resolved),
            "size": metadata.len(),
            "is_dir": metadata.is_dir(),
            "is_file": metadata.is_file(),
            "modified": metadata.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            "readonly": metadata.permissions().readonly(),
        }),
        error: None,
        duration_ms: 0,
    })
}

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            id: "file.glob".to_string(),
            name: "搜索文件".to_string(),
            description: "按 Glob 模式匹配文件路径".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob 模式，如 **/*.rs" },
                    "path": { "type": "string", "description": "搜索根目录（默认当前目录）" }
                },
                "required": ["pattern"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "matches": { "type": "array", "items": { "type": "string" } },
                    "count": { "type": "integer" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_file_glob,
    );

    registry.register(
        ToolSpec {
            id: "file.grep".to_string(),
            name: "搜索内容".to_string(),
            description: "在文件中搜索正则表达式匹配".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "正则表达式" },
                    "path": { "type": "string", "description": "搜索根目录" },
                    "glob": { "type": "string", "description": "文件名过滤 Glob" },
                    "-i": { "type": "boolean", "description": "忽略大小写" },
                    "head_limit": { "type": "integer", "description": "最大结果数（默认100）" }
                },
                "required": ["pattern"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "matches": { "type": "array" },
                    "count": { "type": "integer" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_file_grep,
    );

    registry.register(
        ToolSpec {
            id: "file.read".to_string(),
            name: "读取文件".to_string(),
            description: "读取文件内容（支持行号偏移和限制）".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "文件路径" },
                    "offset": { "type": "integer", "description": "起始行号（默认0）" },
                    "limit": { "type": "integer", "description": "最大行数（默认2000）" }
                },
                "required": ["file_path"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "type": { "type": "string" },
                    "text": { "type": "string" },
                    "total_lines": { "type": "integer" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_file_read,
    );

    registry.register(
        ToolSpec {
            id: "file.write".to_string(),
            name: "写入文件".to_string(),
            description: "将内容写入文件（自动创建父目录）。长报告优先先写入短标题/首段，再用 file.append 分块追加，避免单次工具参数过长。".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "文件路径" },
                    "content": { "type": "string", "description": "写入内容" }
                },
                "required": ["file_path", "content"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "bytes": { "type": "integer" }
                }
            }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_file_write,
    );

    registry.register(
        ToolSpec {
            id: "file.append".to_string(),
            name: "追加写入文件".to_string(),
            description: "向文件末尾追加内容（自动创建父目录），适合把长报告拆成多个较小片段写入。".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "文件路径" },
                    "content": { "type": "string", "description": "追加内容；长文本请拆成多个较小片段" }
                },
                "required": ["file_path", "content"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "bytes_appended": { "type": "integer" },
                    "size": { "type": "integer" }
                }
            }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_file_append,
    );

    registry.register(
        ToolSpec {
            id: "file.edit".to_string(),
            name: "编辑文件".to_string(),
            description: "精确替换文件中的字符串".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "文件路径" },
                    "old_string": { "type": "string", "description": "要替换的原文" },
                    "new_string": { "type": "string", "description": "新内容" },
                    "replace_all": { "type": "boolean", "description": "替换所有匹配（默认false）" }
                },
                "required": ["file_path", "old_string", "new_string"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "success": { "type": "boolean" },
                    "replacements": { "type": "integer" },
                    "path": { "type": "string" }
                }
            }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_file_edit,
    );

    registry.register(
        ToolSpec {
            id: "file.stat".to_string(),
            name: "文件信息".to_string(),
            description: "获取文件元数据（大小、类型、修改时间等）".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "文件路径" }
                },
                "required": ["file_path"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "size": { "type": "integer" },
                    "is_dir": { "type": "boolean" },
                    "is_file": { "type": "boolean" },
                    "modified": { "type": "integer" },
                    "readonly": { "type": "boolean" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_file_stat,
    );
}
