#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectGuardBootstrapReceipt {
    pub guard_directory_created: bool,
    pub local_ignore_created: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectGuardBootstrapError {
    InvalidProjectRoot,
    InvalidGuardDirectory,
    InvalidLocalIgnore,
    Filesystem,
}
