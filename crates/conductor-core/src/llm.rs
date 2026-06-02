use crate::config::LlmConfig;
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
use tokio::time::timeout;

const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LlmProtocol {
    OpenAiCompatible,
    AnthropicCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenAiAuthScheme {
    Bearer,
    ApiKeyHeader,
    XApiKeyHeader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnthropicAuthScheme {
    XApiKeyHeader,
    ApiKeyHeader,
    Bearer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenLimitField {
    MaxTokens,
    MaxCompletionTokens,
}

#[derive(Debug)]
struct LlmHttpError {
    status: u16,
    body: String,
}

#[derive(Debug, Clone)]
pub struct LlmRequestConfig<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub base_url: &'a str,
    pub api_key: Option<&'a str>,
    pub temperature: f64,
    /// Controls how the model uses tools: "auto", "none", "required", or force a specific tool.
    pub tool_choice: Option<serde_json::Value>,
}

impl<'a> LlmRequestConfig<'a> {
    pub fn from_config(config: &'a LlmConfig) -> Self {
        Self {
            provider: &config.provider,
            model: &config.model,
            base_url: &config.base_url,
            api_key: config.api_key.as_deref(),
            temperature: config.temperature,
            tool_choice: None,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Serialize, Clone)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Reasoning/thinking content from the model (e.g. OpenAI reasoning_content, Anthropic thinking).
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmStreamEvent {
    Text(String),
    Reasoning(String),
}

#[derive(Serialize, Clone)]
pub struct OpenaiMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct OpenaiRequest {
    model: String,
    messages: Vec<OpenaiMessage>,
    temperature: f64,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: serde_json::Value,
}

#[derive(Serialize)]
struct AnthropicToolDefinition {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct OpenaiResponse {
    choices: Vec<OpenaiChoice>,
}

#[derive(Deserialize)]
struct OpenaiChoice {
    message: OpenaiResponseMessage,
}

#[derive(Deserialize)]
struct OpenaiResponseMessage {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
}

pub async fn call(
    model: &str,
    system: &str,
    user: &str,
    config: &LlmRequestConfig<'_>,
) -> anyhow::Result<String> {
    let client = build_client()?;
    match protocol(config) {
        LlmProtocol::OpenAiCompatible => {
            let url = build_openai_chat_url(config)?;
            let request = build_openai_request(model, system, user, config);
            let response =
                execute_openai_with_timeout(client, url, request, config.api_key).await?;
            parse_openai_response(response).await
        }
        LlmProtocol::AnthropicCompatible => {
            let url = build_anthropic_messages_url(config)?;
            let request = build_anthropic_request(model, system, user, config);
            let response =
                execute_anthropic_with_timeout(client, url, request, config.api_key).await?;
            parse_anthropic_response(response).await
        }
    }
}

pub async fn call_with_tools(
    model: &str,
    system: &str,
    user: &str,
    config: &LlmRequestConfig<'_>,
    tools: Option<Vec<ToolDefinition>>,
) -> anyhow::Result<LlmResponse> {
    let client = build_client()?;
    match protocol(config) {
        LlmProtocol::OpenAiCompatible => {
            let url = build_openai_chat_url(config)?;
            let request = build_openai_request_with_tools(model, system, user, config, tools);
            let response =
                execute_openai_with_timeout(client, url, request, config.api_key).await?;
            parse_openai_response_with_tools(response).await
        }
        LlmProtocol::AnthropicCompatible => {
            let url = build_anthropic_messages_url(config)?;
            let request = build_anthropic_request_with_tools(model, system, user, config, tools);
            let response =
                execute_anthropic_with_timeout(client, url, request, config.api_key).await?;
            parse_anthropic_response_with_tools(response).await
        }
    }
}

pub async fn call_with_tools_with_messages(
    model: &str,
    messages: &[OpenaiMessage],
    config: &LlmRequestConfig<'_>,
    tools: Option<Vec<ToolDefinition>>,
) -> anyhow::Result<LlmResponse> {
    let client = build_client()?;
    match protocol(config) {
        LlmProtocol::OpenAiCompatible => {
            let url = build_openai_chat_url(config)?;
            let request = OpenaiRequest {
                model: model.to_string(),
                messages: messages.to_vec(),
                temperature: config.temperature,
                max_tokens: 1500,
                tools,
                tool_choice: config.tool_choice.clone(),
                stream: None,
            };
            let response =
                execute_openai_with_timeout(client, url, request, config.api_key).await?;
            parse_openai_response_with_tools(response).await
        }
        LlmProtocol::AnthropicCompatible => {
            let url = build_anthropic_messages_url(config)?;
            let request =
                build_anthropic_request_from_messages(model, messages, config, tools, 1500)?;
            let response =
                execute_anthropic_with_timeout(client, url, request, config.api_key).await?;
            parse_anthropic_response_with_tools(response).await
        }
    }
}

pub async fn call_with_tools_with_messages_streaming<F>(
    model: &str,
    messages: &[OpenaiMessage],
    config: &LlmRequestConfig<'_>,
    tools: Option<Vec<ToolDefinition>>,
    on_event: F,
) -> anyhow::Result<LlmResponse>
where
    F: FnMut(LlmStreamEvent),
{
    let client = build_streaming_client()?;
    let mut on_event = on_event;
    match protocol(config) {
        LlmProtocol::OpenAiCompatible => {
            let url = build_openai_chat_url(config)?;
            let request = OpenaiRequest {
                model: model.to_string(),
                messages: messages.to_vec(),
                temperature: config.temperature,
                max_tokens: 1500,
                tools,
                tool_choice: config.tool_choice.clone(),
                stream: Some(true),
            };
            execute_openai_streaming_with_timeout(
                client,
                url,
                request,
                config.api_key,
                &mut on_event,
            )
            .await
        }
        LlmProtocol::AnthropicCompatible => {
            let url = build_anthropic_messages_url(config)?;
            let mut request =
                build_anthropic_request_from_messages(model, messages, config, tools, 1500)?;
            request.stream = Some(true);
            execute_anthropic_streaming_with_timeout(
                client,
                url,
                request,
                config.api_key,
                &mut on_event,
            )
            .await
        }
    }
}

fn protocol(config: &LlmRequestConfig<'_>) -> LlmProtocol {
    match config.provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" | "claude" | "anthropic_compatible" => LlmProtocol::AnthropicCompatible,
        _ => LlmProtocol::OpenAiCompatible,
    }
}

fn build_openai_chat_url(config: &LlmRequestConfig<'_>) -> anyhow::Result<String> {
    build_endpoint_url(config.base_url, "chat/completions")
}

fn build_anthropic_messages_url(config: &LlmRequestConfig<'_>) -> anyhow::Result<String> {
    build_anthropic_endpoint_url(config.base_url)
}

fn build_endpoint_url(base_url: &str, endpoint: &str) -> anyhow::Result<String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        bail!("LLM base URL cannot be empty");
    }

    let (base_without_query, query) = split_query(base);
    let base_without_query = base_without_query.trim_end_matches('/');
    if base_without_query.ends_with(endpoint) {
        Ok(base.to_string())
    } else {
        Ok(join_endpoint(base_without_query, endpoint, query))
    }
}

fn build_anthropic_endpoint_url(base_url: &str) -> anyhow::Result<String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        bail!("LLM base URL cannot be empty");
    }

    let (base_without_query, query) = split_query(base);
    let base_without_query = base_without_query.trim_end_matches('/');
    if base_without_query.ends_with("/messages") || base_without_query.ends_with("messages") {
        return Ok(base.to_string());
    }

    let endpoint = if base_without_query.ends_with("/v1") {
        "messages"
    } else {
        "v1/messages"
    };

    Ok(join_endpoint(base_without_query, endpoint, query))
}

