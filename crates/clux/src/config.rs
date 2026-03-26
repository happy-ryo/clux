use std::path::PathBuf;

use serde::Deserialize;
use tracing::{info, warn};

/// Application configuration loaded from `clux.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub shell: ShellConfig,
    pub font: FontConfig,
    pub colors: ColorConfig,
    pub scrollback: ScrollbackConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    pub default: String,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            default: "pwsh.exe".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "Consolas".to_owned(),
            size: 14.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ColorConfig {
    pub background: [f32; 3],
    pub foreground: [f32; 3],
    pub palette: Option<Vec<[f32; 3]>>,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            background: [0.118, 0.118, 0.180],
            foreground: [0.847, 0.871, 0.914],
            palette: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ScrollbackConfig {
    pub max_lines: usize,
}

impl Default for ScrollbackConfig {
    fn default() -> Self {
        Self { max_lines: 10_000 }
    }
}

/// Return the path to the configuration file: `%APPDATA%/clux/clux.toml`.
fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("clux").join("clux.toml"))
}

/// Load configuration from disk, falling back to defaults on any error.
pub fn load_config() -> Config {
    let Some(path) = config_path() else {
        info!("Could not determine config directory, using defaults");
        return Config::default();
    };

    if !path.exists() {
        info!(?path, "Config file not found, using defaults");
        return Config::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<Config>(&contents) {
            Ok(config) => {
                info!(?path, "Loaded configuration");
                config
            }
            Err(e) => {
                warn!(?path, %e, "Failed to parse config file, using defaults");
                Config::default()
            }
        },
        Err(e) => {
            warn!(?path, %e, "Failed to read config file, using defaults");
            Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = Config::default();
        assert_eq!(config.shell.default, "pwsh.exe");
        assert!((config.font.size - 14.0).abs() < f32::EPSILON);
        assert_eq!(config.scrollback.max_lines, 10_000);
    }

    #[test]
    fn parse_minimal_toml() {
        let toml_str = r#"
[shell]
default = "cmd.exe"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.shell.default, "cmd.exe");
        // Other fields should use defaults
        assert!((config.font.size - 14.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_full_toml() {
        let toml_str = r#"
[shell]
default = "bash.exe"

[font]
family = "Cascadia Code"
size = 16.0

[colors]
background = [0.0, 0.0, 0.0]
foreground = [1.0, 1.0, 1.0]

[scrollback]
max_lines = 5000
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.shell.default, "bash.exe");
        assert_eq!(config.font.family, "Cascadia Code");
        assert!((config.font.size - 16.0).abs() < f32::EPSILON);
        assert_eq!(config.scrollback.max_lines, 5000);
    }
}
