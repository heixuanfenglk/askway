use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    // 国际
    OpenAI,
    Anthropic,
    Gemini,
    Xai,
    Mistral,
    Groq,
    OpenRouter,
    Together,
    Fireworks,
    Perplexity,
    AzureOpenAI,
    Nvidia,
    // 国内
    DeepSeek,
    Moonshot,
    Zhipu,
    Tongyi,
    Doubao,
    SiliconFlow,
    Baichuan,
    MiniMax,
    Lingyi,
    StepFun,
    Hunyuan,
    Qianfan,
    SenseNova,
    // 本地 / 自定义
    Ollama,
    LocalClaude,
    Custom,
}

impl ProviderKind {
    pub fn all() -> &'static [ProviderKind] {
        &[
            Self::OpenAI,
            Self::Anthropic,
            Self::Gemini,
            Self::Xai,
            Self::Mistral,
            Self::Groq,
            Self::OpenRouter,
            Self::Together,
            Self::Fireworks,
            Self::Perplexity,
            Self::AzureOpenAI,
            Self::Nvidia,
            Self::DeepSeek,
            Self::Moonshot,
            Self::Zhipu,
            Self::Tongyi,
            Self::Doubao,
            Self::SiliconFlow,
            Self::Baichuan,
            Self::MiniMax,
            Self::Lingyi,
            Self::StepFun,
            Self::Hunyuan,
            Self::Qianfan,
            Self::SenseNova,
            Self::Ollama,
            Self::LocalClaude,
            Self::Custom,
        ]
    }

    pub fn category(self) -> &'static str {
        match self {
            Self::OpenAI
            | Self::Anthropic
            | Self::Gemini
            | Self::Xai
            | Self::Mistral
            | Self::Groq
            | Self::OpenRouter
            | Self::Together
            | Self::Fireworks
            | Self::Perplexity
            | Self::AzureOpenAI
            | Self::Nvidia => "国际",
            Self::DeepSeek
            | Self::Moonshot
            | Self::Zhipu
            | Self::Tongyi
            | Self::Doubao
            | Self::SiliconFlow
            | Self::Baichuan
            | Self::MiniMax
            | Self::Lingyi
            | Self::StepFun
            | Self::Hunyuan
            | Self::Qianfan
            | Self::SenseNova => "国内",
            Self::Ollama | Self::LocalClaude | Self::Custom => "本地 / 自定义",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic Claude",
            Self::Gemini => "Google Gemini",
            Self::Xai => "xAI Grok",
            Self::Mistral => "Mistral",
            Self::Groq => "Groq",
            Self::OpenRouter => "OpenRouter",
            Self::Together => "Together AI",
            Self::Fireworks => "Fireworks",
            Self::Perplexity => "Perplexity",
            Self::AzureOpenAI => "Azure OpenAI",
            Self::Nvidia => "NVIDIA NIM",
            Self::DeepSeek => "DeepSeek",
            Self::Moonshot => "Moonshot (Kimi)",
            Self::Zhipu => "智谱 GLM",
            Self::Tongyi => "通义千问",
            Self::Doubao => "豆包 (火山方舟)",
            Self::SiliconFlow => "硅基流动",
            Self::Baichuan => "百川",
            Self::MiniMax => "MiniMax",
            Self::Lingyi => "零一万物 Yi",
            Self::StepFun => "阶跃星辰",
            Self::Hunyuan => "腾讯混元",
            Self::Qianfan => "百度千帆",
            Self::SenseNova => "商汤日日新",
            Self::Ollama => "Ollama (本地)",
            Self::LocalClaude => "Claude Code (CLI)",
            Self::Custom => "自定义 (OpenAI 兼容)",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            Self::Xai => "https://api.x.ai/v1",
            Self::Mistral => "https://api.mistral.ai/v1",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::Together => "https://api.together.xyz/v1",
            Self::Fireworks => "https://api.fireworks.ai/inference/v1",
            Self::Perplexity => "https://api.perplexity.ai",
            Self::AzureOpenAI => "https://YOUR_RESOURCE.openai.azure.com/openai/deployments/YOUR_DEPLOYMENT",
            Self::Nvidia => "https://integrate.api.nvidia.com/v1",
            Self::DeepSeek => "https://api.deepseek.com/v1",
            Self::Moonshot => "https://api.moonshot.cn/v1",
            Self::Zhipu => "https://open.bigmodel.cn/api/paas/v4",
            Self::Tongyi => "https://dashscope.aliyuncs.com/compatible-mode/v1",
            Self::Doubao => "https://ark.cn-beijing.volces.com/api/v3",
            Self::SiliconFlow => "https://api.siliconflow.cn/v1",
            Self::Baichuan => "https://api.baichuan-ai.com/v1",
            Self::MiniMax => "https://api.minimaxi.com/v1",
            Self::Lingyi => "https://api.lingyiwanwu.com/v1",
            Self::StepFun => "https://api.stepfun.com/v1",
            Self::Hunyuan => "https://api.hunyuan.cloud.tencent.com/v1",
            Self::Qianfan => "https://qianfan.baidubce.com/v2",
            Self::SenseNova => "https://api.sensenova.cn/compatible-mode/v1",
            Self::Ollama => "http://127.0.0.1:11434/v1",
            // 复用 base_url 字段存放 Claude CLI 可执行文件路径（非 HTTP）
            Self::LocalClaude => "claude",
            Self::Custom => "https://api.example.com/v1",
        }
    }

    pub fn default_models(self) -> &'static [&'static str] {
        match self {
            Self::OpenAI => &[
                "gpt-5",
                "gpt-5-mini",
                "gpt-5-nano",
                "gpt-4.1",
                "gpt-4.1-mini",
                "gpt-4.1-nano",
                "gpt-4o",
                "gpt-4o-mini",
                "o3",
                "o3-mini",
                "o4-mini",
                "o1",
                "o1-mini",
            ],
            Self::Anthropic => &[
                "claude-opus-4-20250514",
                "claude-sonnet-4-20250514",
                "claude-3-7-sonnet-20250219",
                "claude-3-5-sonnet-20241022",
                "claude-3-5-haiku-20241022",
                "claude-3-opus-20240229",
                "claude-3-haiku-20240307",
            ],
            Self::Gemini => &[
                "gemini-2.5-pro",
                "gemini-2.5-flash",
                "gemini-2.5-flash-lite",
                "gemini-2.0-flash",
                "gemini-2.0-flash-lite",
                "gemini-1.5-pro",
                "gemini-1.5-flash",
            ],
            Self::Xai => &[
                "grok-4",
                "grok-3",
                "grok-3-mini",
                "grok-2",
                "grok-2-vision-1212",
            ],
            Self::Mistral => &[
                "mistral-large-latest",
                "mistral-medium-latest",
                "mistral-small-latest",
                "pixtral-large-latest",
                "codestral-latest",
                "open-mistral-nemo",
            ],
            Self::Groq => &[
                "llama-3.3-70b-versatile",
                "llama-3.1-8b-instant",
                "llama-3.1-70b-versatile",
                "gemma2-9b-it",
                "qwen/qwen3-32b",
                "deepseek-r1-distill-llama-70b",
                "meta-llama/llama-4-scout-17b-16e-instruct",
            ],
            Self::OpenRouter => &[
                "openai/gpt-4o",
                "openai/gpt-4.1",
                "anthropic/claude-sonnet-4",
                "anthropic/claude-opus-4",
                "google/gemini-2.5-pro",
                "google/gemini-2.5-flash",
                "deepseek/deepseek-chat",
                "deepseek/deepseek-r1",
                "meta-llama/llama-3.3-70b-instruct",
                "qwen/qwen3-235b-a22b",
                "x-ai/grok-3",
                "mistralai/mistral-large",
            ],
            Self::Together => &[
                "meta-llama/Llama-3.3-70B-Instruct-Turbo",
                "meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo",
                "Qwen/Qwen2.5-72B-Instruct-Turbo",
                "deepseek-ai/DeepSeek-R1",
                "deepseek-ai/DeepSeek-V3",
                "mistralai/Mixtral-8x22B-Instruct-v0.1",
            ],
            Self::Fireworks => &[
                "accounts/fireworks/models/llama-v3p3-70b-instruct",
                "accounts/fireworks/models/llama-v3p1-405b-instruct",
                "accounts/fireworks/models/deepseek-r1",
                "accounts/fireworks/models/deepseek-v3",
                "accounts/fireworks/models/qwen2p5-72b-instruct",
            ],
            Self::Perplexity => &[
                "sonar-pro",
                "sonar",
                "sonar-reasoning-pro",
                "sonar-reasoning",
                "sonar-deep-research",
            ],
            Self::AzureOpenAI => &[
                "gpt-4o",
                "gpt-4.1",
                "gpt-4o-mini",
                "o3-mini",
                "o1",
            ],
            Self::Nvidia => &[
                "meta/llama-3.3-70b-instruct",
                "meta/llama-3.1-405b-instruct",
                "deepseek-ai/deepseek-r1",
                "qwen/qwen2.5-72b-instruct",
                "mistralai/mistral-large-2-instruct",
                "google/gemma-2-27b-it",
            ],
            Self::DeepSeek => &[
                "deepseek-chat",
                "deepseek-reasoner",
                "deepseek-coder",
            ],
            Self::Moonshot => &[
                "kimi-k2-0905-preview",
                "kimi-latest",
                "moonshot-v1-auto",
                "moonshot-v1-8k",
                "moonshot-v1-32k",
                "moonshot-v1-128k",
                "moonshot-v1-8k-vision-preview",
            ],
            Self::Zhipu => &[
                "glm-4.5",
                "glm-4.5-air",
                "glm-4.5-flash",
                "glm-4-plus",
                "glm-4-air",
                "glm-4-flash",
                "glm-4v-plus",
                "glm-z1-air",
            ],
            Self::Tongyi => &[
                "qwen-max",
                "qwen-plus",
                "qwen-turbo",
                "qwen-long",
                "qwen-vl-max",
                "qwen-vl-plus",
                "qwen3-235b-a22b",
                "qwen3-32b",
                "qwq-plus",
                "deepseek-r1",
                "deepseek-v3",
            ],
            Self::Doubao => &[
                "doubao-seed-1-6-250615",
                "doubao-1-5-pro-32k-250115",
                "doubao-1-5-lite-32k-250115",
                "doubao-1-5-thinking-pro-250415",
                "doubao-1-5-vision-pro-32k-250115",
            ],
            Self::SiliconFlow => &[
                "deepseek-ai/DeepSeek-V3",
                "deepseek-ai/DeepSeek-R1",
                "Qwen/Qwen3-235B-A22B",
                "Qwen/Qwen2.5-72B-Instruct",
                "Qwen/QwQ-32B",
                "THUDM/GLM-4-9B-0414",
                "meta-llama/Llama-3.3-70B-Instruct",
                "Pro/deepseek-ai/DeepSeek-V3",
            ],
            Self::Baichuan => &[
                "Baichuan4-Turbo",
                "Baichuan4-Air",
                "Baichuan4",
                "Baichuan3-Turbo",
                "Baichuan3-Turbo-128k",
            ],
            Self::MiniMax => &[
                "MiniMax-Text-01",
                "MiniMax-M1",
                "abab6.5s-chat",
                "abab6.5-chat",
                "MiniMax-VL-01",
            ],
            Self::Lingyi => &[
                "yi-lightning",
                "yi-large",
                "yi-large-turbo",
                "yi-medium",
                "yi-spark",
                "yi-vision",
            ],
            Self::StepFun => &[
                "step-2-mini",
                "step-2-16k",
                "step-1.5v-mini",
                "step-1v-8k",
                "step-1-flash",
                "step-1-8k",
                "step-1-32k",
                "step-1-128k",
            ],
            Self::Hunyuan => &[
                "hunyuan-turbos-latest",
                "hunyuan-t1-latest",
                "hunyuan-large",
                "hunyuan-standard",
                "hunyuan-lite",
                "hunyuan-vision",
            ],
            Self::Qianfan => &[
                "ernie-4.5-turbo-128k",
                "ernie-4.5-8k",
                "ernie-4.0-8k",
                "ernie-4.0-turbo-8k",
                "ernie-3.5-8k",
                "ernie-speed-pro-128k",
                "deepseek-v3",
                "deepseek-r1",
            ],
            Self::SenseNova => &[
                "SenseChat-5",
                "SenseChat-Turbo",
                "SenseChat-Character",
                "SenseNova-V6",
                "SenseNova-V6-Turbo",
            ],
            Self::Ollama => &[
                "llama3.3",
                "llama3.2",
                "llama3.2-vision",
                "llama3.1",
                "qwen3",
                "qwen2.5",
                "qwen2.5-coder",
                "deepseek-r1",
                "deepseek-v3",
                "gemma3",
                "mistral",
                "phi4",
                "codellama",
            ],
            Self::LocalClaude => &[
                "haiku",
                "sonnet",
                "opus",
                "claude-haiku-4-5-20251001",
                "claude-sonnet-4-6",
                "claude-opus-4-6",
            ],
            Self::Custom => &[
                "gpt-4o",
                "gpt-4o-mini",
                "claude-sonnet-4",
                "gemini-2.5-flash",
                "deepseek-chat",
                "qwen-plus",
            ],
        }
    }

    pub fn allows_empty_api_key(self) -> bool {
        matches!(self, Self::Ollama | Self::LocalClaude)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub enabled: bool,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub custom_name: String,
}