fn split_query(url: &str) -> (&str, Option<&str>) {
    match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    }
}

fn join_endpoint(base_without_query: &str, endpoint: &str, query: Option<&str>) -> String {
    let url = format!(
        "{}/{}",
        base_without_query.trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    );
    match query {
        Some(query) if !query.is_empty() => format!("{url}?{query}"),
        _ => url,
    }
}

fn build_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build()
        .context("failed to build HTTP client")
}

fn build_streaming_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build()
        .context("failed to build streaming HTTP client")
}

fn build_openai_request<'a>(
    model: &str,
    system: &'a str,
    user: &'a str,
    config: &'a LlmRequestConfig<'_>,
) -> OpenaiRequest {
    OpenaiRequest {
        model: model.to_string(),
        messages: vec![
            OpenaiMessage {
                role: "system".to_string(),
                content: Some(system.to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            OpenaiMessage {
                role: "user".to_string(),
                content: Some(user.to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        temperature: config.temperature,
        max_tokens: 500,
        tools: None,
        tool_choice: None,
        stream: None,
    }
}

fn build_openai_request_with_tools(
    model: &str,
    system: &str,
    user: &str,
    config: &LlmRequestConfig<'_>,
    tools: Option<Vec<ToolDefinition>>,
) -> OpenaiRequest {
    OpenaiRequest {
        model: model.to_string(),
        messages: vec![
            OpenaiMessage {
                role: "system".to_string(),
                content: Some(system.to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            OpenaiMessage {
                role: "user".to_string(),
                content: Some(user.to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        temperature: config.temperature,
        max_tokens: 1500,
        tools,
        tool_choice: config.tool_choice.clone(),
        stream: None,
    }
}

fn build_anthropic_request(
    model: &str,
    system: &str,
    user: &str,
    config: &LlmRequestConfig<'_>,
) -> AnthropicRequest {
    AnthropicRequest {
        model: model.to_string(),
        max_tokens: 500,
        temperature: config.temperature,
        system: non_empty_string(system),
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::Value::String(user.to_string()),
        }],
        tools: None,
        tool_choice: None,
        stream: None,
    }
}

fn build_anthropic_request_with_tools(
    model: &str,
    system: &str,
    user: &str,
    config: &LlmRequestConfig<'_>,
    tools: Option<Vec<ToolDefinition>>,
) -> AnthropicRequest {
    AnthropicRequest {
        model: model.to_string(),
        max_tokens: 1500,
        temperature: config.temperature,
        system: non_empty_string(system),
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::Value::String(user.to_string()),
        }],
        tools: convert_tools_for_anthropic(tools),
        tool_choice: config.tool_choice.clone(),
        stream: None,
    }
}

fn build_anthropic_request_from_messages(
    model: &str,
    messages: &[OpenaiMessage],
    config: &LlmRequestConfig<'_>,
    tools: Option<Vec<ToolDefinition>>,
    max_tokens: u32,
) -> anyhow::Result<AnthropicRequest> {
    let mut system_parts = Vec::new();
    let mut anthropic_messages = Vec::new();

    for message in messages {
        match message.role.as_str() {
            "system" => {
                if let Some(content) = message.content.as_deref().filter(|c| !c.trim().is_empty()) {
                    system_parts.push(content.to_string());
                }
            }
            "assistant" => {
                anthropic_messages.push(AnthropicMessage {
                    role: "assistant".to_string(),
                    content: assistant_content_for_anthropic(message),
                });
            }
            "tool" => {
                let tool_use_id = message
                    .tool_call_id
                    .clone()
                    .context("Anthropic tool result is missing tool_call_id")?;
                anthropic_messages.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: json!([{
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": message.content.clone().unwrap_or_default()
                    }]),
                });
            }
            "user" => {
                anthropic_messages.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::Value::String(message.content.clone().unwrap_or_default()),
                });
            }
            _ => {
                anthropic_messages.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::Value::String(message.content.clone().unwrap_or_default()),
                });
            }
        }
    }

    if anthropic_messages.is_empty() {
        bail!("Anthropic request has no user or assistant messages");
    }

    Ok(AnthropicRequest {
        model: model.to_string(),
        max_tokens,
        temperature: config.temperature,
        system: non_empty_string(&system_parts.join("\n\n")),
        messages: anthropic_messages,
        tools: convert_tools_for_anthropic(tools),
        tool_choice: config.tool_choice.clone(),
        stream: None,
    })
}

fn assistant_content_for_anthropic(message: &OpenaiMessage) -> serde_json::Value {
    let tool_calls = message.tool_calls.clone().unwrap_or_default();
    if tool_calls.is_empty() {
        return serde_json::Value::String(message.content.clone().unwrap_or_default());
    }

    let mut blocks = Vec::new();
    if let Some(content) = message.content.as_deref().filter(|c| !c.trim().is_empty()) {
        blocks.push(json!({
            "type": "text",
            "text": content
        }));
    }

    for call in tool_calls {
        let input: serde_json::Value =
            serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| json!({}));
        blocks.push(json!({
            "type": "tool_use",
            "id": call.id,
            "name": call.function.name,
            "input": input
        }));
    }

    serde_json::Value::Array(blocks)
}

