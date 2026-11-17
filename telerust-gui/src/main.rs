//! telerust-gui - Clean, Functional Telegram Desktop Client
//! Modern, website-like design with reliable message fetching

use anyhow::Result;
use eframe::egui;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use telerust_core::{
    config::AppConfig, HighLevelClient, TelegramClient, TelegramClientConfig, Chat, ChatType, Message, User, LoginToken,
};

// ============================================================================
// COLOR SCHEME - Clean, Modern, Website-like
// ============================================================================

const BG_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0x18, 0x1A, 0x1F);
const BG_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0x1F, 0x22, 0x28);
const BG_TERTIARY: egui::Color32 = egui::Color32::from_rgb(0x28, 0x2C, 0x34);
const BG_HOVER: egui::Color32 = egui::Color32::from_rgb(0x2D, 0x32, 0x3C);
const BORDER: egui::Color32 = egui::Color32::from_rgb(0x3A, 0x3F, 0x4A);
const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xE8, 0xEA, 0xED);
const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0x9A, 0xA0, 0xAA);
const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(0x6B, 0x72, 0x80);
const ACCENT_BLUE: egui::Color32 = egui::Color32::from_rgb(0x00, 0x9F, 0xE8);
const ACCENT_GREEN: egui::Color32 = egui::Color32::from_rgb(0x2E, 0xCC, 0x71);
const ACCENT_RED: egui::Color32 = egui::Color32::from_rgb(0xE7, 0x4C, 0x3C);
const ACCENT_ORANGE: egui::Color32 = egui::Color32::from_rgb(0xF3, 0x9C, 0x12);

// ============================================================================
// SIMPLE ANIMATION HELPERS
// ============================================================================

struct AnimValue {
    current: f32,
    target: f32,
    speed: f32,
}

impl AnimValue {
    fn new(speed: f32) -> Self {
        Self { current: 0.0, target: 0.0, speed }
    }
    fn set_target(&mut self, target: f32) { self.target = target; }
    fn set_instant(&mut self, value: f32) { self.current = value; self.target = value; }
    fn update(&mut self, dt: f32) -> f32 {
        let diff = self.target - self.current;
        if diff.abs() < 0.001 { self.current = self.target; return self.current; }
        self.current += diff * (1.0 - (-self.speed * dt).exp());
        self.current
    }
}

// ============================================================================
// APP STATE
// ============================================================================

struct TelerustApp {
    client: Option<Arc<Mutex<HighLevelClient>>>,
    current_chat: Option<Chat>,
    messages: Vec<Message>,
    dialogs: Vec<Chat>,
    me: Option<User>,
    new_message_text: String,
    login_phone: String,
    login_code: String,
    login_token: Option<LoginToken>,
    login_step: LoginStep,
    runtime: tokio::runtime::Runtime,

    // UI State
    search_query: String,
    show_settings: bool,
    show_new_chat: bool,
    show_emoji_picker: bool,
    sidebar_width: f32,
    is_sidebar_collapsed: bool,

    // Animations
    msg_list_opacity: AnimValue,
    sidebar_anim: AnimValue,
    input_focus_anim: AnimValue,

    // Runtime
    last_frame: Instant,
    needs_repaint: bool,
}

#[derive(PartialEq, Clone, Copy)]
enum LoginStep { EnterPhone, EnterCode }

impl Default for TelerustApp {
    fn default() -> Self {
        Self {
            client: None,
            current_chat: None,
            messages: Vec::new(),
            dialogs: Vec::new(),
            me: None,
            new_message_text: String::new(),
            login_phone: String::new(),
            login_code: String::new(),
            login_token: None,
            login_step: LoginStep::EnterPhone,
            runtime: tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"),
            search_query: String::new(),
            show_settings: false,
            show_new_chat: false,
            show_emoji_picker: false,
            sidebar_width: 300.0,
            is_sidebar_collapsed: false,
            msg_list_opacity: AnimValue::new(8.0),
            sidebar_anim: AnimValue::new(10.0),
            input_focus_anim: AnimValue::new(6.0),
            last_frame: Instant::now(),
            needs_repaint: true,
        }
    }
}

impl TelerustApp {
    fn load_config(&self) -> Result<AppConfig> { AppConfig::load() }

    fn connect(&mut self) -> Result<()> {
        let config = self.load_config()?;
        let client_config = TelegramClientConfig {
            api_id: config.telegram.api_id,
            api_hash: config.telegram.api_hash.clone(),
            session_file: config.session_file(),
            download_dir: config.app.download_dir.clone(),
        };
        let client = self.runtime.block_on(async { TelegramClient::new(client_config).await })?;
        let high_level = self.runtime.block_on(async { HighLevelClient::new(client).await });
        self.client = Some(Arc::new(Mutex::new(high_level)));

        let authorized = self.runtime.block_on(async {
            let client = self.client.as_ref().unwrap();
            client.lock().await.get_me().await.is_ok()
        });

        if authorized {
            self.load_initial_data();
        } else {
            self.login_step = LoginStep::EnterPhone;
        }
        Ok(())
    }

    fn load_initial_data(&mut self) {
        self.load_me();
        self.load_dialogs();
        self.msg_list_opacity.set_target(1.0);
    }

