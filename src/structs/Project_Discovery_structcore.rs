use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use super::{CargoProjectKind, FilesystemIdentity, GitCheckoutDiscovery, ProjectDiscovery};

impl ProjectDiscovery {
    /// Discovers a governed Cargo unit without spawning Cargo or Git.
    pub fn discover(start: &Path, explicit_manifest: Option<&Path>) -> io::Result<Self> {
        let start_dir = existing_directory(start)?;
        let selected_manifest_path = match explicit_manifest {
            Some(manifest) => Some(resolve_explicit_manifest(&start_dir, manifest)?),
            None => find_manifest_upwards(&start_dir),
        };

        let (project_root, cargo_workspace_root, cargo_kind) =
            match selected_manifest_path.as_deref() {
                Some(manifest) => {
                    let package_root = manifest.parent().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "selected Cargo manifest has no parent directory",
                        )
                    })?;
                    match find_workspace_root(manifest)? {
                        Some(workspace_root) => (
                            workspace_root.clone(),
                            Some(workspace_root),
                            Some(CargoProjectKind::Workspace),
                        ),
                        None => (
                            package_root.to_path_buf(),
                            None,
                            Some(CargoProjectKind::Package),
                        ),
                    }
                }
                None => {
                    let fallback = find_legacy_governance_root(&start_dir)
                        .unwrap_or_else(|| start_dir.clone());
                    (fallback, None, None)
                }
            };

        let project_root = canonical_or_original(&project_root);
        Ok(Self {
            project_root_identity: filesystem_identity(&project_root),
            git_checkout: discover_git_checkout(&project_root)?,
            project_root,
            selected_manifest_path,
            cargo_workspace_root,
            cargo_kind,
        })
    }

    /// Preserves the legacy plugin layout while making workspace selection use
    /// the same discovered project root everywhere.
    pub fn resolve_legacy_workspace_path(
        &self,
        repository_path: &Path,
        state_root: &Path,
    ) -> PathBuf {
        let project = canonical_or_original(&self.project_root);
        let repository = canonical_or_original(repository_path);
        let state = canonical_or_original(state_root);

        if project == state
            || state_root.parent().map(canonical_or_original).as_ref() == Some(&project)
        {
            return repository_path.to_path_buf();
        }

        if project != repository && repository.starts_with(&project) && state.starts_with(&project)
        {
            return repository_path.to_path_buf();
        }

        self.project_root.clone()
    }

    pub fn is_legacy_worktree(&self, repository_path: &Path, state_root: &Path) -> bool {
        normalized_path(&self.resolve_legacy_workspace_path(repository_path, state_root))
            != normalized_path(repository_path)
    }

    pub fn shares_git_common_dir_with(&self, other: &Self) -> bool {
        match (&self.git_checkout, &other.git_checkout) {
            (Some(left), Some(right)) => {
                match (left.common_dir_identity, right.common_dir_identity) {
                    (Some(left_id), Some(right_id)) => left_id == right_id,
                    _ => normalized_path(&left.common_dir) == normalized_path(&right.common_dir),
                }
            }
            _ => false,
        }
    }
}

fn existing_directory(path: &Path) -> io::Result<PathBuf> {
    let candidate = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let metadata = fs::metadata(candidate)?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project discovery start is not a directory",
        ));
    }
    Ok(canonical_or_original(candidate))
}

fn resolve_explicit_manifest(start: &Path, manifest: &Path) -> io::Result<PathBuf> {
    let candidate = if manifest.is_absolute() {
        manifest.to_path_buf()
    } else {
        start.join(manifest)
    };
    if candidate.file_name().and_then(|name| name.to_str()) != Some("Cargo.toml") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "explicit Cargo manifest must be named Cargo.toml",
        ));
    }
    if !candidate.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "explicit Cargo manifest does not exist",
        ));
    }
    Ok(canonical_or_original(&candidate))
}

fn find_manifest_upwards(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|directory| directory.join("Cargo.toml"))
        .find(|manifest| manifest.is_file())
        .map(|manifest| canonical_or_original(&manifest))
}