fn convert_tools_for_anthropic(
    tools: Option<Vec<ToolDefinition>>,
) -> Option<Vec<AnthropicToolDefinition>> {
    let converted = tools?
        .into_iter()
        .map(|tool| AnthropicToolDefinition {
            name: tool.function.name,
            description: tool.function.description,
            input_schema: tool.function.parameters,
        })
        .collect::<Vec<_>>();

    if converted.is_empty() {
        None
    } else {
        Some(converted)
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

async fn execute_openai_with_timeout(
    client: reqwest::Client,
    url: String,
    request: OpenaiRequest,
    api_key: Option<&str>,
) -> anyhow::Result<String> {
    let api_key = api_key
        .map(|k| k.to_string())
        .or_else(|| request_api_key(LlmProtocol::OpenAiCompatible, &url));

    let preferred_auth = preferred_openai_auth_scheme(&url, api_key.as_deref());
    let preferred_token_field = preferred_token_limit_field(&url);
    let mut attempts = Vec::new();
    push_unique(&mut attempts, (preferred_auth, preferred_token_field));
    push_unique(
        &mut attempts,
        (preferred_auth, alternate_token_field(preferred_token_field)),
    );
    if api_key.is_some() {
        for auth in [
            OpenAiAuthScheme::Bearer,
            OpenAiAuthScheme::ApiKeyHeader,
            OpenAiAuthScheme::XApiKeyHeader,
        ] {
            push_unique(&mut attempts, (auth, preferred_token_field));
            push_unique(
                &mut attempts,
                (auth, alternate_token_field(preferred_token_field)),
            );
        }
    }

    let mut last_error: Option<LlmHttpError> = None;
    for (auth_scheme, token_field) in attempts {
        let body = openai_request_body(&request, token_field)?;
        let mut req_builder = client.post(&url).json(&body);
        if let Some(api_key) = api_key.as_deref() {
            req_builder = apply_openai_auth(req_builder, auth_scheme, api_key);
        }

        match execute_request_raw(req_builder).await? {
            Ok(text) => {
                record_llm_attempt(LlmAttemptLog {
                    protocol: "openai_compatible",
                    model: &request.model,
                    url: &url,
                    auth_scheme: openai_auth_name(auth_scheme),
                    token_field: Some(token_field_name(token_field)),
                    status: None,
                    success: true,
                    error: None,
                })
                .await;
                return Ok(text);
            }
            Err(err) => {
                let retry =
                    is_auth_error(err.status) || is_token_limit_error(err.status, &err.body);
                record_llm_attempt(LlmAttemptLog {
                    protocol: "openai_compatible",
                    model: &request.model,
                    url: &url,
                    auth_scheme: openai_auth_name(auth_scheme),
                    token_field: Some(token_field_name(token_field)),
                    status: Some(err.status),
                    success: false,
                    error: Some(&err.body),
                })
                .await;
                last_error = Some(err);
                if !retry {
                    break;
                }
            }
        }
    }

    bail_http_error(last_error)
}

async fn execute_anthropic_with_timeout(
    client: reqwest::Client,
    url: String,
    request: AnthropicRequest,
    api_key: Option<&str>,
) -> anyhow::Result<String> {
    let api_key = api_key
        .map(|k| k.to_string())
        .or_else(|| request_api_key(LlmProtocol::AnthropicCompatible, &url));

    let mut attempts = Vec::new();
    let preferred = preferred_anthropic_auth_scheme(&url, api_key.as_deref());
    push_unique(&mut attempts, preferred);
    if api_key.is_some() {
        push_unique(&mut attempts, AnthropicAuthScheme::XApiKeyHeader);
        push_unique(&mut attempts, AnthropicAuthScheme::ApiKeyHeader);
        push_unique(&mut attempts, AnthropicAuthScheme::Bearer);
    }

    let mut last_error: Option<LlmHttpError> = None;
    for auth_scheme in attempts {
        let mut req_builder = client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .json(&request);
        if let Some(api_key) = api_key.as_deref() {
            req_builder = apply_anthropic_auth(req_builder, auth_scheme, api_key);
        }

        match execute_request_raw(req_builder).await? {
            Ok(text) => {
                record_llm_attempt(LlmAttemptLog {
                    protocol: "anthropic_compatible",
                    model: &request.model,
                    url: &url,
                    auth_scheme: anthropic_auth_name(auth_scheme),
                    token_field: None,
                    status: None,
                    success: true,
                    error: None,
                })
                .await;
                return Ok(text);
            }
            Err(err) => {
                let retry = is_auth_error(err.status);
                record_llm_attempt(LlmAttemptLog {
                    protocol: "anthropic_compatible",
                    model: &request.model,
                    url: &url,
                    auth_scheme: anthropic_auth_name(auth_scheme),
                    token_field: None,
                    status: Some(err.status),
                    success: false,
                    error: Some(&err.body),
                })
                .await;
                last_error = Some(err);
                if !retry {
                    break;
                }
            }
        }
    }

    bail_http_error(last_error)
}

async fn execute_openai_streaming_with_timeout<F>(
    client: reqwest::Client,
    url: String,
    request: OpenaiRequest,
    api_key: Option<&str>,
    on_event: &mut F,
) -> anyhow::Result<LlmResponse>
where
    F: FnMut(LlmStreamEvent),
{
    let api_key = api_key
        .map(|k| k.to_string())
        .or_else(|| request_api_key(LlmProtocol::OpenAiCompatible, &url));

    let preferred_auth = preferred_openai_auth_scheme(&url, api_key.as_deref());
    let preferred_token_field = preferred_token_limit_field(&url);
    let mut attempts = Vec::new();
    push_unique(&mut attempts, (preferred_auth, preferred_token_field));
    push_unique(
        &mut attempts,
        (preferred_auth, alternate_token_field(preferred_token_field)),
    );
    if api_key.is_some() {
        for auth in [
            OpenAiAuthScheme::Bearer,
            OpenAiAuthScheme::ApiKeyHeader,
            OpenAiAuthScheme::XApiKeyHeader,
        ] {
            push_unique(&mut attempts, (auth, preferred_token_field));
            push_unique(
                &mut attempts,
                (auth, alternate_token_field(preferred_token_field)),
            );
        }
    }

    let mut last_error: Option<LlmHttpError> = None;
    for (auth_scheme, token_field) in attempts {
        let body = openai_request_body(&request, token_field)?;
        let mut req_builder = client.post(&url).json(&body);
        if let Some(api_key) = api_key.as_deref() {
            req_builder = apply_openai_auth(req_builder, auth_scheme, api_key);
        }

        record_llm_attempt_start(
            "openai_compatible_stream",
            &request.model,
            &url,
            openai_auth_name(auth_scheme),
            Some(token_field_name(token_field)),
        )
        .await;
        match execute_streaming_request_raw(req_builder).await? {
            Ok(response) => {
                record_llm_attempt(LlmAttemptLog {
                    protocol: "openai_compatible_stream",
                    model: &request.model,
                    url: &url,
                    auth_scheme: openai_auth_name(auth_scheme),
                    token_field: Some(token_field_name(token_field)),
                    status: None,
                    success: true,
                    error: None,
                })
                .await;
                return parse_openai_streaming_response(response, on_event).await;
            }
            Err(err) => {
                let retry =
                    is_auth_error(err.status) || is_token_limit_error(err.status, &err.body);
                record_llm_attempt(LlmAttemptLog {
                    protocol: "openai_compatible_stream",
                    model: &request.model,
                    url: &url,
                    auth_scheme: openai_auth_name(auth_scheme),
                    token_field: Some(token_field_name(token_field)),
                    status: Some(err.status),
                    success: false,
                    error: Some(&err.body),
                })
                .await;
                last_error = Some(err);
                if !retry {
                    break;
                }
            }
        }
    }

    bail_http_error(last_error).map(|_| unreachable!())
}

async fn execute_anthropic_streaming_with_timeout<F>(
    client: reqwest::Client,
    url: String,
    request: AnthropicRequest,
    api_key: Option<&str>,
    on_event: &mut F,
) -> anyhow::Result<LlmResponse>
where
    F: FnMut(LlmStreamEvent),
{
    let api_key = api_key
        .map(|k| k.to_string())
        .or_else(|| request_api_key(LlmProtocol::AnthropicCompatible, &url));

    let mut attempts = Vec::new();
    let preferred = preferred_anthropic_auth_scheme(&url, api_key.as_deref());
    push_unique(&mut attempts, preferred);
    if api_key.is_some() {
        push_unique(&mut attempts, AnthropicAuthScheme::XApiKeyHeader);
        push_unique(&mut attempts, AnthropicAuthScheme::ApiKeyHeader);
        push_unique(&mut attempts, AnthropicAuthScheme::Bearer);
    }

    let mut last_error: Option<LlmHttpError> = None;
    for auth_scheme in attempts {
        let mut req_builder = client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .json(&request);
        if let Some(api_key) = api_key.as_deref() {
            req_builder = apply_anthropic_auth(req_builder, auth_scheme, api_key);
        }

        record_llm_attempt_start(
            "anthropic_compatible_stream",
            &request.model,
            &url,
            anthropic_auth_name(auth_scheme),
            None,
        )
        .await;
        match execute_streaming_request_raw(req_builder).await? {
            Ok(response) => {
                record_llm_attempt(LlmAttemptLog {
                    protocol: "anthropic_compatible_stream",
                    model: &request.model,
                    url: &url,
                    auth_scheme: anthropic_auth_name(auth_scheme),
                    token_field: None,
                    status: None,
                    success: true,
                    error: None,
                })
                .await;
                return parse_anthropic_streaming_response(response, on_event).await;
            }
            Err(err) => {
                let retry = is_auth_error(err.status);
                record_llm_attempt(LlmAttemptLog {
                    protocol: "anthropic_compatible_stream",
                    model: &request.model,
                    url: &url,
                    auth_scheme: anthropic_auth_name(auth_scheme),
                    token_field: None,
                    status: Some(err.status),
                    success: false,
                    error: Some(&err.body),
                })
                .await;
                last_error = Some(err);
                if !retry {
                    break;
                }
            }
        }
    }

    bail_http_error(last_error).map(|_| unreachable!())
}

async fn execute_request_raw(
    req_builder: reqwest::RequestBuilder,
) -> anyhow::Result<Result<String, LlmHttpError>> {
    let response = timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS), async move {
        req_builder.send().await
    })
    .await
    .context("LLM request timed out")?
    .context("LLM request failed")?;

    let status = response.status();
    let text = response
        .text()
        .await
        .context("failed to read LLM response body")?;
    if !status.is_success() {
        return Ok(Err(LlmHttpError {
            status: status.as_u16(),
            body: text,
        }));
    }

    Ok(Ok(text))
}

