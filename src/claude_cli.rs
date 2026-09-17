//! Claude Code CLI 常驻进程：通过 stdin/stdout stream-json 多轮复用，避免每次冷启动。

use crate::api::{text_with_file_context, ChatRequest, StreamEvent};
use crate::models::{Message, ProviderConfig, Role};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

struct Fingerprint {
    launch_desc: String,
    model: String,
    system_prompt: String,
    api_key: String,
}

impl Fingerprint {
    fn from_req(req: &ChatRequest, launch_desc: String) -> Self {
        Self {
            launch_desc,
            model: req.provider.model.trim().to_string(),
            system_prompt: req.system_prompt.trim().to_string(),
            api_key: req.provider.api_key.trim().to_string(),
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.launch_desc == other.launch_desc
            && self.model == other.model
            && self.system_prompt == other.system_prompt
            && self.api_key == other.api_key
    }
}

struct Daemon {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    fingerprint: Fingerprint,
    /// 上一轮请求里 user|assistant 消息条数
    last_msg_count: usize,
    last_used: Instant,
}

impl Daemon {
    fn is_alive(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            _ => false,
        }
    }
}

fn daemon_slot() -> &'static Mutex<Option<Daemon>> {
    static SLOT: OnceLock<Mutex<Option<Daemon>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

pub fn stream(req: &ChatRequest, tx: &Sender<StreamEvent>) -> Result<(), String> {
    let launch = resolve_claude_launch(&req.provider)?;
    let launch_desc = match &launch {
        ClaudeLaunch::Node { node, script } => format!("{node} {}", script.display()),
        ClaudeLaunch::Binary(bin) => bin.display().to_string(),
    };
    let fp = Fingerprint::from_req(req, launch_desc.clone());

    let msgs: Vec<&Message> = req
        .messages
        .iter()
        .filter(|m| matches!(m.role, Role::User | Role::Assistant))
        .collect();
    if msgs.is_empty() {
        return Err("没有可发送的消息".into());
    }
    if !matches!(msgs.last().map(|m| m.role), Some(Role::User)) {
        return Err("最后一条消息必须是用户消息".into());
    }

    let msg_count = msgs.len();
    let mut slot = daemon_slot()
        .lock()
        .map_err(|_| "Claude CLI 常驻锁异常".to_string())?;

    // 空闲过久则回收，避免长期占着 node
    if let Some(d) = slot.as_mut() {
        if d.last_used.elapsed() > Duration::from_secs(15 * 60) || !d.is_alive() {
            let _ = d.child.kill();
            let _ = d.child.wait();
            *slot = None;
        } else if !d.fingerprint.matches(&fp) {
            let _ = d.child.kill();
            let _ = d.child.wait();
            *slot = None;
        }
    }

    let continue_session = slot.as_ref().is_some_and(|d| {
        // 正常续聊：上一轮 n 条（以 user 结尾）→ 本轮 n+2（多了 assistant + 新 user）
        msg_count == d.last_msg_count + 2
    });

    let prompt = if continue_session {
        text_with_file_context(msgs[msgs.len() - 1])
    } else {
        // 新会话 / 改历史 / 切模型：重启进程，整段历史打成一条
        if let Some(mut d) = slot.take() {
            let _ = d.child.kill();
            let _ = d.child.wait();
        }
        build_prompt_from_msgs(&msgs)
    };

    if prompt.trim().is_empty() {
        return Err("没有可发送的消息".into());
    }

    if slot.is_none() {
        *slot = Some(spawn_daemon(&launch, &launch_desc, req)?);
    }

    // 取出 daemon，释放全局锁后再读写管道，避免长时间占用锁
    let mut daemon = slot.take().ok_or_else(|| "Claude CLI 常驻进程丢失".to_string())?;
    drop(slot);

    let send_result = (|| {
        send_user_message(&mut daemon.stdin, &prompt)?;
        read_until_result(&mut daemon.stdout, tx)
    })();

    let mut slot = daemon_slot()
        .lock()
        .map_err(|_| "Claude CLI 常驻锁异常".to_string())?;

    match send_result {
        Ok(()) => {
            daemon.last_msg_count = msg_count;
            daemon.last_used = Instant::now();
            *slot = Some(daemon);
            Ok(())
        }
        Err(e) => {
            let _ = daemon.child.kill();
            let _ = daemon.child.wait();
            *slot = None;
            Err(e)
        }
    }
}

