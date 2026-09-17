use crate::api::{self, ChatRequest, StreamEvent};
use crate::attachments;
use crate::models::{
    Attachment, Conversation, Message, ProviderConfig, ProviderKind, Role,
};
use crate::storage::{self, AppData};
use eframe::egui::{
    self, pos2, Align, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Rect, RichText,
    ScrollArea, Sense, Stroke, Vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Chat,
    Settings,
}

/// 侧栏会话级搜索摘要
struct SearchHit {
    /// 正文/附件命中次数（标题命中可为 0）
    hit_count: usize,
    snippet: String,
}

/// 单次关键词命中（可跳转）
#[derive(Debug, Clone)]
struct SearchMatch {
    conv_id: Uuid,
    msg_id: Uuid,
    /// 在 `Message.content` 中的字节起点；附件名命中时为 None
    byte_start: Option<usize>,
    byte_len: usize,
}

pub struct AskwayApp {
    data: AppData,
    page: Page,
    input: String,
    pending_attachments: Vec<Attachment>,
    status: String,
    streaming: bool,
    stream_rx: Option<Receiver<StreamEvent>>,
    stream_conv_id: Option<Uuid>,
    stream_msg_id: Option<Uuid>,
    settings_provider: ProviderKind,
    settings_model_filter: String,
    dirty: bool,
    show_delete_confirm: Option<Uuid>,
    /// 是否已按屏幕尺寸调整过窗口
    window_sized: bool,
    md_cache: CommonMarkCache,
    /// 最近一次点「复制」的消息与时间，用于按钮反馈
    copied_msg: Option<(Uuid, Instant)>,
    /// 下一帧将消息列表滚到最新（发送新消息块时置位）
    scroll_messages_to_bottom: bool,
    /// 是否跟随最新消息（贴底时为 true；用户上滑后为 false）
    follow_latest_message: bool,
    /// 上次把流式内容刷新到界面的时间（用于节流，避免每 token 卡 UI）
    last_stream_ui: Instant,
    /// 侧栏全文搜索关键词
    sidebar_search: String,
    /// 已应用到导航的搜索词（用于检测变更）
    search_query_applied: String,
    /// 当前会话内的全部关键词命中
    search_matches: Vec<SearchMatch>,
    /// 当前命中下标
    search_match_idx: usize,
    /// 下一帧滚到当前命中关键词位置
    scroll_to_match: bool,
    /// 搜索命中高亮（消息 id + 开始时间）
    highlight_msg: Option<(Uuid, Instant)>,
}