async fn execute_streaming_request_raw(
    req_builder: reqwest::RequestBuilder,
) -> anyhow::Result<Result<reqwest::Response, LlmHttpError>> {
    let response = timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS), async move {
        req_builder.send().await
    })
    .await
    .context("LLM stream request timed out")?
    .context("LLM stream request failed")?;

    let status = response.status();
    if !status.is_success() {
        let text = response
            .text()
            .await
            .context("failed to read LLM stream error body")?;
        return Ok(Err(LlmHttpError {
            status: status.as_u16(),
            body: text,
        }));
    }

    Ok(Ok(response))
}

fn preferred_openai_auth_scheme(url: &str, api_key: Option<&str>) -> OpenAiAuthScheme {
    if uses_api_key_header(url, api_key) {
        OpenAiAuthScheme::ApiKeyHeader
    } else {
        OpenAiAuthScheme::Bearer
    }
}

fn preferred_anthropic_auth_scheme(url: &str, api_key: Option<&str>) -> AnthropicAuthScheme {
    if uses_api_key_header(url, api_key) {
        AnthropicAuthScheme::ApiKeyHeader
    } else {
        AnthropicAuthScheme::XApiKeyHeader
    }
}

fn uses_api_key_header(url: &str, api_key: Option<&str>) -> bool {
    let lower_url = url.to_ascii_lowercase();
    lower_url.contains("xiaomimimo.com")
        || lower_url.contains("openai.azure.com")
        || api_key
            .map(|key| key.trim_start().starts_with("tp-"))
            .unwrap_or(false)
}

fn preferred_token_limit_field(url: &str) -> TokenLimitField {
    if url.to_ascii_lowercase().contains("xiaomimimo.com") {
        TokenLimitField::MaxCompletionTokens
    } else {
        TokenLimitField::MaxTokens
    }
}

fn alternate_token_field(field: TokenLimitField) -> TokenLimitField {
    match field {
        TokenLimitField::MaxTokens => TokenLimitField::MaxCompletionTokens,
        TokenLimitField::MaxCompletionTokens => TokenLimitField::MaxTokens,
    }
}

fn openai_request_body(
    request: &OpenaiRequest,
    token_field: TokenLimitField,
) -> anyhow::Result<serde_json::Value> {
    let mut body = serde_json::to_value(request)?;
    if token_field == TokenLimitField::MaxCompletionTokens {
        if let Some(max_tokens) = body.get("max_tokens").cloned() {
            if let Some(object) = body.as_object_mut() {
                object.remove("max_tokens");
                object.insert("max_completion_tokens".to_string(), max_tokens);
            }
        }
    }
    Ok(body)
}

fn apply_openai_auth(
    req_builder: reqwest::RequestBuilder,
    auth_scheme: OpenAiAuthScheme,
    api_key: &str,
) -> reqwest::RequestBuilder {
    match auth_scheme {
        OpenAiAuthScheme::Bearer => {
            req_builder.header("Authorization", format!("Bearer {api_key}"))
        }
        OpenAiAuthScheme::ApiKeyHeader => req_builder.header("api-key", api_key),
        OpenAiAuthScheme::XApiKeyHeader => req_builder.header("x-api-key", api_key),
    }
}

