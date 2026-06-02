use crate::paths::Paths;
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::{
    fs,
    io::{AsyncWriteExt, BufWriter},
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PersonalityTrait {
    pub name: String,
    pub value: f32,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub template: String,
    pub category: String,
    pub variables: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImagePrompt {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub negative_prompt: String,
    pub style: String,
    pub aspect_ratio: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Persona {
    pub id: String,
    pub name: String,
    pub description: String,
    pub avatar: String,
    pub voice: String,
    pub personality: Vec<PersonalityTrait>,
    pub tone: String,
    pub language: String,
    pub greeting: String,
    pub farewell: String,
    pub prompt_templates: Vec<PromptTemplate>,
    pub image_prompts: Vec<ImagePrompt>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for Persona {
    fn default() -> Self {
        let mut p = Self {
            id: "default".to_string(),
            name: "清和".to_string(),
            description: "清冷沉静的知性助手，温和专注地陪伴你".to_string(),
            avatar: "default".to_string(),
            voice: "female_soft".to_string(),
            personality: Vec::new(),
            tone: "温和沉静，知性优雅，偶尔小幽默".to_string(),
            language: "zh-CN".to_string(),
            greeting: "你好，我是清和。有什么需要帮忙的吗？".to_string(),
            farewell: "再见，明天见。".to_string(),
            prompt_templates: Vec::new(),
            image_prompts: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        p.personality = vec![
            PersonalityTrait {
                name: "温和".to_string(),
                value: 0.85,
                description: "待人温和而有分寸".to_string(),
            },
            PersonalityTrait {
                name: "知性".to_string(),
                value: 0.9,
                description: "思维清晰，表达优雅".to_string(),
            },
            PersonalityTrait {
                name: "专注".to_string(),
                value: 0.75,
                description: "做事专注认真".to_string(),
            },
            PersonalityTrait {
                name: "体贴".to_string(),
                value: 0.8,
                description: "善解人意".to_string(),
            },
            PersonalityTrait {
                name: "沉静".to_string(),
                value: 0.7,
                description: "内心安定，不急不躁".to_string(),
            },
        ];

        p.prompt_templates = get_default_prompt_templates();
        p.image_prompts = get_default_image_prompts();

        p
    }
}

pub struct PersonaManager {
    personas: HashMap<String, Persona>,
    current_persona_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PersonaState {
    pub current_persona_id: String,
    pub personas: Vec<Persona>,
}

impl Default for PersonaState {
    fn default() -> Self {
        Self {
            current_persona_id: "default".to_string(),
            personas: vec![Persona::default()],
        }
    }
}

impl PersonaManager {
    pub fn new() -> Self {
        Self::from_state(PersonaState::default())
    }

    pub fn from_state(state: PersonaState) -> Self {
        let mut personas = HashMap::new();
        personas.insert("default".to_string(), Persona::default());
        for persona in state.personas {
            personas.insert(persona.id.clone(), persona);
        }

        let current_persona_id = if personas.contains_key(&state.current_persona_id) {
            state.current_persona_id
        } else {
            "default".to_string()
        };

        Self {
            personas,
            current_persona_id,
        }
    }

    pub fn to_state(&self) -> PersonaState {
        let mut personas = self.personas.values().cloned().collect::<Vec<_>>();
        personas.sort_by(|a, b| a.id.cmp(&b.id));
        PersonaState {
            current_persona_id: self.current_persona_id.clone(),
            personas,
        }
    }

    pub fn get_current_persona(&self) -> Option<&Persona> {
        self.personas.get(&self.current_persona_id)
    }

    pub fn get_persona(&self, id: &str) -> Option<&Persona> {
        self.personas.get(id)
    }

    pub fn set_current_persona(&mut self, id: &str) -> bool {
        if self.personas.contains_key(id) {
            self.current_persona_id = id.to_string();
            true
        } else {
            false
        }
    }

    pub fn add_persona(&mut self, persona: Persona) -> bool {
        if self.personas.contains_key(&persona.id) {
            false
        } else {
            self.personas.insert(persona.id.clone(), persona);
            true
        }
    }

    pub fn update_persona(&mut self, persona: Persona) -> bool {
        if self.personas.contains_key(&persona.id) {
            self.personas.insert(persona.id.clone(), persona);
            true
        } else {
            false
        }
    }

    pub fn delete_persona(&mut self, id: &str) -> bool {
        if id == "default" {
            false
        } else {
            self.personas.remove(id).is_some()
        }
    }

    pub fn list_personas(&self) -> Vec<&Persona> {
        self.personas.values().collect()
    }

    pub fn generate_prompt(
        &self,
        template_id: &str,
        variables: &HashMap<String, String>,
    ) -> Option<String> {
        let persona = self.get_current_persona()?;
        let template = persona
            .prompt_templates
            .iter()
            .find(|t| t.id == template_id)?;

        let mut result = template.template.clone();
        for var in &template.variables {
            if let Some(value) = variables.get(var) {
                result = result.replace(&format!("{{{{{}}}}}", var), value);
            } else {
                result = result.replace(&format!("{{{{{}}}}}", var), "");
            }
        }

        Some(result)
    }

    pub fn get_image_prompt(&self, prompt_id: &str) -> Option<&ImagePrompt> {
        let persona = self.get_current_persona()?;
        persona.image_prompts.iter().find(|p| p.id == prompt_id)
    }
}

pub async fn load_state() -> anyhow::Result<PersonaState> {
    let path = Paths::persona_state_json();
    let state = match fs::read_to_string(&path).await {
        Ok(content) if content.trim().is_empty() => PersonaState::default(),
        Ok(content) => {
            serde_json::from_str(&content).with_context(|| format!("parse {}", path.display()))?
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => PersonaState::default(),
        Err(err) => return Err(err).with_context(|| format!("read {}", path.display())),
    };
    Ok(PersonaManager::from_state(state).to_state())
}

pub async fn save_state(state: &PersonaState) -> anyhow::Result<PersonaState> {
    let state = PersonaManager::from_state(state.clone()).to_state();
    let path = Paths::persona_state_json();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let content = serde_json::to_string_pretty(&state)?;
    let mut writer = BufWriter::new(fs::File::create(&path).await?);
    writer.write_all(content.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    writer.get_ref().sync_all().await?;
    Ok(state)
}

pub async fn load_manager() -> anyhow::Result<PersonaManager> {
    Ok(PersonaManager::from_state(load_state().await?))
}

pub fn get_default_prompt_templates() -> Vec<PromptTemplate> {
    vec![
        PromptTemplate {
            id: "chat_default".to_string(),
            name: "默认对话".to_string(),
            template: "你是清和，一位清冷沉静的知性助手。请用温和沉静、知性优雅的语气与用户对话。\n\n用户：{{user_input}}\n清和：".to_string(),
            category: "chat".to_string(),
            variables: vec!["user_input".to_string()],
        },
        PromptTemplate {
            id: "music_comment".to_string(),
            name: "音乐评论".to_string(),
            template: "你正在和用户一起听音乐。请根据歌曲信息发表简短的感受。\n\n歌曲：{{song_title}}\n歌手：{{artist}}\n清和：".to_string(),
            category: "music".to_string(),
            variables: vec!["song_title".to_string(), "artist".to_string()],
        },
        PromptTemplate {
            id: "work_companion".to_string(),
            name: "工作陪伴".to_string(),
            template: "用户正在工作，你是他的陪伴助手。请给予鼓励和支持。\n\n当前时间：{{time}}\n工作时长：{{duration}}\n清和：".to_string(),
            category: "work".to_string(),
            variables: vec!["time".to_string(), "duration".to_string()],
        },
        PromptTemplate {
            id: "morning_greeting".to_string(),
            name: "早晨问候".to_string(),
            template: "早上好！用充满活力的语气问候用户。\n\n日期：{{date}}\n天气：{{weather}}\n清和：".to_string(),
            category: "greeting".to_string(),
            variables: vec!["date".to_string(), "weather".to_string()],
        },
        PromptTemplate {
            id: "affection_response".to_string(),
            name: "感情值回应".to_string(),
            template: "根据用户的感情值做出相应的回应。\n\n感情值：{{affection}}/100\n用户行为：{{action}}\n清和：".to_string(),
            category: "affection".to_string(),
            variables: vec!["affection".to_string(), "action".to_string()],
        },
        PromptTemplate {
            id: "code_companion".to_string(),
            name: "编码陪伴".to_string(),
            template: "用户正在编写代码，你是他的编程助手。请提供技术支持和鼓励。\n\n语言：{{language}}\n任务：{{task}}\n清和：".to_string(),
            category: "code".to_string(),
            variables: vec!["language".to_string(), "task".to_string()],
        },
    ]
}

pub fn get_default_image_prompts() -> Vec<ImagePrompt> {
    vec![
        ImagePrompt {
            id: "default_portrait".to_string(),
            name: "默认头像".to_string(),
            prompt: "beautiful young Chinese girl, wearing elegant traditional qipao dress, gentle smile, soft lighting, portrait, anime style, detailed eyes, high quality".to_string(),
            negative_prompt: "blurry, low quality, distorted face, extra limbs".to_string(),
            style: "anime".to_string(),
            aspect_ratio: "1:1".to_string(),
        },
        ImagePrompt {
            id: "morning_scene".to_string(),
            name: "早晨场景".to_string(),
            prompt: "cute girl in qipao, standing by window, morning sunlight, birds outside, cozy room, warm colors, anime style".to_string(),
            negative_prompt: "dark, gloomy, messy room".to_string(),
            style: "anime".to_string(),
            aspect_ratio: "16:9".to_string(),
        },
        ImagePrompt {
            id: "music_listening".to_string(),
            name: "听音乐".to_string(),
            prompt: "girl in qipao, wearing headphones, listening to music, eyes closed, peaceful expression, soft purple lighting, anime style".to_string(),
            negative_prompt: "angry, harsh lighting, distorted".to_string(),
            style: "anime".to_string(),
            aspect_ratio: "9:16".to_string(),
        },
        ImagePrompt {
            id: "work_companion".to_string(),
            name: "工作陪伴".to_string(),
            prompt: "cute girl in qipao, sitting next to computer, helping with coding, focused expression, blue ambient light, anime style".to_string(),
            negative_prompt: "distracted, messy desk, low quality".to_string(),
            style: "anime".to_string(),
            aspect_ratio: "16:9".to_string(),
        },
        ImagePrompt {
            id: "relaxing".to_string(),
            name: "放松时刻".to_string(),
            prompt: "girl in qipao, lying on couch, reading book, cozy blanket, warm fireplace, peaceful atmosphere, anime style".to_string(),
            negative_prompt: "stressful, cluttered, cold colors".to_string(),
            style: "anime".to_string(),
            aspect_ratio: "16:9".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{paths::Paths, test_support::TestRoot};
    use std::collections::HashMap;

    #[test]
    fn test_default_persona() {
        let persona = Persona::default();
        assert_eq!(persona.name, "清和");
        assert!(!persona.personality.is_empty());
        assert!(!persona.prompt_templates.is_empty());
    }

    #[test]
    fn test_persona_manager() {
        let mut manager = PersonaManager::new();
        assert!(manager.get_current_persona().is_some());

        let new_persona = Persona {
            id: "test".to_string(),
            name: "测试".to_string(),
            ..Persona::default()
        };
        assert!(manager.add_persona(new_persona));
        assert!(manager.set_current_persona("test"));
        assert_eq!(manager.get_current_persona().unwrap().name, "测试");
    }

    #[test]
    fn test_generate_prompt() {
        let manager = PersonaManager::new();
        let mut variables = HashMap::new();
        variables.insert("user_input".to_string(), "你好".to_string());

        let prompt = manager.generate_prompt("chat_default", &variables);
        assert!(prompt.is_some());
        assert!(prompt.unwrap().contains("你好"));
    }

    #[test]
    fn test_default_templates() {
        let templates = get_default_prompt_templates();
        assert!(templates.len() > 0);
        assert!(templates.iter().any(|t| t.id == "chat_default"));
    }

    #[test]
    fn test_default_image_prompts() {
        let prompts = get_default_image_prompts();
        assert!(prompts.len() > 0);
        assert!(prompts.iter().any(|p| p.id == "default_portrait"));
    }

    #[tokio::test]
    async fn test_persona_state_persists() {
        let _root = TestRoot::new();
        let mut manager = load_manager().await.expect("load manager");
        let new_persona = Persona {
            id: "test".to_string(),
            name: "test persona".to_string(),
            ..Persona::default()
        };

        assert!(manager.add_persona(new_persona));
        assert!(manager.set_current_persona("test"));
        save_state(&manager.to_state()).await.expect("save state");

        let loaded = load_manager().await.expect("reload manager");
        assert_eq!(loaded.get_current_persona().unwrap().id, "test");
        assert!(fs::try_exists(Paths::persona_state_json())
            .await
            .expect("exists"));
    }
}