impl AskwayApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);

        let mut data = storage::load();
        if data.conversations.is_empty() {
            let provider = data.settings.active_provider;
            let model = data
                .settings
                .active()
                .map(|p| p.model.clone())
                .unwrap_or_else(|| "deepseek-chat".into());
            let prompt = data.settings.system_prompt.clone();
            let conv = Conversation::new(provider, model, prompt);
            data.current_id = Some(conv.id);
            data.conversations.push(conv);
        }
        if data.current_id.is_none() {
            data.current_id = data.conversations.first().map(|c| c.id);
        }

        Self {
            settings_provider: data.settings.active_provider,
            settings_model_filter: String::new(),
            data,
            page: Page::Chat,
            input: String::new(),
            pending_attachments: Vec::new(),
            status: "就绪".into(),
            streaming: false,
            stream_rx: None,
            stream_conv_id: None,
            stream_msg_id: None,
            dirty: false,
            show_delete_confirm: None,
            window_sized: false,
            md_cache: CommonMarkCache::default(),
            copied_msg: None,
            scroll_messages_to_bottom: false,
            follow_latest_message: true,
            last_stream_ui: Instant::now(),
            sidebar_search: String::new(),
            search_query_applied: String::new(),
            search_matches: Vec::new(),
            search_match_idx: 0,
            scroll_to_match: false,
            highlight_msg: None,
        }
    }

    fn toggle_sidebar(&mut self) {
        self.data.sidebar_open = !self.data.sidebar_open;
        self.dirty = true;
        self.persist();
    }

    /// 右上角侧栏开关：Cursor 风格「左侧栏」线框图标，收起后仍保留
    fn sidebar_toggle_icon_button(&mut self, ui: &mut egui::Ui) {
        let tip = if self.data.sidebar_open {
            "收起侧栏"
        } else {
            "展开侧栏"
        };
        let response = self.paint_sidebar_layout_icon(ui, self.data.sidebar_open);
        if response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(tip)
            .clicked()
        {
            self.toggle_sidebar();
        }
    }

    /// Cursor / VS Code 风格：圆角窗体 + 左侧栏分割线
    fn paint_sidebar_layout_icon(&self, ui: &mut egui::Ui, filled_left: bool) -> egui::Response {
        let size = 28.0;
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
        let response = response.on_hover_cursor(CursorIcon::PointingHand);

        if response.hovered() || response.has_focus() {
            ui.painter().rect(
                rect.shrink(1.5),
                CornerRadius::same(6),
                Color32::from_rgba_unmultiplied(0, 0, 0, 18),
                Stroke::NONE,
                egui::StrokeKind::Inside,
            );
        }

        let icon = Rect::from_center_size(rect.center(), Vec2::new(15.0, 13.0));
        let fg = if response.hovered() {
            Color32::from_rgb(40, 48, 56)
        } else {
            Color32::from_rgb(70, 82, 94)
        };
        let stroke = Stroke::new(1.4, fg);
        let radius = CornerRadius::same(2);

        // 外框
        ui.painter().rect_stroke(icon, radius, stroke, egui::StrokeKind::Outside);

        // 左侧栏分割线（约 35%）
        let split_x = icon.left() + icon.width() * 0.36;
        ui.painter().line_segment(
            [pos2(split_x, icon.top() + 0.5), pos2(split_x, icon.bottom() - 0.5)],
            stroke,
        );

        // 侧栏打开时填充左侧条，贴近 Cursor 的「面板已显示」状态
        if filled_left {
            let left = Rect::from_min_max(icon.left_top(), pos2(split_x, icon.bottom()));
            ui.painter().rect(
                left.shrink2(Vec2::new(1.1, 1.1)),
                CornerRadius {
                    nw: 1,
                    ne: 0,
                    sw: 1,
                    se: 0,
                },
                Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 55),
                Stroke::NONE,
                egui::StrokeKind::Inside,
            );
        }

        response
    }

    /// Cursor 风格设置齿轮线框图标
    fn settings_icon_button(&mut self, ui: &mut egui::Ui) -> bool {
        let size = 28.0;
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
        let response = response.on_hover_cursor(CursorIcon::PointingHand);

        if response.hovered() || response.has_focus() {
            ui.painter().rect(
                rect.shrink(1.5),
                CornerRadius::same(6),
                Color32::from_rgba_unmultiplied(0, 0, 0, 18),
                Stroke::NONE,
                egui::StrokeKind::Inside,
            );
        }

        let c = rect.center();
        let fg = if response.hovered() {
            Color32::from_rgb(40, 48, 56)
        } else {
            Color32::from_rgb(70, 82, 94)
        };
        let stroke = Stroke::new(1.35, fg);
        let r_outer = 6.2;
        let r_inner = 2.4;

        // 齿轮齿
        for i in 0..6 {
            let a = (i as f32) * std::f32::consts::TAU / 6.0;
            let dir = Vec2::new(a.cos(), a.sin());
            let p0 = c + dir * (r_outer * 0.55);
            let p1 = c + dir * r_outer;
            ui.painter().line_segment([p0, p1], Stroke::new(2.0, fg));
        }
        ui.painter().circle_stroke(c, r_outer * 0.62, stroke);
        ui.painter().circle_stroke(c, r_inner, stroke);

        response.on_hover_text("模型设置").clicked()
    }

    fn ensure_window_fits_screen(&mut self, ctx: &egui::Context) {
        if self.window_sized {
            return;
        }
        self.window_sized = true;

        let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) else {
            return;
        };
        if monitor.x < 100.0 || monitor.y < 100.0 {
            return;
        }

        // 约占屏幕 82%，并留出边距；同时不低于最小可用尺寸
        let target_w = (monitor.x * 0.82).clamp(880.0, (monitor.x - 48.0).max(880.0));
        let target_h = (monitor.y * 0.82).clamp(600.0, (monitor.y - 72.0).max(600.0));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(target_w, target_h)));

        // 尽量居中
        let pos_x = ((monitor.x - target_w) * 0.5).max(16.0);
        let pos_y = ((monitor.y - target_h) * 0.45).max(24.0);
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(pos_x, pos_y)));
    }

    fn persist(&mut self) {
        if let Err(e) = storage::save(&self.data) {
            self.status = format!("保存失败: {e}");
        } else {
            self.dirty = false;
        }
    }

    fn current_conv(&self) -> Option<&Conversation> {
        let id = self.data.current_id?;
        self.data.conversations.iter().find(|c| c.id == id)
    }

    fn current_conv_mut(&mut self) -> Option<&mut Conversation> {
        let id = self.data.current_id?;
        self.data.conversations.iter_mut().find(|c| c.id == id)
    }

    /// 会话是否匹配搜索（标题 / 正文 / 附件名），并给出摘要
    fn search_conversation(conv: &Conversation, query: &str) -> Option<SearchHit> {
        let q = query.trim();
        if q.is_empty() {
            return Some(SearchHit {
                hit_count: 0,
                snippet: String::new(),
            });
        }

        let matches = Self::collect_matches(conv, q);
        if let Some(first) = matches.first() {
            let snippet = match first.byte_start {
                Some(start) => {
                    if let Some(msg) = conv.messages.iter().find(|m| m.id == first.msg_id) {
                        snippet_around_byte(&msg.content, start, first.byte_len, 48)
                    } else {
                        String::new()
                    }
                }
                None => {
                    if let Some(msg) = conv.messages.iter().find(|m| m.id == first.msg_id) {
                        let name = msg
                            .attachments
                            .iter()
                            .find(|a| a.name.to_lowercase().contains(&q.to_lowercase()))
                            .map(|a| a.name.as_str())
                            .unwrap_or("附件");
                        format!("附件 · {}", truncate_chars(name, 40))
                    } else {
                        String::new()
                    }
                }
            };
            return Some(SearchHit {
                hit_count: matches.len(),
                snippet,
            });
        }

        let title = if conv.title.is_empty() {
            "新对话"
        } else {
            conv.title.as_str()
        };
        if title.to_lowercase().contains(&q.to_lowercase()) {
            return Some(SearchHit {
                hit_count: 0,
                snippet: format!("标题 · {}", truncate_chars(title, 40)),
            });
        }
        None
    }

    /// 收集会话内全部可跳转命中（按消息顺序，同消息内按出现顺序）
    fn collect_matches(conv: &Conversation, query: &str) -> Vec<SearchMatch> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let q_lower = q.to_lowercase();
        let mut out = Vec::new();

        for msg in &conv.messages {
            for (byte_start, byte_end) in find_all_match_ranges(&msg.content, &q_lower) {
                out.push(SearchMatch {
                    conv_id: conv.id,
                    msg_id: msg.id,
                    byte_start: Some(byte_start),
                    byte_len: byte_end - byte_start,
                });
            }
            for att in &msg.attachments {
                if att.name.to_lowercase().contains(&q_lower) {
                    out.push(SearchMatch {
                        conv_id: conv.id,
                        msg_id: msg.id,
                        byte_start: None,
                        byte_len: 0,
                    });
                }
            }
        }
        out
    }

    fn clear_search_navigation(&mut self) {
        self.search_query_applied.clear();
        self.search_matches.clear();
        self.search_match_idx = 0;
        self.scroll_to_match = false;
        self.highlight_msg = None;
    }

    /// 加载某会话的命中列表；`jump` 为 true 时跳到第 first 处
    fn load_search_matches(&mut self, conv_id: Uuid, jump: bool) {
        let q = self.sidebar_search.trim().to_string();
        self.search_query_applied = q.clone();
        let matches = self
            .data
            .conversations
            .iter()
            .find(|c| c.id == conv_id)
            .map(|c| Self::collect_matches(c, &q))
            .unwrap_or_default();
        self.search_matches = matches;
        self.search_match_idx = 0;
        if jump && !self.search_matches.is_empty() {
            self.focus_current_search_match();
        } else {
            self.scroll_to_match = false;
            if let Some(m) = self.search_matches.first() {
                self.highlight_msg = Some((m.msg_id, Instant::now()));
            } else {
                self.highlight_msg = None;
            }
        }
    }

    fn focus_current_search_match(&mut self) {
        let Some(m) = self.search_matches.get(self.search_match_idx).cloned() else {
            self.scroll_to_match = false;
            return;
        };
        if self.data.current_id != Some(m.conv_id) {
            self.data.current_id = Some(m.conv_id);
            self.sync_settings_from_current_conv();
            self.dirty = true;
            self.persist();
        }
        self.page = Page::Chat;
        self.scroll_to_match = true;
        self.scroll_messages_to_bottom = false;
        self.follow_latest_message = false;
        self.highlight_msg = Some((m.msg_id, Instant::now()));
    }

    fn goto_search_delta(&mut self, delta: isize) {
        let n = self.search_matches.len();
        if n == 0 {
            return;
        }
        self.search_match_idx =
            ((self.search_match_idx as isize + delta).rem_euclid(n as isize)) as usize;
        self.focus_current_search_match();
    }

    /// 搜索词变化时，刷新当前会话命中（不自动跳转）
    fn sync_search_query_if_changed(&mut self) {
        let q = self.sidebar_search.trim().to_string();
        if q == self.search_query_applied {
            return;
        }
        if q.is_empty() {
            self.clear_search_navigation();
            return;
        }
        if let Some(id) = self.data.current_id {
            self.load_search_matches(id, false);
        } else {
            self.search_query_applied = q;
            self.search_matches.clear();
            self.search_match_idx = 0;
            self.scroll_to_match = false;
        }
    }

    /// 将当前会话的厂家/模型同步到全局设置（切换会话或改会话路由后调用）
    fn sync_settings_from_current_conv(&mut self) {
        let Some((kind, model)) = self
            .current_conv()
            .map(|c| (c.provider, c.model.clone()))
        else {
            return;
        };
        self.data.settings.active_provider = kind;
        self.settings_provider = kind;
        if !model.is_empty() {
            if let Some(cfg) = self.data.settings.provider_mut(kind) {
                cfg.model = model;
            }
        }
    }

    /// 切换厂商：自动带上该厂商自己的模型与 Base URL（当前会话 + 全局设置一起改）
    fn switch_to_provider(&mut self, kind: ProviderKind) {
        self.data.settings.ensure_all_providers();
        self.data.settings.active_provider = kind;
        self.settings_provider = kind;

        let (display, model, base_url) = {
            let cfg = self
                .data
                .settings
                .provider_mut(kind)
                .expect("provider must exist after ensure_all_providers");

            // 确保有该厂商自己的 API 地址
            if cfg.base_url.trim().is_empty() {
                cfg.base_url = kind.default_base_url().to_string();
            }

            let defaults = kind.default_models();
            if cfg.model.trim().is_empty() {
                if let Some(m) = defaults.first() {
                    cfg.model = (*m).to_string();
                }
            } else if !defaults.is_empty() && !defaults.iter().any(|m| *m == cfg.model) {
                // 当前保存的模型不属于该厂商默认列表时，自动切到该厂商默认模型
                // （自定义 / 聚合平台保留用户填写的模型名）
                let keep_custom = matches!(
                    kind,
                    ProviderKind::Custom
                        | ProviderKind::OpenRouter
                        | ProviderKind::Ollama
                        | ProviderKind::LocalClaude
                        | ProviderKind::AzureOpenAI
                        | ProviderKind::Fireworks
                        | ProviderKind::Together
                );
                if !keep_custom {
                    cfg.model = defaults[0].to_string();
                }
            }

            (cfg.display_name(), cfg.model.clone(), cfg.base_url.clone())
        };

        if self.current_conv().is_none() {
            self.new_conversation();
        }
        if let Some(conv) = self.current_conv_mut() {
            conv.provider = kind;
            conv.model = model.clone();
            conv.touch();
        }

        self.dirty = true;
        self.persist();
        self.status = format!("已切换: {display} · {model} · {base_url}");
    }

    /// 按当前会话解析实际请求配置（厂家 base_url / key + 会话模型）
    fn provider_config_for_send(&self) -> Option<ProviderConfig> {
        let (kind, model) = if let Some(conv) = self.current_conv() {
            (conv.provider, conv.model.clone())
        } else {
            let kind = self.data.settings.active_provider;
            let model = self
                .data
                .settings
                .active()
                .map(|p| p.model.clone())
                .unwrap_or_default();
            (kind, model)
        };

        let mut cfg = self.data.settings.provider(kind)?.clone();
        // 防止配置异常：始终按 kind 使用对应厂商 endpoint
        cfg.kind = kind;
        if cfg.base_url.trim().is_empty() {
            cfg.base_url = kind.default_base_url().to_string();
        }
        if !model.is_empty() {
            cfg.model = model;
        } else if cfg.model.trim().is_empty() {
            if let Some(m) = kind.default_models().first() {
                cfg.model = (*m).to_string();
            }
        }
        Some(cfg)
    }

    fn new_conversation(&mut self) {
        let provider = self.data.settings.active_provider;
        let model = self
            .data
            .settings
            .active()
            .map(|p| p.model.clone())
            .unwrap_or_default();
        let prompt = self.data.settings.system_prompt.clone();
        let conv = Conversation::new(provider, model, prompt);
        self.data.current_id = Some(conv.id);
        self.data.conversations.insert(0, conv);
        self.input.clear();
        self.pending_attachments.clear();
        self.dirty = true;
        self.persist();
    }

    fn pick_attachments(&mut self) {
        let files = rfd::FileDialog::new()
            .set_title("选择图片或文件")
            .add_filter(
                "常用",
                &[
                    "png", "jpg", "jpeg", "gif", "webp", "bmp", "txt", "md", "json", "csv",
                    "rs", "py", "js", "ts", "toml", "yaml", "yml", "xml", "html", "css", "log",
                ],
            )
            .add_filter("图片", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
            .add_filter("所有文件", &["*"])
            .pick_files();

        if let Some(paths) = files {
            self.add_attachment_paths(paths);
        }
    }

    fn add_attachment_paths(&mut self, paths: Vec<PathBuf>) {
        const MAX_PENDING: usize = 8;
        for path in paths {
            if self.pending_attachments.len() >= MAX_PENDING {
                self.status = format!("最多附加 {MAX_PENDING} 个文件");
                break;
            }
            match attachments::load_attachment_from_path(&path) {
                Ok(att) => {
                    self.status = format!("已添加附件: {}", att.name);
                    self.pending_attachments.push(att);
                }
                Err(e) => {
                    self.status = e;
                }
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_attachment_paths(dropped);
        }
    }

    fn delete_conversation(&mut self, id: Uuid) {
        self.data.conversations.retain(|c| c.id != id);
        if self.data.current_id == Some(id) {
            self.data.current_id = self.data.conversations.first().map(|c| c.id);
        }
        if self.data.conversations.is_empty() {
            self.new_conversation();
        } else {
            self.dirty = true;
            self.persist();
        }
    }

    fn send_message(&mut self) {
        if self.streaming {
            return;
        }
        let text = self.input.trim().to_string();
        let has_attachments = !self.pending_attachments.is_empty();
        if text.is_empty() && !has_attachments {
            return;
        }

        // 先确保有会话，再按会话上的厂家/模型解析 API（避免只用全局 active 导致切换无效）
        if self.data.current_id.is_none() {
            self.new_conversation();
        }

        let Some(provider_cfg) = self.provider_config_for_send() else {
            self.status = "请先在设置中配置提供商".into();
            self.page = Page::Settings;
            return;
        };

        if !provider_cfg.kind.allows_empty_api_key() && provider_cfg.api_key.trim().is_empty() {
            self.status = format!("请先配置 {} 的 API Key", provider_cfg.display_name());
            self.page = Page::Settings;
            self.settings_provider = provider_cfg.kind;
            return;
        }

        // 同步全局 active，使设置页/新对话默认与当前会话一致
        self.data.settings.active_provider = provider_cfg.kind;
        self.settings_provider = provider_cfg.kind;
        if let Some(cfg) = self.data.settings.provider_mut(provider_cfg.kind) {
            cfg.model = provider_cfg.model.clone();
        }

        let conv_id = self.data.current_id.unwrap();
        let attachments = std::mem::take(&mut self.pending_attachments);
        let system_prompt = {
            self.current_conv()
                .map(|c| {
                    if c.system_prompt.trim().is_empty() {
                        self.data.settings.system_prompt.clone()
                    } else {
                        c.system_prompt.clone()
                    }
                })
                .unwrap_or_else(|| self.data.settings.system_prompt.clone())
        };

        {
            let Some(conv) = self.current_conv_mut() else {
                return;
            };
            // 保持会话路由字段与本次请求一致（不反写回旧的全局配置）
            conv.provider = provider_cfg.kind;
            conv.model = provider_cfg.model.clone();
            if conv.system_prompt.trim().is_empty() {
                conv.system_prompt = system_prompt.clone();
            }
            let user_text = if text.is_empty() && !attachments.is_empty() {
                String::new()
            } else {
                text
            };
            conv.messages
                .push(Message::new(Role::User, user_text).with_attachments(attachments));
            let assistant = Message::new(Role::Assistant, String::new());
            let msg_id = assistant.id;
            conv.messages.push(assistant);
            conv.auto_title_from_first_user();
            conv.touch();
            self.stream_msg_id = Some(msg_id);
        }

        self.input.clear();
        self.streaming = true;
        self.scroll_messages_to_bottom = true;
        self.follow_latest_message = true;
        self.status = format!(
            "生成中… {} · {} · {}",
            provider_cfg.display_name(),
            provider_cfg.model,
            provider_cfg.base_url
        );
        self.stream_conv_id = Some(conv_id);
        self.dirty = true;

        let messages = self
            .current_conv()
            .map(|c| {
                c.messages
                    .iter()
                    .filter(|m| m.id != self.stream_msg_id.unwrap_or_default())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let req = ChatRequest {
            provider: provider_cfg,
            messages,
            system_prompt,
            temperature: self.data.settings.temperature,
            max_tokens: self.data.settings.max_tokens,
        };

        let (tx, rx) = mpsc::channel();
        self.stream_rx = Some(rx);
        std::thread::spawn(move || {
            api::stream_chat(req, tx);
        });
    }

    fn poll_stream(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.stream_rx.as_ref() else {
            return;
        };

        let mut tokens = Vec::new();
        let mut done = false;
        let mut error: Option<String> = None;

        // 限制单帧处理量，避免通道积压时 UI 线程长时间空转
        const MAX_EVENTS_PER_FRAME: usize = 64;
        let mut hit_event_limit = false;
        for i in 0..MAX_EVENTS_PER_FRAME {
            match rx.try_recv() {
                Ok(StreamEvent::Token(t)) => tokens.push(t),
                Ok(StreamEvent::Done) => {
                    done = true;
                    break;
                }
                Ok(StreamEvent::Error(e)) => {
                    error = Some(e);
                    done = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if !done {
                        error = Some("生成连接已断开".into());
                        done = true;
                    }
                    break;
                }
            }
            if i + 1 == MAX_EVENTS_PER_FRAME {
                hit_event_limit = true;
            }
        }

        if !tokens.is_empty() {
            if let (Some(cid), Some(mid)) = (self.stream_conv_id, self.stream_msg_id) {
                if let Some(conv) = self.data.conversations.iter_mut().find(|c| c.id == cid) {
                    if let Some(msg) = conv.messages.iter_mut().find(|m| m.id == mid) {
                        for t in tokens {
                            msg.content.push_str(&t);
                        }
                        conv.touch();
                    }
                }
            }
            // 流式过程中不置 dirty / 不写盘；界面约 50ms 刷新一次即可
            if self.last_stream_ui.elapsed() >= std::time::Duration::from_millis(50) {
                self.last_stream_ui = Instant::now();
                ctx.request_repaint();
            }
        }

        if done {
            // 把通道里残余 token 收完再结束
            if let Some(rx) = self.stream_rx.as_ref() {
                while let Ok(ev) = rx.try_recv() {
                    match ev {
                        StreamEvent::Token(t) => {
                            if let (Some(cid), Some(mid)) =
                                (self.stream_conv_id, self.stream_msg_id)
                            {
                                if let Some(conv) =
                                    self.data.conversations.iter_mut().find(|c| c.id == cid)
                                {
                                    if let Some(msg) =
                                        conv.messages.iter_mut().find(|m| m.id == mid)
                                    {
                                        msg.content.push_str(&t);
                                    }
                                }
                            }
                        }
                        StreamEvent::Error(e) if error.is_none() => error = Some(e),
                        _ => {}
                    }
                }
            }

            if let Some(err) = error {
                if let (Some(cid), Some(mid)) = (self.stream_conv_id, self.stream_msg_id) {
                    if let Some(conv) = self.data.conversations.iter_mut().find(|c| c.id == cid) {
                        if let Some(msg) = conv.messages.iter_mut().find(|m| m.id == mid) {
                            if msg.content.is_empty() {
                                msg.content = format!("❌ {err}");
                            } else {
                                msg.content.push_str(&format!("\n\n❌ {err}"));
                            }
                        }
                    }
                }
                self.status = format!("出错: {err}");
            } else {
                self.status = "就绪".into();
            }
            self.streaming = false;
            self.stream_rx = None;
            self.stream_conv_id = None;
            self.stream_msg_id = None;
            self.dirty = true;
            self.persist();
            ctx.request_repaint();
        } else if self.streaming {
            // 有积压时尽快排空；否则 50ms 刷新一次，降低 Markdown/布局压力
            if hit_event_limit {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
    }

    fn ui_sidebar(&mut self, ui: &mut egui::Ui) {
        // 侧栏内容不得横向撑破，否则会溢出成黑条
        let sidebar_w = ui.available_width();
        ui.set_width(sidebar_w);
        ui.set_max_width(sidebar_w);
        ui.set_clip_rect(ui.max_rect());

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Askway").strong().size(18.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.sidebar_toggle_icon_button(ui);
            });
        });
        ui.add_space(2.0);
        ui.label(RichText::new("多提供商对话").weak().size(12.0));
        ui.add_space(10.0);

        if ui
            .add_sized(
                [ui.available_width(), 36.0],
                egui::Button::new(RichText::new("＋ 新对话").size(15.0)),
            )
            .clicked()
        {
            self.new_conversation();
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        ScrollArea::vertical()
            .id_salt("sidebar_convs")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(sidebar_w - 8.0);
                ui.set_max_width(sidebar_w - 8.0);

                let mut select_id = None;
                let mut select_and_jump = false;
                let mut delete_id = None;
                let current = self.data.current_id;
                let query = self.sidebar_search.clone();
                let searching = !query.trim().is_empty();

                let mut matched_any = false;
                for conv in &self.data.conversations {
                    let Some(hit) = Self::search_conversation(conv, &query) else {
                        continue;
                    };
                    matched_any = true;

                    let selected = current == Some(conv.id);
                    let title = if conv.title.is_empty() {
                        "新对话"
                    } else {
                        &conv.title
                    };
                    let meta = if searching && hit.hit_count > 0 {
                        format!(
                            "{} · {} · {} 处",
                            conv.provider.label(),
                            conv.model,
                            hit.hit_count
                        )
                    } else {
                        format!("{} · {}", conv.provider.label(), conv.model)
                    };

                    let row_id = ui.id().with(("conv_row", conv.id));
                    let hovered = ui
                        .ctx()
                        .read_response(row_id)
                        .is_some_and(|r| r.hovered());
                    let bg = if selected {
                        Color32::from_rgb(232, 240, 255)
                    } else if hovered {
                        Color32::from_rgb(240, 244, 248)
                    } else {
                        Color32::TRANSPARENT
                    };

                    let mut delete_clicked = false;
                    let frame_out = Frame::new()
                        .fill(bg)
                        .corner_radius(8.0)
                        .inner_margin(Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            // 整行占满侧栏宽度，空白区域也可点
                            let row_w = ui.available_width();
                            ui.set_width(row_w);
                            ui.set_max_width(row_w);
                            ui.set_min_width(row_w);

                            ui.horizontal(|ui| {
                                let del_w = 26.0;
                                let title_w = (ui.available_width() - del_w).max(40.0);

                                ui.allocate_ui_with_layout(
                                    Vec2::new(title_w, 22.0),
                                    Layout::left_to_right(Align::Center),
                                    |ui| {
                                        ui.set_max_width(title_w);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(title)
                                                    .size(14.0)
                                                    .color(Color32::from_rgb(28, 36, 44)),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui
                                        .small_button("🗑")
                                        .on_hover_cursor(CursorIcon::PointingHand)
                                        .on_hover_text("删除对话")
                                        .clicked()
                                    {
                                        delete_clicked = true;
                                    }
                                });
                            });

                            ui.add(
                                egui::Label::new(
                                    RichText::new(&meta)
                                        .size(11.0)
                                        .color(Color32::from_rgb(120, 130, 140)),
                                )
                                .truncate(),
                            );

                            if searching && !hit.snippet.is_empty() {
                                ui.add_space(2.0);
                                render_inline_search_hits(
                                    ui,
                                    &hit.snippet,
                                    &query,
                                    11.5,
                                    Color32::from_rgb(70, 100, 140),
                                    false,
                                    false,
                                );
                            }
                        });

                    let row_resp = ui
                        .interact(frame_out.response.rect, row_id, Sense::click())
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text(title);

                    if delete_clicked {
                        delete_id = Some(conv.id);
                    } else if row_resp.clicked() {
                        select_id = Some(conv.id);
                        select_and_jump = searching && hit.hit_count > 0;
                    }
                    ui.add_space(4.0);
                }

                if searching && !matched_any {
                    ui.add_space(16.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("未找到匹配的问答")
                                .size(13.0)
                                .color(Color32::from_rgb(120, 130, 140)),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("试试其他关键词")
                                .size(12.0)
                                .color(Color32::from_rgb(150, 158, 166)),
                        );
                    });
                }

                if let Some(id) = select_id {
                    self.data.current_id = Some(id);
                    self.page = Page::Chat;
                    self.sync_settings_from_current_conv();
                    if select_and_jump {
                        self.load_search_matches(id, true);
                    } else if searching {
                        self.load_search_matches(id, false);
                        self.scroll_messages_to_bottom = true;
                        self.follow_latest_message = true;
                    } else {
                        self.clear_search_navigation();
                        self.scroll_messages_to_bottom = true;
                        self.follow_latest_message = true;
                    }
                    self.dirty = true;
                    self.persist();
                }
                if let Some(id) = delete_id {
                    self.show_delete_confirm = Some(id);
                }
            });

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.set_max_width(sidebar_w);
            ui.add_space(8.0);
            if ui
                .add_sized(
                    [ui.available_width(), 34.0],
                    egui::Button::new("⚙ 模型设置"),
                )
                .clicked()
            {
                self.settings_provider = self.data.settings.active_provider;
                self.page = Page::Settings;
            }
            ui.add(
                egui::Label::new(
                    RichText::new(&self.status)
                        .size(12.0)
                        .color(Color32::from_rgb(90, 110, 120)),
                )
                .truncate(),
            );
        });
    }

    /// 主窗口顶部：全文搜索框与上一个/下一个
    fn ui_search_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);

            let searching = !self.sidebar_search.trim().is_empty();
            let nav_w = if searching { 220.0 } else { 56.0 };
            let search_w = (ui.available_width() - nav_w).max(120.0);

            let resp = ui.add_sized(
                [search_w, 28.0],
                egui::TextEdit::singleline(&mut self.sidebar_search)
                    .hint_text(
                        RichText::new("搜索已有问答…")
                            .color(Color32::from_rgb(140, 150, 160)),
                    )
                    .desired_width(search_w),
            );
            if resp.changed() {
                self.sync_search_query_if_changed();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(id) = self.data.current_id {
                    self.load_search_matches(id, true);
                }
            }

            if searching {
                if ui
                    .add(egui::Button::new(RichText::new("清除").size(12.5)))
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    self.sidebar_search.clear();
                    self.clear_search_navigation();
                }

                let total = self.search_matches.len();
                if total > 0 {
                    let cur = self.search_match_idx + 1;
                    ui.label(
                        RichText::new(format!("{cur}/{total}"))
                            .size(13.0)
                            .color(Color32::from_rgb(70, 90, 110)),
                    );

                    let prev = ui
                        .add_enabled(
                            total > 1,
                            egui::Button::new(RichText::new("上一个").size(12.5)),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("跳到上一处关键词（Shift+F3）");
                    if prev.clicked() {
                        self.goto_search_delta(-1);
                    }

                    let next = ui
                        .add(egui::Button::new(RichText::new("下一个").size(12.5)))
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("跳到下一处关键词（F3）");
                    if next.clicked() {
                        self.goto_search_delta(1);
                    }
                } else {
                    ui.label(
                        RichText::new("无匹配")
                            .size(12.5)
                            .color(Color32::from_rgb(150, 110, 90)),
                    );
                }
            }

            if ui.input(|i| i.key_pressed(egui::Key::F3)) {
                if !self.search_matches.is_empty() {
                    if ui.input(|i| i.modifiers.shift) {
                        self.goto_search_delta(-1);
                    } else {
                        self.goto_search_delta(1);
                    }
                } else if let Some(id) = self.data.current_id {
                    self.load_search_matches(id, true);
                }
            }
        });
    }

    fn ui_chat(&mut self, ui: &mut egui::Ui) {
        // 顶栏：标题
        ui.horizontal(|ui| {
            let title = self
                .current_conv()
                .map(|c| {
                    if c.title.is_empty() {
                        "新对话".to_string()
                    } else {
                        c.title.clone()
                    }
                })
                .unwrap_or_else(|| "新对话".into());
            let title_w = (ui.available_width() - 80.0).max(80.0);
            ui.allocate_ui_with_layout(
                Vec2::new(title_w, 28.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_max_width(title_w);
                    ui.add(
                        egui::Label::new(RichText::new(&title).strong().size(17.0)).truncate(),
                    )
                    .on_hover_text(&title);
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.settings_icon_button(ui) {
                    self.settings_provider = self.data.settings.active_provider;
                    self.page = Page::Settings;
                }
                self.sidebar_toggle_icon_button(ui);
            });
        });
        ui.add_space(6.0);
        self.ui_search_bar(ui);
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        let col_w = ui.available_width();

        // 用 Bottom 面板固定输入区，避免 bottom_up 把控件排到视口外
        egui::TopBottomPanel::bottom("chat_composer_panel")
            .resizable(false)
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.set_width(col_w);
                ui.add_space(6.0);
                self.ui_chat_composer(ui);
            });

        // 剩余区域给消息列表
        let force_scroll = self.scroll_messages_to_bottom;
        let output = ScrollArea::vertical()
            .id_salt("messages")
            .auto_shrink([false, false])
            .stick_to_bottom(self.follow_latest_message)
            .animated(!force_scroll)
            .show(ui, |ui| {
                ui.set_width(col_w);
                let messages = self
                    .current_conv()
                    .map(|c| c.messages.clone())
                    .unwrap_or_default();

                if messages.is_empty() {
                    ui.add_space(48.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("有什么我能帮你的吗？")
                                .size(24.0)
                                .color(Color32::from_rgb(36, 48, 58)),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("在下方输入框提问，或拖拽文件到窗口")
                                .size(14.0)
                                .color(Color32::from_rgb(120, 132, 142)),
                        );
                    });
                }

                for msg in &messages {
                    self.render_message(ui, msg);
                }
                ui.add_space(12.0);

                // 仅在新消息块出现时强制滚到底；流式增长交给 stick_to_bottom
                if force_scroll {
                    ui.scroll_to_cursor(Some(Align::BOTTOM));
                }
            });

        self.scroll_messages_to_bottom = false;

        let max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
        self.follow_latest_message =
            max_offset <= 1.0 || output.state.offset.y >= max_offset - 48.0;
    }

    /// 底部输入区：输入框限高，发送按钮始终留在面板内
    fn ui_chat_composer(&mut self, ui: &mut egui::Ui) {
        let send_enabled = !self.streaming
            && (!self.input.trim().is_empty() || !self.pending_attachments.is_empty());

        let col_w = ui.available_width();
        // 固定输入可视高度，长文本在框内滚动，不再撑破布局
        const INPUT_H: f32 = 96.0;

        ui.set_width(col_w);
        ui.set_max_width(col_w);

        if !self.pending_attachments.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                let mut remove_idx = None;
                for (i, att) in self.pending_attachments.iter().enumerate() {
                    Frame::new()
                        .fill(Color32::from_rgb(245, 247, 250))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(220, 226, 232)))
                        .corner_radius(10.0)
                        .inner_margin(Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(att.summary())
                                        .size(12.0)
                                        .color(Color32::from_rgb(60, 75, 90)),
                                );
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("×").size(14.0))
                                            .frame(false),
                                    )
                                    .clicked()
                                {
                                    remove_idx = Some(i);
                                }
                            });
                        });
                }
                if let Some(i) = remove_idx {
                    self.pending_attachments.remove(i);
                }
            });
            ui.add_space(6.0);
        }

        Frame::new()
            .fill(Color32::WHITE)
            .stroke(Stroke::new(1.0, Color32::from_rgb(210, 218, 226)))
            .corner_radius(16.0)
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                let edit = egui::TextEdit::multiline(&mut self.input)
                    .desired_rows(3)
                    .desired_width(ui.available_width())
                    .text_color(Color32::from_rgb(28, 36, 44))
                    .frame(true)
                    .margin(Margin::symmetric(8, 8))
                    .hint_text(
                        RichText::new("输入消息，Enter 发送，Shift+Enter 换行")
                            .color(Color32::from_rgb(140, 150, 160)),
                    );
                // 固定高度：内容超出时 TextEdit 内部滚动
                ui.add_sized([ui.available_width(), INPUT_H], edit);

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("📎").size(15.0))
                                .min_size(Vec2::new(32.0, 32.0)),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("添加图片或文件")
                        .clicked()
                    {
                        self.pick_attachments();
                    }

                    self.ui_composer_model_pills(ui);

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = if self.streaming { "…" } else { "发送" };
                        let send = ui.add_enabled(
                            send_enabled,
                            egui::Button::new(
                                RichText::new(label).color(Color32::WHITE).size(14.0),
                            )
                            .fill(if send_enabled {
                                Color32::from_rgb(50, 120, 255)
                            } else {
                                Color32::from_rgb(180, 190, 200)
                            })
                            .min_size(Vec2::new(64.0, 32.0))
                            .corner_radius(8.0),
                        );
                        let send = send.on_hover_cursor(CursorIcon::PointingHand);
                        if send.clicked() {
                            self.send_message();
                        }
                        if self.streaming {
                            ui.label(
                                RichText::new("生成中")
                                    .size(12.0)
                                    .color(Color32::from_rgb(120, 130, 140)),
                            );
                        }
                    });
                });
            });

        ui.add_space(2.0);
        ui.label(
            RichText::new("内容由 AI 生成，请注意甄别")
                .size(11.0)
                .color(Color32::from_rgb(160, 168, 176)),
        );

        let enter_to_send = ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
        if enter_to_send {
            while self.input.ends_with('\n') || self.input.ends_with('\r') {
                self.input.pop();
            }
            self.send_message();
        }
    }

    /// 作曲家卡片内的厂商/模型胶囊选择（以当前会话为准，并同步到全局设置）
    fn ui_composer_model_pills(&mut self, ui: &mut egui::Ui) {
        // 展示与写入都以当前会话路由为准，避免和全局 active 脱节
        let (active_kind, current_model) = if let Some(conv) = self.current_conv() {
            (conv.provider, conv.model.clone())
        } else {
            (
                self.data.settings.active_provider,
                self.data
                    .settings
                    .active()
                    .map(|p| p.model.clone())
                    .unwrap_or_default(),
            )
        };

        let active_ready = self
            .data
            .settings
            .provider(active_kind)
            .map(|p| p.is_ready())
            .unwrap_or(false);

        let ready_kinds: Vec<ProviderKind> = self
            .data
            .settings
            .ready_providers()
            .iter()
            .map(|p| p.kind)
            .collect();

        let provider_name = if active_ready {
            self.data
                .settings
                .provider(active_kind)
                .map(|p| p.display_name())
                .unwrap_or_else(|| active_kind.label().to_string())
        } else if ready_kinds.is_empty() {
            "配置厂家".into()
        } else {
            format!("{}·未启用", active_kind.label())
        };

        let mut picked_kind = active_kind;
        egui::ComboBox::from_id_salt("chat_provider_combo")
            .width(100.0)
            .selected_text(RichText::new(truncate_chars(&provider_name, 8)).size(12.0))
            .show_ui(ui, |ui| {
                ui.set_min_width(180.0);
                if ready_kinds.is_empty() {
                    ui.label(RichText::new("暂无可用厂家，请先到设置配置").weak().small());
                } else {
                    ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        let mut last_cat = "";
                        for kind in &ready_kinds {
                            if kind.category() != last_cat {
                                ui.label(
                                    RichText::new(kind.category())
                                        .small()
                                        .strong()
                                        .color(Color32::from_rgb(100, 120, 130)),
                                );
                                last_cat = kind.category();
                            }
                            ui.selectable_value(&mut picked_kind, *kind, kind.label());
                        }
                    });
                }
            });

        if picked_kind != active_kind {
            self.switch_to_provider(picked_kind);
        }

        // 厂家可能刚被切换，重新读取会话上的当前厂家
        let kind_now = self
            .current_conv()
            .map(|c| c.provider)
            .unwrap_or(self.data.settings.active_provider);
        let model_now = self
            .current_conv()
            .map(|c| c.model.clone())
            .unwrap_or(current_model);
        let ready_now = self
            .data
            .settings
            .provider(kind_now)
            .map(|p| p.is_ready())
            .unwrap_or(false);

        let model_label = if !ready_now {
            "选择模型".to_string()
        } else if model_now.is_empty() {
            "选择模型".to_string()
        } else {
            model_now.clone()
        };

        let mut picked_model: Option<String> = None;
        ui.add_enabled_ui(ready_now, |ui| {
            let models = kind_now.default_models();
            egui::ComboBox::from_id_salt("chat_model_combo")
                .width(128.0)
                .selected_text(RichText::new(truncate_chars(&model_label, 12)).size(12.0))
                .show_ui(ui, |ui| {
                    ui.set_min_width(240.0);
                    ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for m in models {
                            if ui.selectable_label(model_now == *m, *m).clicked() {
                                picked_model = Some((*m).to_string());
                            }
                        }
                        if !model_now.is_empty() && !models.iter().any(|m| *m == model_now) {
                            ui.separator();
                            ui.label(
                                RichText::new(format!("当前自定义: {model_now}"))
                                    .weak()
                                    .small(),
                            );
                        }
                    });
                });
        });

        if let Some(model) = picked_model {
            if let Some(cfg) = self.data.settings.provider_mut(kind_now) {
                cfg.model = model.clone();
            }
            self.data.settings.active_provider = kind_now;
            if let Some(conv) = self.current_conv_mut() {
                conv.model = model.clone();
                conv.provider = kind_now;
                conv.touch();
            }
            self.dirty = true;
            self.persist();
            let base = self
                .data
                .settings
                .provider(kind_now)
                .map(|p| p.base_url.clone())
                .unwrap_or_default();
            self.status = format!("模型: {model} @ {base}");
        }
    }

    fn render_message(&mut self, ui: &mut egui::Ui, msg: &Message) {
        let is_user = msg.role == Role::User;
        let is_assistant = msg.role == Role::Assistant;
        let active_match = self
            .search_matches
            .get(self.search_match_idx)
            .filter(|m| m.msg_id == msg.id)
            .cloned();
        let (bg, stroke) = if is_user {
            (
                Color32::from_rgb(232, 240, 255),
                Color32::from_rgb(210, 224, 255),
            )
        } else {
            (
                Color32::from_rgb(255, 255, 255),
                Color32::from_rgb(230, 234, 238),
            )
        };

        ui.add_space(10.0);

        let content = if msg.content.is_empty() && self.streaming && is_assistant {
            "…"
        } else if msg.content.is_empty() && !msg.attachments.is_empty() {
            "（仅附件）"
        } else {
            msg.content.as_str()
        };

        let can_copy = is_assistant && !msg.content.trim().is_empty();
        let copied_recently = self
            .copied_msg
            .as_ref()
            .is_some_and(|(id, at)| *id == msg.id && at.elapsed().as_secs() < 2);

        let search_q = self.sidebar_search.trim().to_string();
        let show_search_highlight = !search_q.is_empty()
            && !content.is_empty()
            && content != "…"
            && content.to_lowercase().contains(&search_q.to_lowercase());
        let active_byte_start = active_match.as_ref().and_then(|m| m.byte_start);
        let mut did_scroll_keyword = false;

        let col_w = ui.available_width();
        let frame_resp = ui
            .allocate_ui_with_layout(
                Vec2::new(col_w, 0.0),
                Layout::top_down(Align::Min).with_cross_justify(true),
                |ui| {
                    ui.set_max_width(col_w);
                    Frame::new()
                        .fill(bg)
                        .corner_radius(16.0)
                        .inner_margin(Margin::symmetric(14, 12))
                        .stroke(Stroke::new(1.0, stroke))
                        .show(ui, |ui| {
                            let inner_w = ui.available_width();
                            ui.set_min_width(inner_w);
                            ui.allocate_space(Vec2::new(inner_w, 0.0));

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(msg.role.label())
                                        .strong()
                                        .size(12.0)
                                        .color(Color32::from_rgb(90, 110, 130)),
                                );
                                ui.label(
                                    RichText::new(msg.created_at.format("%H:%M").to_string())
                                        .size(11.0)
                                        .color(Color32::from_rgb(150, 160, 170)),
                                );
                                if can_copy {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let label = if copied_recently { "已复制" } else { "复制" };
                                        let btn = ui.add(
                                            egui::Button::new(
                                                RichText::new(label)
                                                    .size(11.5)
                                                    .color(if copied_recently {
                                                        Color32::from_rgb(46, 125, 50)
                                                    } else {
                                                        Color32::from_rgb(90, 110, 130)
                                                    }),
                                            )
                                            .frame(false)
                                            .min_size(Vec2::new(36.0, 18.0)),
                                        );
                                        if btn
                                            .on_hover_cursor(CursorIcon::PointingHand)
                                            .on_hover_text("复制到剪贴板")
                                            .clicked()
                                        {
                                            ui.ctx().copy_text(msg.content.clone());
                                            self.copied_msg = Some((msg.id, Instant::now()));
                                            self.status = "已复制到剪贴板".into();
                                        }
                                    });
                                }
                            });
                            ui.add_space(4.0);

                            if !msg.attachments.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    for att in &msg.attachments {
                                        let summary = att.summary();
                                        let att_q_hit = !search_q.is_empty()
                                            && att
                                                .name
                                                .to_lowercase()
                                                .contains(&search_q.to_lowercase());
                                        let att_active = att_q_hit
                                            && active_match
                                                .as_ref()
                                                .is_some_and(|m| m.byte_start.is_none());
                                        let att_resp = Frame::new()
                                            .fill(Color32::from_rgb(248, 250, 252))
                                            .stroke(Stroke::new(
                                                1.0,
                                                Color32::from_rgb(220, 226, 232),
                                            ))
                                            .corner_radius(8.0)
                                            .inner_margin(Margin::symmetric(8, 3))
                                            .show(ui, |ui| {
                                                if att_q_hit {
                                                    if render_inline_search_hits(
                                                        ui,
                                                        &summary,
                                                        &search_q,
                                                        12.0,
                                                        Color32::from_rgb(50, 80, 95),
                                                        att_active,
                                                        self.scroll_to_match,
                                                    ) {
                                                        did_scroll_keyword = true;
                                                    }
                                                } else {
                                                    ui.label(
                                                        RichText::new(&summary)
                                                            .size(12.0)
                                                            .color(Color32::from_rgb(50, 80, 95)),
                                                    );
                                                }
                                            })
                                            .response;
                                        if att_active && self.scroll_to_match && !did_scroll_keyword {
                                            att_resp.scroll_to_me(Some(Align::Center));
                                            did_scroll_keyword = true;
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                            }

                            if !content.is_empty() {
                                let streaming_this =
                                    self.streaming && self.stream_msg_id == Some(msg.id);
                                if is_assistant {
                                    // 流式输出中用纯文本，避免每帧重跑 Markdown 卡死 UI；结束后再渲染 MD
                                    if streaming_this {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(content)
                                                    .size(14.5)
                                                    .color(Color32::from_rgb(28, 36, 44)),
                                            )
                                            .wrap(),
                                        );
                                    } else {
                                        let md_w = ui.available_width();
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(md_w, 0.0),
                                            Layout::top_down(Align::Min),
                                            |ui| {
                                                ui.set_max_width(md_w);
                                                if show_search_highlight {
                                                    if render_markdown_with_search_hits(
                                                        ui,
                                                        &mut self.md_cache,
                                                        content,
                                                        md_w,
                                                        &search_q,
                                                        active_byte_start,
                                                        self.scroll_to_match,
                                                    ) {
                                                        did_scroll_keyword = true;
                                                    }
                                                } else {
                                                    for seg in crate::md_table::split_markdown_tables(
                                                        content,
                                                    ) {
                                                        match seg {
                                                            crate::md_table::MdSegment::Markdown(
                                                                md,
                                                            ) => {
                                                                CommonMarkViewer::new()
                                                                    .default_width(Some(
                                                                        md_w as usize,
                                                                    ))
                                                                    .show(
                                                                        ui,
                                                                        &mut self.md_cache,
                                                                        &md,
                                                                    );
                                                            }
                                                            crate::md_table::MdSegment::Table {
                                                                headers,
                                                                rows,
                                                            } => {
                                                                crate::md_table::render_markdown_table(
                                                                    ui, &headers, &rows,
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            },
                                        );
                                    }
                                } else if show_search_highlight && !streaming_this {
                                    // 用户消息为纯文本，可高亮关键词并滚到精确位置
                                    if render_content_with_search_hits(
                                        ui,
                                        content,
                                        &search_q,
                                        active_byte_start,
                                        self.scroll_to_match,
                                    ) {
                                        did_scroll_keyword = true;
                                    }
                                } else {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(content)
                                                .size(14.5)
                                                .color(Color32::from_rgb(28, 36, 44)),
                                        )
                                        .wrap(),
                                    );
                                }
                            }
                        });
                },
            )
            .response;

        if did_scroll_keyword {
            self.scroll_to_match = false;
        } else if self.scroll_to_match && active_match.is_some() {
            // 附件/无正文命中：滚到整条消息
            frame_resp.scroll_to_me(Some(Align::Center));
            self.scroll_to_match = false;
        }
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        const LABEL_W: f32 = 72.0;
        const DETAIL_MAX_W: f32 = 640.0;

        ui.horizontal(|ui| {
            ui.heading(RichText::new("模型设置").size(18.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_sized([100.0, 28.0], egui::Button::new("保存并返回"))
                    .clicked()
                {
                    self.apply_settings_and_back();
                }
                if ui.small_button("← 返回").clicked() {
                    self.page = Page::Chat;
                    self.persist();
                }
                self.sidebar_toggle_icon_button(ui);
            });
        });
        ui.label(
            RichText::new("先点选厂家，再在下方填写 Key 与模型。API Key 仅保存在本机。")
                .weak()
                .size(12.0),
        );
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // —— 厂商：多行网格列表（按分类换行）——
        Frame::new()
            .fill(Color32::from_rgb(246, 248, 250))
            .stroke(Stroke::new(1.0, Color32::from_rgb(220, 226, 230)))
            .inner_margin(Margin::symmetric(10, 8))
            .corner_radius(8.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new("模型厂商").strong().size(13.5));
                ui.add_space(6.0);

                ScrollArea::vertical()
                    .id_salt("settings_providers_grid")
                    .max_height(200.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());

                        // 按分类分组，每类内自动换行成多行
                        let categories = ["国际", "国内", "本地 / 自定义"];
                        for category in categories {
                            let kinds: Vec<ProviderKind> = ProviderKind::all()
                                .iter()
                                .copied()
                                .filter(|k| k.category() == category)
                                .collect();
                            if kinds.is_empty() {
                                continue;
                            }
                            ui.label(
                                RichText::new(category)
                                    .size(11.5)
                                    .strong()
                                    .color(Color32::from_rgb(90, 110, 125)),
                            );
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                                for kind in kinds {
                                    let selected = self.settings_provider == kind;
                                    let is_active = self.data.settings.active_provider == kind;
                                    let configured = self
                                        .data
                                        .settings
                                        .provider(kind)
                                        .map(|p| {
                                            p.enabled
                                                || !p.api_key.is_empty()
                                                || kind.allows_empty_api_key()
                                        })
                                        .unwrap_or(false);

                                    let mark = if is_active {
                                        "★ "
                                    } else if configured {
                                        "● "
                                    } else {
                                        "○ "
                                    };
                                    let label = format!("{mark}{}", kind.label());
                                    let text = if selected {
                                        RichText::new(label).strong().size(12.5)
                                    } else {
                                        RichText::new(label).size(12.5)
                                    };
                                    let btn = if selected {
                                        egui::Button::new(text)
                                            .fill(Color32::from_rgb(200, 228, 234))
                                            .stroke(Stroke::new(
                                                1.0,
                                                Color32::from_rgb(40, 130, 145),
                                            ))
                                    } else {
                                        egui::Button::new(text)
                                            .fill(Color32::from_rgb(255, 255, 255))
                                            .stroke(Stroke::new(
                                                1.0,
                                                Color32::from_rgb(210, 218, 224),
                                            ))
                                    };
                                    let resp = ui
                                        .add(btn)
                                        .on_hover_cursor(CursorIcon::PointingHand);
                                    if resp.clicked() {
                                        // 点选厂家即切换当前对话路由（模型 + API 地址一并切换）
                                        self.switch_to_provider(kind);
                                        self.settings_model_filter.clear();
                                    }
                                    if self.data.settings.active_provider == kind {
                                        resp.on_hover_text("当前对话使用的提供商");
                                    }
                                }
                            });
                            ui.add_space(8.0);
                        }
                    });
            });

        ui.add_space(10.0);

        // —— 下方：选中厂家的配置 ——
        let kind = self.settings_provider;
        let is_active = self.data.settings.active_provider == kind;
        let detail_w = ui.available_width().min(DETAIL_MAX_W);
        let remain_h = (ui.available_height() - 4.0).max(160.0);

        Frame::new()
            .fill(Color32::from_rgb(252, 253, 254))
            .stroke(Stroke::new(1.0, Color32::from_rgb(220, 226, 230)))
            .inner_margin(Margin::symmetric(12, 10))
            .corner_radius(8.0)
            .show(ui, |ui| {
                ui.set_width(detail_w);
                ui.set_min_height(remain_h.min(420.0));

                ScrollArea::vertical()
                    .id_salt("settings_detail")
                    .max_height(remain_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width((detail_w - 24.0).max(240.0));

                        ui.horizontal(|ui| {
                            ui.label(RichText::new(kind.label()).strong().size(16.0));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if is_active {
                                    ui.label(
                                        RichText::new("当前")
                                            .color(Color32::from_rgb(20, 120, 80))
                                            .size(12.0),
                                    );
                                } else if ui.small_button("设为当前").clicked() {
                                    self.data.settings.active_provider = kind;
                                    self.dirty = true;
                                }
                            });
                        });

                        ui.add_space(8.0);
                        section_title(ui, "连接");
                        ui.add_space(4.0);

                        if let Some(cfg) = self.data.settings.provider_mut(kind) {
                            if kind == ProviderKind::Custom {
                                labeled_row(ui, LABEL_W, "名称", |ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut cfg.custom_name)
                                            .desired_width(ui.available_width())
                                            .hint_text("显示名称"),
                                    );
                                });
                                ui.add_space(4.0);
                            }

                            labeled_row(ui, LABEL_W, "API Key", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut cfg.api_key)
                                        .password(true)
                                        .desired_width(ui.available_width())
                                        .hint_text(if kind.allows_empty_api_key() {
                                            "本地通常不需要"
                                        } else {
                                            "粘贴密钥"
                                        }),
                                );
                            });
                            ui.add_space(4.0);

                            labeled_row(
                                ui,
                                LABEL_W,
                                if kind == ProviderKind::LocalClaude {
                                    "CLI 路径"
                                } else {
                                    "Base URL"
                                },
                                |ui| {
                                let w = (ui.available_width() - 44.0).max(100.0);
                                ui.add(
                                    egui::TextEdit::singleline(&mut cfg.base_url).desired_width(w),
                                );
                                if ui.small_button("重置").clicked() {
                                    cfg.base_url = kind.default_base_url().to_string();
                                }
                            });

                            let tip = match kind {
                                ProviderKind::AzureOpenAI => {
                                    Some("Azure：URL 填到 deployments/<部署名>")
                                }
                                ProviderKind::Doubao => Some("豆包：模型填方舟接入点 ID（ep-…）"),
                                ProviderKind::OpenRouter => {
                                    Some("OpenRouter：可用 provider/model 格式")
                                }
                                ProviderKind::LocalClaude => Some(
                                    "Claude Code CLI 常驻：同模型/同系统提示下续聊复用进程；空闲 15 分钟回收。切换模型或新开上下文会重启",
                                ),
                                _ => None,
                            };
                            if let Some(t) = tip {
                                ui.label(RichText::new(t).weak().size(11.0));
                            }

                            ui.add_space(8.0);
                            section_title(ui, "模型");
                            ui.add_space(4.0);

                            labeled_row(ui, LABEL_W, "当前", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut cfg.model)
                                        .desired_width(ui.available_width())
                                        .hint_text("可手动输入或从下方选择"),
                                );
                            });
                            ui.add_space(4.0);

                            labeled_row(ui, LABEL_W, "快捷选", |ui| {
                                let mut picked: Option<String> = None;
                                let current = cfg.model.clone();
                                egui::ComboBox::from_id_salt(("model_combo", kind.label()))
                                    .width((ui.available_width() - 8.0).max(160.0))
                                    .selected_text(RichText::new(&current).size(13.0))
                                    .show_ui(ui, |ui| {
                                        ui.set_min_width(240.0);
                                        ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                                            for m in kind.default_models() {
                                                if ui.selectable_label(current == *m, *m).clicked()
                                                {
                                                    picked = Some((*m).to_string());
                                                }
                                            }
                                        });
                                    });
                                if let Some(m) = picked {
                                    cfg.model = m;
                                    self.dirty = true;
                                }
                            });

                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("筛选").weak().size(12.0));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings_model_filter)
                                        .desired_width(ui.available_width())
                                        .hint_text("输入关键字过滤列表"),
                                );
                            });

                            let filter = self.settings_model_filter.trim().to_ascii_lowercase();
                            let models: Vec<&str> = kind
                                .default_models()
                                .iter()
                                .copied()
                                .filter(|m| {
                                    filter.is_empty() || m.to_ascii_lowercase().contains(&filter)
                                })
                                .collect();

                            ui.add_space(4.0);
                            Frame::new()
                                .fill(Color32::from_rgb(245, 248, 250))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(225, 230, 234)))
                                .corner_radius(6.0)
                                .inner_margin(Margin::symmetric(6, 4))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.label(
                                        RichText::new(format!(
                                            "可选模型 {} 个（点击选用）",
                                            models.len()
                                        ))
                                        .weak()
                                        .size(11.0),
                                    );
                                    ScrollArea::vertical()
                                        .id_salt(("model_list", kind.label()))
                                        .max_height(140.0)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            let mut picked: Option<String> = None;
                                            for m in &models {
                                                let selected = cfg.model == *m;
                                                let text = if selected {
                                                    RichText::new(*m)
                                                        .strong()
                                                        .color(Color32::from_rgb(20, 110, 130))
                                                } else {
                                                    RichText::new(*m).size(12.5)
                                                };
                                                if ui.selectable_label(selected, text).clicked() {
                                                    picked = Some((*m).to_string());
                                                }
                                            }
                                            if models.is_empty() {
                                                ui.label(
                                                    RichText::new("无匹配模型，可直接在上方输入")
                                                        .weak()
                                                        .italics(),
                                                );
                                            }
                                            if let Some(m) = picked {
                                                cfg.model = m;
                                                self.dirty = true;
                                            }
                                        });
                                });

                            ui.add_space(6.0);
                            ui.checkbox(&mut cfg.enabled, "标记为已启用");
                        }

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(6.0);
                        section_title(ui, "系统提示");
                        ui.add_space(4.0);
                        ui.add(
                            egui::TextEdit::multiline(&mut self.data.settings.system_prompt)
                                .desired_rows(3)
                                .desired_width(ui.available_width())
                                .hint_text("设定 AI 角色与风格"),
                        );

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(6.0);
                        section_title(ui, "生成参数");
                        ui.add_space(4.0);
                        labeled_row(ui, LABEL_W, "温度", |ui| {
                            ui.add(
                                egui::Slider::new(&mut self.data.settings.temperature, 0.0..=2.0)
                                    .fixed_decimals(2),
                            );
                        });
                        ui.add_space(4.0);
                        labeled_row(ui, LABEL_W, "Tokens", |ui| {
                            ui.add(egui::Slider::new(
                                &mut self.data.settings.max_tokens,
                                256..=16384,
                            ));
                        });

                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized([120.0, 32.0], egui::Button::new("保存设置"))
                                .clicked()
                            {
                                self.apply_settings_and_back();
                            }
                            if !is_active && ui.button("设为当前并保存").clicked() {
                                self.data.settings.active_provider = kind;
                                self.apply_settings_and_back();
                            }
                        });

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!("数据目录：{}", storage::data_dir().display()))
                                .small()
                                .weak(),
                        );
                    });
            });
    }

    fn apply_settings_and_back(&mut self) {
        // 填写了 API Key 的厂家保存时自动启用，方便出现在聊天快捷切换里
        for p in &mut self.data.settings.providers {
            if !p.api_key.trim().is_empty() {
                p.enabled = true;
            }
        }

        // 正在编辑的厂家设为当前，避免改了配置却仍打旧 API
        self.data.settings.active_provider = self.settings_provider;

        let prompt = self.data.settings.system_prompt.clone();
        let provider = self.data.settings.active_provider;
        let model = self
            .data
            .settings
            .provider(provider)
            .map(|p| p.model.clone())
            .unwrap_or_default();

        if let Some(conv) = self.current_conv_mut() {
            conv.system_prompt = prompt;
            conv.provider = provider;
            if !model.is_empty() {
                conv.model = model;
            }
            conv.touch();
        }

        self.persist();
        self.status = "设置已保存".into();
        self.page = Page::Chat;
    }

    fn ui_delete_modal(&mut self, ctx: &egui::Context) {
        let Some(id) = self.show_delete_confirm else {
            return;
        };
        egui::Modal::new(egui::Id::new("delete_confirm")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("删除对话？");
            ui.label("此操作不可恢复。");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("取消").clicked() {
                    self.show_delete_confirm = None;
                }
                if ui
                    .add(egui::Button::new(RichText::new("删除").color(Color32::WHITE)).fill(Color32::from_rgb(180, 60, 60)))
                    .clicked()
                {
                    self.delete_conversation(id);
                    self.show_delete_confirm = None;
                }
            });
        });
    }
}

