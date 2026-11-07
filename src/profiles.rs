//! Private DSH_HOME per conversation; never import or mutate the interactive home.
use anyhow::{Result, ensure};
use cap_fs_ext::{DirExt, MetadataExt};
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
