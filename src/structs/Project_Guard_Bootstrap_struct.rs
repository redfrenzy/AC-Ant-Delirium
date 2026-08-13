#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectGuardBootstrapReceipt {
    pub guard_directory_created: bool,
    pub git_repository: bool,
    pub projections_ignored: bool,
    pub local_ignore_created: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectGuardBootstrapError {
    InvalidProjectRoot,
    InvalidGitMetadata,
    InvalidGuardDirectory,
    InvalidLocalIgnore,
    TrackedProjection,
    GitInspection,
    Filesystem,
}
