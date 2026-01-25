use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// List of Freedium proxy endpoints to try
    pub endpoints: Vec<String>,
    /// UI theme
    pub theme: Theme,
    /// Article font size in pixels
    pub font_size: u32,
    /// Maximum article width in pixels
    pub max_width: u32,
}

/// Theme options for the UI
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
    Sepia,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            endpoints: vec![
                "https://freedium.cfd/".to_string(),
                "https://freedium-mirror.cfd/".to_string(),
            ],
            theme: Theme::default(),
            font_size: 17,
            max_width: 680,
        }
    }
}

impl AppConfig {
    /// Load configuration from disk, or return defaults if not found
    pub fn load() -> Self {
        let path = Self::config_path();

        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => {
                    match toml::from_str(&contents) {
                        Ok(config) => return config,
                        Err(e) => {
                            tracing::warn!("Failed to parse config file: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read config file: {}", e);
                }
            }
        }

        // Return defaults and try to save them
        let config = Self::default();
        if let Err(e) = config.save() {
            tracing::warn!("Failed to save default config: {}", e);
        }
        config
    }

    /// Save configuration to disk
    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let toml = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        std::fs::write(path, toml)
    }

    /// Get the path to the config file
    fn config_path() -> PathBuf {
        directories::ProjectDirs::from("com", "wallflower", "Wallflower")
            .map(|dirs| dirs.config_dir().join("config.toml"))
            .unwrap_or_else(|| {
                // Fallback to current directory
                PathBuf::from("wallflower-config.toml")
            })
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.endpoints.is_empty() {
            return Err(ConfigError::NoEndpoints);
        }

        if self.font_size < 10 || self.font_size > 32 {
            return Err(ConfigError::InvalidFontSize(self.font_size));
        }

        if self.max_width < 400 || self.max_width > 1200 {
            return Err(ConfigError::InvalidMaxWidth(self.max_width));
        }

        // Validate endpoint URLs
        for endpoint in &self.endpoints {
            if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
                return Err(ConfigError::InvalidEndpoint(endpoint.clone()));
            }
        }

        Ok(())
    }
}

/// Configuration validation errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("No endpoints configured")]
    NoEndpoints,

    #[error("Invalid font size: {0} (must be between 10 and 32)")]
    InvalidFontSize(u32),

    #[error("Invalid max width: {0} (must be between 400 and 1200)")]
    InvalidMaxWidth(u32),

    #[error("Invalid endpoint URL: {0}")]
    InvalidEndpoint(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert!(!config.endpoints.is_empty());
        assert_eq!(config.theme, Theme::System);
        assert_eq!(config.font_size, 17);
        assert_eq!(config.max_width, 680);
    }

    #[test]
    fn test_config_validation() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_no_endpoints() {
        let config = AppConfig {
            endpoints: vec![],
            ..Default::default()
        };
        assert!(matches!(config.validate(), Err(ConfigError::NoEndpoints)));
    }

    #[test]
    fn test_config_validation_invalid_font_size() {
        let config = AppConfig {
            font_size: 50,
            ..Default::default()
        };
        assert!(matches!(config.validate(), Err(ConfigError::InvalidFontSize(_))));
    }

    #[test]
    fn test_theme_serialization() {
        let config = AppConfig {
            theme: Theme::Dark,
            ..Default::default()
        };
        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("theme = \"dark\""));
    }
}
