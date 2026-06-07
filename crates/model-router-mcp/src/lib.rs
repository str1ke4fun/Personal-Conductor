// model-router-mcp::lib
//
// MCP tool definitions and handlers for the model router.
// Implements 3 tools: model.route, model.list, model.invoke
// No conductor-core dependency — routing logic is a static table.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ── Tool input types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RouteInput {
    pub work_kind: Option<String>,
    pub ooda_phase: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InvokeInput {
    pub model_id: String,
    pub system: Option<String>,
    pub user: String,
    pub max_tokens: Option<u32>,
}

// ── Tool output types ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RouteOutput {
    pub model_id: String,
    pub backend_kind: String,
    pub fallback_used: bool,
}

#[derive(Debug, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub backend: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct ListOutput {
    pub models: Vec<ModelEntry>,
}

// ── Static model catalogue ────────────────────────────────────────────────────

pub fn static_model_list() -> Vec<ModelEntry> {
    vec![
        ModelEntry {
            id: "claude-opus-4-8".into(),
            backend: "claude_p".into(),
            description: "Most capable Claude model. Used for deep reasoning (ooda_phase=reason)."
                .into(),
        },
        ModelEntry {
            id: "claude-sonnet-4-6".into(),
            backend: "claude_p".into(),
            description:
                "Balanced Claude model. Default for planning, review, document, and fallback."
                    .into(),
        },
        ModelEntry {
            id: "codex-mini-latest".into(),
            backend: "codex_cli".into(),
            description: "Fast code-focused model. Used for coding and testing work kinds.".into(),
        },
    ]
}

// ── Tool handlers ─────────────────────────────────────────────────────────────

/// model.route — static routing table, no DB required.
///
/// Priority (highest first):
///   1. hint overrides everything
///   2. ooda_phase = reason → opus
///   3. work_kind = coding | testing → codex-mini
///   4. work_kind = review | planning | document → sonnet
///   5. default fallback → sonnet
pub fn handle_route(input: RouteInput) -> Value {
    // 1. hint override
    if let Some(hint) = input.hint.filter(|h| !h.is_empty()) {
        let backend = infer_backend(&hint);
        return json!(RouteOutput {
            model_id: hint,
            backend_kind: backend,
            fallback_used: false,
        });
    }

    // 2. ooda_phase = reason → opus for deliberate reasoning
    if let Some(ref phase) = input.ooda_phase {
        if phase == "reason" {
            return json!(RouteOutput {
                model_id: "claude-opus-4-8".into(),
                backend_kind: "claude_p".into(),
                fallback_used: false,
            });
        }
    }

    // 3 & 4. work_kind dispatch
    if let Some(ref wk) = input.work_kind {
        match wk.as_str() {
            "coding" | "testing" => {
                return json!(RouteOutput {
                    model_id: "codex-mini-latest".into(),
                    backend_kind: "codex_cli".into(),
                    fallback_used: false,
                });
            }
            "review" | "planning" | "document" => {
                return json!(RouteOutput {
                    model_id: "claude-sonnet-4-6".into(),
                    backend_kind: "claude_p".into(),
                    fallback_used: false,
                });
            }
            _ => {}
        }
    }

    // 5. Default fallback
    json!(RouteOutput {
        model_id: "claude-sonnet-4-6".into(),
        backend_kind: "claude_p".into(),
        fallback_used: true,
    })
}

/// model.list — return the static catalogue of known models.
pub fn handle_list() -> Value {
    json!(ListOutput {
        models: static_model_list(),
    })
}

/// model.invoke — intentionally deferred; conductor-core runtime required.
pub fn handle_invoke(_input: InvokeInput) -> Value {
    json!({
        "success": false,
        "error": "model.invoke requires conductor-core runtime — use via conductor-desktop"
    })
}

// ── MCP tool schema helpers ───────────────────────────────────────────────────