fn build_prompt_from_msgs(msgs: &[&Message]) -> String {
    if msgs.len() == 1 {
        return text_with_file_context(msgs[0]);
    }
    let mut parts = Vec::with_capacity(msgs.len() + 1);
    parts.push(
        "以下是对话历史，请基于完整上下文直接回复最后一条用户消息，不要复述角色前缀。"
            .to_string(),
    );
    for m in msgs {
        let role = match m.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => continue,
        };
        parts.push(format!("{role}: {}", text_with_file_context(m)));
    }
    parts.join("\n\n")
}

fn send_user_message(stdin: &mut ChildStdin, text: &str) -> Result<(), String> {
    let msg = json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": text,
        },
        "parent_tool_use_id": Value::Null,
    });
    let line = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
    writeln!(stdin, "{line}").map_err(|e| format!("写入 Claude CLI stdin 失败: {e}"))?;
    stdin
        .flush()
        .map_err(|e| format!("刷新 Claude CLI stdin 失败: {e}"))?;
    Ok(())
}

fn read_until_result(
    stdout: &mut BufReader<ChildStdout>,
    tx: &Sender<StreamEvent>,
) -> Result<(), String> {
    let mut saw_token = false;
    let mut fallback_text = String::new();

    loop {
        let mut line = String::new();
        let n = stdout
            .read_line(&mut line)
            .map_err(|e| format!("读取 Claude CLI 输出失败: {e}"))?;
        if n == 0 {
            return Err("Claude CLI 常驻进程已退出".into());
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(event_type) = v.get("type").and_then(|t| t.as_str()) else {
            continue;
        };

        match event_type {
            "stream_event" => {
                if let Some(event) = v.get("event") {
                    if event.get("type").and_then(|t| t.as_str()) == Some("content_block_delta") {
                        if let Some(token) = event.pointer("/delta/text").and_then(|x| x.as_str()) {
                            if !token.is_empty() {
                                saw_token = true;
                                let _ = tx.send(StreamEvent::Token(token.to_string()));
                            }
                        }
                    }
                }
            }
            "assistant" => {
                if let Some(text) = v
                    .pointer("/message/content/0/text")
                    .and_then(|x| x.as_str())
                {
                    if !text.is_empty() {
                        fallback_text = text.to_string();
                    }
                }
            }
            "result" => {
                let is_error = v
                    .get("is_error")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false);
                if is_error {
                    let msg = v
                        .get("result")
                        .and_then(|x| x.as_str())
                        .or_else(|| v.get("error").and_then(|x| x.as_str()))
                        .unwrap_or("Claude CLI 返回错误");
                    return Err(msg.to_string());
                }
                if !saw_token {
                    if let Some(text) = v.get("result").and_then(|x| x.as_str()) {
                        if !text.is_empty() {
                            fallback_text = text.to_string();
                        }
                    }
                }
                break;
            }
            _ => {}
        }
    }

    if !saw_token && !fallback_text.is_empty() {
        let _ = tx.send(StreamEvent::Token(fallback_text));
    } else if !saw_token {
        return Err("Claude CLI 无输出".into());
    }
    Ok(())
}

fn spawn_daemon(
    launch: &ClaudeLaunch,
    launch_desc: &str,
    req: &ChatRequest,
) -> Result<Daemon, String> {
    let mut cmd = match launch {
        ClaudeLaunch::Node { node, script } => {
            let mut c = Command::new(node);
            c.arg(script);
            c
        }
        ClaudeLaunch::Binary(bin) => Command::new(bin),
    };

    apply_claude_user_env(&mut cmd, &req.provider);

    cmd.arg("-p")
        .arg("--bare")
        .arg("--strict-mcp-config")
        .arg("--disable-slash-commands")
        .arg("--verbose")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--input-format")
        .arg("stream-json")
        .arg("--include-partial-messages")
        .arg("--no-session-persistence")
        .arg("--permission-mode")
        .arg("dontAsk")
        .arg("--tools=");

    let system = req.system_prompt.trim();
    if !system.is_empty() {
        cmd.arg("--system-prompt").arg(system);
    }
    let model = req.provider.model.trim();
    if !model.is_empty() {
        cmd.arg("--model").arg(model);
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .current_dir(std::env::temp_dir());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "启动 Claude CLI 常驻失败 ({launch_desc}): {e}\n请确认已安装 Claude Code / Node.js，或在 CLI 路径填写 cli.js 完整路径"
        )
    })?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法打开 Claude CLI stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法打开 Claude CLI stdout".to_string())?;

    Ok(Daemon {
        child,
        stdin,
        stdout: BufReader::new(stdout),
        fingerprint: Fingerprint::from_req(req, launch_desc.to_string()),
        last_msg_count: 0,
        last_used: Instant::now(),
    })
}

