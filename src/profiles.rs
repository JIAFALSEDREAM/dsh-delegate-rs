//! Private DSH_HOME per conversation; never import or mutate the interactive home.
use anyhow::{Result, ensure};
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::fs::Dir;
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub(crate) struct Profiles {
    root: Arc<Dir>,
    path: PathBuf,
}
pub(crate) struct Profile {
    pub path: PathBuf,
    pub directory: Arc<Dir>,
}

pub(crate) fn child(parent: &Dir, name: &str) -> io::Result<Dir> {
    match parent.create_dir(name) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let directory = parent.open_dir_nofollow(name)?;
    #[cfg(unix)]
    {
        use cap_std::fs::{Permissions, PermissionsExt};
        directory.set_permissions(".", Permissions::from_mode(0o700))?;
    }
    Ok(directory)
}

impl Profiles {
    pub fn new(state: &Path) -> Result<Self> {
        let parent = Dir::open_ambient_dir(state, cap_std::ambient_authority())?;
        Ok(Self {
            root: Arc::new(child(&parent, "conversations")?),
            path: state.join("conversations"),
        })
    }

    fn verify_root(&self) -> Result<()> {
        let current = Dir::open_ambient_dir(&self.path, cap_std::ambient_authority())?;
        let expected = self.root.dir_metadata()?;
        let actual = current.dir_metadata()?;
        ensure!(
            actual.dev() == expected.dev() && actual.ino() == expected.ino(),
            "Conversation storage root changed"
        );
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(id)?.to_string() == id,
            "Invalid conversation root"
        );
        self.verify_root()?;
        match self.root.symlink_metadata(id) {
            Ok(metadata) => ensure!(metadata.is_dir(), "Unsafe conversation directory"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        crate::tree::remove(self.root.open_dir_nofollow(id)?)?;
        Ok(())
    }

    // Optional, versioned cache snapshot; never treat a missing counter as zero.
    pub fn token_usage(&self, root: &str, session: &str) -> Option<(serde_json::Value, u64)> {
        use std::io::Read;
        self.verify_root().ok()?;
        uuid::Uuid::parse_str(root).ok()?;
        if session.is_empty()
            || !session
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return None;
        }
        let mut dir = self.root.open_dir_nofollow(root).ok()?;
        for part in ["storages", "session_projcache", "sessions"] {
            dir = dir.open_dir_nofollow(part).ok()?;
        }
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = dir.open_with(format!("{session}.json"), &options).ok()?;
        if !file.metadata().ok()?.is_file() {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(1_048_577).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 1_048_576 {
            return None;
        }
        let cache: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let row = &cache["record"]["rows"]["tokenUsage"];
        if cache["version"] != 7 || row["ver"] != 2 {
            return None;
        }
        let mut totals = serde_json::Map::new();
        let mut sum = 0u64;
        for key in [
            "uncachedInputTokens",
            "outputTokens",
            "cacheReadTokens",
            "cacheWriteTokens",
        ] {
            let count = row["val"]["totals"][key].as_u64()?;
            sum = sum.checked_add(count)?;
            totals.insert(key.into(), count.into());
        }
        totals.insert("totalTokens".into(), sum.into());
        Some((totals.into(), row["seq"].as_u64()?))
    }

    pub fn prepare(&self, id: &str) -> Result<Profile> {
        ensure!(
            uuid::Uuid::parse_str(id)?.to_string() == id,
            "Invalid conversation root"
        );
        self.verify_root()?;
        let directory = child(&self.root, id)?;
        Ok(Profile {
            path: self.path.join(id),
            directory: Arc::new(directory),
        })
    }
}