    fn request_login_code(&mut self) -> Result<()> {
        if let Some(client) = &self.client {
            let client = Arc::clone(client);
            let phone = self.login_phone.clone();
            let token = self.runtime.block_on(async move {
                client.lock().await.request_login_code(&phone).await
            })?;
            self.login_token = Some(token);
            self.login_step = LoginStep::EnterCode;
        }
        Ok(())
    }

    fn submit_login_code(&mut self) -> Result<()> {
        if let (Some(client), Some(token)) = (&self.client, self.login_token.take()) {
            let client = Arc::clone(client);
            let code = self.login_code.clone();
            self.runtime.block_on(async move {
                client.lock().await.sign_in(token, &code).await
            })?;
            self.login_code.clear();
            self.load_initial_data();
        }
        Ok(())
    }

    fn send_message(&mut self) -> Result<()> {
        if let (Some(client), Some(chat)) = (&self.client, &self.current_chat) {
            let text = self.new_message_text.trim();
            if text.is_empty() { return Ok(()); }
            let client = Arc::clone(client);
            let peer_id = chat.peer_id;
            let text = text.to_string();
            self.new_message_text.clear();
            self.input_focus_anim.set_target(0.0);

            let msg = self.runtime.block_on(async move {
                client.lock().await.send_message_by_full_id(peer_id, &text).await
            })?;
            self.messages.push(msg);
        }
        Ok(())
    }

    fn load_chat_messages(&mut self, chat: &Chat) {
        if let Some(client) = &self.client {
            let client = Arc::clone(client);
            let peer_id = chat.peer_id;
            self.current_chat = Some(chat.clone());
            self.msg_list_opacity.set_target(0.0);

            let fetched = self.runtime.block_on(async move {
                client.lock().await.get_messages_by_full_id(peer_id, 100).await.unwrap_or_default()
            });

            self.messages = fetched.into_iter().rev().collect();
            self.msg_list_opacity.set_target(1.0);
        }
    }

    fn load_dialogs(&mut self) {
        if let Some(client) = &self.client {
            let client = Arc::clone(client);
            self.dialogs = self.runtime.block_on(async move {
                client.lock().await.get_dialogs().await.unwrap_or_default()
            });
        }
    }

    fn load_me(&mut self) {
        if let Some(client) = &self.client {
            let client = Arc::clone(client);
            self.me = self.runtime.block_on(async move {
                client.lock().await.get_me().await.ok()
            });
        }
    }

    fn filtered_dialogs(&self) -> Vec<&Chat> {
        if self.search_query.trim().is_empty() {
            self.dialogs.iter().collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.dialogs.iter()
                .filter(|c| c.title.to_lowercase().contains(&q)
                    || c.username.as_ref().map(|u| u.to_lowercase().contains(&q)).unwrap_or(false))
                .collect()
        }
    }

    fn chat_icon(&self, chat_type: ChatType) -> &'static str {
        match chat_type {
            ChatType::Channel => "📢",
            ChatType::Supergroup | ChatType::Group => "👥",
            ChatType::Community => "🏘️",
            ChatType::Private => "👤",
        }
    }

    fn format_time(&self, ts: i64) -> String {
        chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.format("%H:%M").to_string())
            .unwrap_or_default()
    }
}

