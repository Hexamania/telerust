//! Session management

use anyhow::Result;
use grammers_session::storages::SqliteSession;
use std::path::Path;
use std::sync::Arc;

/// Session manager for handling Grammers sessions
pub struct SessionManager {
    session: Arc<SqliteSession>,
    path: std::path::PathBuf,
}

impl SessionManager {
    /// Load or create a new session
    pub async fn load_or_create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let session = Arc::new(SqliteSession::open(&path).await?);

        tracing::info!("Session loaded/created at {:?}", path);

        Ok(Self { session, path })
    }

    /// Get the session
    pub fn session(&self) -> &Arc<SqliteSession> {
        &self.session
    }

    /// Get the session path
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}