impl ProviderConfig {
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            kind,
            enabled: false,
            api_key: String::new(),
            base_url: kind.default_base_url().to_string(),
            model: kind
                .default_models()
                .first()
                .copied()
                .unwrap_or("gpt-4o-mini")
                .to_string(),
            custom_name: String::new(),
        }
    }

    pub fn display_name(&self) -> String {
        if self.kind == ProviderKind::Custom && !self.custom_name.trim().is_empty() {
            self.custom_name.clone()
        } else {
            self.kind.label().to_string()
        }
    }

    /// 已启用，且已配置 Key（本地 Ollama / Claude 除外）
    pub fn is_ready(&self) -> bool {
        if !self.enabled {
            return false;
        }
        if self.kind.allows_empty_api_key() {
            return true;
        }
        !self.api_key.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_api_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "系统",
            Self::User => "我",
            Self::Assistant => "AI",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AttachmentKind {
    Image,
    Text,
    #[default]
    Binary,
}

impl AttachmentKind {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Image => "🖼",
            Self::Text => "📄",
            Self::Binary => "📎",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Uuid,
    pub name: String,
    pub mime: String,
    #[serde(default)]
    pub kind: AttachmentKind,
    /// 保存在 data_dir/attachments 下的文件名
    pub stored_name: String,
    pub size: u64,
    /// 文本类附件的内容（发送时内联）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
}

impl Attachment {
    pub fn summary(&self) -> String {
        let size = if self.size < 1024 {
            format!("{} B", self.size)
        } else if self.size < 1024 * 1024 {
            format!("{:.1} KB", self.size as f64 / 1024.0)
        } else {
            format!("{:.1} MB", self.size as f64 / (1024.0 * 1024.0))
        };
        format!("{} {} ({})", self.kind.icon(), self.name, size)
    }
}

fn empty_attachments() -> Vec<Attachment> {
    Vec::new()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub role: Role,
    pub content: String,
    pub created_at: DateTime<Utc>,
    #[serde(default = "empty_attachments")]
    pub attachments: Vec<Attachment>,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content: content.into(),
            created_at: Utc::now(),
            attachments: Vec::new(),
        }
    }

    pub fn with_attachments(mut self, attachments: Vec<Attachment>) -> Self {
        self.attachments = attachments;
        self
    }

    pub fn has_attachments(&self) -> bool {
        !self.attachments.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub provider: ProviderKind,
    pub model: String,
    pub system_prompt: String,
    pub messages: Vec<Message>,
    pub updated_at: DateTime<Utc>,
}

