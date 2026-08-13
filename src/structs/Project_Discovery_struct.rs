use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoProjectKind {
    Package,
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilesystemIdentity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCheckoutDiscovery {
    pub top_level: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub common_dir_identity: Option<FilesystemIdentity>,
    pub is_linked_worktree: bool,
}

/// Filesystem-backed discovery result for one governed Cargo unit.
///
/// `project_root` is the Cargo workspace root when one contains the selected
/// manifest, otherwise it is the selected package root. `git_checkout` describes
/// the active checkout and never acts as a canonical project ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDiscovery {
    pub project_root: PathBuf,
    pub selected_manifest_path: Option<PathBuf>,
    pub cargo_workspace_root: Option<PathBuf>,
    pub cargo_kind: Option<CargoProjectKind>,
    pub project_root_identity: Option<FilesystemIdentity>,
    pub git_checkout: Option<GitCheckoutDiscovery>,
}