impl eframe::App for TelerustApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Initialize on first frame
        if self.client.is_none() {
            let _ = self.connect();
        }

        // Delta time
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;

        // Update animations
        let _ = self.msg_list_opacity.update(dt);
        let _ = self.sidebar_anim.update(dt);
        let _ = self.input_focus_anim.update(dt);

        // Setup style
        self.setup_style(ctx);

        // Layout
        if self.me.is_none() {
            // Not logged in - show login
            egui::CentralPanel::default().show(ctx, |ui| self.show_login(ui));
        } else {
            // Logged in - show main UI
            self.show_top_bar(ctx);
            self.show_sidebar(ctx);

            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(chat) = self.current_chat.clone() {
                    self.show_chat(ui, &chat);
                } else {
                    self.show_welcome(ui);
                }
            });

            if self.current_chat.is_some() {
                self.show_chat_info(ctx);
            }

            // Modals
            if self.show_settings { self.show_settings_modal(ctx); }
            if self.show_new_chat { self.show_new_chat_modal(ctx); }
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl TelerustApp {
    // ========================================================================
    // STYLING
    // ========================================================================
    fn setup_style(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        let v = &mut style.visuals;

        v.dark_mode = true;
        v.panel_fill = BG_PRIMARY;
        v.window_fill = BG_SECONDARY;
        v.extreme_bg_color = BG_PRIMARY;
        v.faint_bg_color = BG_SECONDARY;
        v.window_shadow = egui::Shadow::NONE;
        v.popup_shadow = egui::Shadow {
            offset: egui::Vec2::new(0.0, 8.0),
            blur: 24.0,
            spread: 0.0,
            color: egui::Color32::from_black_alpha(180),
        };

        v.selection.bg_fill = ACCENT_BLUE.gamma_multiply(0.2);
        v.selection.stroke = egui::Stroke::new(1.0, ACCENT_BLUE);
        v.hyperlink_color = ACCENT_BLUE;
        v.override_text_color = Some(TEXT_PRIMARY);

        v.widgets.noninteractive.bg_fill = egui::Color32::TRANSPARENT;
        v.widgets.inactive.bg_fill = BG_TERTIARY;
        v.widgets.inactive.rounding = egui::Rounding::same(8.0);
        v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, BORDER);
        v.widgets.hovered.bg_fill = BG_HOVER;
        v.widgets.hovered.rounding = egui::Rounding::same(8.0);
        v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, ACCENT_BLUE.gamma_multiply(0.5));
        v.widgets.active.bg_fill = ACCENT_BLUE.gamma_multiply(0.15);
        v.widgets.active.rounding = egui::Rounding::same(8.0);
        v.widgets.active.fg_stroke = egui::Stroke::new(1.0, ACCENT_BLUE);
        v.widgets.open.bg_fill = ACCENT_BLUE.gamma_multiply(0.15);
        v.widgets.open.rounding = egui::Rounding::same(8.0);

        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.spacing.menu_margin = egui::Margin::same(8.0);
        style.spacing.combo_width = 200.0;
        style.spacing.text_edit_width = 280.0;
        style.spacing.indent = 16.0;
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.slider_width = 200.0;
        style.spacing.window_margin = egui::Margin::same(0.0);
        style.spacing.icon_width = 18.0;

        style.interaction.selectable_labels = true;
        style.interaction.tooltip_delay = 0.3;
        style.animation_time = 0.15;

        ctx.set_style(style);
    }

    // ========================================================================
    // LOGIN SCREEN
    // ========================================================================
    fn show_login(&mut self, ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            ui.add_space(60.0);
            egui::Frame::none()
                .fill(BG_SECONDARY)
                .stroke(egui::Stroke::new(1.0, BORDER))
                .rounding(egui::Rounding::same(16.0))
                .inner_margin(egui::Margin::symmetric(48.0, 40.0))
                .show(ui, |ui| {
                    ui.set_min_width(380.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("telerust").size(32.0).strong().color(ACCENT_BLUE));
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Telegram Client").size(14.0).color(TEXT_SECONDARY));
                        ui.add_space(28.0);

                        match self.login_step {
                            LoginStep::EnterPhone => {
                                ui.label(egui::RichText::new("Enter your phone number").size(14.0).color(TEXT_SECONDARY));
                                ui.add_space(16.0);
                                let resp = ui.add_sized([320.0, 44.0], egui::TextEdit::singleline(&mut self.login_phone)
                                    .hint_text("+1 234 567 8900")
                                    .font(egui::FontId::proportional(16.0))
                                    .frame(true)
                                    .margin(egui::Margin::symmetric(14.0, 10.0)));
                                if self.login_phone.is_empty() { resp.request_focus(); }
                                ui.add_space(20.0);
                                let btn = ui.add_enabled(!self.login_phone.is_empty(),
                                    egui::Button::new(egui::RichText::new("Continue").size(15.0))
                                        .fill(ACCENT_BLUE)
                                        .min_size(egui::vec2(140.0, 42.0))
                                        .rounding(egui::Rounding::same(8.0))
                                        .stroke(egui::Stroke::NONE));
                                if btn.clicked() { let _ = self.request_login_code(); }
                            }
                            LoginStep::EnterCode => {
                                ui.label(egui::RichText::new(format!("Code sent to {}", self.login_phone))
                                    .size(14.0).color(TEXT_SECONDARY));
                                ui.add_space(16.0);
                                let resp = ui.add_sized([320.0, 44.0], egui::TextEdit::singleline(&mut self.login_code)
                                    .hint_text("12345")
                                    .font(egui::FontId::proportional(24.0))
                                    .password(true)
                                    .frame(true)
                                    .margin(egui::Margin::symmetric(14.0, 10.0)));
                                if self.login_code.is_empty() { resp.request_focus(); }
                                ui.add_space(20.0);
                                let btn = ui.add_enabled(self.login_code.len() >= 5,
                                    egui::Button::new(egui::RichText::new("Sign In").size(15.0))
                                        .fill(ACCENT_BLUE)
                                        .min_size(egui::vec2(140.0, 42.0))
                                        .rounding(egui::Rounding::same(8.0))
                                        .stroke(egui::Stroke::NONE));
                                if btn.clicked() { let _ = self.submit_login_code(); }
                                ui.add_space(12.0);
                                let back = ui.add(egui::Button::new("← Back").frame(false));
                                if back.clicked() { self.login_step = LoginStep::EnterPhone; self.login_token = None; self.login_code.clear(); }
                            }
                        }
                        ui.add_space(24.0);
                        ui.label(egui::RichText::new("Built with Rust • grammers • egui").size(11.0).color(TEXT_MUTED));
                    });
                });
        });
    }

    // ========================================================================
    // TOP BAR
    // ========================================================================
    fn show_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .frame(egui::Frame::none().fill(BG_PRIMARY).stroke(egui::Stroke::new(1.0, BORDER)))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if self.is_sidebar_collapsed {
                        let resp = ui.add(egui::Button::new("☰").frame(false).min_size(egui::vec2(40.0, 40.0)).rounding(8.0));
                        if resp.clicked() { self.is_sidebar_collapsed = false; self.sidebar_anim.set_target(1.0); }
                    } else {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("telerust").size(20.0).strong().color(ACCENT_BLUE));
                    }
                    ui.add_space(16.0);

                    // Search
                    let search_w = 300.0 + self.input_focus_anim.current * 100.0;
                    let resp = ui.add_sized([search_w, 38.0], egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("🔍 Search chats...")
                        .font(egui::FontId::proportional(14.0))
                        .frame(true)
                        .margin(egui::Margin::symmetric(14.0, 8.0)));
                    if resp.has_focus() { self.input_focus_anim.set_target(1.0); }
                    else if self.search_query.is_empty() { self.input_focus_anim.set_target(0.0); }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        let settings = ui.add(egui::Button::new("⚙").frame(false).min_size(egui::vec2(36.0, 36.0)).rounding(8.0));
                        if settings.clicked() { self.show_settings = true; }

                        let new_chat = ui.add(egui::Button::new("✎").frame(false).min_size(egui::vec2(36.0, 36.0)).rounding(8.0));
                        if new_chat.clicked() { self.show_new_chat = true; }

                        ui.add_space(8.0);
                        if let Some(me) = &self.me {
                            egui::menu::menu_button(ui, format!(" {} ▼", me.first_name), |ui| {
                                ui.set_min_width(200.0);
                                if ui.selectable_label(false, "👤 Profile").clicked() { ui.close_menu(); }
                                if ui.selectable_label(false, "⚙️ Settings").clicked() { ui.close_menu(); }
                                ui.separator();
                                if ui.selectable_label(false, "🚪 Log out").clicked() {
                                    self.client = None; self.me = None; self.current_chat = None;
                                    self.login_step = LoginStep::EnterPhone; ui.close_menu();
                                }
                            });
                        }
                    });
                });
                ui.add_space(8.0);
            });
    }

    // ========================================================================
    // SIDEBAR
    // ========================================================================
    fn show_sidebar(&mut self, ctx: &egui::Context) {
        let w = if self.is_sidebar_collapsed { 60.0 } else { self.sidebar_width.max(60.0) };

        egui::SidePanel::left("sidebar")
            .default_width(w)
            .width_range(60.0..=400.0)
            .frame(egui::Frame::none().fill(BG_PRIMARY).stroke(egui::Stroke::new(1.0, BORDER)))
            .show(ctx, |ui| {
                if self.me.is_none() { return; }

                if !self.is_sidebar_collapsed {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Chats").size(13.0).color(TEXT_MUTED));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let unread: i32 = self.dialogs.iter().map(|c| c.unread_count).sum();
                            if unread > 0 {
                                let badge = format!("{}", unread);
                                let rect = ui.allocate_response(egui::vec2(22.0, 22.0), egui::Sense::hover()).rect;
                                ui.painter().rect_filled(rect, 11.0, ACCENT_GREEN);
                                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, badge,
                                    egui::FontId::proportional(11.0), egui::Color32::WHITE);
                            }
                        });
                    });
                    ui.add_space(8.0);
                }

                // Clone dialogs to avoid borrow issues in closure
                let dialogs: Vec<Chat> = self.filtered_dialogs().into_iter().cloned().collect();
                let current_id = self.current_chat.as_ref().map(|c| c.id);
                let is_collapsed = self.is_sidebar_collapsed;

                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut clicked_id: Option<i64> = None;

                    for chat in &dialogs {
                        let is_sel = current_id.map(|id| id == chat.id).unwrap_or(false);
                        let clicked = self.render_chat_item(ui, chat, is_sel, is_collapsed);
                        if clicked { clicked_id = Some(chat.id); }
                    }

                    if dialogs.is_empty() && !self.search_query.is_empty() {
                        ui.centered_and_justified(|ui| {
                            ui.add_space(40.0);
                            ui.label(egui::RichText::new("No chats match your search").color(TEXT_MUTED));
                        });
                    } else if dialogs.is_empty() {
                        ui.centered_and_justified(|ui| {
                            ui.add_space(40.0);
                            ui.label(egui::RichText::new("No chats yet").color(TEXT_MUTED));
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new("Click ✎ to start a new chat").size(12.0).color(TEXT_MUTED));
                        });
                    }

                    if let Some(id) = clicked_id {
                        if let Some(idx) = self.dialogs.iter().position(|c| c.id == id) {
                            let chat = self.dialogs[idx].clone();
                            self.load_chat_messages(&chat);
                        }
                    }
                });

                if !self.is_sidebar_collapsed {
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                    let resp = ui.add(egui::Button::new("✎  New Message")
                        .frame(true).fill(ACCENT_BLUE.gamma_multiply(0.1))
                        .rounding(egui::Rounding::same(10.0))
                        .min_size(egui::vec2(ui.available_width(), 40.0)));
                    if resp.clicked() { self.show_new_chat = true; }
                }
            });
    }

    fn render_chat_item(&mut self, ui: &mut egui::Ui, chat: &Chat, selected: bool, collapsed: bool) -> bool {
        let icon = self.chat_icon(chat.chat_type);
        let h = if collapsed { 52.0 } else { 68.0 };

        let resp = ui.allocate_response(egui::vec2(ui.available_width(), h), egui::Sense::click());

        // Background
        let bg = if selected { ACCENT_BLUE.gamma_multiply(0.12) }
            else if resp.hovered() { BG_HOVER }
            else { egui::Color32::TRANSPARENT };
        if bg != egui::Color32::TRANSPARENT {
            ui.painter().rect_filled(resp.rect.shrink(6.0), 10.0, bg);
        }
        if selected {
            ui.painter().rect_stroke(resp.rect.shrink(4.0), 10.0, egui::Stroke::new(1.5, ACCENT_BLUE));
            // Left accent
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(resp.rect.left() + 2.0, resp.rect.top() + 6.0),
                    egui::pos2(resp.rect.left() + 5.0, resp.rect.bottom() - 6.0)
                ), 2.0, ACCENT_BLUE);
        }

        let r = resp.rect.shrink(12.0);

        if collapsed {
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, icon,
                egui::FontId::proportional(20.0), TEXT_PRIMARY);
        } else {
            // Avatar
            let av_r = egui::Rect::from_min_size(egui::pos2(r.left(), r.top() + 2.0), egui::vec2(44.0, 44.0));
            self.draw_avatar(ui.painter(), av_r, &chat.title, icon);

            // Online indicator for private
            if chat.chat_type == ChatType::Private {
                let dot = egui::pos2(av_r.right() - 2.0, av_r.bottom() - 2.0);
                ui.painter().circle_filled(dot, 5.0, BG_PRIMARY);
                ui.painter().circle_filled(dot, 4.0, ACCENT_GREEN);
            }

            let tx = av_r.right() + 12.0;
            let ty = r.top() + 2.0;

            // Title
            ui.painter().text(egui::pos2(tx, ty), egui::Align2::LEFT_TOP, &chat.title,
                egui::FontId::proportional(14.0), TEXT_PRIMARY);

            // Subtitle
            if let Some(user) = &chat.username {
                ui.painter().text(egui::pos2(tx, ty + 22.0), egui::Align2::LEFT_TOP, format!("@{}", user),
                    egui::FontId::proportional(11.0), TEXT_MUTED);
            } else if let Some(lm) = &chat.last_message {
                let t = if lm.len() > 35 { format!("{}...", &lm[..35]) } else { lm.clone() };
                ui.painter().text(egui::pos2(tx, ty + 22.0), egui::Align2::LEFT_TOP, t,
                    egui::FontId::proportional(11.0), TEXT_SECONDARY);
            }

            // Unread badge
            if chat.unread_count > 0 {
                let txt = if chat.unread_count > 99 { "99+" } else { &chat.unread_count.to_string() };
                let br = egui::Rect::from_center_size(
                    egui::pos2(r.right() - 18.0, r.top() + 14.0),
                    egui::vec2(28.0, 20.0));
                ui.painter().rect_filled(br, 10.0, ACCENT_GREEN);
                ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, txt,
                    egui::FontId::proportional(10.0), egui::Color32::WHITE);
            }

            // Mute/Pin
            let mut ix = r.right() - 24.0;
            if chat.is_muted {
                ui.painter().text(egui::pos2(ix, r.top() + 16.0), egui::Align2::CENTER_CENTER, "🔕",
                    egui::FontId::proportional(13.0), TEXT_MUTED);
                ix -= 20.0;
            }
            if chat.is_pinned {
                ui.painter().text(egui::pos2(ix, r.top() + 16.0), egui::Align2::CENTER_CENTER, "📌",
                    egui::FontId::proportional(13.0), ACCENT_BLUE);
            }
        }

        resp.clicked()
    }

    fn draw_avatar(&self, p: &egui::Painter, r: egui::Rect, name: &str, icon: &str) {
        // Generate gradient from name hash
        let hash: u32 = name.bytes().fold(0, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
        let h1 = (hash % 360) as f32;
        let h2 = ((hash / 360) % 360) as f32;

        fn hsv(h: f32, s: f32, v: f32) -> egui::Color32 {
            let c = v * s; let x = c * (1.0 - ((h/60.0)%2.0 - 1.0).abs()); let m = v - c;
            let (r,g,b) = if h<60.0 {(c,x,0.0)} else if h<120.0 {(x,c,0.0)} else if h<180.0 {(0.0,c,x)} else if h<240.0 {(0.0,x,c)} else if h<300.0 {(x,0.0,c)} else {(c,0.0,x)};
            egui::Color32::from_rgb(((r+m)*255.0) as u8, ((g+m)*255.0) as u8, ((b+m)*255.0) as u8)
        }
        let c1 = hsv(h1, 0.55, 0.7); let c2 = hsv(h2, 0.55, 0.5);

        // Draw gradient background
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            let col = egui::Color32::from_rgba_premultiplied(
                (c1.r() as f32 + (c2.r() as f32 - c1.r() as f32) * t) as u8,
                (c1.g() as f32 + (c2.g() as f32 - c1.g() as f32) * t) as u8,
                (c1.b() as f32 + (c2.b() as f32 - c1.b() as f32) * t) as u8,
                255
            );
            p.circle_filled(r.center(), r.width()/2.0 * (1.0 - t * 0.25), col);
        }
        p.text(r.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(r.width()*0.5), egui::Color32::WHITE);
        p.circle_stroke(r.center(), r.width()/2.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(255,255,255,40)));
    }

    // ========================================================================
    // WELCOME SCREEN
    // ========================================================================
    fn show_welcome(&self, ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("telerust").size(44.0).strong().color(ACCENT_BLUE));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Clean, fast Telegram client").size(16.0).color(TEXT_SECONDARY));
                ui.add_space(32.0);

                egui::Grid::new("features").spacing([32.0, 16.0]).show(ui, |ui| {
                    self.feat(ui, "⚡", "Fast", "Native Rust performance");
                    self.feat(ui, "🔒", "Secure", "MTProto encryption");
                    self.feat(ui, "💾", "Lightweight", "Low memory usage");
                    self.feat(ui, "🎨", "Clean UI", "Modern, distraction-free");
                });

                ui.add_space(32.0);
                ui.label(egui::RichText::new("Select a chat from the sidebar →").size(13.0).color(TEXT_MUTED));
            });
        });
    }

    fn feat(&self, ui: &mut egui::Ui, icon: &str, title: &str, desc: &str) {
        let resp = ui.allocate_response(egui::vec2(150.0, 90.0), egui::Sense::hover());
        let bg = if resp.hovered() { BG_HOVER } else { BG_TERTIARY };
        ui.painter().rect_filled(resp.rect, 12.0, bg);
        if resp.hovered() { ui.painter().rect_stroke(resp.rect, 12.0, egui::Stroke::new(1.0, ACCENT_BLUE.gamma_multiply(0.4))); }

        ui.scope(|ui| {
            ui.set_clip_rect(resp.rect);
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(icon).size(26.0).color(ACCENT_BLUE));
                ui.add_space(6.0);
                ui.label(egui::RichText::new(title).size(13.0).strong());
                ui.add_space(2.0);
                ui.label(egui::RichText::new(desc).size(10.0).color(TEXT_MUTED));
            });
        });
        ui.end_row();
    }

    // ========================================================================
    // CHAT VIEW
    // ========================================================================
    fn show_chat(&mut self, ui: &mut egui::Ui, chat: &Chat) {
        // Input panel at bottom
        egui::TopBottomPanel::bottom("input_panel")
            .frame(egui::Frame::none().fill(BG_PRIMARY).stroke(egui::Stroke::new(1.0, BORDER)))
            .show_inside(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);

                    let resp = ui.add_sized([ui.available_width() - 100.0, 42.0],
                        egui::TextEdit::singleline(&mut self.new_message_text)
                            .hint_text("Message...")
                            .font(egui::FontId::proportional(14.0))
                            .frame(true)
                            .margin(egui::Margin::symmetric(14.0, 8.0)));
                    if self.new_message_text.is_empty() && !(self.input_focus_anim.current > 0.5) {
                        resp.request_focus();
                    }

                    ui.add_space(8.0);
                    let attach = ui.add(egui::Button::new("📎").frame(false).min_size(egui::vec2(36.0, 36.0)).rounding(8.0));
                    ui.add_space(4.0);
                    let send = ui.add_enabled(!self.new_message_text.trim().is_empty(),
                        egui::Button::new(egui::RichText::new("➤").size(18.0))
                            .fill(ACCENT_BLUE)
                            .min_size(egui::vec2(42.0, 42.0))
                            .rounding(egui::Rounding::same(21.0))
                            .stroke(egui::Stroke::NONE));
                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !ui.input(|i| i.modifiers.shift);
                    if send.clicked() || enter { let _ = self.send_message(); }

                    ui.add_space(12.0);
                });
                ui.add_space(8.0);
            });

        // Chat header
        egui::TopBottomPanel::top("chat_header")
            .frame(egui::Frame::none().fill(BG_PRIMARY).stroke(egui::Stroke::new(1.0, BORDER)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    let icon = self.chat_icon(chat.chat_type);
                    let av_r = egui::Rect::from_min_size(ui.cursor().min + egui::vec2(0.0, 4.0), egui::vec2(40.0, 40.0));
                    self.draw_avatar(ui.painter(), av_r, &chat.title, icon);

                    if chat.chat_type == ChatType::Private {
                        let dot = egui::pos2(av_r.right() - 2.0, av_r.bottom() - 2.0);
                        ui.painter().circle_filled(dot, 4.5, BG_PRIMARY);
                        ui.painter().circle_filled(dot, 4.0, ACCENT_GREEN);
                    }

                    ui.add_space(48.0);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new(&chat.title).size(16.0).strong());
                        if let Some(u) = &chat.username {
                            ui.label(egui::RichText::new(format!("@{}", u)).size(11.0).color(TEXT_MUTED));
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        let call = ui.add(egui::Button::new("📞").frame(false).min_size(egui::vec2(36.0, 36.0)).rounding(8.0));
                        let vid = ui.add(egui::Button::new("📹").frame(false).min_size(egui::vec2(36.0, 36.0)).rounding(8.0));
                        let menu = ui.add(egui::Button::new("⋮").frame(false).min_size(egui::vec2(36.0, 36.0)).rounding(8.0));
                    });
                });
            });

        // Messages
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.add_space(12.0);

                let opacity = self.msg_list_opacity.current.clamp(0.0, 1.0);
                ui.set_opacity(opacity);

                for msg in &self.messages {
                    self.render_message(ui, msg);
                }

                ui.add_space(16.0);
            });
    }

    fn render_message(&self, ui: &mut egui::Ui, msg: &Message) {
        let outgoing = msg.is_outgoing;
        let align = if outgoing { egui::Align::RIGHT } else { egui::Align::LEFT };
        let bubble_bg = if outgoing { ACCENT_BLUE } else { BG_TERTIARY };
        let text_col = if outgoing { egui::Color32::WHITE } else { TEXT_PRIMARY };

        let rounding = if outgoing {
            egui::Rounding { nw: 18.0, ne: 4.0, sw: 18.0, se: 18.0 }
        } else {
            egui::Rounding { nw: 4.0, ne: 18.0, sw: 18.0, se: 18.0 }
        };

        ui.with_layout(egui::Layout::top_down(align), |ui| {
            let max_w = ui.available_width() * 0.75;

            egui::Frame::none()
                .fill(bubble_bg)
                .rounding(rounding)
                .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                .shadow(egui::Shadow {
                    offset: egui::Vec2::new(0.0, 2.0),
                    blur: 8.0,
                    spread: 0.0,
                    color: egui::Color32::from_black_alpha(80),
                })
                .show(ui, |ui| {
                    ui.set_max_width(max_w);

                    if !msg.text.is_empty() {
                        ui.label(egui::RichText::new(&msg.text).color(text_col).size(14.5).line_height(Some(21.0)));
                    }

                    if msg.has_media {
                        if let Some(mt) = &msg.media_type {
                            ui.add_space(6.0);
                            let icon = match mt.as_str() {
                                "Photo" => "🖼️", "Video" => "🎬", "Document" => "📄",
                                "Audio" => "🎵", "Voice" => "🎤", "Sticker" => "🎭",
                                "Animation" => "🎞️", _ => "📎",
                            };
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("{} {}", icon, mt))
                                    .color(text_col.gamma_multiply(0.8)).size(11.0));
                            });
                        }
                    }
                });

            // Time + status
            let time = self.format_time(msg.date);
            let status = if outgoing { format!("{}  ✓✓", time) } else { time };
            ui.with_layout(egui::Layout::top_down(align), |ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(status).size(9.5).color(TEXT_MUTED));
            });
            ui.add_space(10.0);
        });
    }

    // ========================================================================
    // CHAT INFO PANEL (Right)
    // ========================================================================
    fn show_chat_info(&mut self, ctx: &egui::Context) {
        if let Some(chat) = &self.current_chat {
            egui::SidePanel::right("chat_info")
                .default_width(280.0)
                .width_range(250.0..=350.0)
                .frame(egui::Frame::none().fill(BG_PRIMARY).stroke(egui::Stroke::new(1.0, BORDER)))
                .show(ctx, |ui| {
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Chat Info").size(15.0).color(TEXT_MUTED));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let c = ui.add(egui::Button::new("✕").frame(false).min_size(egui::vec2(28.0, 28.0)).rounding(6.0));
                        });
                    });
                    ui.add_space(16.0);

                    let icon = self.chat_icon(chat.chat_type);
                    let av_r = egui::Rect::from_min_size(
                        egui::pos2((ui.available_width() - 80.0)/2.0, ui.cursor().min.y),
                        egui::vec2(80.0, 80.0));
                    self.draw_avatar(ui.painter(), av_r, &chat.title, icon);

                    if chat.chat_type == ChatType::Private {
                        let dot = egui::pos2(av_r.right() - 2.0, av_r.bottom() - 2.0);
                        ui.painter().circle_filled(dot, 7.0, BG_PRIMARY);
                        ui.painter().circle_filled(dot, 6.0, ACCENT_GREEN);
                    }
                    ui.add_space(90.0);

                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new(&chat.title).size(20.0).strong());
                        if let Some(u) = &chat.username {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(format!("@{}", u)).size(13.0).color(TEXT_MUTED));
                        }
                    });

                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(16.0);

                    let tp = match chat.chat_type {
                        ChatType::Channel => "Channel", ChatType::Supergroup => "Supergroup",
                        ChatType::Group => "Group", ChatType::Community => "Community",
                        ChatType::Private => "Private Chat",
                    };
                    self.info_row(ui, "Type", tp, ACCENT_BLUE);
                    self.info_row(ui, "ID", &chat.id.to_string(), TEXT_SECONDARY);
                    self.info_row(ui, "Unread", &chat.unread_count.to_string(), ACCENT_GREEN);
                    self.info_row(ui, "Muted", if chat.is_muted { "Yes" } else { "No" },
                        if chat.is_muted { ACCENT_ORANGE } else { TEXT_MUTED });
                    self.info_row(ui, "Pinned", if chat.is_pinned { "Yes" } else { "No" },
                        if chat.is_pinned { ACCENT_BLUE } else { TEXT_MUTED });

                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(16.0);

                    let actions = [
                        ("🔍", "Search Messages"), ("📎", "Media"), ("🔗", "Links"),
                        ("📁", "Files"), ("🎙️", "Voice Chats"), ("👥", "Members"),
                    ];
                    for (ic, label) in actions {
                        let r = ui.add(egui::Button::new(format!("{}  {}", ic, label))
                            .frame(true).fill(BG_TERTIARY)
                            .rounding(egui::Rounding::same(10.0))
                            .min_size(egui::vec2(ui.available_width(), 38.0)));
                    }
                });
        }
    }

    fn info_row(&self, ui: &mut egui::Ui, label: &str, val: &str, color: egui::Color32) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).size(12.0).color(TEXT_MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(val).size(12.0).strong().color(color));
            });
        });
        ui.add_space(10.0);
    }

    // ========================================================================
    // SETTINGS MODAL
    // ========================================================================
    fn show_settings_modal(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("⚙ Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .frame(egui::Frame::none().fill(BG_SECONDARY).stroke(egui::Stroke::new(1.0, BORDER)).rounding(16.0))
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("Settings").size(22.0).strong().color(ACCENT_BLUE));
                    ui.add_space(20.0);
                });
                ui.separator(); ui.add_space(12.0);

                ui.label(egui::RichText::new("Appearance").size(13.0).color(TEXT_MUTED));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Theme");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_label("").selected_text("Dark").show_ui(ui, |ui| {
                            ui.selectable_value(&mut 0, 0, "Dark");
                            ui.selectable_value(&mut 0, 1, "Light");
                        });
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Accent");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (c, name) in [(ACCENT_BLUE,"Blue"),(ACCENT_GREEN,"Green"),(ACCENT_ORANGE,"Orange"),(ACCENT_RED,"Red")] {
                            let r = ui.add(egui::Button::new("").frame(true).fill(c).min_size(egui::vec2(28.0,28.0)).rounding(6.0));
                        }
                    });
                });
                ui.add_space(16.0); ui.separator(); ui.add_space(12.0);

                ui.label(egui::RichText::new("Notifications").size(13.0).color(TEXT_MUTED));
                ui.add_space(8.0);
                ui.checkbox(&mut true, "Message notifications");
                ui.add_space(6.0);
                ui.checkbox(&mut false, "Sound");
                ui.add_space(6.0);
                ui.checkbox(&mut true, "Desktop notifications");
                ui.add_space(16.0); ui.separator(); ui.add_space(12.0);

                ui.label(egui::RichText::new("About").size(13.0).color(TEXT_MUTED));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("telerust v0.1.0").size(12.0).color(TEXT_SECONDARY));
                ui.label(egui::RichText::new("Rust • grammers • egui").size(11.0).color(TEXT_MUTED));
                ui.add_space(6.0);
                ui.hyperlink_to("GitHub", "https://github.com/telerust");
                ui.add_space(16.0);
            });
        if !open { self.show_settings = false; }
    }

    // ========================================================================
    // NEW CHAT MODAL
    // ========================================================================
    fn show_new_chat_modal(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("✎ New Message")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .frame(egui::Frame::none().fill(BG_SECONDARY).stroke(egui::Stroke::new(1.0, BORDER)).rounding(16.0))
            .show(ctx, |ui| {
                ui.set_min_width(460.0);
                ui.set_max_height(550.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("New Message").size(22.0).strong().color(ACCENT_BLUE));
                    ui.add_space(16.0);
                });
                ui.separator(); ui.add_space(12.0);

                // Search
                let resp = ui.add_sized([ui.available_width(), 40.0],
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("🔍 Search contacts...")
                        .font(egui::FontId::proportional(14.0))
                        .frame(true).margin(egui::Margin::symmetric(14.0, 8.0)));
                resp.request_focus();
                ui.add_space(12.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Clone contacts to avoid borrow issues in closure
                    let contacts: Vec<Chat> = self.dialogs.iter()
                        .filter(|c| c.chat_type == ChatType::Private)
                        .cloned()
                        .collect();

                    for chat in &contacts {
                        let q = self.search_query.to_lowercase();
                        let matches = q.is_empty() || chat.title.to_lowercase().contains(&q)
                            || chat.username.as_ref().map(|u| u.to_lowercase().contains(&q)).unwrap_or(false);
                        if !matches { continue; }

                        let r = ui.add(egui::Button::new("").frame(true).fill(BG_TERTIARY)
                            .rounding(egui::Rounding::same(10.0))
                            .min_size(egui::vec2(ui.available_width(), 56.0)));

                        let rect = r.rect.shrink(10.0);
                        let av_r = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - 18.0), egui::vec2(36.0, 36.0));
                        self.draw_avatar(ui.painter(), av_r, &chat.title, "👤");

                        ui.painter().text(egui::pos2(rect.left() + 48.0, rect.top() + 4.0),
                            egui::Align2::LEFT_TOP, &chat.title, egui::FontId::proportional(14.0), TEXT_PRIMARY);
                        if let Some(u) = &chat.username {
                            ui.painter().text(egui::pos2(rect.left() + 48.0, rect.top() + 24.0),
                                egui::Align2::LEFT_TOP, format!("@{}", u), egui::FontId::proportional(11.0), TEXT_MUTED);
                        }

                        if r.clicked() { self.load_chat_messages(chat); self.show_new_chat = false; }
                    }

                    if contacts.is_empty() {
                        ui.centered_and_justified(|ui| { ui.add_space(40.0); ui.label(egui::RichText::new("No contacts").color(TEXT_MUTED)); });
                    }
                });
                ui.add_space(16.0);
            });
        if !open { self.show_new_chat = false; }
    }
}

fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let config = AppConfig::load()?;
    if config.telegram.api_id == 0 || config.telegram.api_hash.is_empty() {
        eprintln!("Error: Telegram API credentials not configured!");
        eprintln!("Create config at %APPDATA%\\telerust\\telerust\\config.toml:");
        eprintln!("[telegram]");
        eprintln!("api_id = 12345");
        eprintln!("api_hash = \"your_api_hash\"");
        std::process::exit(1);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("telerust — Telegram Client"),
        renderer: eframe::Renderer::Glow,
        multisampling: 4,
        ..Default::default()
    };

    eframe::run_native("telerust", options, Box::new(|_cc| Ok(Box::new(TelerustApp::default()))))
        .map_err(|e| anyhow::anyhow!("GUI error: {}", e))?;
    Ok(())
}