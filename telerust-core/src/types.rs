//! Common types used across the application

use grammers_client::peer::{User as GrammersUser, Peer};
use grammers_client::message::Message as GrammersMessage;
use serde::{Deserialize, Serialize};

/// Chat type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatType {
    Private,
    Group,
    Supergroup,
    Channel,
    Community,
}

/// Simplified chat representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub id: i64,  // Bare ID (for display/comparison)
    pub peer_id: i64,  // Full PeerId with type bits (bot_api_dialog_id)
    pub title: String,
    pub username: Option<String>,
    pub chat_type: ChatType,
    pub unread_count: i32,
    pub last_message: Option<String>,
    pub last_message_date: Option<i64>,
    pub is_muted: bool,
    pub is_pinned: bool,
}

/// Simplified message representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i32,
    pub chat_id: i64,
    pub sender_id: Option<i64>,
    pub text: String,
    pub date: i64,
    pub is_outgoing: bool,
    pub has_media: bool,
    pub media_type: Option<String>,
    pub reply_to: Option<i32>,
    pub edit_date: Option<i64>,
}

/// User representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: Option<String>,
    pub is_bot: bool,
    pub is_verified: bool,
}

/// Convert grammers chat to our Chat type
impl From<Peer> for Chat {
    fn from(peer: Peer) -> Self {
        let (chat_type, bare_id, peer_id) = match &peer {
            Peer::Channel(c) => {
                // Broadcast channel
                let chat_type = ChatType::Channel;
                (chat_type, c.id().bare_id_unchecked(), c.id().bot_api_dialog_id_unchecked())
            }
            Peer::Group(g) => {
                // Group can be small group or megagroup (supergroup)
                let chat_type = if g.is_megagroup() {
                    ChatType::Supergroup
                } else {
                    ChatType::Group
                };
                (chat_type, g.id().bare_id_unchecked(), g.id().bot_api_dialog_id_unchecked())
            }
            Peer::User(u) => (ChatType::Private, u.id().bare_id_unchecked(), u.id().bot_api_dialog_id_unchecked()),
            Peer::Community(c) => (ChatType::Community, c.id().bare_id_unchecked(), c.id().bot_api_dialog_id_unchecked()),
        };

        Self {
            id: bare_id,
            peer_id,
            title: peer.name().unwrap_or_default().to_string(),
            username: peer.username().map(|s| s.to_string()),
            chat_type,
            unread_count: 0,
            last_message: None,
            last_message_date: None,
            is_muted: false,
            is_pinned: false,
        }
    }
}

/// Convert grammers message to our Message type
impl From<GrammersMessage> for Message {
    fn from(msg: GrammersMessage) -> Self {
        let chat_id = msg.peer_id().bare_id_unchecked();
        let sender_id = msg.sender_id().map(|id| id.bare_id_unchecked());
        let has_media = msg.media().is_some();
        let media_type = msg.media().map(|m| format!("{:?}", m).split('(').next().unwrap_or("Media").to_string());

        Self {
            id: msg.id(),
            chat_id,
            sender_id,
            text: msg.text().to_string(),
            date: msg.date().as_second(),
            is_outgoing: msg.outgoing(),
            has_media,
            media_type,
            reply_to: None,
            edit_date: None,
        }
    }
}

/// Convert grammers user to our User type
impl From<GrammersUser> for User {
    fn from(user: GrammersUser) -> Self {
        Self {
            id: user.id().bare_id_unchecked(),
            username: user.username().map(|s| s.to_string()),
            first_name: user.first_name().unwrap_or_default().to_string(),
            last_name: user.last_name().map(|s| s.to_string()),
            is_bot: user.is_bot(),
            is_verified: user.verified(),
        }
    }
}