impl Conversation {
    pub fn new(provider: ProviderKind, model: String, system_prompt: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: "新对话".to_string(),
            provider,
            model,
            system_prompt,
            messages: Vec::new(),
            updated_at: Utc::now(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn auto_title_from_first_user(&mut self) {
        if self.title != "新对话" {
            return;
        }
        if let Some(msg) = self.messages.iter().find(|m| m.role == Role::User) {
            let t = msg.content.trim();
            let title: String = if t.is_empty() {
                msg.attachments
                    .first()
                    .map(|a| format!("附件: {}", a.name))
                    .unwrap_or_else(|| "新对话".to_string())
            } else {
                let short: String = t.chars().take(24).collect();
                if t.chars().count() > 24 {
                    format!("{short}…")
                } else {
                    short
                }
            };
            self.title = if title.is_empty() {
                "新对话".to_string()
            } else {
                title
            };
        }
    }
}

fn default_system_prompt() -> String {
    "你是一个有帮助的 AI 助手。".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub providers: Vec<ProviderConfig>,
    pub active_provider: ProviderKind,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        let providers: Vec<_> = ProviderKind::all()
            .iter()
            .copied()
            .map(ProviderConfig::new)
            .collect();
        Self {
            providers,
            active_provider: ProviderKind::DeepSeek,
            system_prompt: default_system_prompt(),
            temperature: 0.7,
            max_tokens: 4096,
        }
    }
}

impl AppSettings {
    pub fn provider_mut(&mut self, kind: ProviderKind) -> Option<&mut ProviderConfig> {
        self.providers.iter_mut().find(|p| p.kind == kind)
    }

    pub fn provider(&self, kind: ProviderKind) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.kind == kind)
    }

    pub fn active(&self) -> Option<&ProviderConfig> {
        self.provider(self.active_provider)
    }

    /// 聊天快捷切换：仅返回已启用且已配置 Key 的厂家
    pub fn ready_providers(&self) -> Vec<&ProviderConfig> {
        self.providers.iter().filter(|p| p.is_ready()).collect()
    }

    /// 兼容旧配置：补齐新增厂家，并按内置顺序排列
    pub fn ensure_all_providers(&mut self) {
        for kind in ProviderKind::all() {
            if self.provider(*kind).is_none() {
                self.providers.push(ProviderConfig::new(*kind));
            }
        }
        let mut ordered = Vec::with_capacity(ProviderKind::all().len());
        for kind in ProviderKind::all() {
            if let Some(pos) = self.providers.iter().position(|p| p.kind == *kind) {
                ordered.push(self.providers.remove(pos));
            }
        }
        // 保留未知/重复项（理论上不应有）
        ordered.append(&mut self.providers);
        self.providers = ordered;
    }
}
