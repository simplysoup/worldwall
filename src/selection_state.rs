//! Private, atomic persistence for the opt-in adaptive family/band state.
//!
//! The file deliberately stores no weather measurements, location, timezone,
//! selected precipitation variant, or other environment data.

use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::scene_registry::{AdaptiveSelectionState, TemperatureBand};

const MAX_STATE_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionStateError(String);

impl fmt::Display for SelectionStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SelectionStateError {}

fn error(message: impl Into<String>) -> SelectionStateError {
    SelectionStateError(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptiveSelectionStateStore {
    path: PathBuf,
}

impl AdaptiveSelectionStateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_path() -> Result<Self, SelectionStateError> {
        let root = match std::env::var_os("XDG_STATE_HOME") {
            Some(root) => {
                let root = PathBuf::from(root);
                if !root.is_absolute() {
                    return Err(error("XDG_STATE_HOME must be an absolute path"));
                }
                root
            }
            None => {
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .ok_or_else(|| error("HOME is required for adaptive selection state"))?;
                if !home.is_absolute() {
                    return Err(error("HOME must be an absolute path"));
                }
                home.join(".local/state")
            }
        };
        Ok(Self::new(root.join("worldwall/weather-selection.state")))
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<AdaptiveSelectionState>, SelectionStateError> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error_value) if error_value.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error_value) => {
                return Err(error(format!(
                    "adaptive state metadata failed: {error_value}"
                )));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(error("adaptive state must be a regular non-symlink file"));
        }
        if metadata.len() > MAX_STATE_BYTES as u64 {
            return Err(error("adaptive state exceeds the bounded size"));
        }
        let file = File::open(&self.path)
            .map_err(|error_value| error(format!("adaptive state open failed: {error_value}")))?;
        let mut contents = String::new();
        file.take((MAX_STATE_BYTES + 1) as u64)
            .read_to_string(&mut contents)
            .map_err(|error_value| error(format!("adaptive state read failed: {error_value}")))?;
        if contents.len() > MAX_STATE_BYTES {
            return Err(error("adaptive state exceeds the bounded size"));
        }
        parse_state(&contents).map(Some)
    }

    pub fn save(&self, state: &AdaptiveSelectionState) -> Result<(), SelectionStateError> {
        validate_state(state)?;
        let contents = format!(
            "schema_version = 1\nfamily_id = \"{}\"\ntemperature_band = \"{}\"\n",
            state.family_id,
            state.temperature_band.as_str(),
        );
        if contents.len() > MAX_STATE_BYTES {
            return Err(error("serialized adaptive state exceeds the bounded size"));
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| error("adaptive state path has no parent"))?;
        fs::create_dir_all(parent).map_err(|error_value| {
            error(format!(
                "adaptive state directory creation failed: {error_value}"
            ))
        })?;
        let parent_metadata = fs::symlink_metadata(parent).map_err(|error_value| {
            error(format!(
                "adaptive state directory metadata failed: {error_value}"
            ))
        })?;
        if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
            return Err(error(
                "adaptive state parent must be a regular non-symlink directory",
            ));
        }
        restrict_directory(parent)?;

        let temporary = parent.join(format!(
            ".weather-selection.{}.{}.tmp",
            std::process::id(),
            unique_suffix()
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error_value| {
                    error(format!(
                        "adaptive state temporary creation failed: {error_value}"
                    ))
                })?;
            restrict_file(&file)?;
            file.write_all(contents.as_bytes()).map_err(|error_value| {
                error(format!("adaptive state write failed: {error_value}"))
            })?;
            file.sync_all().map_err(|error_value| {
                error(format!("adaptive state sync failed: {error_value}"))
            })?;
            fs::rename(&temporary, &self.path).map_err(|error_value| {
                error(format!(
                    "adaptive state atomic replacement failed: {error_value}"
                ))
            })?;
            restrict_path(&self.path)?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error_value| {
                    error(format!(
                        "adaptive state directory sync failed: {error_value}"
                    ))
                })?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn parse_state(input: &str) -> Result<AdaptiveSelectionState, SelectionStateError> {
    let value: toml::Value =
        toml::from_str(input).map_err(|_| error("adaptive state TOML is malformed"))?;
    let table = value
        .as_table()
        .ok_or_else(|| error("adaptive state root must be a TOML table"))?;
    if table.keys().any(|key| {
        !matches!(
            key.as_str(),
            "schema_version" | "family_id" | "temperature_band"
        )
    }) {
        return Err(error("adaptive state contains an unknown field"));
    }
    if table
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        != Some(1)
    {
        return Err(error("adaptive state schema_version must be 1"));
    }
    let family_id = table
        .get("family_id")
        .and_then(toml::Value::as_str)
        .filter(|value| valid_family_id(value))
        .ok_or_else(|| error("adaptive state family_id is invalid"))?
        .to_owned();
    let band = table
        .get("temperature_band")
        .and_then(toml::Value::as_str)
        .and_then(parse_band)
        .ok_or_else(|| error("adaptive state temperature_band is invalid"))?;
    Ok(AdaptiveSelectionState {
        family_id,
        temperature_band: band,
    })
}