impl eframe::App for AskwayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_stream(ctx);
        self.handle_dropped_files(ctx);
        self.ensure_window_fits_screen(ctx);

        if let Some((_, at)) = self.copied_msg {
            if at.elapsed().as_secs() < 2 {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            } else {
                self.copied_msg = None;
            }
        }

        if let Some((_, at)) = self.highlight_msg {
            if at.elapsed().as_secs() < 4 {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            } else {
                self.highlight_msg = None;
            }
        }

        if self.data.sidebar_open {
            egui::SidePanel::left("sidebar")
                .exact_width(240.0)
                .resizable(false)
                .show_separator_line(true)
                .show(ctx, |ui| {
                    Frame::new()
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            self.ui_sidebar(ui);
                        });
                });
        }
        // 收起后完全隐藏侧栏；通过右上角图标按钮展开

        // 聊天页：面板左右各留 20px 外边距，框体不再二次水平加宽
        egui::CentralPanel::default().show(ctx, |ui| {
            Frame::new()
                .inner_margin(Margin::symmetric(20, 10))
                .show(ui, |ui| match self.page {
                    Page::Chat => self.ui_chat(ui),
                    Page::Settings => self.ui_settings(ui),
                });
        });

        self.ui_delete_modal(ctx);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        // 流式生成时跳过写盘，避免大 JSON 序列化卡死 UI；结束时 persist() 会落盘
        if self.streaming {
            return;
        }
        let _ = storage::save(&self.data);
    }
}

