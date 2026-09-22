//! Versioned, transactional runtime configuration.
//!
//! Parsing and validation happen on a complete candidate before the caller
//! replaces its active copy. Registry validation loads every listed package,
//! including the declared fallback, so a future selection cannot be pointed at
//! an incomplete registry by a successful config reload.

use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

use crate::{scene_registry::SceneRegistry, scheduler::Fps};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;
const MAX_CONFIG_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ConfigError {}

fn error(message: impl Into<String>) -> ConfigError {
    ConfigError(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub schema_version: u32,
    pub registry_dir: Option<PathBuf>,
    pub fps: Fps,
    pub source_path: Option<PathBuf>,
}

impl AppConfig {
    pub fn from_environment() -> Result<Self, ConfigError> {
        if let Some(path) = config_path_from_environment() {
            return Self::load(path);
        }
        let fps = match std::env::var("WORLDWALL_FPS").ok().as_deref() {
            Some("15") => Fps::Fifteen,
            Some("30") | None => Fps::Thirty,
            Some(value) => {
                return Err(error(format!(
                    "WORLDWALL_FPS must be 15 or 30, got {value:?}"
                )));
            }
        };
        let registry_dir = std::env::var_os("WORLDWALL_REGISTRY_DIR").map(PathBuf::from);
        let config = Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            registry_dir,
            fps,
            source_path: None,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let metadata = fs::symlink_metadata(path).map_err(|io_error| {
            error(format!(
                "config {} is unavailable: {io_error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(error("config path must be a regular non-symlink file"));
        }
        let bytes = fs::read(path)
            .map_err(|io_error| error(format!("reading config failed: {io_error}")))?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(error("config exceeds the bounded size"));
        }
        let text = String::from_utf8(bytes).map_err(|_| error("config must be UTF-8"))?;
        let mut schema = None;
        let mut registry_dir = None;
        let mut fps = None;
        for (line_number, raw) in text.lines().enumerate() {
            let line_number = line_number + 1;
            let line = raw.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| error(format!("config line {line_number} must be key = value")))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "schema_version" => assign_once(
                    &mut schema,
                    value.parse::<u32>().map_err(|_| {
                        error(format!(
                            "config line {line_number}: schema_version is invalid"
                        ))
                    })?,
                    key,
                    line_number,
                )?,
                "registry_dir" => {
                    let value = parse_string(value, key, line_number)?;
                    assign_once(
                        &mut registry_dir,
                        (value != "none").then(|| PathBuf::from(value)),
                        key,
                        line_number,
                    )?;
                }
                "fps" => {
                    let parsed = match value {
                        "15" => Fps::Fifteen,
                        "30" => Fps::Thirty,
                        _ => {
                            return Err(error(format!(
                                "config line {line_number}: fps must be 15 or 30"
                            )));
                        }
                    };
                    assign_once(&mut fps, parsed, key, line_number)?;
                }
                _ => {
                    return Err(error(format!(
                        "config line {line_number}: unknown key {key:?}"
                    )));
                }
            }
        }
        let config = Self {
            schema_version: schema.ok_or_else(|| error("config schema_version is required"))?,
            registry_dir: registry_dir.ok_or_else(|| error("config registry_dir is required"))?,
            fps: fps.ok_or_else(|| error("config fps is required"))?,
            source_path: Some(path.to_path_buf()),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(error(format!(
                "unsupported config schema_version {}; expected {CONFIG_SCHEMA_VERSION}",
                self.schema_version
            )));
        }
        if let Some(root) = &self.registry_dir {
            let _ = SceneRegistry::load(root)
                .map_err(|error| ConfigError(format!("registry validation failed: {error}")))?;
        }
        Ok(())
    }

    pub fn reload_path(&self) -> Result<PathBuf, ConfigError> {
        self.source_path.clone().ok_or_else(|| {
            error("reload-config requires WORLDWALL_CONFIG or an explicit config path")
        })
    }
}

fn assign_once<T>(
    slot: &mut Option<T>,
    value: T,
    key: &str,
    line: usize,
) -> Result<(), ConfigError> {
    if slot.is_some() {
        return Err(error(format!("config line {line}: duplicate key {key:?}")));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_string(value: &str, key: &str, line: usize) -> Result<String, ConfigError> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .filter(|value| !value.contains('"') && !value.contains('\n'))
        .map(ToOwned::to_owned)
        .ok_or_else(|| error(format!("config line {line}: {key} must be a quoted string")))
}

fn config_path_from_environment() -> Option<PathBuf> {
    std::env::var_os("WORLDWALL_CONFIG").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "worldwall-s10-config-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn config_rejects_unknown_duplicate_and_invalid_values_atomically() {
        let path = path("invalid");
        fs::write(
            &path,
            "schema_version = 1\nfps = 60\nregistry_dir = \"none\"\n",
        )
        .unwrap();
        assert!(AppConfig::load(&path).is_err());
        fs::write(
            &path,
            "schema_version = 1\nfps = 30\nfps = 15\nregistry_dir = \"none\"\n",
        )
        .unwrap();
        assert!(AppConfig::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn none_registry_config_is_valid_and_preserves_future_selection_safety() {
        let path = path("valid");
        fs::write(
            &path,
            "schema_version = 1\nfps = 15\nregistry_dir = \"none\"\n",
        )
        .unwrap();
        let config = AppConfig::load(&path).unwrap();
        assert_eq!(config.registry_dir, None);
        assert_eq!(config.fps, Fps::Fifteen);
        fs::remove_file(path).unwrap();
    }
}