/// Return the JSON Schema `inputSchema` for each tool, used in tools/list.
pub fn tool_definitions() -> Value {
    json!([
        {
            "name": "model.route",
            "description": "Resolve which model and backend to use for a given work kind and OODA phase. Returns model_id, backend_kind, and whether a fallback was used.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "work_kind": {
                        "type": "string",
                        "enum": ["coding", "planning", "review", "testing", "document", "external_action"],
                        "description": "The kind of work being dispatched."
                    },
                    "ooda_phase": {
                        "type": "string",
                        "enum": ["reason", "explore", "bootstrap", "review"],
                        "description": "Optional OODA phase. 'reason' upgrades to the most capable model."
                    },
                    "hint": {
                        "type": "string",
                        "description": "Optional model-id hint that overrides all routing rules."
                    }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "model.list",
            "description": "List all known models with their backend and description.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "model.invoke",
            "description": "Invoke a model with a system and user prompt. NOTE: This tool is currently DEFERRED — it always returns an error. Use conductor-desktop for actual model invocation.",
            "x_deferred": true,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "model_id": {
                        "type": "string",
                        "description": "The model to invoke."
                    },
                    "system": {
                        "type": "string",
                        "description": "Optional system prompt."
                    },
                    "user": {
                        "type": "string",
                        "description": "The user message."
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Maximum tokens to generate.",
                        "default": 1000
                    }
                },
                "required": ["model_id", "user"],
                "additionalProperties": false
            }
        }
    ])
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Infer the backend string from a model-id hint using simple prefix matching.
fn infer_backend(model_id: &str) -> String {
    if model_id.starts_with("codex") {
        "codex_cli".into()
    } else {
        "claude_p".into()
    }
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

/// Dispatch a `tools/call` request to the correct handler.
/// Returns the MCP `content` array value (text content wrapping the JSON result).
pub fn dispatch_tool(name: &str, arguments: Value) -> Result<Value, String> {
    match name {
        "model.route" => {
            let input: RouteInput = serde_json::from_value(arguments)
                .map_err(|e| format!("invalid arguments for model.route: {e}"))?;
            Ok(handle_route(input))
        }
        "model.list" => Ok(handle_list()),
        "model.invoke" => {
            let input: InvokeInput = serde_json::from_value(arguments)
                .map_err(|e| format!("invalid arguments for model.invoke: {e}"))?;
            Ok(handle_invoke(input))
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper: unwrap Ok result and return the Value.
    fn ok(result: Result<Value, String>) -> Value {
        result.expect("expected Ok but got Err")
    }

    #[test]
    fn test_route_hint_override() {
        let result = ok(dispatch_tool(
            "model.route",
            json!({"hint": "my-custom-model"}),
        ));
        assert_eq!(result["model_id"], "my-custom-model");
        assert_eq!(result["fallback_used"], false);
    }

    #[test]
    fn test_route_reason_phase_returns_opus() {
        let result = ok(dispatch_tool(
            "model.route",
            json!({"work_kind": "planning", "ooda_phase": "reason"}),
        ));
        let model_id = result["model_id"].as_str().expect("model_id is a string");
        assert!(
            model_id.contains("opus"),
            "expected model_id to contain 'opus', got '{model_id}'"
        );
    }

    #[test]
    fn test_route_coding_returns_codex() {
        let result = ok(dispatch_tool("model.route", json!({"work_kind": "coding"})));
        let model_id = result["model_id"].as_str().expect("model_id is a string");
        assert!(
            model_id.contains("codex"),
            "expected model_id to contain 'codex', got '{model_id}'"
        );
    }

    #[test]
    fn test_route_fallback_returns_sonnet() {
        let result = ok(dispatch_tool("model.route", json!({})));
        let model_id = result["model_id"].as_str().expect("model_id is a string");
        assert!(
            model_id.contains("sonnet"),
            "expected model_id to contain 'sonnet', got '{model_id}'"
        );
        assert_eq!(result["fallback_used"], true);
    }

    #[test]
    fn test_list_returns_models() {
        let result = ok(dispatch_tool("model.list", json!({})));
        let models = result["models"].as_array().expect("models is an array");
        assert!(
            models.len() >= 2,
            "expected at least 2 models, got {}",
            models.len()
        );
    }

    #[test]
    fn test_invoke_deferred() {
        let result = dispatch_tool(
            "model.invoke",
            json!({"model_id": "x", "system": "s", "user": "u"}),
        );
        assert!(result.is_ok(), "expected Ok, got Err: {:?}", result);
        let val = result.unwrap();
        let has_error_field = val.get("error").is_some();
        let success_false = val.get("success").and_then(|v| v.as_bool()) == Some(false);
        assert!(
            has_error_field || success_false,
            "expected 'error' field or 'success: false' in result, got: {val}"
        );
    }

    #[test]
    fn test_unknown_tool_errors() {
        let result = dispatch_tool("nonexistent.tool", json!({}));
        assert!(
            result.is_err(),
            "expected Err for unknown tool, got Ok: {:?}",
            result
        );
    }
}