fn configure_style(ctx: &egui::Context) {
    setup_fonts(ctx);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.scroll.floating = true;
    style.visuals.window_fill = Color32::from_rgb(252, 253, 254);
    style.visuals.panel_fill = Color32::from_rgb(252, 253, 254);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(236, 242, 244);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(210, 230, 235);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(40, 130, 145);
    style.visuals.selection.bg_fill = Color32::from_rgb(40, 130, 145);
    ctx.set_style(style);

    ctx.style_mut(|s| {
        s.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(14.5, egui::FontFamily::Proportional),
        );
        s.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        );
        s.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(20.0, egui::FontFamily::Proportional),
        );
    });
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .strong()
            .size(14.5)
            .color(Color32::from_rgb(40, 70, 85)),
    );
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

/// 在原文中按小写查询找出全部命中字节区间（保证落在 char boundary）
fn find_all_match_ranges(content: &str, query_lower: &str) -> Vec<(usize, usize)> {
    if query_lower.is_empty() || content.is_empty() {
        return Vec::new();
    }
    let lower = content.to_lowercase();
    let q_len = query_lower.len();
    let mut out = Vec::new();
    let mut start = 0;
    while start <= lower.len() {
        let Some(pos) = lower[start..].find(query_lower) else {
            break;
        };
        let byte_start = start + pos;
        let byte_end = byte_start + q_len;
        if content.is_char_boundary(byte_start) && content.is_char_boundary(byte_end) {
            out.push((byte_start, byte_end));
        }
        start = byte_end.max(byte_start + 1);
        if start > lower.len() {
            break;
        }
    }
    out
}

