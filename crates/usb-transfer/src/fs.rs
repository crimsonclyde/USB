//! Linux directory-handle operations. No path-based writes or symlink fallback.
use crate::{Error, Result};
use rustix::fs::{self, AtFlags, Mode, OFlags, ResolveFlags};
use std::{
    fs::{File, Metadata},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

pub(crate) struct Directory(pub File);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    pub device: u64,
    pub inode: u64,
    pub bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
    pub mode: u32,
}
impl Identity {
    pub fn of(metadata: Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            bytes: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
            mode: metadata.mode(),
        }
    }
    pub fn same_node(&self, other: &Self) -> bool {
        self.device == other.device && self.inode == other.inode
    }
}

pub(crate) fn io<T>(
    operation: &'static str,
    path: impl AsRef<Path>,
    result: std::io::Result<T>,
) -> Result<T> {
    result.map_err(|source| Error::Io {
        operation,
        path: path.as_ref().to_owned(),
        source,
    })
}
fn syscall<T>(
    operation: &'static str,
    path: impl AsRef<Path>,
    result: rustix::io::Result<T>,
) -> Result<T> {
    io(operation, path, result.map_err(Into::into))
}
fn relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(Error::Invalid(format!("invalid relative path: {path:?}")));
    }
    Ok(())
}

