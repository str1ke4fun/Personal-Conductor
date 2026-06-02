use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeishuConfig {
    pub enabled: bool,
    pub webhook_url: String,
    pub bot_name: String,
    pub mention_me: bool,
}

impl Default for FeishuConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            webhook_url: String::new(),
            bot_name: "Conductor".to_string(),
            mention_me: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub title: String,
    pub content: String,
    pub priority: NotificationPriority,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NotificationPriority {
    Low,
    Normal,
    Urgent,
}

pub fn build_card_message(notification: &Notification) -> serde_json::Value {
    let template = match notification.priority {
        NotificationPriority::Urgent => "red",
        NotificationPriority::Normal => "blue",
        NotificationPriority::Low => "grey",
    };

    serde_json::json!({
        "msg_type": "interactive",
        "card": {
            "header": {
                "title": {
                    "tag": "plain_text",
                    "content": notification.title
                },
                "template": template
            },
            "elements": [{
                "tag": "markdown",
                "content": notification.content
            }]
        }
    })
}

pub async fn send(config: &FeishuConfig, notification: &Notification) -> anyhow::Result<()> {
    if !config.enabled {
        return Ok(());
    }

    if config.webhook_url.is_empty() {
        anyhow::bail!("飞书 webhook URL 未配置");
    }

    let payload = build_card_message(notification);

    let client = reqwest::Client::new();
    let response = client
        .post(&config.webhook_url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .context("发送飞书通知失败")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("飞书 API 返回错误: {} - {}", status, body);
    }

    Ok(())
}

pub async fn test_connection(config: &FeishuConfig) -> anyhow::Result<bool> {
    if !config.enabled {
        return Ok(false);
    }

    if config.webhook_url.is_empty() {
        return Ok(false);
    }

    let notification = Notification {
        title: "🔔 Conductor 连接测试".to_string(),
        content: "飞书推送功能已启用，连接正常".to_string(),
        priority: NotificationPriority::Normal,
    };

    match send(config, &notification).await {
        Ok(_) => Ok(true),
        Err(e) => {
            tracing::warn!("飞书连接测试失败: {}", e);
            Ok(false)
        }
    }
}

pub async fn notify_pending_pileup(config: &FeishuConfig, count: u32) -> anyhow::Result<()> {
    let priority = if count > 5 {
        NotificationPriority::Urgent
    } else {
        NotificationPriority::Normal
    };

    let notification = Notification {
        title: "📋 Conductor 待审提醒".to_string(),
        content: format!("你有 {} 条待审任务需要处理", count),
        priority,
    };

    send(config, &notification).await
}

pub async fn notify_user_back(config: &FeishuConfig) -> anyhow::Result<()> {
    let notification = Notification {
        title: "👋 Conductor 欢迎回来".to_string(),
        content: "你已回到桌面，有一些待处理的事项等你查看".to_string(),
        priority: NotificationPriority::Normal,
    };

    send(config, &notification).await
}

pub async fn notify_agent_stalled(
    config: &FeishuConfig,
    agent_name: &str,
    duration_minutes: u32,
) -> anyhow::Result<()> {
    let notification = Notification {
        title: "⚠️ Conductor Agent 卡住".to_string(),
        content: format!(
            "Agent [{}] 已静止 {} 分钟，可能需要人工介入",
            agent_name, duration_minutes
        ),
        priority: NotificationPriority::Urgent,
    };

    send(config, &notification).await
}

pub async fn notify_daily_digest(config: &FeishuConfig, summary: &str) -> anyhow::Result<()> {
    let notification = Notification {
        title: "📊 Conductor 每日摘要".to_string(),
        content: summary.to_string(),
        priority: NotificationPriority::Normal,
    };

    send(config, &notification).await
}

pub fn mask_webhook_url(url: &str) -> String {
    if url.chars().count() <= 20 {
        return "***".to_string();
    }
    let prefix: String = url.chars().take(10).collect();
    let suffix: String = url
        .chars()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}***{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_card_message_urgent() {
        let notification = Notification {
            title: "Test".to_string(),
            content: "Test content".to_string(),
            priority: NotificationPriority::Urgent,
        };

        let msg = build_card_message(&notification);
        assert_eq!(msg["card"]["header"]["template"], "red");
    }

    #[test]
    fn test_build_card_message_normal() {
        let notification = Notification {
            title: "Test".to_string(),
            content: "Test content".to_string(),
            priority: NotificationPriority::Normal,
        };

        let msg = build_card_message(&notification);
        assert_eq!(msg["card"]["header"]["template"], "blue");
    }

    #[test]
    fn test_build_card_message_low() {
        let notification = Notification {
            title: "Test".to_string(),
            content: "Test content".to_string(),
            priority: NotificationPriority::Low,
        };

        let msg = build_card_message(&notification);
        assert_eq!(msg["card"]["header"]["template"], "grey");
    }

    #[test]
    fn test_mask_webhook_url() {
        let url = "https://open.feishu.cn/webhook/abc123xyz";
        let masked = mask_webhook_url(url);
        assert!(masked.contains("***"));
        assert!(!masked.contains("abc123"));
    }

    #[test]
    fn test_mask_webhook_url_short() {
        let url = "short";
        let masked = mask_webhook_url(url);
        assert_eq!(masked, "***");
    }

    #[test]
    fn test_mask_webhook_url_utf8_safe() {
        let url = "https://飞书.example.com/webhook/测试abcdef";
        let masked = mask_webhook_url(url);
        assert!(masked.contains("***"));
        assert!(masked.ends_with("bcdef"));
    }
}