/// 按命中字节位置截取上下文摘要
fn snippet_around_byte(content: &str, byte_start: usize, byte_len: usize, max_chars: usize) -> String {
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    if chars.is_empty() {
        return String::new();
    }
    let match_char_idx = chars
        .iter()
        .position(|(i, _)| *i >= byte_start)
        .unwrap_or(0);
    let query_chars = content
        .get(byte_start..byte_start + byte_len)
        .map(|s| s.chars().count())
        .unwrap_or(1)
        .max(1);
    let half = max_chars.saturating_sub(query_chars) / 2;
    let start = match_char_idx.saturating_sub(half);
    let end = (start + max_chars).min(chars.len());
    let start = end.saturating_sub(max_chars).min(start);

    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    for (_, ch) in &chars[start..end] {
        if *ch == '\n' || *ch == '\r' {
            out.push(' ');
        } else {
            out.push(*ch);
        }
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// 按段落渲染 Markdown；含关键词的块改为纯文本高亮关键词（不整块涂色）
fn render_markdown_with_search_hits(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    content: &str,
    md_w: f32,
    query: &str,
    active_byte_start: Option<usize>,
    do_scroll: bool,
) -> bool {
    let q_lower = query.trim().to_lowercase();
    let blocks = split_md_scroll_blocks(content);
    let mut scrolled = false;

    for (start, end, block) in blocks {
        let has_hit = !find_all_match_ranges(block, &q_lower).is_empty();
        if has_hit {
            let active = active_byte_start.filter(|abs| {
                *abs >= start && *abs < end.max(start.saturating_add(1))
            }).map(|abs| abs - start);
            if render_content_with_search_hits(
                ui,
                block,
                query,
                active,
                do_scroll && !scrolled,
            ) {
                scrolled = true;
            }
        } else {
            ui.allocate_ui_with_layout(
                Vec2::new(md_w, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_max_width(md_w);
                    for seg in crate::md_table::split_markdown_tables(block) {
                        match seg {
                            crate::md_table::MdSegment::Markdown(md) => {
                                CommonMarkViewer::new()
                                    .default_width(Some(md_w as usize))
                                    .show(ui, cache, &md);
                            }
                            crate::md_table::MdSegment::Table { headers, rows } => {
                                crate::md_table::render_markdown_table(ui, &headers, &rows);
                            }
                        }
                    }
                },
            );
        }
        let _ = end;
    }

    scrolled
}

/// 按空行拆成可独立渲染的 Markdown 块（代码围栏内不拆），并保留原文字节区间
fn split_md_scroll_blocks(content: &str) -> Vec<(usize, usize, &str)> {
    if content.is_empty() {
        return Vec::new();
    }

    let mut blocks = Vec::new();
    let mut block_start = 0usize;
    let mut line_start = 0usize;
    let mut in_fence = false;

    for line in content.split_inclusive('\n') {
        let line_end = line_start + line.len();
        let trimmed = line.trim_start().trim_end_matches(['\r', '\n']);
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
        }

        let is_blank = trimmed.is_empty();
        if !in_fence && is_blank && block_start < line_start {
            let block = &content[block_start..line_start];
            if !block.trim().is_empty() {
                blocks.push((block_start, line_start, block));
            }
            block_start = line_end;
        }
        line_start = line_end;
    }

    if block_start < content.len() {
        let block = &content[block_start..];
        if !block.trim().is_empty() {
            blocks.push((block_start, content.len(), block));
        }
    }

    if blocks.is_empty() {
        blocks.push((0, content.len(), content));
    }
    blocks
}

/// 单行文本中高亮关键词（附件名等）
fn render_inline_search_hits(
    ui: &mut egui::Ui,
    text: &str,
    query: &str,
    size: f32,
    color: Color32,
    is_active_widget: bool,
    do_scroll: bool,
) -> bool {
    let q_lower = query.trim().to_lowercase();
    let ranges = find_all_match_ranges(text, &q_lower);
    if ranges.is_empty() {
        ui.label(RichText::new(text).size(size).color(color));
        return false;
    }

    let hit_bg = Color32::from_rgb(255, 236, 150);
    let active_bg = Color32::from_rgb(255, 196, 60);
    let mut scrolled = false;
    let mut pos = 0usize;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        for (s, e) in ranges {
            if pos < s {
                ui.label(RichText::new(&text[pos..s]).size(size).color(color));
            }
            let resp = ui.label(
                RichText::new(&text[s..e])
                    .size(size)
                    .color(color)
                    .strong()
                    .background_color(if is_active_widget { active_bg } else { hit_bg }),
            );
            if is_active_widget && do_scroll && !scrolled {
                resp.scroll_to_me(Some(Align::Center));
                scrolled = true;
            }
            pos = e;
        }
        if pos < text.len() {
            ui.label(RichText::new(&text[pos..]).size(size).color(color));
        }
    });
    scrolled
}