fn find_workspace_root(selected_manifest: &Path) -> io::Result<Option<PathBuf>> {
    let selected_source = fs::read_to_string(selected_manifest)?;
    let selected_root = selected_manifest.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "selected Cargo manifest has no parent directory",
        )
    })?;

    if manifest_has_section(&selected_source, "workspace") {
        return Ok(Some(canonical_or_original(selected_root)));
    }

    if let Some(explicit_workspace) = package_workspace_path(&selected_source) {
        let workspace_root = canonical_or_original(&selected_root.join(explicit_workspace));
        let workspace_manifest = workspace_root.join("Cargo.toml");
        if !workspace_manifest.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "package.workspace does not point to a Cargo workspace",
            ));
        }
        let source = fs::read_to_string(&workspace_manifest)?;
        if !manifest_has_section(&source, "workspace") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "package.workspace target has no [workspace] section",
            ));
        }
        return Ok(Some(workspace_root));
    }

    for ancestor in selected_root.ancestors().skip(1) {
        let candidate = ancestor.join("Cargo.toml");
        if !candidate.is_file() {
            continue;
        }
        let source = fs::read_to_string(candidate)?;
        if manifest_has_section(&source, "workspace") {
            return Ok(Some(canonical_or_original(ancestor)));
        }
    }

    Ok(None)
}

fn manifest_has_section(source: &str, expected: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim();
        line.strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .map(str::trim)
            == Some(expected)
    })
}

fn package_workspace_path(source: &str) -> Option<PathBuf> {
    let mut in_package = false;
    for line in source.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "workspace" {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.split_once('"').map(|(path, _)| path))?;
        if !value.is_empty() {
            return Some(PathBuf::from(value));
        }
    }
    None
}

fn find_legacy_governance_root(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|path| {
        let config = path.join("ant_delirium_config");
        (config.join("type_1").is_dir()
            || config.join("default_hierarchy.toml").is_file()
            || path.join("delirium.toml").is_file())
        .then(|| path.to_path_buf())
    })
}

fn discover_git_checkout(start: &Path) -> io::Result<Option<GitCheckoutDiscovery>> {
    for top_level in start.ancestors() {
        let dot_git = top_level.join(".git");
        if dot_git.is_dir() && looks_like_git_dir(&dot_git) {
            let git_dir = canonical_or_original(&dot_git);
            return Ok(Some(GitCheckoutDiscovery {
                top_level: canonical_or_original(top_level),
                common_dir_identity: filesystem_identity(&git_dir),
                common_dir: git_dir.clone(),
                git_dir,
                is_linked_worktree: false,
            }));
        }
        if !dot_git.is_file() {
            continue;
        }

        let marker = fs::read_to_string(&dot_git)?;
        let raw_git_dir = marker
            .lines()
            .find_map(|line| line.trim().strip_prefix("gitdir:"))
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, ".git file has no gitdir target")
            })?;
        let raw_git_dir = PathBuf::from(raw_git_dir);
        let git_dir = if raw_git_dir.is_absolute() {
            raw_git_dir
        } else {
            top_level.join(raw_git_dir)
        };
        let git_dir = canonical_or_original(&git_dir);
        if !looks_like_git_dir(&git_dir) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                ".git file points to an invalid Git directory",
            ));
        }
        let common_marker = git_dir.join("commondir");
        let (common_dir, is_linked_worktree) = if common_marker.is_file() {
            let raw_common = fs::read_to_string(&common_marker)?;
            let raw_common = raw_common.trim();
            if raw_common.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Git commondir marker is empty",
                ));
            }
            let raw_common = PathBuf::from(raw_common);
            let common_dir = if raw_common.is_absolute() {
                raw_common
            } else {
                git_dir.join(raw_common)
            };
            (canonical_or_original(&common_dir), true)
        } else {
            (git_dir.clone(), false)
        };

        return Ok(Some(GitCheckoutDiscovery {
            top_level: canonical_or_original(top_level),
            git_dir,
            common_dir_identity: filesystem_identity(&common_dir),
            common_dir,
            is_linked_worktree,
        }));
    }

    Ok(None)
}

fn looks_like_git_dir(path: &Path) -> bool {
    path.join("HEAD").is_file()
        && (path.join("objects").is_dir() || path.join("commondir").is_file())
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn normalized_path(path: &Path) -> String {
    canonical_or_original(path)
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

#[cfg(unix)]
fn filesystem_identity(path: &Path) -> Option<FilesystemIdentity> {
    fs::metadata(path).ok().map(|metadata| FilesystemIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(not(unix))]
fn filesystem_identity(_path: &Path) -> Option<FilesystemIdentity> {
    None
}
