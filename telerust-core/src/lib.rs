//! telerust-core - Core Telegram client functionality

pub mod client;
pub mod config;
pub mod session;
pub mod handlers;
pub mod types;

use anyhow::Result;
use grammers_client::Client;
pub use grammers_client::client::{LoginToken, PasswordToken, UpdateStream};
use grammers_session::storages::SqliteSession;
use grammers_mtsender::{SenderPool, UpdatesConfiguration};
use std::path::PathBuf;
use std::sync::Arc;

pub use self::types::*;
pub use self::config::AppConfig;
pub use self::client::HighLevelClient;

/// Configuration for creating a TelegramClient
#[derive(Debug, Clone)]
pub struct TelegramClientConfig {
    pub api_id: i32,
    pub api_hash: String,
    pub session_file: PathBuf,
    pub download_dir: PathBuf,
}

/// High-level wrapper around grammers client with session management
pub struct TelegramClient {
    client: Client,
    pool_handle: grammers_mtsender::SenderPoolFatHandle,
    pool_runner: Option<tokio::task::JoinHandle<()>>,
    updates_stream: Option<UpdateStream>,
    config: TelegramClientConfig,
    session: Arc<SqliteSession>,
}

impl TelegramClient {
    /// Create a new Telegram client and connect
    pub async fn new(config: TelegramClientConfig) -> Result<Self> {
        let session = Arc::new(SqliteSession::open(&config.session_file).await?);

        let SenderPool {
            runner,
            handle,
            updates,
        } = SenderPool::new(Arc::clone(&session), config.api_id);

        let client = Client::new(handle.clone());
        let pool_runner = Some(tokio::spawn(runner.run()));

        let updates_stream = client
            .stream_updates(updates, UpdatesConfiguration { catch_up: true })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create update stream: {}", e))?;

        Ok(Self {
            client,
            pool_handle: handle,
            pool_runner,
            updates_stream: Some(updates_stream),
            config,
            session,
        })
    }

    /// Get the underlying grammers client
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Check if the client is authorized
    pub async fn is_authorized(&self) -> Result<bool> {
        Ok(self.client.is_authorized().await?)
    }

    /// Request login code for phone number
    pub async fn request_login_code(&self, phone: &str) -> Result<LoginToken> {
        Ok(self.client.request_login_code(phone, &self.config.api_hash).await?)
    }

    /// Sign in with the code
    pub async fn sign_in(&self, token: LoginToken, code: &str) -> Result<()> {
        self.client.sign_in(&token, code).await?;
        Ok(())
    }

    /// Check password for 2FA
    pub async fn check_password(&self, password_token: PasswordToken, password: &str) -> Result<()> {
        self.client.check_password(password_token, password).await?;
        Ok(())
    }

    /// Get the next update
    pub async fn next_update(&mut self) -> Option<Result<grammers_client::update::Update>> {
        if let Some(ref mut stream) = self.updates_stream {
            // stream.next() returns Result<Update, InvocationError>
            match stream.next().await {
                Ok(update) => Some(Ok(update)),
                Err(e) => Some(Err(anyhow::anyhow!("{}", e))),
            }
        } else {
            None
        }
    }

    /// Sync update state to session
    pub async fn sync_update_state(&mut self) -> Result<()> {
        if let Some(ref mut stream) = self.updates_stream {
            stream.sync_update_state().await.map_err(|e| anyhow::anyhow!("{}", e))?;
        }
        Ok(())
    }

    /// Gracefully shutdown
    pub async fn shutdown(mut self) -> Result<()> {
        self.sync_update_state().await?;
        self.pool_handle.quit();
        if let Some(handle) = self.pool_runner.take() {
            let _ = handle.await;
        }
        Ok(())
    }
}

impl Drop for TelegramClient {
    fn drop(&mut self) {
        // Note: We can't await in Drop, so the session might not be saved perfectly
        // Call shutdown() explicitly for clean shutdown
        let _ = self.pool_handle.quit();
    }
}