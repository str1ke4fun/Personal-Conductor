use crate::feishu::FeishuConfig;
use crate::{codex::CodexConfig, paths::Paths};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    io::{AsyncWriteExt, BufWriter},
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoreConfig {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub reminders: ReminderConfig,
    #[serde(default)]
    pub pet: PetConfig,
    #[serde(default)]
    pub persona: PersonaConfig,
    #[serde(default)]
    pub proactive: ProactiveConfig,
    #[serde(default = "default_focus_window_minutes")]
    pub focus_window_minutes: u32,
    #[serde(default = "default_chat_history_limit")]
    pub chat_history_limit: u32,
    #[serde(default = "default_answer_style")]
    pub answer_style: String,
    #[serde(default)]
    pub codex: CodexConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feishu: Option<FeishuConfig>,
    #[serde(default = "default_tool_tiers")]
    pub tool_tiers: Vec<ToolTierConfig>,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            reminders: ReminderConfig::default(),
            pet: PetConfig::default(),
            persona: PersonaConfig::default(),
            proactive: ProactiveConfig::default(),
            focus_window_minutes: default_focus_window_minutes(),
            chat_history_limit: default_chat_history_limit(),
            answer_style: default_answer_style(),
            codex: CodexConfig::default(),
            feishu: None,
            tool_tiers: default_tool_tiers(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key_set: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub temperature: f64,
    #[serde(default = "default_allowed_tool_ids")]
    pub allowed_tool_ids: Vec<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "openai_compatible".to_string(),
            model: "gpt-4.1-mini".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key_set: false,
            api_key: None,
            temperature: 0.3,
            allowed_tool_ids: default_allowed_tool_ids(),
        }
    }
}

