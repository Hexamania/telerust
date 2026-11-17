//! telerust-gui-tauri - Telegram Client using Tauri 2
//! Modern web-based UI with native Rust backend

use anyhow::Result;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use telerust_core::{
    config::AppConfig, HighLevelClient, TelegramClient, TelegramClientConfig, Chat, Message, User, LoginToken,
};

// ============================================================================
// State
// ============================================================================

struct AppState {
    client: Arc<Mutex<Option<Arc<Mutex<HighLevelClient>>>>>,
    login_token: Arc<Mutex<Option<LoginToken>>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl AppState {
    fn new() -> Result<Self> {
        Ok(Self {
            client: Arc::new(Mutex::new(None)),
            login_token: Arc::new(Mutex::new(None)),
            runtime: Arc::new(tokio::runtime::Runtime::new()?),
        })
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
async fn connect(state: State<'_, AppState>) -> Result<(), String> {
    let config = AppConfig::load().map_err(|e| e.to_string())?;
    let client_config = TelegramClientConfig {
        api_id: config.telegram.api_id,
        api_hash: config.telegram.api_hash.clone(),
        session_file: config.session_file(),
        download_dir: config.app.download_dir.clone(),
    };

    let client = TelegramClient::new(client_config).await.map_err(|e| e.to_string())?;
    let high_level = HighLevelClient::new(client).await;

    let authorized = high_level.get_me().await.is_ok();

    let mut guard = state.client.lock().await;
    *guard = Some(Arc::new(Mutex::new(high_level)));

    if !authorized {
        return Err("Not authorized".to_string());
    }

    Ok(())
}

#[tauri::command]
async fn request_login_code(state: State<'_, AppState>, phone: String) -> Result<(), String> {
    let client_guard = state.client.lock().await;
    let Some(client) = client_guard.as_ref() else {
        return Err("Not connected".to_string());
    };
    let client = client.lock().await;

    let token = client.request_login_code(&phone).await.map_err(|e| e.to_string())?;

    let mut token_guard = state.login_token.lock().await;
    *token_guard = Some(token);

    Ok(())
}

#[tauri::command]
async fn submit_login_code(state: State<'_, AppState>, code: String) -> Result<(), String> {
    let client_guard = state.client.lock().await;
    let Some(client) = client_guard.as_ref() else {
        return Err("Not connected".to_string());
    };
    let client = client.lock().await;

    let mut token_guard = state.login_token.lock().await;
    let Some(token) = token_guard.take() else {
        return Err("No login token available".to_string());
    };

    client.sign_in(token, &code).await.map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn get_me(state: State<'_, AppState>) -> Result<User, String> {
    let client_guard = state.client.lock().await;
    let Some(client) = client_guard.as_ref() else {
        return Err("Not connected".to_string());
    };
    let client = client.lock().await;

    client.get_me().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_dialogs(state: State<'_, AppState>) -> Result<Vec<Chat>, String> {
    let client_guard = state.client.lock().await;
    let Some(client) = client_guard.as_ref() else {
        return Err("Not connected".to_string());
    };
    let client = client.lock().await;

    client.get_dialogs().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_contacts(state: State<'_, AppState>) -> Result<Vec<telerust_core::types::Contact>, String> {
    let client_guard = state.client.lock().await;
    let Some(client) = client_guard.as_ref() else {
        return Err("Not connected".to_string());
    };
    let client = client.lock().await;

    client.get_contacts().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_messages(state: State<'_, AppState>, peer_id: i64, limit: usize) -> Result<Vec<Message>, String> {
    let client_guard = state.client.lock().await;
    let Some(client) = client_guard.as_ref() else {
        return Err("Not connected".to_string());
    };
    let client = client.lock().await;

    client.get_messages_by_full_id(peer_id, limit).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_message(state: State<'_, AppState>, peer_id: i64, text: String) -> Result<Message, String> {
    let client_guard = state.client.lock().await;
    let Some(client) = client_guard.as_ref() else {
        return Err("Not connected".to_string());
    };
    let client = client.lock().await;

    client.send_message_by_full_id(peer_id, &text).await.map_err(|e| e.to_string())
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = AppState::new().expect("Failed to create app state");
            app.manage(state);

            // Auto-connect on startup
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    if let Err(e) = connect(state).await {
                        eprintln!("Auto-connect failed: {}", e);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            request_login_code,
            submit_login_code,
            get_me,
            get_dialogs,
            get_contacts,
            get_messages,
            send_message,
        ])
        .run(tauri::generate_context!())
        .expect("Error running tauri application");
}