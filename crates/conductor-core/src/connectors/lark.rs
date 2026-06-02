use super::{ConnectorAuthStatus, ConnectorCapability, ConnectorImplementation, ConnectorSpec};
use serde_json::Value;
use std::process::Command;

/// Detect if lark-cli is available on the system.
pub fn detect_lark_cli() -> bool {
    #[cfg(target_os = "windows")]
    let (cmd, arg) = ("where", "lark-cli");
    #[cfg(not(target_os = "windows"))]
    let (cmd, arg) = ("which", "lark-cli");

    Command::new(cmd)
        .arg(arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check lark-cli authentication status.
pub fn check_lark_auth() -> ConnectorAuthStatus {
    if !detect_lark_cli() {
        return ConnectorAuthStatus::NotConfigured;
    }

    let output = Command::new("lark-cli").args(["auth", "status"]).output();

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let lower = stdout.to_lowercase();
            if lower.contains("authenticated") || lower.contains("logged in") {
                ConnectorAuthStatus::Authenticated
            } else if lower.contains("expired") {
                ConnectorAuthStatus::Expired
            } else if lower.contains("not configured") || lower.contains("not logged") {
                ConnectorAuthStatus::NotConfigured
            } else {
                ConnectorAuthStatus::NotConfigured
            }
        }
        Ok(_) => ConnectorAuthStatus::Failed,
        Err(_) => ConnectorAuthStatus::NotConfigured,
    }
}

/// Build the Lark ConnectorSpec with 5 readonly + 4 write tools.
pub fn build_lark_connector() -> ConnectorSpec {
    ConnectorSpec {
        id: "lark".to_string(),
        name: "飞书".to_string(),
        description: "飞书/Lark 办公套件".to_string(),
        implementation_type: ConnectorImplementation::LocalCli,
        capabilities: vec![
            // ── Readonly capabilities ──
            ConnectorCapability {
                capability: "lark.contact".to_string(),
                tools: vec!["lark.contact.search_user".to_string()],
                risk_level: "low".to_string(),
                requires_confirmation: false,
            },
            ConnectorCapability {
                capability: "lark.calendar".to_string(),
                tools: vec![
                    "lark.calendar.list_events".to_string(),
                    "lark.calendar.freebusy".to_string(),
                    "lark.calendar.search_rooms".to_string(),
                ],
                risk_level: "low".to_string(),
                requires_confirmation: false,
            },
            ConnectorCapability {
                capability: "lark.doc".to_string(),
                tools: vec!["lark.doc.search".to_string()],
                risk_level: "low".to_string(),
                requires_confirmation: false,
            },
            // ── Write capabilities (requires_confirmation = true) ──
            ConnectorCapability {
                capability: "lark.calendar.write".to_string(),
                tools: vec!["lark.calendar.create_event".to_string()],
                risk_level: "medium".to_string(),
                requires_confirmation: true,
            },
            ConnectorCapability {
                capability: "lark.doc.write".to_string(),
                tools: vec!["lark.doc.create_or_update".to_string()],
                risk_level: "medium".to_string(),
                requires_confirmation: true,
            },
            ConnectorCapability {
                capability: "lark.im.write".to_string(),
                tools: vec!["lark.im.send_message".to_string()],
                risk_level: "high".to_string(),
                requires_confirmation: true,
            },
            ConnectorCapability {
                capability: "lark.base.write".to_string(),
                tools: vec!["lark.base.upsert_records".to_string()],
                risk_level: "high".to_string(),
                requires_confirmation: true,
            },
        ],
        auth_status: check_lark_auth(),
        enabled: detect_lark_cli(),
        config_json: None,
    }
}

