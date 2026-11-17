//! telerust-gui-wry - Telegram Client using wry (WebView2/WebKit)
//! Modern web-based UI with native Rust backend

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use tokio::sync::{Mutex, mpsc};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use wry::{WebViewBuilder, http::Request};

use telerust_core::{
    config::AppConfig, HighLevelClient, TelegramClient, TelegramClientConfig,
};

// ============================================================================
// IPC Handler Module (defined first so it's available)
// ============================================================================

mod ipc {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "action", content = "payload")]
    pub enum IpcMessage {
        RequestLoginCode { phone: String },
        SubmitLoginCode { code: String },
        GetMe,
        GetDialogs,
        GetContacts,
        GetMessages { peer_id: i64, limit: i32 },
        SendMessage { peer_id: i64, text: String },
        GetOrCreateDialog { user_id: i64 },
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct IpcResponse {
        pub success: bool,
        #[serde(flatten)]
        pub data: serde_json::Value,
        pub error: Option<String>,
    }

    impl IpcResponse {
        pub fn ok(data: serde_json::Value) -> Self {
            Self { success: true, data, error: None }
        }
        pub fn err(error: String) -> Self {
            Self { success: false, data: serde_json::Value::Null, error: Some(error) }
        }
    }

    pub struct IpcHandler {
        client: Arc<Mutex<Option<Arc<Mutex<HighLevelClient>>>>>,
        runtime: Arc<tokio::runtime::Runtime>,
    }

    impl IpcHandler {
        pub fn new() -> Self {
            Self {
                client: Arc::new(Mutex::new(None)),
                runtime: Arc::new(tokio::runtime::Runtime::new().expect("Failed to create runtime")),
            }
        }

        pub async fn connect(&self) -> Result<()> {
            let config = AppConfig::load()?;
            let client_config = TelegramClientConfig {
                api_id: config.telegram.api_id,
                api_hash: config.telegram.api_hash.clone(),
                session_file: config.session_file(),
                download_dir: config.app.download_dir.clone(),
            };
            let client = TelegramClient::new(client_config).await?;
            let high_level = HighLevelClient::new(client).await;

            let authorized = high_level.get_me().await.is_ok();

            let mut guard = self.client.lock().await;
            *guard = Some(Arc::new(Mutex::new(high_level)));

            if !authorized {
                // Will need login flow
            }

            Ok(())
        }

        pub fn handle(&self, msg: IpcMessage) -> IpcResponse {
            let client = self.client.clone();
            let runtime = self.runtime.clone();

            runtime.block_on(async move {
                let client_guard = client.lock().await;
                let Some(client) = client_guard.as_ref() else {
                    return IpcResponse::err("Not connected".into());
                };
                let client = client.lock().await;

                match msg {
                    IpcMessage::RequestLoginCode { phone } => {
                        match client.request_login_code(&phone).await {
                            Ok(_token) => {
                                IpcResponse::ok(serde_json::json!({ "token": "stored" }))
                            }
                            Err(e) => IpcResponse::err(e.to_string()),
                        }
                    }
                    IpcMessage::SubmitLoginCode { code } => {
                        IpcResponse::err("Token not stored in this simplified version".into())
                    }
                    IpcMessage::GetMe => {
                        match client.get_me().await {
                            Ok(user) => IpcResponse::ok(serde_json::json!({ "user": user })),
                            Err(e) => IpcResponse::err(e.to_string()),
                        }
                    }
                    IpcMessage::GetDialogs => {
                        match client.get_dialogs().await {
                            Ok(dialogs) => IpcResponse::ok(serde_json::json!({ "dialogs": dialogs })),
                            Err(e) => IpcResponse::err(e.to_string()),
                        }
                    }
                    IpcMessage::GetContacts => {
                        match client.get_contacts().await {
                            Ok(contacts) => IpcResponse::ok(serde_json::json!({ "contacts": contacts })),
                            Err(e) => IpcResponse::err(e.to_string()),
                        }
                    }
                    IpcMessage::GetMessages { peer_id, limit } => {
                        match client.get_messages_by_full_id(peer_id, limit as usize).await {
                            Ok(messages) => IpcResponse::ok(serde_json::json!({ "messages": messages })),
                            Err(e) => IpcResponse::err(e.to_string()),
                        }
                    }
                    IpcMessage::SendMessage { peer_id, text } => {
                        match client.send_message_by_full_id(peer_id, &text).await {
                            Ok(msg) => IpcResponse::ok(serde_json::json!({ "message": msg })),
                            Err(e) => IpcResponse::err(e.to_string()),
                        }
                    }
                    IpcMessage::GetOrCreateDialog { user_id } => {
                        IpcResponse::err("Not implemented".into())
                    }
                }
            })
        }
    }
}