/// 渲染带关键词高亮的正文；若滚到当前命中则返回 true
fn render_content_with_search_hits(
    ui: &mut egui::Ui,
    content: &str,
    query: &str,
    active_byte_start: Option<usize>,
    do_scroll: bool,
) -> bool {
    let q_lower = query.trim().to_lowercase();
    let ranges = find_all_match_ranges(content, &q_lower);
    if ranges.is_empty() {
        ui.add(
            egui::Label::new(
                RichText::new(content)
                    .size(14.5)
                    .color(Color32::from_rgb(28, 36, 44)),
            )
            .wrap(),
        );
        return false;
    }

    let text_color = Color32::from_rgb(28, 36, 44);
    let hit_bg = Color32::from_rgb(255, 236, 150);
    let active_bg = Color32::from_rgb(255, 196, 60);
    let mut scrolled = false;

    // 按行渲染，保证换行后仍能 scroll_to_me 到关键词控件
    ui.vertical(|ui| {
        ui.set_max_width(ui.available_width());
        let mut line_start = 0usize;
        while line_start <= content.len() {
            let line_end = content[line_start..]
                .find('\n')
                .map(|i| line_start + i)
                .unwrap_or(content.len());
            let line = &content[line_start..line_end];

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let mut pos = line_start;
                let line_ranges: Vec<(usize, usize)> = ranges
                    .iter()
                    .copied()
                    .filter(|(s, e)| *e > line_start && *s < line_end)
                    .map(|(s, e)| (s.max(line_start), e.min(line_end)))
                    .collect();

                if line_ranges.is_empty() {
                    if !line.is_empty() {
                        ui.label(RichText::new(line).size(14.5).color(text_color));
                    } else {
                        ui.label(RichText::new(" ").size(14.5));
                    }
                } else {
                    for (s, e) in line_ranges {
                        if pos < s {
                            ui.label(
                                RichText::new(&content[pos..s])
                                    .size(14.5)
                                    .color(text_color),
                            );
                        }
                        // 以原始命中起点为准（未裁到行首的伪起点）
                        let is_active = active_byte_start == Some(s);
                        let bg = if is_active { active_bg } else { hit_bg };
                        let resp = ui.label(
                            RichText::new(&content[s..e])
                                .size(14.5)
                                .color(text_color)
                                .strong()
                                .background_color(bg),
                        );
                        if is_active && do_scroll && !scrolled {
                            resp.scroll_to_me(Some(Align::Center));
                            scrolled = true;
                        }
                        pos = e;
                    }
                    if pos < line_end {
                        ui.label(
                            RichText::new(&content[pos..line_end])
                                .size(14.5)
                                .color(text_color),
                        );
                    }
                }
            });

            if line_end >= content.len() {
                break;
            }
            line_start = line_end + 1;
        }
    });

    scrolled
}

