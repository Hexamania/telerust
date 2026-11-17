//! Common types used across the application

use grammers_client::peer::{Channel, Group, Community, User as GrammersUser, Peer};
use grammers_client::message::Message as GrammersMessage;
use serde::{Deserialize, Serialize};

/// Simplified chat representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub id: i64,
    pub title: String,
    pub username: Option<String>,
    pub is_channel: bool,
    pub is_group: bool,
    pub unread_count: i32,
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
}

/// User representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: Option<String>,
    pub is_bot: bool,
}

/// Convert grammers chat to our Chat type
impl From<Peer> for Chat {
    fn from(peer: Peer) -> Self {
        // Use references to avoid moving the peer
        let (is_channel, is_group, peer_id) = match &peer {
            Peer::Channel(c) => (true, false, c.id()),
            Peer::Group(g) => (false, true, g.id()),
            Peer::User(u) => (false, false, u.id()),
            Peer::Community(c) => (false, true, c.id()),
        };

        Self {
            id: peer_id.bare_id_unchecked(),
            title: peer.name().unwrap_or_default().to_string(),
            username: peer.username().map(|s| s.to_string()),
            is_channel,
            is_group,
            unread_count: 0, // Would need to fetch separately
        }
    }
}

/// Convert grammers message to our Message type
impl From<GrammersMessage> for Message {
    fn from(msg: GrammersMessage) -> Self {
        let chat_id = msg.peer_id().bare_id_unchecked();
        let sender_id = msg.sender_id().map(|id| id.bare_id_unchecked());

        Self {
            id: msg.id(),
            chat_id,
            sender_id,
            text: msg.text().to_string(),
            date: msg.date().as_second(),
            is_outgoing: msg.outgoing(),
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
        }
    }
}