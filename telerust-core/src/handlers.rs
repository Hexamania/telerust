//! Update handlers

use grammers_client::update::Update;
use tracing::{debug, info};

/// Handle incoming updates
pub fn handle_update(update: Update) {
    match update {
        Update::NewMessage(msg) => {
            info!("New message in chat {}: {}", msg.peer_id(), msg.text());
            // TODO: Process message
        }
        Update::MessageEdited(msg) => {
            debug!("Message edited in chat {}", msg.peer_id());
        }
        Update::MessageDeleted(deleted) => {
            debug!("Messages deleted: {:?}", deleted.raw);
        }
        Update::CallbackQuery(_) => {
            debug!("Callback query received");
        }
        Update::InlineQuery(_) => {
            debug!("Inline query received");
        }
        Update::InlineSend(_) => {
            debug!("Inline send received");
        }
        Update::GuestChatQuery(_) => {
            debug!("Guest chat query received");
        }
        Update::Raw(_) => {
            debug!("Raw update received");
        }
        _ => {
            debug!("Unhandled update type");
        }
    }
}