impl Directory {
    pub fn open_root(path: &Path) -> Result<(Self, PathBuf)> {
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            io("working directory", path, std::env::current_dir())?.join(path)
        };
        let slash = syscall(
            "open filesystem root",
            "/",
            fs::open(
                "/",
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            ),
        )?;
        let relative = absolute
            .strip_prefix("/")
            .map_err(|_| Error::Invalid("expected absolute path".into()))?;
        let relative = if relative.as_os_str().is_empty() {
            Path::new(".")
        } else {
            relative
        };
        let fd = syscall(
            "open directory without symlinks",
            &absolute,
            fs::openat2(
                &slash,
                relative,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
                ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
            ),
        )?;
        let canonical = io("canonicalize", &absolute, std::fs::canonicalize(&absolute))?;
        Ok((Self(File::from(fd)), canonical))
    }
    pub fn open(&self, path: &Path) -> Result<Self> {
        relative(path)?;
        Ok(Self(File::from(syscall(
            "open child directory",
            path,
            fs::openat2(
                &self.0,
                path,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
                resolve(),
            ),
        )?)))
    }
    pub fn file(&self, path: &Path) -> Result<File> {
        relative(path)?;
        let file = File::from(syscall(
            "open regular source",
            path,
            fs::openat2(
                &self.0,
                path,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
                Mode::empty(),
                resolve(),
            ),
        )?);
        if !io("stat source", path, file.metadata())?.is_file() {
            return Err(Error::Invalid(format!("not a regular file: {path:?}")));
        }
        Ok(file)
    }
    pub fn create_file(&self, path: &Path) -> Result<File> {
        relative(path)?;
        Ok(File::from(syscall(
            "create new file",
            path,
            fs::openat2(
                &self.0,
                path,
                OFlags::CREATE | OFlags::EXCL | OFlags::RDWR | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
                resolve(),
            ),
        )?))
    }
    pub fn create_directory(&self, path: &Path) -> Result<Self> {
        relative(path)?;
        let parent_path = path.parent().filter(|p| !p.as_os_str().is_empty());
        let parent = if let Some(path) = parent_path {
            self.open(path)?
        } else {
            self.duplicate()?
        };
        let name = path
            .file_name()
            .ok_or_else(|| Error::Invalid("missing directory name".into()))?;
        syscall(
            "create new directory",
            path,
            fs::mkdirat(&parent.0, name, Mode::from_raw_mode(0o755)),
        )?;
        let directory = parent.open(Path::new(name))?;
        parent.sync()?;
        Ok(directory)
    }
    pub fn exists(&self, path: &Path) -> Result<bool> {
        relative(path)?;
        match fs::statat(&self.0, path, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Ok(true),
            Err(rustix::io::Errno::NOENT) => Ok(false),
            Err(error) => syscall("check destination entry", path, Err(error)),
        }
    }
    pub fn duplicate(&self) -> Result<Self> {
        Ok(Self(io(
            "duplicate directory handle",
            ".",
            self.0.try_clone(),
        )?))
    }
    pub fn identity(&self) -> Result<Identity> {
        Ok(Identity::of(io("stat directory", ".", self.0.metadata())?))
    }
    pub fn sync(&self) -> Result<()> {
        #[cfg(test)]
        io(
            "sync directory",
            ".",
            crate::faults::check(crate::faults::Point::DirectorySync),
        )?;
        io("sync directory", ".", self.0.sync_all())
    }
    pub fn lock(&self) -> Result<()> {
        syscall(
            "lock destination (another transfer may be active)",
            ".",
            fs::flock(&self.0, fs::FlockOperation::NonBlockingLockExclusive),
        )
    }
    pub fn entries(&self) -> Result<Vec<(String, fs::FileType)>> {
        let iterator = syscall("list source directory", ".", fs::Dir::read_from(&self.0))?;
        let mut entries = Vec::new();
        for entry in iterator {
            let entry = syscall("read source entry", ".", entry)?;
            let name = entry
                .file_name()
                .to_str()
                .map_err(|_| Error::Invalid("non-UTF-8 source entry".into()))?;
            if name != "." && name != ".." {
                entries.push((name.to_owned(), entry.file_type()));
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }
    pub fn publish(&self, name: &str, destination: &Self) -> Result<()> {
        syscall(
            "publish complete game without replacement",
            name,
            fs::renameat_with(
                &self.0,
                name,
                &destination.0,
                name,
                fs::RenameFlags::NOREPLACE,
            ),
        )?;
        // A successful rename followed by a sync failure must never be rolled back.
        destination.sync()?;
        self.sync()
    }
    pub fn space(&self) -> Result<Space> {
        let stat = syscall("measure free space", ".", fs::fstatvfs(&self.0))?;
        let unit = stat.f_frsize.max(stat.f_bsize);
        let available = stat
            .f_bavail
            .checked_mul(stat.f_frsize)
            .ok_or(Error::Overflow)?;
        Ok(Space { available, unit })
    }
}
fn resolve() -> ResolveFlags {
    ResolveFlags::BENEATH
        | ResolveFlags::NO_SYMLINKS
        | ResolveFlags::NO_MAGICLINKS
        | ResolveFlags::NO_XDEV
}

#[derive(Clone, Copy)]
pub(crate) struct Space {
    pub available: u64,
    pub unit: u64,
}

/// Detect a destination inside the archive even through a bind-mount alias.
/// No visitor file contents or directory listings are read.
pub(crate) fn reject_overlap(
    source: &Directory,
    destination: &Directory,
    source_path: &Path,
    destination_path: &Path,
) -> Result<()> {
    if source_path.starts_with(destination_path) || destination_path.starts_with(source_path) {
        return Err(Error::Invalid("archive and destination overlap".into()));
    }
    let target = destination.identity()?;
    let mut pending = vec![PathBuf::new()];
    while let Some(path) = pending.pop() {
        let dir = if path.as_os_str().is_empty() {
            source.duplicate()?
        } else {
            source.open(&path)?
        };
        if dir.identity()?.same_node(&target) {
            return Err(Error::Invalid(
                "archive and destination alias the same directory".into(),
            ));
        }
        for (name, kind) in dir.entries()? {
            // Open every directory under the archive with NO_XDEV and NO_SYMLINKS.
            if kind == fs::FileType::Directory {
                pending.push(path.join(&name));
            } else if kind == fs::FileType::Unknown {
                // Do not trust d_type to omit directories on filesystems lacking it.
                let stat = syscall(
                    "stat archive entry",
                    &name,
                    fs::statat(&dir.0, &name, AtFlags::SYMLINK_NOFOLLOW),
                )?;
                if fs::FileType::from_raw_mode(stat.st_mode) == fs::FileType::Directory {
                    pending.push(path.join(&name));
                }
            }
        }
    }
    Ok(())
}