fn default_claude_bin() -> String {
    if cfg!(windows) {
        "claude.cmd".into()
    } else {
        "claude".into()
    }
}

enum ClaudeLaunch {
    Node { node: String, script: PathBuf },
    Binary(PathBuf),
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            for ext in ["", ".cmd", ".exe", ".bat"] {
                let candidate = dir.join(format!("{name}{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn cli_js_beside_wrapper(wrapper: &Path) -> Option<PathBuf> {
    let dir = wrapper.parent()?;
    let script = dir
        .join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("cli.js");
    if script.is_file() {
        Some(script)
    } else {
        None
    }
}

fn resolve_claude_launch(cfg: &ProviderConfig) -> Result<ClaudeLaunch, String> {
    let raw = cfg.base_url.trim();
    let configured = if raw.is_empty() || raw.starts_with("http://") || raw.starts_with("https://")
    {
        None
    } else {
        Some(PathBuf::from(raw))
    };

    if let Some(path) = configured.as_ref() {
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("js"))
        {
            let node = which_on_path("node").unwrap_or_else(|| PathBuf::from("node"));
            return Ok(ClaudeLaunch::Node {
                node: node.to_string_lossy().into_owned(),
                script: path.clone(),
            });
        }
    }

    let wrapper = configured
        .clone()
        .filter(|p| p.is_file())
        .or_else(|| which_on_path("claude.cmd"))
        .or_else(|| which_on_path("claude"));

    if let Some(wrapper) = wrapper {
        if let Some(script) = cli_js_beside_wrapper(&wrapper) {
            let node = which_on_path("node").unwrap_or_else(|| PathBuf::from("node"));
            return Ok(ClaudeLaunch::Node {
                node: node.to_string_lossy().into_owned(),
                script,
            });
        }
        if !wrapper
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        {
            return Ok(ClaudeLaunch::Binary(wrapper));
        }
    }

    if let Some(home) = dirs::home_dir() {
        #[cfg(windows)]
        let candidates =
            [home.join(r"AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\cli.js")];
        #[cfg(not(windows))]
        let candidates = [
            home.join(".npm-global/lib/node_modules/@anthropic-ai/claude-code/cli.js"),
            PathBuf::from("/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js"),
            PathBuf::from("/usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js"),
        ];
        for script in candidates {
            if script.is_file() {
                let node = which_on_path("node").unwrap_or_else(|| PathBuf::from("node"));
                return Ok(ClaudeLaunch::Node {
                    node: node.to_string_lossy().into_owned(),
                    script,
                });
            }
        }
    }

    let fallback = configured.unwrap_or_else(|| PathBuf::from(default_claude_bin()));
    if cfg!(windows)
        && fallback
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
    {
        return Err(
            "无法定位 Claude Code 的 cli.js。请安装 Claude Code，或在 CLI 路径填写 cli.js 完整路径"
                .into(),
        );
    }
    Ok(ClaudeLaunch::Binary(fallback))
}

fn claude_settings_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

fn apply_claude_user_env(cmd: &mut Command, provider: &ProviderConfig) {
    use std::collections::HashMap;

    let mut envs = HashMap::new();
    envs.insert(
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
        "1".to_string(),
    );
    envs.insert("CLAUDE_CODE_SIMPLE".to_string(), "1".to_string());

    if let Some(path) = claude_settings_path() {
        if let Ok(raw) = std::fs::read_to_string(path) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                if let Some(map) = v.get("env").and_then(|e| e.as_object()) {
                    for (k, val) in map {
                        if let Some(s) = val.as_str().filter(|s| !s.is_empty()) {
                            envs.insert(k.clone(), s.to_string());
                        }
                    }
                }
            }
        }
    }

    let app_key = provider.api_key.trim();
    if !app_key.is_empty() {
        envs.insert("ANTHROPIC_API_KEY".to_string(), app_key.to_string());
        envs.insert("ANTHROPIC_AUTH_TOKEN".to_string(), app_key.to_string());
    } else if !envs.contains_key("ANTHROPIC_API_KEY") {
        if let Some(token) = envs.get("ANTHROPIC_AUTH_TOKEN").cloned() {
            envs.insert("ANTHROPIC_API_KEY".to_string(), token);
        }
    }

    for (k, v) in envs {
        cmd.env(k, v);
    }
}