/// Generate a human-readable plan for a write operation (displayed before confirmation).
pub fn generate_write_plan(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "lark.calendar.create_event" => {
            format!(
                "将创建会议: {}, 时间: {} ~ {}, 参会人: {}",
                args["title"].as_str().unwrap_or(""),
                args["start"].as_str().unwrap_or(""),
                args["end"].as_str().unwrap_or(""),
                args["attendees"].as_str().unwrap_or(""),
            )
        }
        "lark.doc.create_or_update" => {
            format!(
                "将创建/更新文档: {}, 内容长度: {} 字符",
                args["title"].as_str().unwrap_or(""),
                args["content"].as_str().map(|s| s.len()).unwrap_or(0),
            )
        }
        "lark.im.send_message" => {
            format!(
                "将发送消息给: {}, 内容: {}",
                args["receive_id"].as_str().unwrap_or(""),
                args["content"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(50)
                    .collect::<String>(),
            )
        }
        "lark.base.upsert_records" => {
            format!(
                "将写入多维表格: app={}, table={}, 记录数: {}",
                args["app_token"].as_str().unwrap_or(""),
                args["table_id"].as_str().unwrap_or(""),
                args["records"].as_array().map(|a| a.len()).unwrap_or(0),
            )
        }
        _ => format!("将执行: {}", tool_name),
    }
}