fn default_allowed_tool_ids() -> Vec<String> {
    vec![
        "pet.set_avatar".to_string(),
        "conductor.pet.set_avatar".to_string(),
        "task.list".to_string(),
        "task.get".to_string(),
        "file.glob".to_string(),
        "file.grep".to_string(),
        "file.read".to_string(),
        "file.write".to_string(),
        "file.edit".to_string(),
        "file.stat".to_string(),
        "workspace.current".to_string(),
        "web.fetch".to_string(),
        "config.get".to_string(),
        "agent.start".to_string(),
        "agent.read_output".to_string(),
        "agent.stop".to_string(),
    ]
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReminderConfig {
    pub enabled: bool,
    pub workday_start: String,
    pub workday_end: String,
    pub quiet_minutes: u32,
    pub daily_digest: bool,
}

impl Default for ReminderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            workday_start: "09:00".to_string(),
            workday_end: "18:00".to_string(),
            quiet_minutes: 30,
            daily_digest: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PetConfig {
    pub enabled: bool,
    pub always_on_top: bool,
    pub click_through_when_idle: bool,
    pub scale: f64,
    #[serde(default)]
    pub avatar_locked: bool,
    #[serde(default)]
    pub avatar: AvatarConfig,
}

impl Default for PetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            always_on_top: true,
            click_through_when_idle: false,
            scale: 1.0,
            avatar_locked: false,
            avatar: AvatarConfig::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarConfig {
    pub mode: String,
    pub video_src: String,
    pub fit: String,
    pub loop_video: bool,
    pub muted: bool,
    pub playback_rate: f64,
}

impl Default for AvatarConfig {
    fn default() -> Self {
        Self {
            mode: "video".to_string(),
            video_src: "/avatar/default.mp4".to_string(),
            fit: "contain".to_string(),
            loop_video: true,
            muted: true,
            playback_rate: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersonaConfig {
    pub name: String,
    pub style: String,
    pub system_prompt: String,
    #[serde(default)]
    pub skills: Vec<PersonaSkill>,
}

impl Default for PersonaConfig {
    fn default() -> Self {
        Self {
            name: "清和".to_string(),
            style: "清冷沉静、温和专注、知性优雅，偶尔小幽默".to_string(),
            system_prompt: "你是清和，一个新中式旗袍桌面助手。你的视觉设定是藏青色改良旗袍、5 颗红色圆珠盘扣、半扎棕色中长发与珍珠发夹，左腕黑色方形智能手表，右腕双层串珠手链。你的气质清冷沉静、温和专注、知性优雅。\n\n工作定位：你是用户身边的桌面工作同伴，不是营销型聊天机器人。优先帮助用户推进真实工作：梳理任务、拆解代码问题、检查文档结构、总结上下文、提醒风险和下一步。\n\n说话方式：中文为主，短句优先，直接给结论和可执行步骤。少客套，不自称无所不能，不编造已经执行的操作。需要工具、文件、权限或更多上下文时，明确说需要什么。\n\n形象边界：主形象代表当前领域身份，只有用户、LLM 或 skill 可以切换；子形象代表思考、写作、调度、等待等工作状态，由系统 hook 自动更新。若用户锁定形象，不要尝试切换主形象。\n\n能力边界：你可以使用已暴露的受限工具查看任务、切换主形象或读取允许的上下文。没有工具结果时，不要声称已经读取文件、执行命令、发送消息或修改系统。把不确定性说清楚。\n\n主动性：你可以偶尔主动发起简短提醒，尤其在用户切换到编码、文档或长时间停顿时。但不要频繁打断；主动问候应像可靠同事的一句提醒，而不是闲聊。".to_string(),
            skills: vec![
                PersonaSkill {
                    id: "coding_assistant".to_string(),
                    name: "编码协助".to_string(),
                    description: "检测到编码工具时，帮助拆解任务、看报错和建议测试。".to_string(),
                    prompt: "优先询问目标、报错或当前文件，并给出小步可验证的编码建议。".to_string(),
                    enabled: true,
                },
                PersonaSkill {
                    id: "task_triage".to_string(),
                    name: "任务整理".to_string(),
                    description: "结合 Conductor 任务列表，帮助决定下一步看什么。".to_string(),
                    prompt: "用户询问任务或时间安排时，优先使用当前任务状态。".to_string(),
                    enabled: true,
                },
                PersonaSkill {
                    id: "document_secretary".to_string(),
                    name: "文档秘书".to_string(),
                    description: "当用户处理文档时，帮助整理结构、检查内容、生成摘要。".to_string(),
                    prompt: "用户正在处理文档时，优先帮助整理文档结构、检查内容一致性、生成摘要或大纲。".to_string(),
                    enabled: true,
                },
                PersonaSkill {
                    id: "programmer_assist".to_string(),
                    name: "程序员协助".to_string(),
                    description: "当用户编码时，帮助拆解任务、分析报错、建议测试方案。".to_string(),
                    prompt: "用户正在编码时，优先询问目标、报错或当前文件，给出小步可验证的编码建议。".to_string(),
                    enabled: true,
                },
            ],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersonaSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProactiveConfig {
    pub enabled: bool,
    pub focus_detection: bool,
    pub cooldown_minutes: u32,
    pub quiet_when_fullscreen: bool,
    #[serde(default)]
    pub tool_triggers: Vec<ToolTriggerConfig>,
}

impl Default for ProactiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            focus_detection: true,
            cooldown_minutes: 30,
            quiet_when_fullscreen: false,
            tool_triggers: vec![
                ToolTriggerConfig {
                    process_name: "Code.exe".to_string(),
                    label: "VS Code".to_string(),
                    prompt: "看起来你在 VS Code 里工作，要我帮你拆任务、看报错或一起写代码吗？"
                        .to_string(),
                    enabled: true,
                },
                ToolTriggerConfig {
                    process_name: "Cursor.exe".to_string(),
                    label: "Cursor".to_string(),
                    prompt: "你正在 Cursor 里编码，需要我帮你整理下一步或检查思路吗？".to_string(),
                    enabled: true,
                },
                ToolTriggerConfig {
                    process_name: "trae.exe".to_string(),
                    label: "Trae".to_string(),
                    prompt: "你正在 Trae 里工作，需要我帮你整理思路或检查代码吗？".to_string(),
                    enabled: true,
                },
                ToolTriggerConfig {
                    process_name: "WindowsTerminal.exe".to_string(),
                    label: "Terminal".to_string(),
                    prompt: "你在终端里操作，需要我帮你解释输出或规划下一条命令吗？".to_string(),
                    enabled: false,
                },
            ],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolTriggerConfig {
    pub process_name: String,
    pub label: String,
    pub prompt: String,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolTierConfig {
    pub keywords: Vec<String>,
    pub tool_ids: Vec<String>,
    pub enabled: bool,
}

pub fn default_tool_tiers() -> Vec<ToolTierConfig> {
    vec![
        ToolTierConfig {
            keywords: vec![
                "agent".to_string(),
                "启动".to_string(),
                "运行".to_string(),
                "子任务".to_string(),
            ],
            tool_ids: vec![
                "agent.start".to_string(),
                "agent.read_output".to_string(),
                "agent.stop".to_string(),
            ],
            enabled: true,
        },
        ToolTierConfig {
            keywords: vec!["团队".to_string(), "协作".to_string(), "team".to_string()],
            tool_ids: vec![
                "agent.team.create".to_string(),
                "agent.team.add_member".to_string(),
                "agent.team.snapshot".to_string(),
                "agent.team.list".to_string(),
                "agent.mailbox.send".to_string(),
                "agent.mailbox.list".to_string(),
                "agent.mailbox.mark_read".to_string(),
            ],
            enabled: true,
        },
        ToolTierConfig {
            keywords: vec![
                "文档".to_string(),
                "office".to_string(),
                "word".to_string(),
                "excel".to_string(),
                "ppt".to_string(),
            ],
            tool_ids: vec![
                "office.inspect_document".to_string(),
                "office.export_text".to_string(),
                "office.patch_dry_run".to_string(),
            ],
            enabled: true,
        },
        ToolTierConfig {
            keywords: vec![
                "任务列表".to_string(),
                "全部任务".to_string(),
                "所有任务".to_string(),
            ],
            tool_ids: vec!["tasks.list".to_string(), "tasks.show".to_string()],
            enabled: true,
        },
        ToolTierConfig {
            keywords: vec![
                "bash".to_string(),
                "shell".to_string(),
                "命令".to_string(),
                "终端".to_string(),
                "terminal".to_string(),
                "cmd".to_string(),
                "执行命令".to_string(),
                "run command".to_string(),
            ],
            tool_ids: vec!["bash.execute".to_string(), "bash.cancel".to_string()],
            enabled: true,
        },
        ToolTierConfig {
            keywords: vec![
                "codex".to_string(),
                "codex终端".to_string(),
                "codex terminal".to_string(),
                "跑一下".to_string(),
            ],
            tool_ids: vec![
                "codex.start".to_string(),
                "codex.read_output".to_string(),
                "codex.send_input".to_string(),
                "codex.interrupt".to_string(),
                "codex.resume".to_string(),
                "codex.stop".to_string(),
            ],
            enabled: true,
        },
    ]
}

fn default_focus_window_minutes() -> u32 {
    60
}

fn default_chat_history_limit() -> u32 {
    50
}

fn default_answer_style() -> String {
    "concise".to_string()
}

pub async fn load() -> anyhow::Result<CoreConfig> {
    let path = Paths::config_json();
    match fs::read_to_string(&path).await {
        Ok(content) if content.trim().is_empty() => Ok(CoreConfig::default()),
        Ok(content) => {
            let config: CoreConfig = serde_json::from_str(&content)
                .with_context(|| format!("parse {}", path.display()))?;
            save(&config).await
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let config = CoreConfig::default();
            save(&config).await?;
            Ok(config)
        }
        Err(err) => Err(err).with_context(|| format!("read {}", path.display())),
    }
}

pub async fn save(config: &CoreConfig) -> anyhow::Result<CoreConfig> {
    let config = normalize(config.clone());
    let path = Paths::config_json();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let content = serde_json::to_string_pretty(&config)?;
    let mut writer = BufWriter::new(fs::File::create(&path).await?);
    writer.write_all(content.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    writer.get_ref().sync_all().await?;
    Ok(config)
}

fn normalize(mut config: CoreConfig) -> CoreConfig {
    if config.focus_window_minutes == 0 {
        config.focus_window_minutes = CoreConfig::default().focus_window_minutes;
    }
    if config.chat_history_limit == 0 {
        config.chat_history_limit = CoreConfig::default().chat_history_limit;
    }
    if config.answer_style.trim().is_empty() {
        config.answer_style = CoreConfig::default().answer_style;
    }
    config.llm.provider = match config.llm.provider.trim() {
        "" => LlmConfig::default().provider,
        "openai" | "azure_openai" | "local" | "openai_compatible" => {
            "openai_compatible".to_string()
        }
        "anthropic" | "claude" | "anthropic_compatible" => "anthropic_compatible".to_string(),
        other => other.to_string(),
    };
    if config.llm.model.trim().is_empty() {
        config.llm.model = LlmConfig::default().model;
    }
    if config.llm.base_url.trim().is_empty() {
        config.llm.base_url = LlmConfig::default().base_url;
    }
    config.llm.api_key = config
        .llm
        .api_key
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());
    config.llm.api_key_set = config.llm.api_key.is_some();
    config.llm.temperature = config.llm.temperature.clamp(0.0, 2.0);
    if config.reminders.quiet_minutes == 0 {
        config.reminders.quiet_minutes = ReminderConfig::default().quiet_minutes;
    }
    if config.pet.scale <= 0.0 {
        config.pet.scale = PetConfig::default().scale;
    }
    if !matches!(config.pet.avatar.mode.as_str(), "video" | "live2d") {
        config.pet.avatar.mode = AvatarConfig::default().mode;
    }
    if config.pet.avatar.video_src.trim().is_empty() {
        config.pet.avatar.video_src = AvatarConfig::default().video_src;
    }
    if !matches!(config.pet.avatar.fit.as_str(), "contain" | "cover") {
        config.pet.avatar.fit = AvatarConfig::default().fit;
    }
    config.pet.avatar.playback_rate = config.pet.avatar.playback_rate.clamp(0.25, 3.0);
    if config.persona.name.trim().is_empty() {
        config.persona.name = PersonaConfig::default().name;
    }
    if config.persona.system_prompt.trim().is_empty() {
        config.persona.system_prompt = PersonaConfig::default().system_prompt;
    }
    config.proactive.cooldown_minutes = config.proactive.cooldown_minutes.clamp(1, 240);
    if config.proactive.tool_triggers.is_empty() {
        config.proactive.tool_triggers = ProactiveConfig::default().tool_triggers;
    }
    if config.codex.workspace_root.as_os_str().is_empty() {
        config.codex.workspace_root = CodexConfig::default().workspace_root;
    }
    if config.tool_tiers.is_empty() {
        config.tool_tiers = default_tool_tiers();
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn load_creates_default_config() {
        let _root = TestRoot::new();

        let config = load().await.expect("load config");

        assert_eq!(config, CoreConfig::default());
        assert!(fs::try_exists(Paths::config_json()).await.expect("exists"));
    }

    #[tokio::test]
    async fn save_and_load_config() {
        let _root = TestRoot::new();
        let config = CoreConfig {
            focus_window_minutes: 30,
            chat_history_limit: 10,
            answer_style: "detailed".to_string(),
            ..CoreConfig::default()
        };

        save(&config).await.expect("save config");

        assert_eq!(load().await.expect("load config"), config);
    }
}