fn apply_anthropic_auth(
    req_builder: reqwest::RequestBuilder,
    auth_scheme: AnthropicAuthScheme,
    api_key: &str,
) -> reqwest::RequestBuilder {
    match auth_scheme {
        AnthropicAuthScheme::XApiKeyHeader => req_builder.header("x-api-key", api_key),
        AnthropicAuthScheme::ApiKeyHeader => req_builder.header("api-key", api_key),
        AnthropicAuthScheme::Bearer => {
            req_builder.header("Authorization", format!("Bearer {api_key}"))
        }
    }
}

fn openai_auth_name(auth_scheme: OpenAiAuthScheme) -> &'static str {
    match auth_scheme {
        OpenAiAuthScheme::Bearer => "bearer",
        OpenAiAuthScheme::ApiKeyHeader => "api-key",
        OpenAiAuthScheme::XApiKeyHeader => "x-api-key",
    }
}

fn anthropic_auth_name(auth_scheme: AnthropicAuthScheme) -> &'static str {
    match auth_scheme {
        AnthropicAuthScheme::XApiKeyHeader => "x-api-key",
        AnthropicAuthScheme::ApiKeyHeader => "api-key",
        AnthropicAuthScheme::Bearer => "bearer",
    }
}

fn token_field_name(field: TokenLimitField) -> &'static str {
    match field {
        TokenLimitField::MaxTokens => "max_tokens",
        TokenLimitField::MaxCompletionTokens => "max_completion_tokens",
    }
}

struct LlmAttemptLog<'a> {
    protocol: &'a str,
    model: &'a str,
    url: &'a str,
    auth_scheme: &'a str,
    token_field: Option<&'a str>,
    status: Option<u16>,
    success: bool,
    error: Option<&'a str>,
}

async fn record_llm_attempt_start(
    protocol: &str,
    model: &str,
    url: &str,
    auth_scheme: &str,
    token_field: Option<&str>,
) {
    let _ = crate::events::append(
        "llm",
        "request_attempt_start",
        &json!({
            "protocol": protocol,
            "model": model,
            "url": redact_url(url),
            "auth_scheme": auth_scheme,
            "token_field": token_field,
        }),
    )
    .await;
}

async fn record_llm_attempt(log: LlmAttemptLog<'_>) {
    let _ = crate::events::append(
        "llm",
        "request_attempt",
        &json!({
            "protocol": log.protocol,
            "model": log.model,
            "url": redact_url(log.url),
            "auth_scheme": log.auth_scheme,
            "token_field": log.token_field,
            "status": log.status,
            "success": log.success,
            "error_preview": log.error.map(response_preview),
        }),
    )
    .await;
}

fn redact_url(url: &str) -> String {
    let (base, query) = split_query(url);
    match query {
        Some(query) if !query.is_empty() => {
            let keys = query
                .split('&')
                .map(|part| part.split_once('=').map(|(key, _)| key).unwrap_or(part))
                .collect::<Vec<_>>()
                .join("&");
            format!("{base}?{keys}=<redacted>")
        }
        _ => base.to_string(),
    }
}

fn is_auth_error(status: u16) -> bool {
    status == 401 || status == 403
}

fn is_token_limit_error(status: u16, body: &str) -> bool {
    if status != 400 && status != 422 {
        return false;
    }
    let body = body.to_ascii_lowercase();
    (body.contains("max_tokens") || body.contains("max_completion_tokens"))
        && (body.contains("unsupported")
            || body.contains("unrecognized")
            || body.contains("unknown")
            || body.contains("invalid")
            || body.contains("max_completion_tokens"))
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

fn bail_http_error(last_error: Option<LlmHttpError>) -> anyhow::Result<String> {
    if let Some(err) = last_error {
        bail!(
            "LLM request failed with HTTP {}: {}",
            err.status,
            response_preview(&err.body)
        );
    }
    bail!("LLM request failed before an HTTP response was received")
}

fn request_api_key(protocol: LlmProtocol, url: &str) -> Option<String> {
    let env_names: &[&str] = match protocol {
        LlmProtocol::OpenAiCompatible if url.contains("openai.com") => {
            &["OPENAI_API_KEY", "LLM_API_KEY"]
        }
        LlmProtocol::OpenAiCompatible => &["LLM_API_KEY", "OPENAI_API_KEY"],
        LlmProtocol::AnthropicCompatible => &["ANTHROPIC_API_KEY", "LLM_API_KEY"],
    };

    env_names
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
}

struct SseEvent {
    event: Option<String>,
    data: String,
}

#[derive(Default)]
struct OpenaiStreamToolCall {
    id: Option<String>,
    call_type: Option<String>,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct OpenaiStreamState {
    content: String,
    reasoning_content: String,
    tool_calls: BTreeMap<usize, OpenaiStreamToolCall>,
}

#[derive(Default)]
struct AnthropicStreamBlock {
    block_type: String,
    id: Option<String>,
    name: Option<String>,
    text: String,
    input_json: String,
}

#[derive(Default)]
struct AnthropicStreamState {
    content: String,
    reasoning_content: String,
    blocks: BTreeMap<usize, AnthropicStreamBlock>,
}

async fn parse_sse_response<F>(
    mut response: reqwest::Response,
    mut handle_event: F,
) -> anyhow::Result<()>
where
    F: FnMut(SseEvent) -> anyhow::Result<bool>,
{
    let mut buffer = Vec::new();

    loop {
        let chunk = timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS), response.chunk())
            .await
            .context("LLM stream chunk timed out")?
            .context("failed to read LLM stream chunk")?;
        let Some(chunk) = chunk else {
            break;
        };

        buffer.extend_from_slice(&chunk);
        while let Some((event_end, delimiter_len)) = find_sse_boundary(&buffer) {
            let event_bytes = buffer[..event_end].to_vec();
            buffer.drain(..event_end + delimiter_len);
            if let Some(event) = parse_sse_event(&event_bytes)? {
                if !handle_event(event)? {
                    return Ok(());
                }
            }
        }
    }

    if !buffer.is_empty() {
        if let Some(event) = parse_sse_event(&buffer)? {
            let _ = handle_event(event)?;
        }
    }

    Ok(())
}

fn find_sse_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    if buffer.len() < 2 {
        return None;
    }

    for i in 0..buffer.len() - 1 {
        if i + 3 < buffer.len()
            && buffer[i] == b'\r'
            && buffer[i + 1] == b'\n'
            && buffer[i + 2] == b'\r'
            && buffer[i + 3] == b'\n'
        {
            return Some((i, 4));
        }
        if buffer[i] == b'\n' && buffer[i + 1] == b'\n' {
            return Some((i, 2));
        }
    }

    None
}