fn labeled_row(
    ui: &mut egui::Ui,
    label_w: f32,
    label: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(label_w, ui.spacing().interact_size.y),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.label(RichText::new(label).size(13.5));
            },
        );
        add_contents(ui);
    });
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut changed = false;

    // 字体链顺序必须是：egui 默认（含基础 emoji）→ 系统 emoji → 中文。
    // 中文若插到最前，不少 CJK 字体会给 emoji 码位提供 □ 占位符，导致回退失效。

    let emoji_candidates = [
        r"C:\Windows\Fonts\seguiemj.ttf", // Segoe UI Emoji
        r"C:\Windows\Fonts\seguisym.ttf", // Segoe UI Symbol
        "/System/Library/Fonts/Apple Color Emoji.ttc",
        "/usr/share/fonts/truetype/noto/NotoEmoji-Regular.ttf",
        "/usr/share/fonts/opentype/noto/NotoEmoji-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    ];
    for path in emoji_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "emoji".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("emoji".into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("emoji".into());
            changed = true;
            break;
        }
    }

    let cjk_candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];
    for path in cjk_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "chinese".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            // 追加到末尾：只承接默认字体没有的汉字，不覆盖 emoji
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("chinese".into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("chinese".into());
            changed = true;
            break;
        }
    }

    if changed {
        ctx.set_fonts(fonts);
    }
}
