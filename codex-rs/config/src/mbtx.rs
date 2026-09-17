use codex_utils_absolute_path::AbsolutePathBuf;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// Experimental programmable MoonBit tool. Executables are selected by the host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MbtxConfig {
    #[serde(default)]
    pub enabled: bool,
    pub moon: Option<AbsolutePathBuf>,
    pub moonrun: Option<AbsolutePathBuf>,
    /// Preinstalled MoonBit dependency-source cache; copied once per session.
    pub dependency_cache: Option<AbsolutePathBuf>,
    /// Trusted read-only reference documents, available to both execution interfaces.
    pub reference_directory: Option<AbsolutePathBuf>,
    /// Host-owned full-output archive, outside task-writable roots.
    pub output_directory: Option<AbsolutePathBuf>,
}

impl MbtxConfig {
    /// Validate host-selected executables before advertising the optional tool.
    pub fn validate(&self) -> std::io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        for (name, path) in [("moon", &self.moon), ("moonrun", &self.moonrun)] {
            let path = path.as_ref().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("mbtx.{name} is required when mbtx.enabled is true"),
                )
            })?;
            let metadata = std::fs::metadata(path).map_err(|error| {
                std::io::Error::new(
                    error.kind(),
                    format!("invalid mbtx.{name} executable {}: {error}", path.display()),
                )
            })?;
            if !metadata.is_file() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("mbtx.{name} must be a regular executable file"),
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("mbtx.{name} is not executable"),
                    ));
                }
            }
        }
        let cache = self.dependency_cache.as_ref().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "mbtx.dependency_cache is required when mbtx.enabled is true",
            )
        })?;
        if !cache.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "mbtx.dependency_cache must be a directory containing preinstalled dependency sources",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "mbtx_tests.rs"]
mod tests;
