use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::libres::System_Random_unit::random_crockford_id;

use super::{ProjectGuardBootstrapError, ProjectGuardBootstrapReceipt, ProjectStateRoot};

const LOCAL_IGNORE_CONTENT: &[u8] = b"/to-do_list.json\n/governance_error.json\n/.gitignore\n";
const LOCAL_IGNORE_NAME: &str = ".gitignore";
const ATOMIC_CREATE_ATTEMPTS: usize = 8;

pub fn materialize_project_guard(
    state_root: &ProjectStateRoot,
) -> Result<ProjectGuardBootstrapReceipt, ProjectGuardBootstrapError> {
    let project_root = state_root.project_root();
    require_regular_directory(project_root, ProjectGuardBootstrapError::InvalidProjectRoot)?;
    let guard_directory = state_root.project_guard_dir();
    let local_ignore = guard_directory.join(LOCAL_IGNORE_NAME);
    let guard_exists = path_exists_without_following(&guard_directory)?;
    if guard_exists {
        require_regular_directory(
            &guard_directory,
            ProjectGuardBootstrapError::InvalidGuardDirectory,
        )?;
        if path_exists_without_following(&local_ignore)? {
            require_regular_file(
                &local_ignore,
                ProjectGuardBootstrapError::InvalidLocalIgnore,
            )?;
        }
    }

    let guard_directory_created = materialize_guard_directory(&guard_directory)?;

    if path_exists_without_following(&local_ignore)? {
        require_regular_file(
            &local_ignore,
            ProjectGuardBootstrapError::InvalidLocalIgnore,
        )?;
        return Ok(ProjectGuardBootstrapReceipt {
            guard_directory_created,
            local_ignore_created: false,
        });
    }

    let local_ignore_created = atomic_create_local_ignore(&guard_directory, &local_ignore)?;
    require_regular_file(
        &local_ignore,
        ProjectGuardBootstrapError::InvalidLocalIgnore,
    )?;
    Ok(ProjectGuardBootstrapReceipt {
        guard_directory_created,
        local_ignore_created,
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

fn atomic_create_local_ignore(
    guard_directory: &Path,
    destination: &Path,
) -> Result<bool, ProjectGuardBootstrapError> {
    for _attempt in 0..ATOMIC_CREATE_ATTEMPTS {
        let token = random_crockford_id("").map_err(|_| ProjectGuardBootstrapError::Filesystem)?;
        let temporary = guard_directory.join(format!(".gitignore.antdelirium-create-{token}"));
        let mut file = match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ProjectGuardBootstrapError::Filesystem),
        };
        let result = write_and_link_local_ignore(&mut file, &temporary, destination);
        drop(file);
        let _ = fs::remove_file(&temporary);
        return result;
    }
    Err(ProjectGuardBootstrapError::Filesystem)
}

fn write_and_link_local_ignore(
    file: &mut File,
    temporary: &Path,
    destination: &Path,
) -> Result<bool, ProjectGuardBootstrapError> {
    file.write_all(LOCAL_IGNORE_CONTENT)
        .map_err(|_| ProjectGuardBootstrapError::Filesystem)?;
    set_public_file_permissions(file)?;
    file.sync_all()
        .map_err(|_| ProjectGuardBootstrapError::Filesystem)?;
    match fs::hard_link(temporary, destination) {
        Ok(()) => {
            sync_parent_directory(destination)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
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

fn require_regular_file(
    path: &Path,
    invalid: ProjectGuardBootstrapError,
) -> Result<(), ProjectGuardBootstrapError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid);
    }
    Ok(())
}

#[cfg(unix)]
fn set_public_file_permissions(file: &File) -> Result<(), ProjectGuardBootstrapError> {
    file.set_permissions(fs::Permissions::from_mode(0o644))
        .map_err(|_| ProjectGuardBootstrapError::Filesystem)
}

#[cfg(not(unix))]
fn set_public_file_permissions(_file: &File) -> Result<(), ProjectGuardBootstrapError> {
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

#[cfg(test)]
pub(crate) fn local_ignore_content() -> &'static [u8] {
    LOCAL_IGNORE_CONTENT
}
