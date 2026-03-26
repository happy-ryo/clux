use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session not found: {name}")]
    NotFound { name: String },

    #[error("failed to read session file at {path}: {source}")]
    ReadFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write session file at {path}: {source}")]
    WriteFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse session file at {path}: {source}")]
    ParseFailed {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to serialize session state: {0}")]
    SerializeFailed(#[from] serde_json::Error),

    #[error("could not determine application data directory")]
    NoAppDataDir,

    #[error("failed to create session directory at {path}: {source}")]
    CreateDirFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to delete session file at {path}: {source}")]
    DeleteFailed {
        path: PathBuf,
        source: std::io::Error,
    },
}
