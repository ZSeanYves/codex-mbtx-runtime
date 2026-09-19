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
    /// Explicit host-selected Unix receipt socket. Grants only this IPC path
    /// to both interfaces; does not enable TCP, DNS or a managed proxy.
    pub observation_socket: Option<AbsolutePathBuf>,
    /// Host-owned, read-only MoonRun policy applied to every Wasm execution.
    pub runtime_policy: Option<AbsolutePathBuf>,
    /// Host PATH used by MoonRun to resolve the policy's registered programs.
    pub execution_path: Option<String>,
    /// Absolute experiment deadline, shared by compilation and execution.
    pub attempt_deadline_unix_ms: Option<u64>,
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
        match (&self.runtime_policy, &self.execution_path) {
            (Some(policy), Some(path)) => {
                let metadata = std::fs::symlink_metadata(policy)?;
                if !metadata.is_file() || !metadata.permissions().readonly() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "mbtx.runtime_policy must be a read-only regular file",
                    ));
                }
                if path.is_empty() || path.contains('\0') {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "mbtx.execution_path must contain absolute read-only directories",
                    ));
                }
                for directory in std::env::split_paths(path) {
                    if !directory.is_absolute() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "mbtx.execution_path must contain absolute read-only directories",
                        ));
                    }
                    let metadata = std::fs::metadata(directory)?;
                    if !metadata.is_dir() || !metadata.permissions().readonly() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "mbtx.execution_path must contain absolute read-only directories",
                        ));
                    }
                }
            }
            (None, None) => {}
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "mbtx.runtime_policy and mbtx.execution_path must be configured together",
                ));
            }
        }
        Ok(())
    }

    /// Remaining host-selected attempt time, never exceeding the two-hour ceiling.
    /// Call again before each spawn so preparation and approval consume this budget.
    pub fn remaining_attempt_ms(&self) -> std::io::Result<Option<u64>> {
        let Some(deadline) = self.attempt_deadline_unix_ms else {
            return Ok(None);
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_millis();
        let remaining = u128::from(deadline).saturating_sub(now);
        if remaining == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "MBTX attempt deadline expired before process startup",
            ));
        }
        Ok(Some(remaining.min(7_200_000) as u64))
    }
}

#[cfg(test)]
#[path = "mbtx_tests.rs"]
mod tests;
