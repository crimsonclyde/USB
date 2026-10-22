//! Bind selected metadata and bytes to a read-only source snapshot.
use crate::{
    Error, Result, cancelled,
    fs::{Directory, Identity, io},
    stream::hash,
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use usb_core::catalogue::{Catalogue, Game, GameManifest};

pub(crate) struct Manifest {
    path: PathBuf,
    identity: Identity,
    digest: Vec<u8>,
}
impl Manifest {
    pub fn capture(root: &Directory, path: &Path) -> Result<(Self, Vec<u8>)> {
        let mut file = root.file(path)?;
        let identity = Identity::of(io("stat manifest", path, file.metadata())?);
        // Manifest resources are bounded even if the archive changes while read.
        if identity.bytes > 1024 * 1024 {
            return Err(Error::Invalid(format!("manifest exceeds 1 MiB: {path:?}")));
        }
        let mut bytes = Vec::new();
        io(
            "read manifest",
            path,
            (&mut file).take(1024 * 1024 + 1).read_to_end(&mut bytes),
        )?;
        if bytes.len() as u64 != identity.bytes
            || identity != Identity::of(io("stat manifest", path, file.metadata())?)
        {
            return Err(Error::SourceChanged(path.to_owned()));
        }
        Ok((
            Self {
                path: path.to_owned(),
                identity,
                digest: Sha256::digest(&bytes).to_vec(),
            },
            bytes,
        ))
    }
    pub fn verify(&self, root: &Directory) -> Result<()> {
        let (current, _) = Self::capture(root, &self.path)?;
        if self.identity != current.identity || self.digest != current.digest {
            return Err(Error::SourceChanged(self.path.clone()));
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Tree {
    // Root is stored under an empty relative name. Atime deliberately excluded.
    pub directories: Vec<(String, Identity)>,
    pub files: Vec<(String, Identity)>,
}
pub(crate) fn walk(root: &Directory) -> Result<Tree> {
    let mut tree = Tree {
        directories: vec![(String::new(), root.identity()?)],
        files: Vec::new(),
    };
    let mut pending = vec![String::new()];
    while let Some(prefix) = pending.pop() {
        let dir = if prefix.is_empty() {
            root.duplicate()?
        } else {
            root.open(Path::new(&prefix))?
        };
        for (name, _) in dir.entries()? {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            // lstat is only for choosing the open operation; openat2 independently
            // enforces confinement and rejects symlinks, including concurrent swaps.
            let info =
                rustix::fs::statat(&dir.0, name.as_str(), rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(|e| Error::Io {
                        operation: "stat payload",
                        path: path.clone().into(),
                        source: e.into(),
                    })?;
            match rustix::fs::FileType::from_raw_mode(info.st_mode) {
                rustix::fs::FileType::Directory => {
                    let child = dir.open(Path::new(&name))?;
                    tree.directories.push((path.clone(), child.identity()?));
                    pending.push(path);
                }
                rustix::fs::FileType::RegularFile => {
                    let file = dir.file(Path::new(&name))?;
                    tree.files.push((
                        path.clone(),
                        Identity::of(io("stat payload", &path, file.metadata())?),
                    ));
                }
                _ => return Err(Error::Invalid(format!("unsafe payload entry: {path:?}"))),
            }
        }
    }
    tree.directories.sort_by(|a, b| a.0.cmp(&b.0));
    tree.files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(tree)
}

pub(crate) struct SourceFile {
    pub path: String,
    pub identity: Identity,
    pub digest: String,
}

pub(crate) struct Snapshot {
    pub game: Game,
    pub content: PathBuf,
    manifest: Manifest,
    tree: Tree,
    pub files: Vec<SourceFile>,
}
impl Snapshot {
    pub fn capture(
        root: &Directory,
        catalogue: &Catalogue,
        game: &Game,
        cancel: &AtomicBool,
    ) -> Result<Self> {
        cancelled(cancel)?;
        let content = game
            .source
            .strip_prefix(&catalogue.root)
            .map_err(|_| Error::Invalid("source escapes archive".into()))?
            .to_owned();
        let manifest_path = content
            .parent()
            .ok_or_else(|| Error::Invalid("missing game root".into()))?
            .join("game.json");
        let (manifest, bytes) = Manifest::capture(root, &manifest_path)?;
        let metadata: GameManifest = serde_json::from_slice(&bytes)?;
        if serde_json::to_vec(&metadata)? != serde_json::to_vec(&game.metadata)? {
            return Err(Error::SourceChanged(manifest_path));
        }
        let directory = root.open(&content)?;
        let tree = walk(&directory)?;
        if tree.files.len() != game.files.len()
            || tree
                .directories
                .iter()
                .filter(|(p, _)| !p.is_empty())
                .map(|(p, _)| p)
                .ne(game.directories.iter())
            || tree
                .files
                .iter()
                .zip(&game.files)
                .any(|((path, identity), file)| *path != file.path || identity.bytes != file.bytes)
        {
            return Err(Error::SourceChanged(content));
        }
        let mut files = Vec::new();
        for (path, identity) in &tree.files {
            cancelled(cancel)?;
            let mut file = directory.file(Path::new(path))?;
            verify_identity(&file, path, identity)?;
            let digest = hash(&mut file, identity.bytes, cancel)?;
            verify_identity(&file, path, identity)?;
            files.push(SourceFile {
                path: path.clone(),
                identity: identity.clone(),
                digest,
            });
        }
        let snapshot = Self {
            game: game.clone(),
            content,
            manifest,
            tree,
            files,
        };
        snapshot.verify(root)?;
        Ok(snapshot)
    }
    pub fn verify(&self, root: &Directory) -> Result<Directory> {
        self.manifest.verify(root)?;
        let directory = root.open(&self.content)?;
        if walk(&directory)? != self.tree {
            return Err(Error::SourceChanged(self.content.clone()));
        }
        Ok(directory)
    }
}
pub(crate) fn verify_identity(file: &File, path: &str, expected: &Identity) -> Result<()> {
    if &Identity::of(io("revalidate source identity", path, file.metadata())?) != expected {
        return Err(Error::SourceChanged(path.into()));
    }
    Ok(())
}
