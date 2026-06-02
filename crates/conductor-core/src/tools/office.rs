use crate::proposals::RiskLevel;
use chrono::{DateTime, Utc};

use super::registry::{
    ToolExecutionResult, ToolPermission, ToolProviderKind, ToolRegistry, ToolSpec,
};

fn execute_office_inspect_document(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing path"))?;

    let path_obj = std::path::Path::new(path);
    let metadata = std::fs::metadata(path_obj)?;

    let filename = path_obj
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let last_modified: DateTime<Utc> = metadata.modified()?.into();

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "path": path,
            "filename": filename,
            "size": metadata.len(),
            "last_modified": last_modified.to_rfc3339(),
            "page_count": 0,
            "word_count": 0,
            "title": filename.clone(),
            "author": "unknown",
            "metadata": {},
        }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_office_export_text(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing path"))?;

    let text = std::fs::read_to_string(path)?;

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "success": true,
            "text": text,
            "output_path": null,
        }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_office_patch_dry_run(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing path"))?;

    let patches = input
        .get("patches")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing patches"))?;

    let mut preview = format!("=== 模拟预览：{}\n\n", path);
    let mut estimated_changes = 0;

    for (i, patch) in patches.iter().enumerate() {
        let operation = patch
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let target = patch
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let content = patch.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let position = patch.get("position").and_then(|v| v.as_i64()).unwrap_or(0);

        preview.push_str(&format!(
            "[Patch {}] {} {} at position {}:\n{}\n\n",
            i + 1,
            operation,
            target,
            position,
            content
        ));
        estimated_changes += content.len();
    }

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "success": true,
            "preview": preview,
            "patch_count": patches.len(),
            "estimated_changes": estimated_changes,
        }),
        error: None,
        duration_ms: 0,
    })
}

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            id: "office.inspect_document".to_string(),
            name: "检查文档".to_string(),
            description: "检查文档的元数据和结构信息".to_string(),
            provider: ToolProviderKind::Cli,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "filename": { "type": "string" },
                    "size": { "type": "integer" },
                    "last_modified": { "type": "string" },
                    "page_count": { "type": "integer" },
                    "word_count": { "type": "integer" },
                    "title": { "type": "string" },
                    "author": { "type": "string" },
                    "metadata": { "type": "object" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadExternalPath],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_office_inspect_document,
    );

    registry.register(
        ToolSpec {
            id: "office.export_text".to_string(),
            name: "导出文档文本".to_string(),
            description: "将文档内容导出为纯文本".to_string(),
            provider: ToolProviderKind::Cli,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "output_path": { "type": "string" }
                },
                "required": ["path"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "success": { "type": "boolean" },
                    "text": { "type": "string" },
                    "output_path": { "type": ["string", "null"] }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadExternalPath],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_office_export_text,
    );

    registry.register(
        ToolSpec {
            id: "office.patch_dry_run".to_string(),
            name: "模拟文档修改".to_string(),
            description: "模拟对文档进行修改，不实际写入".to_string(),
            provider: ToolProviderKind::Cli,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "patches": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "operation": { "type": "string", "enum": ["replace", "insert", "delete"] },
                                "target": { "type": "string" },
                                "content": { "type": "string" },
                                "position": { "type": "integer" }
                            },
                            "required": ["operation", "target"]
                        }
                    }
                },
                "required": ["path", "patches"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "success": { "type": "boolean" },
                    "preview": { "type": "string" },
                    "patch_count": { "type": "integer" },
                    "estimated_changes": { "type": "integer" }
                }
            }),
            risk_level: RiskLevel::DraftOnly,
            permissions: vec![ToolPermission::ReadExternalPath],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_office_patch_dry_run,
    );
}