fn parse_band(value: &str) -> Option<TemperatureBand> {
    Some(match value {
        "cold" => TemperatureBand::Cold,
        "cool" => TemperatureBand::Cool,
        "mild" => TemperatureBand::Mild,
        "warm" => TemperatureBand::Warm,
        "hot" => TemperatureBand::Hot,
        _ => return None,
    })
}

fn valid_family_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn validate_state(state: &AdaptiveSelectionState) -> Result<(), SelectionStateError> {
    if !valid_family_id(&state.family_id) {
        return Err(error("adaptive state family_id is invalid"));
    }
    Ok(())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), SelectionStateError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error_value| {
        error(format!(
            "adaptive state directory permissions failed: {error_value}"
        ))
    })
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), SelectionStateError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file(file: &File) -> Result<(), SelectionStateError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error_value| {
            error(format!(
                "adaptive state file permissions failed: {error_value}"
            ))
        })
}

#[cfg(not(unix))]
fn restrict_file(_file: &File) -> Result<(), SelectionStateError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_path(path: &Path) -> Result<(), SelectionStateError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error_value| error(format!("adaptive state permissions failed: {error_value}")))
}

#[cfg(not(unix))]
fn restrict_path(_path: &Path) -> Result<(), SelectionStateError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_store(name: &str) -> AdaptiveSelectionStateStore {
        let root = std::env::temp_dir().join(format!(
            "worldwall-adaptive-state-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        AdaptiveSelectionStateStore::new(root.join("worldwall/weather-selection.state"))
    }

    fn state(family_id: &str, temperature_band: TemperatureBand) -> AdaptiveSelectionState {
        AdaptiveSelectionState {
            family_id: family_id.into(),
            temperature_band,
        }
    }

    #[test]
    fn state_round_trips_atomically_with_private_permissions_and_no_weather_or_location() {
        let store = test_store("roundtrip");
        let state = state("v29-seascape", TemperatureBand::Warm);
        store.save(&state).unwrap();
        assert_eq!(store.load().unwrap(), Some(state));
        let text = fs::read_to_string(store.path()).unwrap();
        assert!(text.contains("family_id"));
        assert!(!text.contains("latitude"));
        assert!(!text.contains("longitude"));
        assert!(!text.contains("temperature_c"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(store.path().parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        fs::remove_dir_all(store.path().parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn state_load_rejects_unknown_fields_symlinks_and_corrupt_data() {
        let store = test_store("invalid");
        assert_eq!(store.load().unwrap(), None);
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(
            store.path(),
            "schema_version = 1\nfamily_id = \"v29-seascape\"\ntemperature_band = \"warm\"\nlatitude = 1.0\n",
        )
        .unwrap();
        assert!(
            store
                .load()
                .unwrap_err()
                .to_string()
                .contains("unknown field")
        );
        fs::remove_file(store.path()).unwrap();
        let target = store.path().with_extension("target");
        fs::write(&target, "schema_version = 1\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, store.path()).unwrap();
        #[cfg(unix)]
        assert!(
            store
                .load()
                .unwrap_err()
                .to_string()
                .contains("non-symlink")
        );
        let _ = fs::remove_dir_all(store.path().parent().unwrap().parent().unwrap());
    }

    #[test]
    fn invalid_state_does_not_replace_previous_valid_state() {
        let store = test_store("preserve");
        store
            .save(&state("v29-seascape", TemperatureBand::Warm))
            .unwrap();
        assert!(
            store
                .save(&state("../invalid", TemperatureBand::Cold))
                .is_err()
        );
        assert_eq!(
            store.load().unwrap(),
            Some(state("v29-seascape", TemperatureBand::Warm))
        );
        fs::remove_dir_all(store.path().parent().unwrap().parent().unwrap()).unwrap();
    }
}
