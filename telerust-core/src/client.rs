//! High-level client wrapper

use anyhow::Result;
use grammers_client::{update::Update, peer::Peer, client::LoginToken};
use grammers_session::types::{PeerId};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info};

use super::{Chat as AppChat, Message as AppMessage, TelegramClient, types::User as AppUser};

/// High-level client with convenient methods
pub struct HighLevelClient {
    inner: Arc<Mutex<TelegramClient>>,
}

impl HighLevelClient {
    /// Create a new high-level client
    pub async fn new(client: TelegramClient) -> Self {
        Self {
            inner: Arc::new(Mutex::new(client)),
        }
    }

    /// Get dialogs (chats list)
    pub async fn get_dialogs(&self) -> Result<Vec<AppChat>> {
        let client = self.inner.lock().await;
        let mut dialogs = Vec::new();

        let mut iter = client.client().iter_dialogs();

        while let Ok(Some(dialog)) = iter.next().await {
            let chat = dialog.peer().clone();
            dialogs.push(chat.into());
        }

        Ok(dialogs)
    }

    /// Get messages from a chat
    pub async fn get_messages(&self, chat_id: i64, limit: usize) -> Result<Vec<AppMessage>> {
        let client = self.inner.lock().await;
        let mut messages = Vec::new();

        // Convert i64 to PeerId and then to PeerRef
        let peer_ref = PeerId::user(chat_id)
            .or_else(|| PeerId::chat(chat_id))
            .or_else(|| PeerId::channel(chat_id))
            .map(|id| id.to_ambient_ref())
            .ok_or_else(|| anyhow::anyhow!("Invalid peer ID: {}", chat_id))?;

        let mut iter = client.client().iter_messages(peer_ref);
        let mut count = 0;

        while let Ok(Some(m)) = iter.next().await {
            if count >= limit {
                break;
            }
            messages.push(m.into());
            count += 1;
        }

        Ok(messages)
    }

    /// Send a message
    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<AppMessage> {
        let client = self.inner.lock().await;
        // Convert i64 to PeerId and then to PeerRef
        let peer_ref = PeerId::user(chat_id)
            .or_else(|| PeerId::chat(chat_id))
            .or_else(|| PeerId::channel(chat_id))
            .map(|id| id.to_ambient_ref())
            .ok_or_else(|| anyhow::anyhow!("Invalid peer ID: {}", chat_id))?;

        let sent = client.client().send_message(peer_ref, text).await?;
        Ok(sent.into())
    }

    /// Get current user info
    pub async fn get_me(&self) -> Result<AppUser> {
        let client = self.inner.lock().await;
        let me = client.client().get_me().await?;
        Ok(me.into())
    }

    /// Search for a user by username
    pub async fn resolve_username(&self, username: &str) -> Result<Option<AppUser>> {
        let client = self.inner.lock().await;
        match client.client().resolve_username(username).await? {
            Some(peer) => {
                if let Peer::User(user) = peer {
                    Ok(Some(user.into()))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Request login code
    pub async fn request_login_code(&self, phone: &str) -> Result<LoginToken> {
        let client = self.inner.lock().await;
        client.request_login_code(phone).await
    }

    /// Sign in with code
    pub async fn sign_in(&self, token: LoginToken, code: &str) -> Result<()> {
        let client = self.inner.lock().await;
        client.sign_in(token, code).await
    }

    /// Sync update state
    pub async fn sync_update_state(&self) -> Result<()> {
        let mut client = self.inner.lock().await;
        client.sync_update_state().await
    }

    /// Start listening for updates
    pub async fn run_updates<F>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(Update) + Send + 'static,
    {
        let mut client = self.inner.lock().await;
        info!("Starting update loop...");

        while let Some(update) = client.next_update().await {
            match update {
                Ok(update) => {
                    handler(update);
                }
                Err(e) => {
                    tracing::error!("Update error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Get the inner TelegramClient for direct access
    pub async fn with_client<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&TelegramClient) -> R,
    {
        let client = self.inner.lock().await;
        f(&client)
    }

    /// Get mutable access to the inner TelegramClient (for async operations)
    pub async fn with_client_mut<F, Fut, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut TelegramClient) -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        let mut client = self.inner.lock().await;
        f(&mut client).await
    }
}