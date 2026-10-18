//! Load of the machine and free disk space for the overview.

use std::future::Future;
use std::path::{Path, PathBuf};

/// One reading of CPU, memory and processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemSample {
    /// CPU load over all cores, 0 to 100.
    pub cpu_percent: u8,
    /// Memory in use, bytes.
    pub memory_used: u64,
    /// Installed memory, bytes.
    pub memory_total: u64,
    /// Running processes.
    pub processes: u32,
}

impl SystemSample {
    /// Memory in use, 0 to 100.
    #[must_use]
    pub fn memory_percent(&self) -> u8 {
        if self.memory_total == 0 {
            return 0;
        }
        let percent = u128::from(self.memory_used) * 100 / u128::from(self.memory_total);
        u8::try_from(percent.min(100)).unwrap_or(100)
    }
}

/// Free space of one mounted volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// Name shown to the user, e.g. `Macintosh HD`.
    pub name: String,
    /// Where it is mounted.
    pub mount: PathBuf,
    /// Space available for new files, bytes.
    pub available: u64,
    /// Size, bytes.
    pub total: u64,
}

impl Volume {
    /// The volume in `volumes` that holds `path`: the one mounted deepest above it.
    #[must_use]
    pub fn holding<'a>(volumes: &'a [Self], path: &Path) -> Option<&'a Self> {
        volumes
            .iter()
            .filter(|volume| path.starts_with(&volume.mount))
            .max_by_key(|volume| volume.mount.components().count())
    }
}

/// Reads the current load and the mounted volumes.
pub trait SystemProbe: Send + Sync {
    /// A fresh reading.
    fn sample(&self) -> impl Future<Output = SystemSample> + Send;

    /// Every mounted volume with its free space.
    fn volumes(&self) -> impl Future<Output = Vec<Volume>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(mount: &str) -> Volume {
        Volume {
            name: mount.to_owned(),
            mount: PathBuf::from(mount),
            available: 0,
            total: 0,
        }
    }

    #[test]
    fn a_path_belongs_to_the_deepest_mount_above_it() {
        let volumes = [volume("/"), volume("/Volumes/Ext"), volume("/Volumes/Ex")];
        let holding =
            |path: &str| Volume::holding(&volumes, Path::new(path)).map(|v| v.name.as_str());
        assert_eq!(holding("/Volumes/Ext/workspace"), Some("/Volumes/Ext"));
        assert_eq!(holding("/Users/me/workspace"), Some("/"));
    }
}
