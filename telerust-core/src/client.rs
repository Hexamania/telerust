//! High-level client wrapper

use anyhow::Result;
use grammers_client::{update::Update, peer::Peer, client::LoginToken};
use grammers_session::{Session, types::{PeerId, PeerKind}};
use grammers_session::storages::SqliteSession;
use grammers_tl_types as tl;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info};

use super::{Chat as AppChat, Message as AppMessage, TelegramClient, types::User as AppUser};

/// Convert PeerId to InputPeer for API calls using session to get access_hash
async fn peer_id_to_input_peer(session: &Arc<SqliteSession>, peer_id: PeerId) -> Result<tl::enums::InputPeer> {
    // Get PeerInfo from session to retrieve access_hash
    let peer_info = session.peer(peer_id).await?
        .ok_or_else(|| anyhow::anyhow!("Peer not found in session: {}", peer_id.bot_api_dialog_id_unchecked()))?;

    let access_hash = peer_info.auth().map(|a| a.hash()); // PeerAuth has hash() method

    match peer_id.kind() {
        PeerKind::User => Ok(tl::enums::InputPeer::User(tl::types::InputPeerUser {
            user_id: peer_id.bare_id_unchecked(),
            access_hash: access_hash.ok_or_else(|| anyhow::anyhow!("Missing access_hash for user"))?,
        })),
        PeerKind::Chat => Ok(tl::enums::InputPeer::Chat(tl::types::InputPeerChat {
            chat_id: peer_id.bare_id_unchecked(),
        })),
        PeerKind::Channel => Ok(tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
            channel_id: peer_id.bare_id_unchecked(),
            access_hash: access_hash.ok_or_else(|| anyhow::anyhow!("Missing access_hash for channel"))?,
        })),
    }
}

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

        // Handle errors gracefully - don't stop iteration on first error
        loop {
            match iter.next().await {
                Ok(Some(dialog)) => {
                    let chat = dialog.peer().clone();
                    dialogs.push(chat.into());
                }
                Ok(None) => break, // End of iteration
                Err(e) => {
                    tracing::warn!("Error fetching dialog, continuing: {}", e);
                    // Continue to next dialog instead of stopping
                }
            }
        }

        Ok(dialogs)
    }

    /// Get messages from a chat (using bare ID - deprecated, use get_messages_by_full_id)
    pub async fn get_messages(&self, chat_id: i64, limit: usize) -> Result<Vec<AppMessage>> {
        let client = self.inner.lock().await;
        let mut messages = Vec::new();

        // Try to reconstruct PeerId from bare ID - this is ambiguous without type info
        // Try as user first, then chat, then channel
        let peer_id = PeerId::user(chat_id)
            .or_else(|| PeerId::chat(chat_id))
            .or_else(|| PeerId::channel(chat_id))
            .ok_or_else(|| anyhow::anyhow!("Invalid peer ID: {}", chat_id))?;
        let peer_ref = peer_id.to_ambient_ref();

        let mut iter = client.client().iter_messages(peer_ref);
        let mut count = 0;

        // Handle errors gracefully
        loop {
            match iter.next().await {
                Ok(Some(m)) => {
                    if count >= limit {
                        break;
                    }
                    messages.push(m.into());
                    count += 1;
                }
                Ok(None) => break, // End of iteration
                Err(e) => {
                    tracing::warn!("Error fetching message, continuing: {}", e);
                    // Continue to next message instead of stopping
                }
            }
        }

        Ok(messages)
    }

    /// Send a message (using bare ID - deprecated, use send_message_by_full_id)
    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<AppMessage> {
        let client = self.inner.lock().await;

        let peer_id = PeerId::user(chat_id)
            .or_else(|| PeerId::chat(chat_id))
            .or_else(|| PeerId::channel(chat_id))
            .ok_or_else(|| anyhow::anyhow!("Invalid peer ID: {}", chat_id))?;
        let peer_ref = peer_id.to_ambient_ref();

        let sent: grammers_client::message::Message = client.client().send_message(peer_ref, text).await?;
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

    /// Get messages from a chat using full peer ID (bot_api_dialog_id)
    pub async fn get_messages_by_full_id(&self, full_peer_id: i64, limit: usize) -> Result<Vec<AppMessage>> {
        let mut client = self.inner.lock().await;
        let mut messages = Vec::new();

        // Reconstruct PeerId from the stored bot_api_dialog_id, then convert to InputPeer
        // using the session to get the access_hash
        let peer_id = PeerId::from_bot_api_dialog_id(full_peer_id)
            .ok_or_else(|| anyhow::anyhow!("Invalid peer ID: {}", full_peer_id))?;

        let session = client.session().clone();
        let input_peer = peer_id_to_input_peer(&session, peer_id).await?;

        let mut iter = client.client().iter_messages(input_peer);
        let mut count = 0;

        loop {
            match iter.next().await {
                Ok(Some(m)) => {
                    if count >= limit {
                        break;
                    }
                    messages.push(m.into());
                    count += 1;
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!("Error fetching message, continuing: {}", e);
                }
            }
        }

        Ok(messages)
    }

    /// Send a message using full peer ID (bot_api_dialog_id)
    pub async fn send_message_by_full_id(&self, full_peer_id: i64, text: &str) -> Result<AppMessage> {
        let mut client = self.inner.lock().await;

        let peer_id = PeerId::from_bot_api_dialog_id(full_peer_id)
            .ok_or_else(|| anyhow::anyhow!("Invalid peer ID: {}", full_peer_id))?;

        let session = client.session().clone();
        let input_peer = peer_id_to_input_peer(&session, peer_id).await?;

        let sent: grammers_client::message::Message = client.client().send_message(input_peer, text).await?;
        Ok(sent.into())
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