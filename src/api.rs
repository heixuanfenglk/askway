use crate::attachments;
use crate::models::{Attachment, AttachmentKind, Message, ProviderConfig, ProviderKind, Role};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub provider: ProviderConfig,
    pub messages: Vec<Message>,
    pub system_prompt: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    Done,
    Error(String),
}

pub fn stream_chat(req: ChatRequest, tx: Sender<StreamEvent>) {
    let result = match req.provider.kind {
        ProviderKind::LocalClaude => crate::claude_cli::stream(&req, &tx),
        ProviderKind::Anthropic => stream_anthropic(&req, &tx),
        ProviderKind::Gemini => stream_gemini(&req, &tx),
        _ => stream_openai_compat(&req, &tx),
    };

    match result {
        Ok(()) => {
            let _ = tx.send(StreamEvent::Done);
        }
        Err(e) => {
            let _ = tx.send(StreamEvent::Error(e));
        }
    }
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .connect_timeout(std::time::Duration::from_secs(20))
        .pool_max_idle_per_host(2)
        .tcp_nodelay(true)
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

fn format_reqwest_error(context: &str, err: reqwest::Error) -> String {
    let mut parts = vec![format!("{context}: {err}")];
    let mut source = std::error::Error::source(&err);
    while let Some(s) = source {
        parts.push(format!("原因: {s}"));
        source = s.source();
    }

    let hint = if err.is_timeout() {
        Some("请求超时。请检查网络，或确认代理软件（如 Clash）已启动。")
    } else if err.is_connect() {
        Some(
            "无法建立连接。若系统设置了 HTTP_PROXY=127.0.0.1:7890，请先打开代理；\
或不需要代理时，清除环境变量 HTTP_PROXY / HTTPS_PROXY / ALL_PROXY 后重启本程序。",
        )
    } else if format!("{err}").contains("proxy") || format!("{err}").contains("7890") {
        Some("代理连接失败。请启动 Clash / V2Ray 等，或关闭系统代理环境变量后重试。")
    } else {
        None
    };

    if let Some(h) = hint {
        parts.push(format!("提示: {h}"));
    }
    parts.join("\n")
}

pub(crate) fn text_with_file_context(msg: &Message) -> String {
    let mut parts = Vec::new();
    if !msg.content.trim().is_empty() {
        parts.push(msg.content.clone());
    }
    for att in &msg.attachments {
        match att.kind {
            AttachmentKind::Text => {
                let body = att
                    .text_content
                    .clone()
                    .unwrap_or_else(|| "[无法读取文本内容]".into());
                parts.push(format!(
                    "\n\n----- 附件文件: {} -----\n{}\n----- 文件结束 -----",
                    att.name, body
                ));
            }
            AttachmentKind::Binary => {
                parts.push(format!(
                    "\n\n[用户上传了二进制文件: {} ({}, {})，当前接口无法直接解析该格式，请根据文件名理解意图]",
                    att.name,
                    att.mime,
                    attachments::format_size(att.size)
                ));
            }
            AttachmentKind::Image => {}
        }
    }
    if parts.is_empty() {
        if msg.attachments.iter().any(|a| a.kind == AttachmentKind::Image) {
            "请查看图片并回答。".into()
        } else {
            String::new()
        }
    } else {
        parts.join("")
    }
}

fn build_openai_content(msg: &Message) -> Result<Value, String> {
    let text = text_with_file_context(msg);
    let images: Vec<&Attachment> = msg
        .attachments
        .iter()
        .filter(|a| a.kind == AttachmentKind::Image)
        .collect();

    if images.is_empty() {
        return Ok(Value::String(text));
    }

    let mut content = Vec::new();
    if !text.trim().is_empty() {
        content.push(json!({
            "type": "text",
            "text": text,
        }));
    }
    for img in images {
        let b64 = attachments::read_base64(img)?;
        content.push(json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:{};base64,{}", img.mime, b64)
            }
        }));
    }
    Ok(Value::Array(content))
}

fn build_openai_messages(req: &ChatRequest) -> Result<Vec<Value>, String> {
    let mut list = Vec::new();
    if !req.system_prompt.trim().is_empty() {
        list.push(json!({
            "role": "system",
            "content": req.system_prompt,
        }));
    }
    for m in &req.messages {
        if m.role == Role::System {
            continue;
        }
        list.push(json!({
            "role": m.role.as_api_str(),
            "content": build_openai_content(m)?,
        }));
    }
    Ok(list)
}

fn build_anthropic_content(msg: &Message) -> Result<Value, String> {
    let text = text_with_file_context(msg);
    let mut blocks = Vec::new();

    for img in msg
        .attachments
        .iter()
        .filter(|a| a.kind == AttachmentKind::Image)
    {
        let b64 = attachments::read_base64(img)?;
        let media = match img.mime.as_str() {
            "image/png" | "image/jpeg" | "image/gif" | "image/webp" => img.mime.clone(),
            _ => "image/png".to_string(),
        };
        blocks.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media,
                "data": b64,
            }
        }));
    }

    if !text.trim().is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": text,
        }));
    } else if blocks.is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": "",
        }));
    }

    Ok(Value::Array(blocks))
}