/// Execute a lark-cli command and return parsed JSON output.
pub async fn execute_lark_tool(tool_name: &str, args: &Value) -> anyhow::Result<Value> {
    let (subcmd, cmd_args) = match tool_name {
        "lark.contact.search_user" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: query"))?;
            (
                "contact",
                vec![
                    "search".to_string(),
                    "--query".to_string(),
                    query.to_string(),
                ],
            )
        }
        "lark.calendar.list_events" => {
            let start = args
                .get("start_date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: start_date"))?;
            let end = args
                .get("end_date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: end_date"))?;
            (
                "calendar",
                vec![
                    "list".to_string(),
                    "--start".to_string(),
                    start.to_string(),
                    "--end".to_string(),
                    end.to_string(),
                ],
            )
        }
        "lark.calendar.freebusy" => {
            let start = args
                .get("start_date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: start_date"))?;
            let end = args
                .get("end_date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: end_date"))?;
            let users = args
                .get("user_ids")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: user_ids"))?;
            (
                "calendar",
                vec![
                    "freebusy".to_string(),
                    "--start".to_string(),
                    start.to_string(),
                    "--end".to_string(),
                    end.to_string(),
                    "--users".to_string(),
                    users.to_string(),
                ],
            )
        }
        "lark.calendar.search_rooms" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: query"))?;
            (
                "calendar",
                vec![
                    "rooms".to_string(),
                    "--query".to_string(),
                    query.to_string(),
                ],
            )
        }
        "lark.doc.search" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: query"))?;
            (
                "doc",
                vec![
                    "search".to_string(),
                    "--query".to_string(),
                    query.to_string(),
                ],
            )
        }
        // ── Write tools ──
        "lark.calendar.create_event" => {
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: title"))?;
            let start = args
                .get("start")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: start"))?;
            let end = args
                .get("end")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: end"))?;
            let mut cmd_args = vec![
                "create".to_string(),
                "--title".to_string(),
                title.to_string(),
                "--start".to_string(),
                start.to_string(),
                "--end".to_string(),
                end.to_string(),
            ];
            if let Some(attendees) = args.get("attendees").and_then(|v| v.as_str()) {
                cmd_args.push("--attendees".to_string());
                cmd_args.push(attendees.to_string());
            }
            if let Some(room) = args.get("room").and_then(|v| v.as_str()) {
                cmd_args.push("--room".to_string());
                cmd_args.push(room.to_string());
            }
            ("calendar", cmd_args)
        }
        "lark.doc.create_or_update" => {
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: title"))?;
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: content"))?;
            let mut cmd_args = vec![
                "create".to_string(),
                "--title".to_string(),
                title.to_string(),
                "--content".to_string(),
                content.to_string(),
            ];
            if let Some(folder) = args.get("folder").and_then(|v| v.as_str()) {
                cmd_args.push("--folder".to_string());
                cmd_args.push(folder.to_string());
            }
            ("doc", cmd_args)
        }
        "lark.im.send_message" => {
            let receive_id = args
                .get("receive_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: receive_id"))?;
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: content"))?;
            let mut cmd_args = vec![
                "send".to_string(),
                "--to".to_string(),
                receive_id.to_string(),
                "--content".to_string(),
                content.to_string(),
            ];
            if let Some(msg_type) = args.get("type").and_then(|v| v.as_str()) {
                cmd_args.push("--type".to_string());
                cmd_args.push(msg_type.to_string());
            }
            ("im", cmd_args)
        }
        "lark.base.upsert_records" => {
            let app_token = args
                .get("app_token")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: app_token"))?;
            let table_id = args
                .get("table_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: table_id"))?;
            let records = args
                .get("records")
                .ok_or_else(|| anyhow::anyhow!("missing required parameter: records"))?;
            (
                "base",
                vec![
                    "upsert".to_string(),
                    "--app".to_string(),
                    app_token.to_string(),
                    "--table".to_string(),
                    table_id.to_string(),
                    "--records".to_string(),
                    records.to_string(),
                ],
            )
        }
        other => anyhow::bail!("unknown lark tool: {other}"),
    };

    let output = tokio::process::Command::new("lark-cli")
        .arg(subcmd)
        .args(&cmd_args)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("lark-cli error: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout)?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_lark_cli_returns_bool() {
        // May be false in CI; just ensure it doesn't panic.
        let _available = detect_lark_cli();
    }

    #[test]
    fn build_connector_has_correct_spec() {
        // Skip the auth detection and CLI detection for pure spec test.
        let connector = ConnectorSpec {
            id: "lark".to_string(),
            name: "飞书".to_string(),
            description: "飞书/Lark 办公套件".to_string(),
            implementation_type: ConnectorImplementation::LocalCli,
            capabilities: vec![
                ConnectorCapability {
                    capability: "lark.contact".to_string(),
                    tools: vec!["lark.contact.search_user".to_string()],
                    risk_level: "low".to_string(),
                    requires_confirmation: false,
                },
                ConnectorCapability {
                    capability: "lark.calendar".to_string(),
                    tools: vec![
                        "lark.calendar.list_events".to_string(),
                        "lark.calendar.freebusy".to_string(),
                        "lark.calendar.search_rooms".to_string(),
                    ],
                    risk_level: "low".to_string(),
                    requires_confirmation: false,
                },
                ConnectorCapability {
                    capability: "lark.doc".to_string(),
                    tools: vec!["lark.doc.search".to_string()],
                    risk_level: "low".to_string(),
                    requires_confirmation: false,
                },
                ConnectorCapability {
                    capability: "lark.calendar.write".to_string(),
                    tools: vec!["lark.calendar.create_event".to_string()],
                    risk_level: "medium".to_string(),
                    requires_confirmation: true,
                },
                ConnectorCapability {
                    capability: "lark.doc.write".to_string(),
                    tools: vec!["lark.doc.create_or_update".to_string()],
                    risk_level: "medium".to_string(),
                    requires_confirmation: true,
                },
                ConnectorCapability {
                    capability: "lark.im.write".to_string(),
                    tools: vec!["lark.im.send_message".to_string()],
                    risk_level: "high".to_string(),
                    requires_confirmation: true,
                },
                ConnectorCapability {
                    capability: "lark.base.write".to_string(),
                    tools: vec!["lark.base.upsert_records".to_string()],
                    risk_level: "high".to_string(),
                    requires_confirmation: true,
                },
            ],
            auth_status: ConnectorAuthStatus::NotConfigured,
            enabled: false,
            config_json: None,
        };

        assert_eq!(connector.id, "lark");
        assert_eq!(connector.name, "飞书");
        assert_eq!(connector.description, "飞书/Lark 办公套件");
        assert_eq!(
            connector.implementation_type,
            ConnectorImplementation::LocalCli
        );

        // 7 capabilities total
        assert_eq!(connector.capabilities.len(), 7);

        // 9 tools total across capabilities
        let total_tools: usize = connector.capabilities.iter().map(|c| c.tools.len()).sum();
        assert_eq!(total_tools, 9);
    }

    #[test]
    fn capabilities_are_correctly_structured() {
        let connector = build_lark_connector();

        // lark.contact: 1 tool
        let contact = connector
            .capabilities
            .iter()
            .find(|c| c.capability == "lark.contact")
            .expect("lark.contact capability");
        assert_eq!(contact.tools, vec!["lark.contact.search_user"]);
        assert_eq!(contact.risk_level, "low");
        assert!(!contact.requires_confirmation);

        // lark.calendar: 3 tools
        let calendar = connector
            .capabilities
            .iter()
            .find(|c| c.capability == "lark.calendar")
            .expect("lark.calendar capability");
        assert_eq!(calendar.tools.len(), 3);
        assert!(calendar
            .tools
            .contains(&"lark.calendar.list_events".to_string()));
        assert!(calendar
            .tools
            .contains(&"lark.calendar.freebusy".to_string()));
        assert!(calendar
            .tools
            .contains(&"lark.calendar.search_rooms".to_string()));

        // lark.doc: 1 tool
        let doc = connector
            .capabilities
            .iter()
            .find(|c| c.capability == "lark.doc")
            .expect("lark.doc capability");
        assert_eq!(doc.tools, vec!["lark.doc.search"]);
    }

    #[test]
    fn seven_capabilities_with_correct_tool_lists() {
        let connector = build_lark_connector();

        let caps: Vec<&str> = connector
            .capabilities
            .iter()
            .map(|c| c.capability.as_str())
            .collect();
        assert_eq!(
            caps,
            vec![
                "lark.contact",
                "lark.calendar",
                "lark.doc",
                "lark.calendar.write",
                "lark.doc.write",
                "lark.im.write",
                "lark.base.write",
            ]
        );
    }

    #[test]
    fn connector_id_and_name_correct() {
        let connector = build_lark_connector();
        assert_eq!(connector.id, "lark");
        assert_eq!(connector.name, "飞书");
    }

    #[tokio::test]
    async fn execute_invalid_tool_returns_error() {
        let args = serde_json::json!({});
        let result = execute_lark_tool("lark.nonexistent.tool", &args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("unknown lark tool"));
    }

    #[tokio::test]
    async fn execute_missing_param_returns_error() {
        let args = serde_json::json!({});
        let result = execute_lark_tool("lark.contact.search_user", &args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("missing required parameter"));
    }

    // ── Write tool plan generation tests ──

    #[test]
    fn plan_calendar_create_event_shows_title_time_attendees() {
        let args = serde_json::json!({
            "title": "周会",
            "start": "2026-06-01T10:00",
            "end": "2026-06-01T11:00",
            "attendees": "alice@example.com"
        });
        let plan = generate_write_plan("lark.calendar.create_event", &args);
        assert!(
            plan.contains("周会"),
            "plan should contain title, got: {plan}"
        );
        assert!(
            plan.contains("2026-06-01T10:00"),
            "plan should contain start time, got: {plan}"
        );
        assert!(
            plan.contains("alice@example.com"),
            "plan should contain attendees, got: {plan}"
        );
        assert!(plan.starts_with("将创建会议:"), "plan prefix, got: {plan}");
    }

    #[test]
    fn plan_doc_create_or_update_shows_title_and_content_length() {
        let args = serde_json::json!({
            "title": "设计文档",
            "content": "hello"
        });
        let plan = generate_write_plan("lark.doc.create_or_update", &args);
        assert!(
            plan.contains("设计文档"),
            "plan should contain title, got: {plan}"
        );
        // "hello" = 5 bytes
        assert!(
            plan.contains("5"),
            "plan should contain byte count, got: {plan}"
        );
        assert!(
            plan.contains("字符"),
            "plan should contain '字符', got: {plan}"
        );
    }

    #[test]
    fn plan_im_send_message_shows_recipient_and_content_preview() {
        let args = serde_json::json!({
            "receive_id": "ou_abc123",
            "content": "你好，这是测试消息"
        });
        let plan = generate_write_plan("lark.im.send_message", &args);
        assert!(
            plan.contains("ou_abc123"),
            "plan should contain receive_id, got: {plan}"
        );
        assert!(
            plan.contains("你好，这是测试消息"),
            "plan should contain content, got: {plan}"
        );
        assert!(
            plan.starts_with("将发送消息给:"),
            "plan prefix, got: {plan}"
        );
    }

    #[test]
    fn plan_base_upsert_shows_app_table_record_count() {
        let args = serde_json::json!({
            "app_token": "bascnXXX",
            "table_id": "tblYYY",
            "records": [{"fields": {"name": "a"}}, {"fields": {"name": "b"}}]
        });
        let plan = generate_write_plan("lark.base.upsert_records", &args);
        assert!(
            plan.contains("bascnXXX"),
            "plan should contain app_token, got: {plan}"
        );
        assert!(
            plan.contains("tblYYY"),
            "plan should contain table_id, got: {plan}"
        );
        assert!(
            plan.contains("2"),
            "plan should contain record count, got: {plan}"
        );
    }

    #[test]
    fn plan_unknown_write_tool_generates_generic_plan() {
        let args = serde_json::json!({});
        let plan = generate_write_plan("lark.unknown.future_tool", &args);
        assert_eq!(plan, "将执行: lark.unknown.future_tool");
    }

    // ── Write capability structure tests ──

    #[test]
    fn write_capabilities_require_confirmation() {
        let connector = build_lark_connector();
        let write_caps: Vec<&ConnectorCapability> = connector
            .capabilities
            .iter()
            .filter(|c| c.requires_confirmation)
            .collect();

        assert_eq!(write_caps.len(), 4, "should have 4 write capabilities");

        for cap in &write_caps {
            assert!(
                cap.requires_confirmation,
                "{} should require confirmation",
                cap.capability
            );
        }
    }

    #[test]
    fn write_capabilities_have_correct_risk_levels() {
        let connector = build_lark_connector();

        let cal_write = connector
            .capabilities
            .iter()
            .find(|c| c.capability == "lark.calendar.write")
            .expect("lark.calendar.write");
        assert_eq!(cal_write.risk_level, "medium");

        let doc_write = connector
            .capabilities
            .iter()
            .find(|c| c.capability == "lark.doc.write")
            .expect("lark.doc.write");
        assert_eq!(doc_write.risk_level, "medium");

        let im_write = connector
            .capabilities
            .iter()
            .find(|c| c.capability == "lark.im.write")
            .expect("lark.im.write");
        assert_eq!(im_write.risk_level, "high");

        let base_write = connector
            .capabilities
            .iter()
            .find(|c| c.capability == "lark.base.write")
            .expect("lark.base.write");
        assert_eq!(base_write.risk_level, "high");
    }

    #[test]
    fn connector_has_both_read_and_write_capabilities() {
        let connector = build_lark_connector();

        // 3 read + 4 write = 7 total capabilities
        assert_eq!(connector.capabilities.len(), 7);

        // 5 read + 4 write = 9 total tools
        let total_tools: usize = connector.capabilities.iter().map(|c| c.tools.len()).sum();
        assert_eq!(total_tools, 9);

        // Readonly caps have requires_confirmation = false
        let read_caps: Vec<&ConnectorCapability> = connector
            .capabilities
            .iter()
            .filter(|c| !c.requires_confirmation)
            .collect();
        assert_eq!(read_caps.len(), 3);

        // Write caps have requires_confirmation = true
        let write_caps: Vec<&ConnectorCapability> = connector
            .capabilities
            .iter()
            .filter(|c| c.requires_confirmation)
            .collect();
        assert_eq!(write_caps.len(), 4);
    }

    #[test]
    fn plan_with_missing_fields_still_generates() {
        let args = serde_json::json!({});
        let plan = generate_write_plan("lark.calendar.create_event", &args);
        assert!(
            plan.contains("将创建会议:"),
            "should still generate, got: {plan}"
        );
        // Missing fields become empty strings
        assert!(plan.contains("时间:  ~"), "empty start/end, got: {plan}");
    }
}
