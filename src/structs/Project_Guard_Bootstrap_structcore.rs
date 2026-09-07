use std::fs::{self, File};
use std::path::Path;

use super::{ProjectGuardBootstrapError, ProjectGuardBootstrapReceipt, ProjectStateRoot};

pub fn materialize_project_guard(
    state_root: &ProjectStateRoot,
) -> Result<ProjectGuardBootstrapReceipt, ProjectGuardBootstrapError> {
    let project_root = state_root.project_root();
    require_regular_directory(project_root, ProjectGuardBootstrapError::InvalidProjectRoot)?;
    let guard_directory = state_root.project_guard_dir();
    let guard_exists = path_exists_without_following(&guard_directory)?;
    if guard_exists {
        require_regular_directory(
            &guard_directory,
            ProjectGuardBootstrapError::InvalidGuardDirectory,
        )?;
    }

    let guard_directory_created = materialize_guard_directory(&guard_directory)?;
    Ok(ProjectGuardBootstrapReceipt {
        guard_directory_created,
        local_ignore_created: false,
    })
}

fn materialize_guard_directory(guard_directory: &Path) -> Result<bool, ProjectGuardBootstrapError> {
    match fs::create_dir(guard_directory) {
        Ok(()) => {
            sync_parent_directory(guard_directory)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            require_regular_directory(
                guard_directory,
                ProjectGuardBootstrapError::InvalidGuardDirectory,
            )?;
            Ok(false)
        }
        Err(_) => Err(ProjectGuardBootstrapError::Filesystem),
    }
}

fn path_exists_without_following(path: &Path) -> Result<bool, ProjectGuardBootstrapError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(ProjectGuardBootstrapError::Filesystem),
    }
}

fn require_regular_directory(
    path: &Path,
    invalid: ProjectGuardBootstrapError,
) -> Result<(), ProjectGuardBootstrapError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid);
    }
    Ok(())
}

fn sync_parent_directory(path: &Path) -> Result<(), ProjectGuardBootstrapError> {
    let parent = path
        .parent()
        .ok_or(ProjectGuardBootstrapError::Filesystem)?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| ProjectGuardBootstrapError::Filesystem)
}
