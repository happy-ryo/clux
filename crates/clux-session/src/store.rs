use std::fs;
use std::path::PathBuf;

use tracing::{debug, info};

use crate::error::SessionError;
use crate::state::SessionState;

/// Return the base directory for session files: `%APPDATA%/clux/sessions`.
fn sessions_dir() -> Result<PathBuf, SessionError> {
    let data_dir = dirs::data_dir().ok_or(SessionError::NoAppDataDir)?;
    Ok(data_dir.join("clux").join("sessions"))
}

/// Build the path for a named session file.
fn session_path(name: &str) -> Result<PathBuf, SessionError> {
    let dir = sessions_dir()?;
    Ok(dir.join(format!("{name}.json")))
}

/// Save a session state to `%APPDATA%/clux/sessions/{name}.json`.
///
/// Creates intermediate directories if they do not exist.
pub fn save(state: &SessionState) -> Result<(), SessionError> {
    let path = session_path(&state.name)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| SessionError::CreateDirFailed {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let json = serde_json::to_string_pretty(state)?;
    fs::write(&path, &json).map_err(|source| SessionError::WriteFailed {
        path: path.clone(),
        source,
    })?;

    info!(?path, name = %state.name, "Session saved");
    Ok(())
}

/// Load a session state from disk by name.
pub fn load(name: &str) -> Result<SessionState, SessionError> {
    let path = session_path(name)?;

    if !path.exists() {
        return Err(SessionError::NotFound {
            name: name.to_string(),
        });
    }

    let contents = fs::read_to_string(&path).map_err(|source| SessionError::ReadFailed {
        path: path.clone(),
        source,
    })?;

    let state: SessionState =
        serde_json::from_str(&contents).map_err(|source| SessionError::ParseFailed {
            path: path.clone(),
            source,
        })?;

    debug!(?path, name, "Session loaded");
    Ok(state)
}

/// List all saved session names (without the `.json` extension).
pub fn list_sessions() -> Result<Vec<String>, SessionError> {
    let dir = sessions_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|source| SessionError::ReadFailed {
        path: dir.clone(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| SessionError::ReadFailed {
            path: dir.clone(),
            source,
        })?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            names.push(stem.to_string());
        }
    }

    names.sort();
    Ok(names)
}

/// Delete a saved session file.
pub fn delete(name: &str) -> Result<(), SessionError> {
    let path = session_path(name)?;

    if !path.exists() {
        return Err(SessionError::NotFound {
            name: name.to_string(),
        });
    }

    fs::remove_file(&path).map_err(|source| SessionError::DeleteFailed {
        path: path.clone(),
        source,
    })?;

    info!(?path, name, "Session deleted");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{LayoutNodeState, PaneSnapshot, TabState};
    use std::path::PathBuf;

    fn test_session(name: &str) -> SessionState {
        SessionState {
            name: name.to_string(),
            active_tab: 0,
            tabs: vec![TabState {
                name: "main".to_string(),
                active_pane: 0,
                layout: LayoutNodeState::Leaf { pane_id: 0 },
            }],
            panes: vec![PaneSnapshot {
                pane_id: 0,
                cwd: Some(PathBuf::from("C:\\Users\\test")),
                shell: "pwsh.exe".to_string(),
                cols: 120,
                rows: 30,
            }],
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let session = test_session("test-roundtrip");
        save(&session).unwrap();
        let loaded = load("test-roundtrip").unwrap();
        assert_eq!(loaded.name, session.name);
        assert_eq!(loaded.tabs.len(), 1);
        assert_eq!(loaded.panes.len(), 1);
        // Cleanup
        let _ = delete("test-roundtrip");
    }

    #[test]
    fn load_nonexistent_returns_not_found() {
        let result = load("nonexistent-session-name-12345");
        assert!(result.is_err());
    }

    #[test]
    fn list_sessions_works() {
        let session = test_session("test-list-sessions");
        save(&session).unwrap();
        let names = list_sessions().unwrap();
        assert!(names.contains(&"test-list-sessions".to_string()));
        // Cleanup
        let _ = delete("test-list-sessions");
    }

    #[test]
    fn delete_removes_file() {
        let session = test_session("test-delete");
        save(&session).unwrap();
        assert!(load("test-delete").is_ok());
        delete("test-delete").unwrap();
        assert!(load("test-delete").is_err());
    }
}