fn parse_sse_event(raw: &[u8]) -> anyhow::Result<Option<SseEvent>> {
    let text = std::str::from_utf8(raw).context("failed to decode LLM SSE event as UTF-8")?;
    let mut event = None;
    let mut data = Vec::new();

    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start().to_string());
        }
    }

    if event.is_none() && data.is_empty() {
        return Ok(None);
    }

    Ok(Some(SseEvent {
        event,
        data: data.join("\n"),
    }))
}

async fn parse_openai_streaming_response<F>(
    response: reqwest::Response,
    on_event: &mut F,
) -> anyhow::Result<LlmResponse>
where
    F: FnMut(LlmStreamEvent),
{
    let mut state = OpenaiStreamState::default();
    parse_sse_response(response, |event| {
        apply_openai_stream_event(&mut state, event, on_event)
    })
    .await?;

    let tool_calls = openai_stream_tool_calls(state.tool_calls)?;
    Ok(LlmResponse {
        content: (!state.content.is_empty()).then_some(state.content),
        tool_calls,
        reasoning_content: (!state.reasoning_content.is_empty()).then_some(state.reasoning_content),
    })
}

fn apply_openai_stream_event<F>(
    state: &mut OpenaiStreamState,
    event: SseEvent,
    on_event: &mut F,
) -> anyhow::Result<bool>
where
    F: FnMut(LlmStreamEvent),
{
    let data = event.data.trim();
    if data.is_empty() {
        return Ok(true);
    }
    if data == "[DONE]" {
        return Ok(false);
    }

    let value: serde_json::Value = serde_json::from_str(data).with_context(|| {
        format!(
            "failed to parse OpenAI stream event: {}",
            response_preview(data)
        )
    })?;
    if let Some(error) = value.get("error") {
        bail!(
            "LLM stream returned error: {}",
            response_preview(&error.to_string())
        );
    }

    let choices = value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .cloned()
        .unwrap_or_default();
    for choice in choices {
        let Some(delta) = choice.get("delta") else {
            continue;
        };

        if let Some(text) = json_str(delta, "content") {
            state.content.push_str(text);
            on_event(LlmStreamEvent::Text(text.to_string()));
        }
        if let Some(reasoning) =
            json_str(delta, "reasoning_content").or_else(|| json_str(delta, "reasoning"))
        {
            state.reasoning_content.push_str(reasoning);
            on_event(LlmStreamEvent::Reasoning(reasoning.to_string()));
        }
        if let Some(tool_deltas) = delta.get("tool_calls").and_then(|v| v.as_array()) {
            for tool_delta in tool_deltas {
                let index = tool_delta
                    .get("index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(state.tool_calls.len() as u64) as usize;
                let entry = state.tool_calls.entry(index).or_default();
                if let Some(id) = json_str(tool_delta, "id") {
                    entry.id = Some(id.to_string());
                }
                if let Some(call_type) = json_str(tool_delta, "type") {
                    entry.call_type = Some(call_type.to_string());
                }
                if let Some(function) = tool_delta.get("function") {
                    if let Some(name) = json_str(function, "name") {
                        entry.name.push_str(name);
                    }
                    if let Some(arguments) = json_str(function, "arguments") {
                        entry.arguments.push_str(arguments);
                    }
                }
            }
        }
    }

    Ok(true)
}

fn openai_stream_tool_calls(
    tool_calls: BTreeMap<usize, OpenaiStreamToolCall>,
) -> anyhow::Result<Option<Vec<ToolCall>>> {
    let mut calls = Vec::new();
    for (index, call) in tool_calls {
        if call.name.trim().is_empty() && call.arguments.trim().is_empty() && call.id.is_none() {
            continue;
        }
        let name = if call.name.trim().is_empty() {
            bail!("OpenAI streamed tool call #{index} is missing a function name");
        } else {
            call.name
        };
        calls.push(ToolCall {
            id: call.id.unwrap_or_else(|| format!("call_{index}")),
            call_type: call.call_type.unwrap_or_else(|| "function".to_string()),
            function: ToolCallFunction {
                name,
                arguments: non_empty_json(call.arguments),
            },
        });
    }

    Ok((!calls.is_empty()).then_some(calls))
}

async fn parse_anthropic_streaming_response<F>(
    response: reqwest::Response,
    on_event: &mut F,
) -> anyhow::Result<LlmResponse>
where
    F: FnMut(LlmStreamEvent),
{
    let mut state = AnthropicStreamState::default();
    parse_sse_response(response, |event| {
        apply_anthropic_stream_event(&mut state, event, on_event)
    })
    .await?;

    let tool_calls = anthropic_stream_tool_calls(state.blocks)?;
    Ok(LlmResponse {
        content: (!state.content.is_empty()).then_some(state.content),
        tool_calls,
        reasoning_content: (!state.reasoning_content.is_empty()).then_some(state.reasoning_content),
    })
}

fn apply_anthropic_stream_event<F>(
    state: &mut AnthropicStreamState,
    event: SseEvent,
    on_event: &mut F,
) -> anyhow::Result<bool>
where
    F: FnMut(LlmStreamEvent),
{
    if event.data.trim().is_empty() {
        return Ok(true);
    }

    let value: serde_json::Value = serde_json::from_str(event.data.trim()).with_context(|| {
        format!(
            "failed to parse Anthropic stream event: {}",
            response_preview(event.data.trim())
        )
    })?;
    let event_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .or(event.event.as_deref())
        .unwrap_or_default();

    match event_type {
        "content_block_start" => {
            let index = json_index(&value);
            let block_value = value
                .get("content_block")
                .unwrap_or(&serde_json::Value::Null);
            let block = state.blocks.entry(index).or_default();
            if let Some(block_type) = json_str(block_value, "type") {
                block.block_type = block_type.to_string();
            }
            if let Some(id) = json_str(block_value, "id") {
                block.id = Some(id.to_string());
            }
            if let Some(name) = json_str(block_value, "name") {
                block.name = Some(name.to_string());
            }
            if let Some(text) = json_str(block_value, "text") {
                block.text.push_str(text);
                state.content.push_str(text);
                on_event(LlmStreamEvent::Text(text.to_string()));
            }
        }
        "content_block_delta" => {
            let index = json_index(&value);
            let delta = value.get("delta").unwrap_or(&serde_json::Value::Null);
            let block = state.blocks.entry(index).or_default();
            match json_str(delta, "type").unwrap_or_default() {
                "text_delta" => {
                    if let Some(text) = json_str(delta, "text") {
                        block.text.push_str(text);
                        state.content.push_str(text);
                        on_event(LlmStreamEvent::Text(text.to_string()));
                    }
                }
                "thinking_delta" => {
                    if let Some(thinking) = json_str(delta, "thinking") {
                        block.text.push_str(thinking);
                        state.reasoning_content.push_str(thinking);
                        on_event(LlmStreamEvent::Reasoning(thinking.to_string()));
                    }
                }
                "input_json_delta" => {
                    if let Some(partial_json) = json_str(delta, "partial_json") {
                        block.input_json.push_str(partial_json);
                    }
                }
                _ => {}
            }
        }
        "message_stop" => return Ok(false),
        "error" => {
            bail!(
                "LLM stream returned error: {}",
                response_preview(&value.to_string())
            );
        }
        _ => {}
    }

    Ok(true)
}

fn anthropic_stream_tool_calls(
    blocks: BTreeMap<usize, AnthropicStreamBlock>,
) -> anyhow::Result<Option<Vec<ToolCall>>> {
    let mut calls = Vec::new();
    for (index, block) in blocks {
        if block.block_type != "tool_use" {
            continue;
        }
        let id = block
            .id
            .with_context(|| format!("Anthropic streamed tool_use block #{index} missing id"))?;
        let name = block
            .name
            .with_context(|| format!("Anthropic streamed tool_use block #{index} missing name"))?;
        calls.push(ToolCall {
            id,
            call_type: "function".to_string(),
            function: ToolCallFunction {
                name,
                arguments: non_empty_json(block.input_json),
            },
        });
    }

    Ok((!calls.is_empty()).then_some(calls))
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

fn json_index(value: &serde_json::Value) -> usize {
    value.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize
}

fn non_empty_json(value: String) -> String {
    if value.trim().is_empty() {
        "{}".to_string()
    } else {
        value
    }
}

async fn parse_openai_response(response_text: String) -> anyhow::Result<String> {
    let response: OpenaiResponse = serde_json::from_str(&response_text).with_context(|| {
        format!(
            "failed to parse LLM response: {}",
            response_preview(&response_text)
        )
    })?;

    response
        .choices
        .first()
        .and_then(|c| {
            c.message
                .content
                .clone()
                .or_else(|| c.message.reasoning_content.clone())
        })
        .context("LLM response has no content")
}

async fn parse_openai_response_with_tools(response_text: String) -> anyhow::Result<LlmResponse> {
    let response: OpenaiResponse = serde_json::from_str(&response_text).with_context(|| {
        format!(
            "failed to parse LLM response: {}",
            response_preview(&response_text)
        )
    })?;

    let choice = response
        .choices
        .first()
        .context("LLM response has no choices")?;
    Ok(LlmResponse {
        content: choice
            .message
            .content
            .clone()
            .or_else(|| choice.message.reasoning_content.clone()),
        tool_calls: choice.message.tool_calls.clone(),
        reasoning_content: choice.message.reasoning_content.clone(),
    })
}

async fn parse_anthropic_response(response_text: String) -> anyhow::Result<String> {
    parse_anthropic_response_with_tools(response_text)
        .await?
        .content
        .context("LLM response has no content")
}

async fn parse_anthropic_response_with_tools(response_text: String) -> anyhow::Result<LlmResponse> {
    let response: AnthropicResponse = serde_json::from_str(&response_text).with_context(|| {
        format!(
            "failed to parse LLM response: {}",
            response_preview(&response_text)
        )
    })?;

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut thinking_parts = Vec::new();

    for block in response.content {
        match block.block_type.as_str() {
            "text" => {
                if let Some(text) = block.text {
                    text_parts.push(text);
                }
            }
            "thinking" => {
                if let Some(thinking) = block.text {
                    thinking_parts.push(thinking);
                }
            }
            "tool_use" => {
                let id = block.id.context("Anthropic tool_use block missing id")?;
                let name = block
                    .name
                    .context("Anthropic tool_use block missing name")?;
                let arguments = serde_json::to_string(&block.input.unwrap_or_else(|| json!({})))?;
                tool_calls.push(ToolCall {
                    id,
                    call_type: "function".to_string(),
                    function: ToolCallFunction { name, arguments },
                });
            }
            _ => {}
        }
    }

    Ok(LlmResponse {
        content: (!text_parts.is_empty()).then(|| text_parts.join("\n")),
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        reasoning_content: (!thinking_parts.is_empty()).then(|| thinking_parts.join("\n")),
    })
}

fn response_preview(text: &str) -> String {
    text.chars().take(500).collect()
}

pub fn detect_user_language(transcript_tail: &[crate::transcript::TranscriptMessage]) -> &str {
    for msg in transcript_tail.iter().rev() {
        let preview = &msg.text_preview;
        if preview.contains("中文") || preview.contains("什么") || preview.contains("的") {
            return "zh";
        }
        let has_chinese = preview.chars().any(|c| c >= '\u{4e00}' && c <= '\u{9fff}');
        if has_chinese {
            return "zh";
        }
    }
    "en"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config<'a>(provider: &'a str, base_url: &'a str) -> LlmRequestConfig<'a> {
        LlmRequestConfig {
            provider,
            model: "test-model",
            base_url,
            api_key: None,
            temperature: 0.3,
            tool_choice: None,
        }
    }

    #[test]
    fn test_build_openai_chat_url() {
        let url = build_openai_chat_url(&config("openai_compatible", "https://api.openai.com/v1"))
            .unwrap();
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_build_openai_chat_url_trailing_slash() {
        let url = build_openai_chat_url(&config("openai_compatible", "https://api.openai.com/v1/"))
            .unwrap();
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_build_openai_chat_url_does_not_duplicate_endpoint() {
        let url = build_openai_chat_url(&config(
            "openai_compatible",
            "https://api.openai.com/v1/chat/completions",
        ))
        .unwrap();
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_build_openai_chat_url_preserves_full_endpoint_query() {
        let base = "https://example.openai.azure.com/openai/deployments/test/chat/completions?api-version=2024-02-15-preview";
        let url = build_openai_chat_url(&config("openai_compatible", base)).unwrap();
        assert_eq!(url, base);
    }

    #[test]
    fn test_build_anthropic_messages_url() {
        let url = build_anthropic_messages_url(&config(
            "anthropic_compatible",
            "https://api.anthropic.com/v1",
        ))
        .unwrap();
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_build_anthropic_messages_url_from_root() {
        let url = build_anthropic_messages_url(&config(
            "anthropic_compatible",
            "https://api.anthropic.com",
        ))
        .unwrap();
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_build_anthropic_messages_url_from_provider_prefix() {
        let url = build_anthropic_messages_url(&config(
            "anthropic_compatible",
            "https://token-plan-cn.xiaomimimo.com/anthropic",
        ))
        .unwrap();
        assert_eq!(
            url,
            "https://token-plan-cn.xiaomimimo.com/anthropic/v1/messages"
        );
    }

    #[test]
    fn test_build_anthropic_messages_url_from_volc_coding_prefix() {
        let url = build_anthropic_messages_url(&config(
            "anthropic_compatible",
            "https://ark.cn-beijing.volces.com/api/coding",
        ))
        .unwrap();
        assert_eq!(
            url,
            "https://ark.cn-beijing.volces.com/api/coding/v1/messages"
        );
    }

    #[test]
    fn test_build_anthropic_messages_url_does_not_duplicate_endpoint() {
        let url = build_anthropic_messages_url(&config(
            "anthropic_compatible",
            "https://api.anthropic.com/v1/messages",
        ))
        .unwrap();
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_openai_request_body_can_use_max_completion_tokens() {
        let request = build_openai_request(
            "test-model",
            "system",
            "user",
            &config("openai_compatible", "https://api.example.com/v1"),
        );
        let body = openai_request_body(&request, TokenLimitField::MaxCompletionTokens).unwrap();
        assert!(body.get("max_tokens").is_none());
        assert_eq!(
            body.get("max_completion_tokens").and_then(|v| v.as_u64()),
            Some(500)
        );
    }

    #[test]
    fn test_auth_scheme_prefers_api_key_for_token_plan() {
        let auth = preferred_openai_auth_scheme(
            "https://token-plan-cn.xiaomimimo.com/v1/chat/completions",
            Some("tp-test"),
        );
        assert_eq!(auth, OpenAiAuthScheme::ApiKeyHeader);
    }

    #[test]
    fn test_anthropic_request_moves_system_to_top_level() {
        let messages = vec![
            OpenaiMessage {
                role: "system".to_string(),
                content: Some("system rules".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            OpenaiMessage {
                role: "user".to_string(),
                content: Some("hello".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let request = build_anthropic_request_from_messages(
            "claude-test",
            &messages,
            &config("anthropic_compatible", "https://api.anthropic.com/v1"),
            None,
            1500,
        )
        .unwrap();

        assert_eq!(request.system.as_deref(), Some("system rules"));
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].role, "user");
    }

    #[tokio::test]
    async fn test_parse_anthropic_tool_use_response() {
        let response = json!({
            "content": [
                { "type": "text", "text": "I will check." },
                {
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "task__list",
                    "input": { "limit": 3 }
                }
            ]
        })
        .to_string();

        let parsed = parse_anthropic_response_with_tools(response).await.unwrap();
        assert_eq!(parsed.content.as_deref(), Some("I will check."));
        let calls = parsed.tool_calls.unwrap();
        assert_eq!(calls[0].id, "toolu_1");
        assert_eq!(calls[0].function.name, "task__list");
        assert_eq!(calls[0].function.arguments, r#"{"limit":3}"#);
    }

    #[test]
    fn test_parse_sse_event_collects_data_lines() {
        let event = parse_sse_event(
            b"event: content_block_delta\r\ndata: {\"a\":1}\r\ndata: {\"b\":2}\r\n",
        )
        .unwrap()
        .unwrap();

        assert_eq!(event.event.as_deref(), Some("content_block_delta"));
        assert_eq!(event.data, "{\"a\":1}\n{\"b\":2}");
    }

    #[test]
    fn test_openai_stream_event_accumulates_text_and_tool_delta() {
        let mut state = OpenaiStreamState::default();
        let mut events = Vec::new();

        apply_openai_stream_event(
            &mut state,
            SseEvent {
                event: None,
                data: json!({
                    "choices": [{
                        "delta": {
                            "content": "你好",
                            "tool_calls": [{
                                "index": 0,
                                "id": "call_1",
                                "type": "function",
                                "function": {
                                    "name": "file__read",
                                    "arguments": "{\"path\":\"a"
                                }
                            }]
                        }
                    }]
                })
                .to_string(),
            },
            &mut |event| events.push(event),
        )
        .unwrap();
        apply_openai_stream_event(
            &mut state,
            SseEvent {
                event: None,
                data: json!({
                    "choices": [{
                        "delta": {
                            "tool_calls": [{
                                "index": 0,
                                "function": { "arguments": ".txt\"}" }
                            }]
                        }
                    }]
                })
                .to_string(),
            },
            &mut |event| events.push(event),
        )
        .unwrap();

        assert_eq!(state.content, "你好");
        assert_eq!(events, vec![LlmStreamEvent::Text("你好".to_string())]);
        let calls = openai_stream_tool_calls(state.tool_calls).unwrap().unwrap();
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "file__read");
        assert_eq!(calls[0].function.arguments, r#"{"path":"a.txt"}"#);
    }

    #[test]
    fn test_anthropic_stream_event_accumulates_text_and_tool_delta() {
        let mut state = AnthropicStreamState::default();
        let mut events = Vec::new();

        apply_anthropic_stream_event(
            &mut state,
            SseEvent {
                event: Some("content_block_start".to_string()),
                data: json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": { "type": "text", "text": "" }
                })
                .to_string(),
            },
            &mut |event| events.push(event),
        )
        .unwrap();
        apply_anthropic_stream_event(
            &mut state,
            SseEvent {
                event: Some("content_block_delta".to_string()),
                data: json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": { "type": "text_delta", "text": "done" }
                })
                .to_string(),
            },
            &mut |event| events.push(event),
        )
        .unwrap();
        apply_anthropic_stream_event(
            &mut state,
            SseEvent {
                event: Some("content_block_start".to_string()),
                data: json!({
                    "type": "content_block_start",
                    "index": 1,
                    "content_block": { "type": "tool_use", "id": "toolu_1", "name": "task__list", "input": {} }
                })
                .to_string(),
            },
            &mut |event| events.push(event),
        )
        .unwrap();
        apply_anthropic_stream_event(
            &mut state,
            SseEvent {
                event: Some("content_block_delta".to_string()),
                data: json!({
                    "type": "content_block_delta",
                    "index": 1,
                    "delta": { "type": "input_json_delta", "partial_json": "{\"limit\":3}" }
                })
                .to_string(),
            },
            &mut |event| events.push(event),
        )
        .unwrap();

        assert_eq!(state.content, "done");
        assert_eq!(events, vec![LlmStreamEvent::Text("done".to_string())]);
        let calls = anthropic_stream_tool_calls(state.blocks).unwrap().unwrap();
        assert_eq!(calls[0].id, "toolu_1");
        assert_eq!(calls[0].function.name, "task__list");
        assert_eq!(calls[0].function.arguments, r#"{"limit":3}"#);
    }

    #[test]
    fn test_detect_user_language_chinese() {
        let messages = vec![crate::transcript::TranscriptMessage {
            role: "user".to_string(),
            text_preview: "帮我写一个函数".to_string(),
            raw: serde_json::json!({}),
        }];
        assert_eq!(detect_user_language(&messages), "zh");
    }

    #[test]
    fn test_detect_user_language_english() {
        let messages = vec![crate::transcript::TranscriptMessage {
            role: "user".to_string(),
            text_preview: "help me write a function".to_string(),
            raw: serde_json::json!({}),
        }];
        assert_eq!(detect_user_language(&messages), "en");
    }
}
