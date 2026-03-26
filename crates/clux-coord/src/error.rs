use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoordError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("peer not found: {0}")]
    PeerNotFound(String),

    #[error("server error: {0}")]
    Server(String),
}

pub type Result<T> = std::result::Result<T, CoordError>;
