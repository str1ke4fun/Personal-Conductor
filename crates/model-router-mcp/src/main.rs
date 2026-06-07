// model-router-mcp::main
//
// Minimal JSON-RPC 2.0 stdin/stdout MCP server.
// Reads newline-delimited JSON requests, dispatches to tool handlers,
// writes newline-delimited JSON responses.
//
// Supported methods:
//   initialize   → return server capabilities
//   tools/list   → return the 3 tool definitions
//   tools/call   → dispatch to model.route / model.list / model.invoke

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use model_router_mcp::{dispatch_tool, tool_definitions};

#[tokio::main]
async fn main() -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // EOF — client closed the connection
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                // Parse error — respond with JSON-RPC parse error
                let resp = json_rpc_error(Value::Null, -32700, &format!("Parse error: {e}"));
                write_response(&mut stdout, &resp).await?;
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let response = handle_request(id, method, &request);
        write_response(&mut stdout, &response).await?;
    }

    Ok(())
}

// ── Request dispatcher ────────────────────────────────────────────────────────

fn handle_request(id: Value, method: &str, request: &Value) -> Value {
    match method {
        "initialize" => handle_initialize(id, request),
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(id, request),
        // Notifications (no id) — acknowledged silently via null result
        "notifications/initialized" => json_rpc_result(id, json!(null)),
        other => json_rpc_error(id, -32601, &format!("Method not found: {other}")),
    }
}

// ── Method handlers ───────────────────────────────────────────────────────────

fn handle_initialize(id: Value, request: &Value) -> Value {
    // Echo back the negotiated protocol version (default 2024-11-05).
    let client_version = request
        .pointer("/params/protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("2024-11-05");

    json_rpc_result(
        id,
        json!({
            "protocolVersion": client_version,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "model-router-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
    )
}

fn handle_tools_list(id: Value) -> Value {
    json_rpc_result(
        id,
        json!({
            "tools": tool_definitions()
        }),
    )
}

fn handle_tools_call(id: Value, request: &Value) -> Value {
    let params = match request.get("params") {
        Some(p) => p,
        None => {
            return json_rpc_error(id, -32602, "Missing params");
        }
    };

    let name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => {
            return json_rpc_error(id, -32602, "Missing params.name");
        }
    };

    // arguments is optional — default to empty object
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match dispatch_tool(name, arguments) {
        Ok(result) => {
            // MCP tools/call success: wrap result in content array
            json_rpc_result(
                id,
                json!({
                    "content": [
                        {
                            "type": "text",
                            "text": serde_json::to_string(&result)
                                .unwrap_or_else(|_| "{}".into())
                        }
                    ],
                    "isError": false
                }),
            )
        }
        Err(err) => {
            // MCP tools/call error: isError=true, message in content
            json_rpc_result(
                id,
                json!({
                    "content": [
                        {
                            "type": "text",
                            "text": err
                        }
                    ],
                    "isError": true
                }),
            )
        }
    }
}

// ── JSON-RPC helpers ──────────────────────────────────────────────────────────

fn json_rpc_result(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn json_rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

// ── I/O helper ────────────────────────────────────────────────────────────────

async fn write_response(stdout: &mut tokio::io::Stdout, response: &Value) -> Result<()> {
    let mut line = serde_json::to_string(response)?;
    line.push('\n');
    stdout.write_all(line.as_bytes()).await?;
    stdout.flush().await?;
    Ok(())
}
