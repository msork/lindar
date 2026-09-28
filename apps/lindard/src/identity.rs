//! Persistent local node identity and host metadata.

use std::{
    env,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use lindar_core::{NodeId, NodeInfo, OperatingSystem};
use uuid::Uuid;

/// Loads or creates this user's stable node identity and detects the hostname.
pub fn node_info() -> Result<NodeInfo, IdentityError> {
    let hostname = gethostname::gethostname()
        .into_string()
        .map_err(|_| IdentityError::InvalidHostname)?;
    if hostname.trim().is_empty() {
        return Err(IdentityError::InvalidHostname);
    }

    Ok(NodeInfo {
        id: load_or_create_identity(&identity_path()?)?,
        hostname,
        operating_system: OperatingSystem::current(),
    })
}

fn identity_path() -> Result<PathBuf, IdentityError> {
    identity_path_for(
        OperatingSystem::current(),
        env::var_os("XDG_STATE_HOME").as_deref(),
        env::var_os("HOME").as_deref(),
    )
}

fn identity_path_for(
    os: OperatingSystem,
    xdg_state_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<PathBuf, IdentityError> {
    let home = home.map(PathBuf::from);
    let directory = match os {
        OperatingSystem::Linux => match xdg_state_home.map(PathBuf::from) {
            Some(path) if path.is_absolute() => path.join("lindar"),
            _ => home
                .map(|path| path.join(".local").join("state").join("lindar"))
                .ok_or(IdentityError::MissingHome)?,
        },
        OperatingSystem::MacOs => home
            .map(|path| {
                path.join("Library")
                    .join("Application Support")
                    .join("Lindar")
            })
            .ok_or(IdentityError::MissingHome)?,
        OperatingSystem::Other => return Err(IdentityError::UnsupportedPlatform),
    };
    Ok(directory.join("node-id"))
}

fn load_or_create_identity(path: &Path) -> Result<NodeId, IdentityError> {
    match read_identity(path) {
        Ok(node_id) => return Ok(node_id),
        Err(IdentityError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let parent = path
        .parent()
        .ok_or_else(|| IdentityError::InvalidPath(path.to_owned()))?;
    fs::create_dir_all(parent)
        .map_err(|source| IdentityError::io("create identity directory", parent, source))?;

    // A hard link installs the fully written temporary file only if no other
    // process has created the identity yet. Concurrent launches then load the
    // same winning UUID instead of replacing each other's identity.
    let generated = Uuid::new_v4();
    let temporary = parent.join(format!(".node-id.{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|source| {
            IdentityError::io("create temporary identity file", &temporary, source)
        })?;

    let write_result = file
        .write_all(generated.to_string().as_bytes())
        .and_then(|()| file.sync_all());
    if let Err(source) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(IdentityError::io("write identity file", &temporary, source));
    }
    drop(file);

    match fs::hard_link(&temporary, path) {
        Ok(()) => {
            let _ = fs::remove_file(&temporary);
            Ok(NodeId::new(generated.to_string()))
        }
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&temporary);
            read_identity(path)
        }
        Err(source) => {
            let _ = fs::remove_file(&temporary);
            Err(IdentityError::io("install identity file", path, source))
        }
    }
}

fn read_identity(path: &Path) -> Result<NodeId, IdentityError> {
    let contents = fs::read_to_string(path)
        .map_err(|source| IdentityError::io("read identity file", path, source))?;
    let parsed = Uuid::parse_str(contents.trim()).map_err(|error| IdentityError::Corrupt {
        path: path.to_owned(),
        reason: error.to_string(),
    })?;
    if parsed.get_version() != Some(uuid::Version::Random) {
        return Err(IdentityError::Corrupt {
            path: path.to_owned(),
            reason: "stored UUID is not version 4".to_owned(),
        });
    }
    Ok(NodeId::new(parsed.to_string()))
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("could not determine the system hostname")]
    InvalidHostname,
    #[error("cannot determine a home directory for Lindar identity storage")]
    MissingHome,
    #[error("persistent node identity is supported on Linux and macOS")]
    UnsupportedPlatform,
    #[error("invalid identity file path: {0}")]
    InvalidPath(PathBuf),
    #[error("identity file at {path} is invalid: {reason}")]
    Corrupt { path: PathBuf, reason: String },
    #[error("failed to {action} at {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl IdentityError {
    fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_owned(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, fs, path::PathBuf};

    use lindar_core::OperatingSystem;
    use tempfile::tempdir;
    use uuid::{Uuid, Version};

    use super::{identity_path_for, load_or_create_identity, IdentityError};

    #[test]
    fn creates_a_random_v4_identity_and_reloads_it() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("nested").join("node-id");

        let first = load_or_create_identity(&path).expect("create identity");
        let second = load_or_create_identity(&path).expect("reload identity");
        let parsed = Uuid::parse_str(first.as_str()).expect("UUID identity");

        assert_eq!(parsed.get_version(), Some(Version::Random));
        assert_eq!(first, second);
        assert_eq!(
            fs::read_to_string(path).expect("identity contents"),
            first.as_str()
        );
    }

    #[test]
    fn corrupt_identity_returns_an_error_without_replacing_the_file() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("node-id");
        fs::write(&path, "not-a-uuid").expect("write corrupt identity");

        let error = load_or_create_identity(&path).expect_err("corrupt identity must fail");

        assert!(matches!(error, IdentityError::Corrupt { .. }));
        assert_eq!(
            fs::read_to_string(path).expect("read corrupt file"),
            "not-a-uuid"
        );
    }

    #[test]
    fn identity_paths_follow_linux_xdg_and_macos_application_support() {
        let linux = identity_path_for(
            OperatingSystem::Linux,
            Some(OsStr::new("/state")),
            Some(OsStr::new("/home/user")),
        )
        .expect("Linux path");
        assert_eq!(linux, PathBuf::from("/state/lindar/node-id"));

        let linux_fallback =
            identity_path_for(OperatingSystem::Linux, None, Some(OsStr::new("/home/user")))
                .expect("Linux fallback path");
        assert_eq!(
            linux_fallback,
            PathBuf::from("/home/user/.local/state/lindar/node-id")
        );

        let macos = identity_path_for(
            OperatingSystem::MacOs,
            None,
            Some(OsStr::new("/Users/user")),
        )
        .expect("macOS path");
        assert_eq!(
            macos,
            PathBuf::from("/Users/user/Library/Application Support/Lindar/node-id")
        );
    }
}