fn build_gemini_parts(msg: &Message) -> Result<Vec<Value>, String> {
    let mut parts = Vec::new();
    let text = text_with_file_context(msg);
    if !text.trim().is_empty() {
        parts.push(json!({ "text": text }));
    }
    for img in msg
        .attachments
        .iter()
        .filter(|a| a.kind == AttachmentKind::Image)
    {
        let b64 = attachments::read_base64(img)?;
        parts.push(json!({
            "inline_data": {
                "mime_type": img.mime,
                "data": b64,
            }
        }));
    }
    if parts.is_empty() {
        parts.push(json!({ "text": "" }));
    }
    Ok(parts)
}

fn stream_openai_compat(req: &ChatRequest, tx: &Sender<StreamEvent>) -> Result<(), String> {
    let client = http_client()?;
    let base = req.provider.base_url.trim_end_matches('/');
    let url = if req.provider.kind == ProviderKind::AzureOpenAI {
        if base.contains("api-version=") {
            format!("{base}/chat/completions")
        } else if base.contains('?') {
            format!("{base}&api-version=2024-12-01-preview")
        } else {
            // 期望 Base URL 指向 deployment，例如 .../deployments/gpt-4o
            format!("{base}/chat/completions?api-version=2024-12-01-preview")
        }
    } else {
        format!("{base}/chat/completions")
    };
    let messages = build_openai_messages(req)?;

    let mut builder = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&json!({
            "model": req.provider.model,
            "messages": messages,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
            "stream": true,
        }));

    if !req.provider.api_key.trim().is_empty() {
        if req.provider.kind == ProviderKind::AzureOpenAI {
            builder = builder.header("api-key", req.provider.api_key.trim());
        } else {
            builder = builder.bearer_auth(req.provider.api_key.trim());
        }
    }

    let response = builder
        .send()
        .map_err(|e| format_reqwest_error("网络请求失败", e))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("API 错误 ({status}): {body}"));
    }

    let reader = BufReader::new(response);
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let data = line.strip_prefix("data:").map(str::trim).unwrap_or(line);
        if data == "[DONE]" {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(token) = v
            .pointer("/choices/0/delta/content")
            .and_then(|x| x.as_str())
        {
            if !token.is_empty() {
                let _ = tx.send(StreamEvent::Token(token.to_string()));
            }
        }
    }
    Ok(())
}


fn stream_anthropic(req: &ChatRequest, tx: &Sender<StreamEvent>) -> Result<(), String> {
    let client = http_client()?;
    let base = req.provider.base_url.trim_end_matches('/');
    let url = format!("{base}/v1/messages");

    let mut messages = Vec::new();
    for m in &req.messages {
        if !matches!(m.role, Role::User | Role::Assistant) {
            continue;
        }
        let content = if m.role == Role::User && m.has_attachments() {
            build_anthropic_content(m)?
        } else if m.role == Role::User {
            Value::String(text_with_file_context(m))
        } else {
            Value::String(m.content.clone())
        };
        messages.push(json!({
            "role": m.role.as_api_str(),
            "content": content,
        }));
    }

    let mut builder = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .json(&json!({
            "model": req.provider.model,
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "system": req.system_prompt,
            "messages": messages,
            "stream": true,
        }));

    // 本地 Anthropic 兼容服务通常可不鉴权；有 Key 时再带上
    let api_key = req.provider.api_key.trim();
    if !api_key.is_empty() {
        builder = builder.header("x-api-key", api_key);
    }

    let response = builder
        .send()
        .map_err(|e| format_reqwest_error("网络请求失败", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("API 错误 ({status}): {body}"));
    }

    let reader = BufReader::new(response);
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if !line.starts_with("data:") {
            continue;
        }
        let data = line.trim_start_matches("data:").trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) == Some("content_block_delta") {
            if let Some(token) = v.pointer("/delta/text").and_then(|x| x.as_str()) {
                if !token.is_empty() {
                    let _ = tx.send(StreamEvent::Token(token.to_string()));
                }
            }
        }
    }
    Ok(())
}

fn stream_gemini(req: &ChatRequest, tx: &Sender<StreamEvent>) -> Result<(), String> {
    let client = http_client()?;
    let base = req.provider.base_url.trim_end_matches('/');
    let model = &req.provider.model;
    let key = req.provider.api_key.trim();
    let url = format!("{base}/models/{model}:streamGenerateContent?alt=sse&key={key}");

    let mut contents = Vec::new();
    for m in &req.messages {
        let role = match m.role {
            Role::User => "user",
            Role::Assistant => "model",
            Role::System => continue,
        };
        let parts = if m.role == Role::Assistant {
            vec![json!({ "text": m.content })]
        } else {
            build_gemini_parts(m)?
        };
        contents.push(json!({
            "role": role,
            "parts": parts,
        }));
    }

    let mut body = json!({
        "contents": contents,
        "generationConfig": {
            "temperature": req.temperature,
            "maxOutputTokens": req.max_tokens,
        }
    });

    if !req.system_prompt.trim().is_empty() {
        body["systemInstruction"] = json!({
            "parts": [{"text": req.system_prompt}]
        });
    }

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format_reqwest_error("网络请求失败", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("API 错误 ({status}): {body}"));
    }

    let reader = BufReader::new(response);
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if !line.starts_with("data:") {
            continue;
        }
        let data = line.trim_start_matches("data:").trim();
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(token) = v
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(|x| x.as_str())
        {
            if !token.is_empty() {
                let _ = tx.send(StreamEvent::Token(token.to_string()));
            }
        }
    }
    Ok(())
}
