//! Manifest-led catalogue. Only `content/` beneath a game manifest is payload.
use crate::{Error, Result, compatibility::Filesystem, io};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveManifest {
    pub schema_version: u32,
    pub platforms: Vec<Group>,
    pub collections: Vec<Group>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GameManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub platform: String,
    pub collection: String,
    pub release_year: Option<u16>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub redistribution_approved: bool,
    /// Curator's rights evidence; required for approved content.
    pub approval_note: Option<String>,
    /// Absent permits all supported targets; an empty list permits none.
    pub filesystems: Option<Vec<Filesystem>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    /// UTF-8, slash-separated path relative to the game's content directory.
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Game {
    pub metadata: GameManifest,
    pub source: PathBuf,
    pub files: Vec<FileEntry>,
    pub directories: Vec<String>,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct Catalogue {
    pub root: PathBuf,
    pub platforms: BTreeMap<String, Group>,
    pub collections: BTreeMap<String, Group>,
    pub games: Vec<Game>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn version(version: u32) -> Result<()> {
    if version != 1 {
        return Err(invalid(format!("unsupported schema version {version}")));
    }
    Ok(())
}

fn groups(groups: Vec<Group>) -> Result<BTreeMap<String, Group>> {
    let mut result = BTreeMap::new();
    for group in groups {
        if !valid_id(&group.id) || group.name.trim().is_empty() {
            return Err(invalid("invalid group ID or empty name"));
        }
        if result.insert(group.id.clone(), group).is_some() {
            return Err(invalid("duplicate group ID"));
        }
    }
    Ok(result)
}

fn metadata(path: &Path) -> Result<fs::Metadata> {
    let metadata = io(path, fs::symlink_metadata(path))?;
    if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
        return Err(invalid(format!(
            "symlink or special file rejected: {}",
            path.display()
        )));
    }
    Ok(metadata)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    if !metadata(path)?.is_file() {
        return Err(invalid(format!(
            "expected regular manifest: {}",
            path.display()
        )));
    }
    let bytes = io(path, fs::read(path))?;
    serde_json::from_slice(&bytes).map_err(|source| Error::Json {
        path: path.to_owned(),
        source,
    })
}

fn children(path: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = io(path, fs::read_dir(path))?
        .map(|entry| io(path, entry).map(|entry| entry.path()))
        .collect::<Result<Vec<_>>>()?;
    entries.sort();
    Ok(entries)
}

fn relative(root: &Path, path: &Path) -> Result<String> {
    path.strip_prefix(root)
        .ok()
        .and_then(Path::to_str)
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("non-UTF-8 or invalid path: {}", path.display())))
}

fn payload(root: &Path) -> Result<(Vec<FileEntry>, Vec<String>, u64)> {
    if !metadata(root)?.is_dir() {
        return Err(invalid("game content must be a directory"));
    }
    let mut pending = vec![root.to_owned()];
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut bytes = 0_u64;
    while let Some(dir) = pending.pop() {
        for path in children(&dir)? {
            let info = metadata(&path)?;
            let name = relative(root, &path)?;
            if info.is_dir() {
                directories.push(name);
                pending.push(path);
            } else {
                bytes = bytes.checked_add(info.len()).ok_or(Error::Overflow)?;
                files.push(FileEntry {
                    path: name,
                    bytes: info.len(),
                });
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    directories.sort();
    if files.is_empty() {
        return Err(invalid(format!(
            "game has no payload files: {}",
            root.display()
        )));
    }
    Ok((files, directories, bytes))
}

/// Scan a stable, curator-controlled archive. Reject the whole scan on unsafe or
/// ambiguous input; discovery never grants redistribution approval.
pub fn scan(root: &Path) -> Result<Catalogue> {
    if !metadata(root)?.is_dir() {
        return Err(invalid("archive root must be a directory"));
    }
    let root = io(root, fs::canonicalize(root))?;
    let manifest: ArchiveManifest = read_json(&root.join("archive.json"))?;
    version(manifest.schema_version)?;
    let mut catalogue = Catalogue {
        root: root.clone(),
        platforms: groups(manifest.platforms)?,
        collections: groups(manifest.collections)?,
        games: Vec::new(),
    };
    let mut pending = vec![root];
    let mut ids = BTreeSet::new();
    while let Some(dir) = pending.pop() {
        let manifest_path = dir.join("game.json");
        // symlink_metadata also sees dangling manifest symlinks.
        match fs::symlink_metadata(&manifest_path) {
            Ok(_) => {
                let game: GameManifest = read_json(&manifest_path)?;
                version(game.schema_version)?;
                if !valid_id(&game.id) || game.name.trim().is_empty() {
                    return Err(invalid("invalid game ID or empty name"));
                }
                if !ids.insert(game.id.clone()) {
                    return Err(invalid(format!("duplicate game ID: {}", game.id)));
                }
                if !catalogue.platforms.contains_key(&game.platform)
                    || !catalogue.collections.contains_key(&game.collection)
                {
                    return Err(invalid(format!(
                        "unknown platform or collection for {}",
                        game.id
                    )));
                }
                if game.redistribution_approved
                    && game
                        .approval_note
                        .as_ref()
                        .is_none_or(|s| s.trim().is_empty())
                {
                    return Err(invalid(format!(
                        "approved game {} lacks rights evidence",
                        game.id
                    )));
                }
                if game.release_year == Some(0) {
                    return Err(invalid("release year must be positive"));
                }
                let source = dir.join("content");
                let (files, directories, bytes) = payload(&source)?;
                catalogue.games.push(Game {
                    metadata: game,
                    source,
                    files,
                    directories,
                    bytes,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                for path in children(&dir)? {
                    if metadata(&path)?.is_dir() {
                        pending.push(path);
                    }
                }
            }
            Err(e) => return io(&manifest_path, Err(e)),
        }
    }
    catalogue
        .games
        .sort_by(|a, b| a.metadata.id.cmp(&b.metadata.id));
    Ok(catalogue)
}