use ipc::{IpcHandler, IpcMessage, IpcResponse};

// ============================================================================
// HTML/CSS/JS for the Telegram Web UI
// ============================================================================

const HTML: &str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>telerust</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        :root {
            --bg-primary: #181a1f;
            --bg-secondary: #1f2228;
            --bg-tertiary: #282c34;
            --bg-hover: #2d323c;
            --border: #3a3f4a;
            --text-primary: #e8eaed;
            --text-secondary: #9aa0aa;
            --text-muted: #6b7280;
            --accent-blue: #009fe8;
            --accent-green: #2ecc71;
            --accent-red: #e74c3c;
            --accent-orange: #f39c12;
            --sidebar-width: 320px;
        }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            height: 100vh;
            width: 100vw;
            overflow: hidden;
            display: flex;
        }
        /* Login Screen */
        .login-screen {
            position: fixed; inset: 0; background: var(--bg-primary);
            display: flex; align-items: center; justify-content: center; z-index: 1000;
            flex-direction: column; gap: 24px;
        }
        .login-screen.hidden { display: none !important; }
        .login-box {
            background: var(--bg-secondary); border: 1px solid var(--border);
            border-radius: 16px; padding: 48px 40px; width: 100%; max-width: 400px;
        }
        .login-title { font-size: 28px; font-weight: 600; margin-bottom: 8px; text-align: center; }
        .login-subtitle { color: var(--text-secondary); text-align: center; margin-bottom: 32px; }
        .input-group { display: flex; flex-direction: column; gap: 8px; margin-bottom: 20px; }
        .input-label { font-size: 13px; color: var(--text-secondary); font-weight: 500; }
        .input-field {
            background: var(--bg-tertiary); border: 1px solid var(--border);
            border-radius: 10px; padding: 14px 16px; color: var(--text-primary);
            font-size: 15px; outline: none; transition: border-color 0.15s, box-shadow 0.15s;
        }
        .input-field:focus { border-color: var(--accent-blue); box-shadow: 0 0 0 3px rgba(0, 159, 232, 0.15); }
        .btn {
            background: var(--accent-blue); color: white; border: none;
            border-radius: 10px; padding: 14px 24px; font-size: 15px; font-weight: 600;
            cursor: pointer; transition: opacity 0.15s, transform 0.1s;
            width: 100%;
        }
        .btn:hover { opacity: 0.9; }
        .btn:active { transform: scale(0.98); }
        .btn:disabled { opacity: 0.5; cursor: not-allowed; }
        .btn-secondary {
            background: var(--bg-tertiary); color: var(--text-primary); border: 1px solid var(--border);
        }
        .error-msg { color: var(--accent-red); font-size: 13px; text-align: center; min-height: 20px; }
        .spinner { display: inline-block; width: 18px; height: 18px; border: 2px solid transparent; border-top-color: currentColor; border-radius: 50%; animation: spin 0.8s linear infinite; margin-right: 8px; vertical-align: middle; }
        @keyframes spin { to { transform: rotate(360deg); } }

        /* Main App Layout */
        .app { display: flex; width: 100%; height: 100%; }
        .sidebar {
            width: var(--sidebar-width); min-width: var(--sidebar-width);
            background: var(--bg-secondary); border-right: 1px solid var(--border);
            display: flex; flex-direction: column; overflow: hidden;
        }
        .sidebar-header {
            padding: 16px; border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 12px;
        }
        .avatar {
            width: 40px; height: 40px; border-radius: 50%; background: var(--accent-blue);
            display: flex; align-items: center; justify-content: center; font-weight: 600; font-size: 16px; color: white;
            overflow: hidden; flex-shrink: 0;
        }
        .avatar img { width: 100%; height: 100%; object-fit: cover; }
        .user-info { flex: 1; min-width: 0; }
        .user-name { font-weight: 600; font-size: 15px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
        .user-status { font-size: 12px; color: var(--text-secondary); }

        .search-box { padding: 12px; border-bottom: 1px solid var(--border); }
        .search-input {
            width: 100%; background: var(--bg-tertiary); border: 1px solid var(--border);
            border-radius: 10px; padding: 10px 14px; color: var(--text-primary); font-size: 14px;
            outline: none; transition: border-color 0.15s;
        }
        .search-input:focus { border-color: var(--accent-blue); }

        .chat-list { flex: 1; overflow-y: auto; padding: 8px 4px; }
        .chat-item {
            display: flex; align-items: center; gap: 12px; padding: 10px 12px;
            border-radius: 10px; cursor: pointer; transition: background 0.1s;
            position: relative;
        }
        .chat-item:hover { background: var(--bg-hover); }
        .chat-item.active { background: var(--bg-hover); }
        .chat-avatar {
            width: 48px; height: 48px; border-radius: 50%; background: var(--bg-tertiary);
            display: flex; align-items: center; justify-content: center; font-weight: 600; font-size: 18px; color: var(--text-secondary);
            overflow: hidden; flex-shrink: 0;
        }
        .chat-avatar img { width: 100%; height: 100%; object-fit: cover; }
        .chat-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
        .chat-header { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
        .chat-name { font-weight: 600; font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
        .chat-time { font-size: 11px; color: var(--text-muted); white-space: nowrap; flex-shrink: 0; }
        .chat-preview { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
        .chat-last-msg { font-size: 13px; color: var(--text-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; }
        .chat-unread {
            background: var(--accent-blue); color: white; font-size: 11px; font-weight: 600;
            min-width: 20px; height: 20px; border-radius: 10px; display: flex; align-items: center; justify-content: center; padding: 0 6px;
            flex-shrink: 0;
        }

        .main-area { flex: 1; display: flex; flex-direction: column; background: var(--bg-primary); overflow: hidden; }
        .empty-state { flex: 1; display: flex; align-items: center; justify-content: center; flex-direction: column; gap: 16px; color: var(--text-muted); }
        .empty-state svg { width: 80px; height: 80px; opacity: 0.5; }

        .chat-header-bar {
            padding: 12px 16px; border-bottom: 1px solid var(--border);
            background: var(--bg-secondary); display: flex; align-items: center; gap: 12px;
        }
        .chat-header-avatar { width: 40px; height: 40px; border-radius: 50%; background: var(--bg-tertiary); display: flex; align-items: center; justify-content: center; font-weight: 600; font-size: 16px; color: var(--text-secondary); overflow: hidden; }
        .chat-header-avatar img { width: 100%; height: 100%; object-fit: cover; }
        .chat-header-info { flex: 1; min-width: 0; }
        .chat-header-name { font-weight: 600; font-size: 15px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
        .chat-header-status { font-size: 12px; color: var(--text-secondary); }
        .chat-header-actions { display: flex; gap: 8px; }
        .icon-btn {
            background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 50%;
            width: 36px; height: 36px; display: flex; align-items: center; justify-content: center;
            cursor: pointer; color: var(--text-secondary); transition: background 0.1s, color 0.1s;
        }
        .icon-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
        .icon-btn svg { width: 20px; height: 20px; }

        .messages-area { flex: 1; overflow-y: auto; padding: 20px; display: flex; flex-direction: column; gap: 12px; }
        .message { display: flex; gap: 8px; max-width: 80%; animation: fadeIn 0.2s ease; }
        @keyframes fadeIn { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: translateY(0); } }
        .message.outgoing { align-self: flex-end; flex-direction: row-reverse; }
        .message-avatar { width: 32px; height: 32px; border-radius: 50%; background: var(--bg-tertiary); display: flex; align-items: center; justify-content: center; font-weight: 600; font-size: 12px; color: var(--text-secondary); flex-shrink: 0; overflow: hidden; margin-bottom: 4px; }
        .message-avatar img { width: 100%; height: 100%; object-fit: cover; }
        .message-bubble {
            background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 16px;
            padding: 10px 14px; position: relative; word-wrap: break-word;
        }
        .message.outgoing .message-bubble { background: var(--accent-blue); border-color: var(--accent-blue); border-bottom-right-radius: 4px; }
        .message.incoming .message-bubble { border-bottom-left-radius: 4px; }
        .message-sender { font-size: 12px; font-weight: 600; margin-bottom: 4px; color: var(--text-secondary); }
        .message.outgoing .message-sender { display: none; }
        .message-text { font-size: 14px; line-height: 1.5; white-space: pre-wrap; word-wrap: break-word; }
        .message-text a { color: var(--accent-blue); text-decoration: none; }
        .message-text a:hover { text-decoration: underline; }
        .message-time { font-size: 11px; color: var(--text-muted); margin-top: 4px; text-align: right; }
        .message.outgoing .message-time { color: rgba(255,255,255,0.7); }

        /* Media messages */
        .message-media { margin-top: 8px; border-radius: 12px; overflow: hidden; max-width: 100%; }
        .message-media img { max-width: 100%; height: auto; display: block; max-height: 400px; object-fit: contain; }
        .message-media video { max-width: 100%; max-height: 400px; display: block; }
        .message-media audio { width: 100%; max-width: 300px; display: block; margin-top: 8px; }
        .message-file { display: flex; align-items: center; gap: 12px; padding: 12px; background: var(--bg-tertiary); border-radius: 12px; }
        .message-file-icon { font-size: 32px; }
        .message-file-info { flex: 1; min-width: 0; }
        .message-file-name { font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
        .message-file-size { font-size: 12px; color: var(--text-muted); }

        .input-area { padding: 16px; border-top: 1px solid var(--border); background: var(--bg-secondary); }
        .input-wrapper { display: flex; align-items: flex-end; gap: 12px; max-width: 900px; margin: 0 auto; }
        .message-input {
            flex: 1; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 24px;
            padding: 12px 16px; color: var(--text-primary); font-size: 14px; line-height: 1.5;
            outline: none; resize: none; min-height: 48px; max-height: 150px; font-family: inherit;
            transition: border-color 0.15s;
        }
        .message-input:focus { border-color: var(--accent-blue); }
        .send-btn {
            background: var(--accent-blue); color: white; border: none; border-radius: 50%;
            width: 44px; height: 44px; display: flex; align-items: center; justify-content: center;
            cursor: pointer; transition: background 0.15s, transform 0.1s; flex-shrink: 0;
        }
        .send-btn:hover { background: #0088cc; }
        .send-btn:active { transform: scale(0.95); }
        .send-btn:disabled { opacity: 0.5; cursor: not-allowed; }
        .send-btn svg { width: 20px; height: 20px; }

        /* Scrollbar */
        ::-webkit-scrollbar { width: 8px; height: 8px; }
        ::-webkit-scrollbar-track { background: transparent; }
        ::-webkit-scrollbar-thumb { background: var(--border); border-radius: 4px; }
        ::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }

        /* New chat modal */
        .modal-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.6); display: none; align-items: center; justify-content: center; z-index: 100; }
        .modal-overlay.visible { display: flex; }
        .modal { background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 16px; padding: 24px; width: 100%; max-width: 480px; max-height: 80vh; overflow: auto; }
        .modal-title { font-size: 18px; font-weight: 600; margin-bottom: 16px; }
        .contact-item { display: flex; align-items: center; gap: 12px; padding: 12px; border-radius: 10px; cursor: pointer; transition: background 0.1s; }
        .contact-item:hover { background: var(--bg-hover); }
        .contact-avatar { width: 44px; height: 44px; border-radius: 50%; background: var(--bg-tertiary); display: flex; align-items: center; justify-content: center; font-weight: 600; font-size: 16px; color: var(--text-secondary); overflow: hidden; }
        .contact-avatar img { width: 100%; height: 100%; object-fit: cover; }
        .contact-name { font-weight: 500; }
        .contact-username { font-size: 12px; color: var(--text-muted); }
    </style>
</head>
<body>
    <!-- Login Screen -->
    <div class="login-screen" id="loginScreen">
        <div class="login-box">
            <div class="login-title">telerust</div>
            <div class="login-subtitle">Enter your phone number to continue</div>
            <div class="input-group">
                <label class="input-label">Phone Number</label>
                <input type="tel" class="input-field" id="phoneInput" placeholder="+1 555 123 4567" autocomplete="tel">
            </div>
            <button class="btn" id="requestCodeBtn"><span class="spinner" id="requestSpinner" style="display:none;"></span>Next</button>
            <div class="error-msg" id="loginError"></div>
        </div>
    </div>

    <!-- Code Verification Screen -->
    <div class="login-screen hidden" id="codeScreen">
        <div class="login-box">
            <div class="login-title">Enter Code</div>
            <div class="login-subtitle">We sent a code to <span id="codePhoneDisplay"></span></div>
            <div class="input-group">
                <label class="input-label">Code</label>
                <input type="text" class="input-field" id="codeInput" placeholder="12345" autocomplete="one-time-code" maxlength="5">
            </div>
            <button class="btn" id="submitCodeBtn"><span class="spinner" id="submitSpinner" style="display:none;"></span>Sign In</button>
            <button class="btn btn-secondary" style="margin-top:12px;" id="backToPhoneBtn">Change Number</button>
            <div class="error-msg" id="codeError"></div>
        </div>
    </div>

    <!-- Main App -->
    <div class="app hidden" id="app">
        <aside class="sidebar">
            <div class="sidebar-header">
                <div class="avatar" id="myAvatar">?</div>
                <div class="user-info">
                    <div class="user-name" id="myName">Loading...</div>
                    <div class="user-status" id="myStatus">Connecting...</div>
                </div>
            </div>
            <div class="search-box">
                <input type="text" class="search-input" id="searchInput" placeholder="Search messages...">
            </div>
            <div class="chat-list" id="chatList"></div>
        </aside>
        <main class="main-area">
            <div class="empty-state" id="emptyState">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"></path></svg>
                <div>Select a chat to start messaging</div>
            </div>
            <div class="chat-view hidden" id="chatView">
                <div class="chat-header-bar">
                    <div class="chat-header-avatar" id="chatAvatar">?</div>
                    <div class="chat-header-info">
                        <div class="chat-header-name" id="chatName">Chat</div>
                        <div class="chat-header-status" id="chatStatus"></div>
                    </div>
                    <div class="chat-header-actions">
                        <button class="icon-btn" title="Voice Call"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"></path></svg></button>
                        <button class="icon-btn" title="Video Call"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="23 7 16 12 23 17 23 7"></polygon><rect x="1" y="5" width="15" height="14" rx="2" ry="2"></rect></svg></button>
                        <button class="icon-btn" title="More"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="1"></circle><circle cx="19" cy="12" r="1"></circle><circle cx="5" cy="12" r="1"></circle></svg></button>
                    </div>
                </div>
                <div class="messages-area" id="messagesArea"></div>
                <div class="input-area">
                    <div class="input-wrapper">
                        <textarea class="message-input" id="messageInput" placeholder="Message" rows="1"></textarea>
                        <button class="send-btn" id="sendBtn" disabled><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="22" y1="2" x2="11" y2="13"></line><polygon points="22 2 15 22 11 13 2 9 22 2"></polygon></svg></button>
                    </div>
                </div>
            </div>
        </main>
    </div>

    <!-- New Chat Modal -->
    <div class="modal-overlay" id="newChatModal">
        <div class="modal">
            <div class="modal-title">New Message</div>
            <div class="search-box" style="border:none; padding:0 0 16px;">
                <input type="text" class="search-input" id="contactSearch" placeholder="Search contacts...">
            </div>
            <div id="contactList"></div>
        </div>
    </div>

    <script>
        // ============================================================================
        // State & IPC
        // ============================================================================
        let currentChatId = null;
        let currentChatPeerId = null;
        let dialogs = [];
        let contacts = [];
        let myInfo = null;

        function ipc(action, payload = {}) {
            return window.ipc.postMessage({ action, payload });
        }

        // ============================================================================
        // UI Helpers
        // ============================================================================
        function showScreen(id) {
            document.querySelectorAll('.login-screen, #app').forEach(el => el.classList.add('hidden'));
            document.getElementById(id)?.classList.remove('hidden');
        }

        function setBtnLoading(btnId, loading, text) {
            const btn = document.getElementById(btnId);
            const spinner = btn.querySelector('.spinner');
            if (loading) {
                btn.disabled = true;
                spinner.style.display = 'inline-block';
                btn.childNodes.forEach(n => { if (n.nodeType === 3) n.textContent = ' ' + text; });
            } else {
                btn.disabled = false;
                spinner.style.display = 'none';
                btn.childNodes.forEach(n => { if (n.nodeType === 3) n.textContent = ' ' + text; });
            }
        }

        function showError(id, msg) {
            const el = document.getElementById(id);
            if (el) { el.textContent = msg; setTimeout(() => el.textContent = '', 5000); }
        }

        function escapeHtml(text) {
            const div = document.createElement('div');
            div.textContent = text;
            return div.innerHTML;
        }

        function linkify(text) {
            const urlRegex = /(https?:\/\/[^\s]+)/g;
            return escapeHtml(text).replace(urlRegex, '<a href="$1" target="_blank" rel="noopener">$1</a>');
        }

        function formatTime(date) {
            const now = new Date();
            const d = new Date(date);
            if (d.toDateString() === now.toDateString()) {
                return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            } else if (d.getFullYear() === now.getFullYear()) {
                return d.toLocaleDateString([], { month: 'short', day: 'numeric' }) + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            } else {
                return d.toLocaleDateString([], { month: 'short', day: 'numeric', year: 'numeric' });
            }
        }

        function getInitials(name) {
            return name.split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2);
        }

        function getColorFromString(str) {
            let hash = 0;
            for (let i = 0; i < str.length; i++) hash = str.charCodeAt(i) + ((hash << 5) - hash);
            const hue = Math.abs(hash) % 360;
            return `hsl(${hue}, 65%, 45%)`;
        }

        // ============================================================================
        // Login Flow
        // ============================================================================
        document.getElementById('requestCodeBtn').addEventListener('click', async () => {
            const phone = document.getElementById('phoneInput').value.trim();
            if (!phone) return showError('loginError', 'Enter a phone number');
            setBtnLoading('requestCodeBtn', true, 'Sending...');
            try {
                const res = await ipc('request_login_code', { phone });
                if (res.success) {
                    document.getElementById('codePhoneDisplay').textContent = phone;
                    showScreen('codeScreen');
                    document.getElementById('codeInput').focus();
                } else {
                    showError('loginError', res.error || 'Failed to send code');
                }
            } catch (e) {
                showError('loginError', e.message || 'Network error');
            }
            setBtnLoading('requestCodeBtn', false, 'Next');
        });

        document.getElementById('submitCodeBtn').addEventListener('click', async () => {
            const code = document.getElementById('codeInput').value.trim();
            if (!code || code.length < 5) return showError('codeError', 'Enter the 5-digit code');
            setBtnLoading('submitCodeBtn', true, 'Signing in...');
            try {
                const res = await ipc('submit_login_code', { code });
                if (res.success) {
                    await initApp();
                } else {
                    showError('codeError', res.error || 'Invalid code');
                }
            } catch (e) {
                showError('codeError', e.message || 'Network error');
            }
            setBtnLoading('submitCodeBtn', false, 'Sign In');
        });

        document.getElementById('backToPhoneBtn').addEventListener('click', () => {
            showScreen('loginScreen');
            document.getElementById('phoneInput').focus();
        });

        document.getElementById('phoneInput').addEventListener('keypress', e => { if (e.key === 'Enter') document.getElementById('requestCodeBtn').click(); });
        document.getElementById('codeInput').addEventListener('keypress', e => { if (e.key === 'Enter') document.getElementById('submitCodeBtn').click(); });

        // ============================================================================
        // App Initialization
        // ============================================================================
        async function initApp() {
            try {
                const [meRes, dialogsRes, contactsRes] = await Promise.all([
                    ipc('get_me', {}),
                    ipc('get_dialogs', {}),
                    ipc('get_contacts', {})
                ]);
                myInfo = meRes.user;
                dialogs = dialogsRes.dialogs || [];
                contacts = contactsRes.contacts || [];
                renderMyInfo();
                renderChatList();
                showScreen('app');
            } catch (e) {
                console.error('Init failed:', e);
                showError('loginError', 'Failed to initialize app');
            }
        }

        function renderMyInfo() {
            if (!myInfo) return;
            const name = myInfo.first_name + (myInfo.last_name ? ' ' + myInfo.last_name : '');
            document.getElementById('myName').textContent = name || 'Unknown';
            document.getElementById('myStatus').textContent = 'Online';
            const avatar = document.getElementById('myAvatar');
            if (myInfo.photo?.photo_small) {
                avatar.innerHTML = `<img src="${myInfo.photo.photo_small}" alt="">`;
            } else {
                avatar.textContent = getInitials(name) || '?';
                avatar.style.background = getColorFromString(name || '?');
            }
        }

        // ============================================================================
        // Chat List
        // ============================================================================
        function renderChatList(filter = '') {
            const list = document.getElementById('chatList');
            const filtered = dialogs.filter(d =>
                d.title.toLowerCase().includes(filter.toLowerCase()) ||
                (d.username && d.username.toLowerCase().includes(filter.toLowerCase()))
            );
            list.innerHTML = filtered.map(chat => {
                const initials = getInitials(chat.title);
                const color = getColorFromString(chat.title);
                const time = chat.last_message_date ? formatTime(chat.last_message_date * 1000) : '';
                const preview = chat.last_message ? escapeHtml(chat.last_message).slice(0, 50) : '';
                const unread = chat.unread_count > 0 ? `<span class="chat-unread">${chat.unread_count > 99 ? '99+' : chat.unread_count}</span>` : '';
                const avatarHtml = chat.photo?.photo_small
                    ? `<img src="${chat.photo.photo_small}" alt="">`
                    : initials;
                const activeClass = currentChatId === chat.id ? ' active' : '';
                return `
                    <div class="chat-item${activeClass}" data-chat-id="${chat.id}" data-peer-id="${chat.peer_id}">
                        <div class="chat-avatar" style="background:${color}">${avatarHtml}</div>
                        <div class="chat-info">
                            <div class="chat-header">
                                <span class="chat-name">${escapeHtml(chat.title)}</span>
                                <span class="chat-time">${time}</span>
                            </div>
                            <div class="chat-preview">
                                <span class="chat-last-msg">${preview}</span>
                                ${unread}
                            </div>
                        </div>
                    </div>
                `;
            }).join('');

            list.querySelectorAll('.chat-item').forEach(item => {
                item.addEventListener('click', () => selectChat(item.dataset.chatId, item.dataset.peerId));
            });
        }

        document.getElementById('searchInput').addEventListener('input', e => renderChatList(e.target.value));

        // ============================================================================
        // Chat Selection
        // ============================================================================
        async function selectChat(chatId, peerId) {
            currentChatId = chatId;
            currentChatPeerId = peerId;
            document.querySelectorAll('.chat-item').forEach(el => el.classList.toggle('active', el.dataset.chatId === chatId));
            document.getElementById('emptyState').classList.add('hidden');
            document.getElementById('chatView').classList.remove('hidden');
            renderChatHeader(chatId);
            await loadMessages(peerId);
        }

        function renderChatHeader(chatId) {
            const chat = dialogs.find(d => d.id == chatId);
            if (!chat) return;
            document.getElementById('chatName').textContent = chat.title;
            document.getElementById('chatStatus').textContent = chat.type === 'channel' ? 'Channel' : chat.type === 'group' ? 'Group' : '';
            const avatar = document.getElementById('chatAvatar');
            const initials = getInitials(chat.title);
            const color = getColorFromString(chat.title);
            if (chat.photo?.photo_small) {
                avatar.innerHTML = `<img src="${chat.photo.photo_small}" alt="">`;
            } else {
                avatar.textContent = initials;
                avatar.style.background = color;
            }
        }

        // ============================================================================
        // Messages
        // ============================================================================
        async function loadMessages(peerId) {
            try {
                const res = await ipc('get_messages', { peer_id: parseInt(peerId), limit: 100 });
                renderMessages(res.messages || []);
            } catch (e) {
                console.error('Load messages failed:', e);
            }
        }

        function renderMessages(messages) {
            const area = document.getElementById('messagesArea');
            messages.reverse().forEach(msg => {
                const el = createMessageElement(msg);
                area.appendChild(el);
            });
            scrollToBottom();
        }

        function createMessageElement(msg) {
            const div = document.createElement('div');
            const isOutgoing = msg.outgoing;
            div.className = `message ${isOutgoing ? 'outgoing' : 'incoming'}`;

            const sender = msg.sender_id ? dialogs.find(d => d.id == msg.sender_id) || contacts.find(c => c.id == msg.sender_id) : null;
            const senderName = sender ? (sender.first_name + (sender.last_name ? ' ' + sender.last_name : '')) : 'Unknown';
            const senderInitials = getInitials(senderName);
            const senderColor = getColorFromString(senderName);
            const time = msg.date ? formatTime(msg.date * 1000) : '';
            const avatarHtml = sender?.photo?.photo_small
                ? `<img src="${sender.photo.photo_small}" alt="">`
                : senderInitials;

            let contentHtml = '';
            if (msg.media) {
                if (msg.media.type === 'photo' && msg.media.photo_url) {
                    contentHtml += `<div class="message-media"><img src="${msg.media.photo_url}" alt="Photo" loading="lazy"></div>`;
                } else if (msg.media.type === 'video' && msg.media.video_url) {
                    contentHtml += `<div class="message-media"><video src="${msg.media.video_url}" controls></video></div>`;
                } else if (msg.media.type === 'audio' && msg.media.audio_url) {
                    contentHtml += `<div class="message-media"><audio src="${msg.media.audio_url}" controls></audio></div>`;
                } else if (msg.media.type === 'document') {
                    contentHtml += `
                        <div class="message-file">
                            <span class="message-file-icon">📄</span>
                            <div class="message-file-info">
                                <div class="message-file-name">${escapeHtml(msg.media.file_name || 'File')}</div>
                                <div class="message-file-size">${formatBytes(msg.media.file_size || 0)}</div>
                            </div>
                        </div>
                    `;
                }
            }
            if (msg.text) {
                contentHtml += `<div class="message-text">${linkify(msg.text)}</div>`;
            }

            div.innerHTML = `
                ${!isOutgoing ? `<div class="message-avatar" style="background:${senderColor}">${avatarHtml}</div>` : ''}
                <div class="message-bubble">
                    ${!isOutgoing ? `<div class="message-sender">${escapeHtml(senderName)}</div>` : ''}
                    ${contentHtml}
                    <div class="message-time">${time}</div>
                </div>
                ${isOutgoing ? `<div class="message-avatar" style="background:${getColorFromString(myInfo?.first_name || 'me')}">${getInitials(myInfo?.first_name || 'me')}</div>` : ''}
            `;
            return div;
        }

        function formatBytes(bytes) {
            if (bytes < 1024) return bytes + ' B';
            if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
            return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
        }

        function scrollToBottom() {
            const area = document.getElementById('messagesArea');
            area.scrollTop = area.scrollHeight;
        }

        // Send message
        document.getElementById('sendBtn').addEventListener('click', sendMessage);
        document.getElementById('messageInput').addEventListener('keydown', e => {
            if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
        });
        document.getElementById('messageInput').addEventListener('input', function() {
            this.style.height = 'auto';
            this.style.height = Math.min(this.scrollHeight, 150) + 'px';
            document.getElementById('sendBtn').disabled = !this.value.trim() || !currentChatPeerId;
        });

        async function sendMessage() {
            const input = document.getElementById('messageInput');
            const text = input.value.trim();
            if (!text || !currentChatPeerId) return;
            input.value = '';
            input.style.height = 'auto';
            document.getElementById('sendBtn').disabled = true;

            try {
                await ipc('send_message', { peer_id: parseInt(currentChatPeerId), text });
                await loadMessages(currentChatPeerId);
            } catch (e) {
                console.error('Send failed:', e);
            }
        }

        // ============================================================================
        // New Chat Modal
        // ============================================================================
        document.getElementById('myAvatar')?.addEventListener('click', openNewChatModal);

        function openNewChatModal() {
            renderContacts('');
            document.getElementById('newChatModal').classList.add('visible');
            document.getElementById('contactSearch').focus();
        }

        function closeNewChatModal() {
            document.getElementById('newChatModal').classList.remove('visible');
        }

        document.getElementById('newChatModal').addEventListener('click', e => {
            if (e.target === e.currentTarget) closeNewChatModal();
        });

        document.getElementById('contactSearch').addEventListener('input', e => renderContacts(e.target.value));

        function renderContacts(filter) {
            const list = document.getElementById('contactList');
            const filtered = contacts.filter(c =>
                `${c.first_name} ${c.last_name || ''}`.toLowerCase().includes(filter.toLowerCase()) ||
                (c.username && c.username.toLowerCase().includes(filter.toLowerCase()))
            );
            list.innerHTML = filtered.map(c => {
                const name = `${c.first_name} ${c.last_name || ''}`.trim();
                const initials = getInitials(name);
                const color = getColorFromString(name);
                const avatarHtml = c.photo?.photo_small
                    ? `<img src="${c.photo.photo_small}" alt="">`
                    : initials;
                return `
                    <div class="contact-item" data-user-id="${c.id}">
                        <div class="contact-avatar" style="background:${color}">${avatarHtml}</div>
                        <div>
                            <div class="contact-name">${escapeHtml(name)}</div>
                            ${c.username ? `<div class="contact-username">@${escapeHtml(c.username)}</div>` : ''}
                        </div>
                    </div>
                `;
            }).join('');
            list.querySelectorAll('.contact-item').forEach(item => {
                item.addEventListener('click', () => {
                    const userId = item.dataset.userId;
                    closeNewChatModal();
                    openChatWithUser(userId);
                });
            });
        }

        async function openChatWithUser(userId) {
            try {
                const res = await ipc('get_or_create_dialog', { user_id: parseInt(userId) });
                if (res.dialog) {
                    selectChat(res.dialog.id, res.dialog.peer_id);
                }
            } catch (e) {
                console.error('Open chat failed:', e);
            }
        }

        // ============================================================================
        // IPC Response Handler (for async responses)
        // ============================================================================
        window.handleIpcResponse = function(response) {
            // Handle async responses if needed
            console.log('IPC Response:', response);
        };

        // Start by showing login screen
        showScreen('loginScreen');
        document.getElementById('phoneInput').focus();
    </script>
</body>
</html>
"#;

// ============================================================================
// Main Application
// ============================================================================

fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Create event loop
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("telerust")
        .with_inner_size(tao::dpi::LogicalSize::new(1280, 800))
        .with_min_inner_size(tao::dpi::LogicalSize::new(900, 600))
        .build(&event_loop)?;

    // Create IPC handler
    let ipc_handler = Arc::new(Mutex::new(IpcHandler::new()));

    // Channel for sending responses back to JavaScript
    let (response_tx, mut response_rx) = mpsc::unbounded_channel::<String>();

    // Connect to Telegram in background
    let handler_clone = ipc_handler.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            if let Err(e) = handler_clone.lock().await.connect().await {
                eprintln!("Failed to connect: {}", e);
            }
        });
    });

    // Build webview - wry 0.38 API requires window handle
    let ipc_handler_clone = ipc_handler.clone();
    let response_tx_clone = response_tx.clone();
    let webview = WebViewBuilder::new(&window)
        .with_html(HTML)
        .with_ipc_handler(move |request: Request<String>| {
            let handler = ipc_handler_clone.clone();
            let response_tx = response_tx_clone.clone();
            let body = request.body();
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(body) {
                let response = handler.blocking_lock().handle(msg);
                let response_json = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());
                // Send response to channel for event loop to process
                let _ = response_tx.send(response_json);
            }
        })
        .build()?;

    // Run event loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Process IPC responses
        while let Ok(response_json) = response_rx.try_recv() {
            let script = format!("window.handleIpcResponse({})", response_json);
            let _ = webview.evaluate_script(&script);
        }

        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}