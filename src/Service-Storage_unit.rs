use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::fd::{AsRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::{fill_system_random, random_crockford_id};
use ac_ant_delirium::structs::{
    AuthorizedCapability, BackupCheckpointRecord, BackupCheckpointStore, CanonicalMutationOutcome,
    CanonicalStateStore, CanonicalTaskAdvance, CanonicalTaskAdvanceOutcome, CanonicalTaskPackage,
    CapabilityActor, CapabilityChannel, CapabilityGrantAuditReceipt, CapabilityOperation,
    CapabilityRevocationAuditReceipt, NewAuditEvent, NewCapabilityGrant, ProjectDatabase,
    ProvisionalProjectRegistration, ReviewStore, ServiceSecurityAuditReceipt, StorageProject,
    StorageWorkspace, SubmissionStore, TransactionalStorageError, AUDIT_EVENT_CONTRACT_VERSION,
    STORAGE_SCHEMA_VERSION,
};

use crate::service_authority::VerifiedServiceAuthority;

const STATE_DIRECTORY_ENV: &str = "STATE_DIRECTORY";
const REGISTRY_DIR_NAME: &str = "registry";
const PROJECTS_DIR_NAME: &str = "projects";
const MIGRATION_STAGING_DIR_NAME: &str = "migration-staging";
const MAX_PROVISIONAL_PROJECTS_PER_UID: i64 = 64;
const BOOTSTRAP_CAPABILITY_LIFETIME_SECONDS: i64 = 300;
const MIN_MANAGED_CAPABILITY_LIFETIME_SECONDS: i64 = 60;
const MAX_MANAGED_CAPABILITY_LIFETIME_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtectedStorageError {
    InvalidRoot,
    WrongOwner,
    InvalidProjectIdentity,
    ResourceUnavailable,
    Storage,
}

impl From<TransactionalStorageError> for ProtectedStorageError {
    fn from(_error: TransactionalStorageError) -> Self {
        Self::Storage
    }
}

pub(crate) struct ProtectedServiceStorage {
    projects_dir: PathBuf,
    migration_staging_dir: PathBuf,
    registry: ProjectDatabase,
    bootstrap_lock: Mutex<()>,
    backup_transition_lock: Mutex<()>,
    protected_gate: Option<crate::protected_gate::ProtectedGateVerifier>,
}

pub(crate) struct IssuedLimitedCapability {
    pub(crate) capability_id: String,
    pub(crate) token: [u8; 32],
}

pub(crate) struct ProvisionalBootstrapResult {
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) compile: IssuedLimitedCapability,
    pub(crate) projection: IssuedLimitedCapability,
    pub(crate) backup_required: bool,
}

pub(crate) struct ManagedCapabilityGrantRequest {
    pub(crate) grant_id: String,
    pub(crate) secret_hash: [u8; 32],
    pub(crate) subject_uid: u32,
    pub(crate) actor: CapabilityActor,
    pub(crate) channel: CapabilityChannel,
    pub(crate) operation: CapabilityOperation,
    pub(crate) project_id: Option<String>,
    pub(crate) workspace_id: Option<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) lifetime_seconds: i64,
    pub(crate) revocation_epoch: u64,
}

// This service-only handle deliberately exposes neither project-key nor path.
// It can only be created from the unforgeable result of capability authorization.
#[allow(dead_code)]
pub(crate) struct ResolvedAuthorizedResource {
    database: ProjectDatabase,
    project_id: String,
    workspace_id: Option<String>,
    task_id: Option<String>,
}

#[allow(dead_code)]
impl ResolvedAuthorizedResource {
    pub(crate) fn database(&self) -> &ProjectDatabase {
        &self.database
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub(crate) fn task_id(&self) -> Option<&str> {
        self.task_id.as_deref()
    }
}

impl ProtectedServiceStorage {
    pub(crate) fn registry(&self) -> &ProjectDatabase {
        &self.registry
    }

    #[cfg(unix)]
    pub(crate) fn validate_workspace_descriptor(
        &self,
        peer_uid: u32,
        project_id: &str,
        workspace_id: &str,
        descriptor: &OwnedFd,
    ) -> Result<(), ()> {
        let registration = self
            .registry
            .provisional_registration_for_scope(peer_uid, project_id, workspace_id)
            .map_err(|_| ())?
            .ok_or(())?;
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(descriptor.as_raw_fd(), &mut stat) } != 0
            || stat.st_mode & libc::S_IFMT != libc::S_IFDIR
        {
            return Err(());
        }
        let mut material = b"antdelirium-provisional-workspace-identity-v2\0".to_vec();
        material.extend_from_slice(&registration.project_identity_fingerprint);
        material.extend_from_slice(b"inode\0");
        material.extend_from_slice(&(stat.st_dev as u64).to_be_bytes());
        material.extend_from_slice(&(stat.st_ino as u64).to_be_bytes());
        if sha256(&material) != registration.workspace_identity_fingerprint {
            return Err(());
        }
        Ok(())
    }

    #[cfg(all(unix, test))]
    pub(crate) fn commit_baseline_snapshot_from_descriptor(
        &self,
        authority: &VerifiedServiceAuthority,
        peer_uid: u32,
        project_id: &str,
        workspace_id: &str,
        compilation_fingerprint: &[u8; 32],
        descriptor: &OwnedFd,
    ) -> Result<crate::protected_git_backup::BackupCommitReceipt, ()> {
        self.commit_baseline_snapshot(
            authority,
            peer_uid,
            project_id,
            workspace_id,
            compilation_fingerprint,
            descriptor,
            None,
        )
    }

    #[cfg(unix)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_baseline_snapshot_from_transferred_entries(
        &self,
        authority: &VerifiedServiceAuthority,
        peer_uid: u32,
        project_id: &str,
        workspace_id: &str,
        compilation_fingerprint: &[u8; 32],
        workspace_descriptor: &OwnedFd,
        entries: Vec<crate::protected_git_backup::BackupSnapshotEntry>,
    ) -> Result<crate::protected_git_backup::BackupCommitReceipt, ()> {
        self.validate_workspace_descriptor(
            peer_uid,
            project_id,
            workspace_id,
            workspace_descriptor,
        )?;
        let _backup_guard = self.backup_transition_lock.lock().map_err(|_| ())?;
        let project = self
            .registry
            .registered_projects()
            .map_err(|_| ())?
            .into_iter()
            .find(|project| project.project_id == project_id)
            .ok_or(())?;
        validate_project_identity(&project).map_err(|_| ())?;
        let project_root = self.projects_dir.join(&project.project_key);
        require_owned_directory(&project_root, authority.effective_uid()).map_err(|_| ())?;
        let database = ProjectDatabase::open_existing_at(&project_root).map_err(|_| ())?;
        require_single_project(&database, &project).map_err(|_| ())?;
        let state = database
            .load_canonical_state(project_id, workspace_id)
            .map_err(|_| ())?;
        let current_task = state.current_task_id.as_deref().and_then(|current_id| {
            state
                .tasks
                .iter()
                .find(|task| task.task_id == current_id)
                .map(|task| (task.task_id.clone(), task.task_revision))
        });
        if state.current_task_id.is_some() != current_task.is_some() {
            return Err(());
        }
        let backup = crate::protected_git_backup::ProtectedGitBackup::initialize(
            &project_root,
            authority.effective_uid(),
        )
        .map_err(|_| ())?;
        if let Some(checkpoint) = database
            .backup_checkpoint(project_id, workspace_id, "baseline", "baseline")
            .map_err(|_| ())?
        {
            return backup_receipt_from_checkpoint(&checkpoint);
        }
        let occurred_at_unix = trusted_service_unix_time().map_err(|_| ())?;
        let metadata = crate::protected_git_backup::BackupCommitMetadata {
            transition: crate::protected_git_backup::BackupTransition::Baseline,
            project_id: project_id.to_string(),
            workspace_id: workspace_id.to_string(),
            task_id: current_task.as_ref().map(|task| task.0.clone()),
            task_revision: current_task.as_ref().map(|task| task.1),
            state_revision: state.state_revision,
            occurred_at_unix,
            observed_compilation_fingerprint: Some(lowercase_hex(compilation_fingerprint)),
            transition_id: None,
        };
        let receipt = backup
            .commit_snapshot_entries(&metadata, entries)
            .map_err(|_| ())?;
        persist_backup_checkpoint(
            &database,
            "baseline",
            "baseline",
            project_id,
            workspace_id,
            current_task.as_ref().map(|task| task.0.as_str()),
            current_task.as_ref().map(|task| task.1),
            state.state_revision,
            &receipt,
        )?;
        Ok(receipt)
    }

    #[cfg(all(unix, test))]
    #[allow(clippy::too_many_arguments)]
    fn commit_baseline_snapshot(
        &self,
        authority: &VerifiedServiceAuthority,
        peer_uid: u32,
        project_id: &str,
        workspace_id: &str,
        compilation_fingerprint: &[u8; 32],
        descriptor: &OwnedFd,
        archive_descriptor: Option<&OwnedFd>,
    ) -> Result<crate::protected_git_backup::BackupCommitReceipt, ()> {
        self.validate_workspace_descriptor(peer_uid, project_id, workspace_id, descriptor)?;
        let _backup_guard = self.backup_transition_lock.lock().map_err(|_| ())?;
        let project = self
            .registry
            .registered_projects()
            .map_err(|_| ())?
            .into_iter()
            .find(|project| project.project_id == project_id)
            .ok_or(())?;
        validate_project_identity(&project).map_err(|_| ())?;
        let project_root = self.projects_dir.join(&project.project_key);
        require_owned_directory(&project_root, authority.effective_uid()).map_err(|_| ())?;
        let database = ProjectDatabase::open_existing_at(&project_root).map_err(|_| ())?;
        require_single_project(&database, &project).map_err(|_| ())?;
        let state = database
            .load_canonical_state(project_id, workspace_id)
            .map_err(|_| ())?;
        let current_task = state.current_task_id.as_deref().and_then(|current_id| {
            state
                .tasks
                .iter()
                .find(|task| task.task_id == current_id)
                .map(|task| (task.task_id.clone(), task.task_revision))
        });
        if state.current_task_id.is_some() != current_task.is_some() {
            return Err(());
        }
        let backup = crate::protected_git_backup::ProtectedGitBackup::initialize(
            &project_root,
            authority.effective_uid(),
        )
        .map_err(|_| ())?;
        let occurred_at_unix = trusted_service_unix_time().map_err(|_| ())?;
        let metadata = crate::protected_git_backup::BackupCommitMetadata {
            transition: crate::protected_git_backup::BackupTransition::Baseline,
            project_id: project_id.to_string(),
            workspace_id: workspace_id.to_string(),
            task_id: current_task.as_ref().map(|task| task.0.clone()),
            task_revision: current_task.as_ref().map(|task| task.1),
            state_revision: state.state_revision,
            occurred_at_unix,
            observed_compilation_fingerprint: Some(lowercase_hex(compilation_fingerprint)),
            transition_id: None,
        };
        let receipt = match archive_descriptor {
            Some(archive) => backup.commit_snapshot_from_archive_descriptor(&metadata, archive),
            None => backup.commit_snapshot_from_descriptor(&metadata, descriptor),
        }
        .map_err(|_| ())?;
        persist_backup_checkpoint(
            &database,
            "baseline",
            "baseline",
            project_id,
            workspace_id,
            current_task.as_ref().map(|task| task.0.as_str()),
            current_task.as_ref().map(|task| task.1),
            state.state_revision,
            &receipt,
        )?;
        Ok(receipt)
    }

    #[cfg(all(unix, test))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_submission_snapshot_from_descriptor(
        &self,
        authority: &VerifiedServiceAuthority,
        peer_uid: u32,
        project_id: &str,
        workspace_id: &str,
        task_id: &str,
        task_revision: i64,
        submission_id: &str,
        descriptor: &OwnedFd,
    ) -> Result<BackupCheckpointRecord, ()> {
        self.commit_submission_snapshot(
            authority,
            peer_uid,
            project_id,
            workspace_id,
            task_id,
            task_revision,
            submission_id,
            descriptor,
            None,
        )
    }

    #[cfg(unix)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_submission_snapshot_from_transferred_descriptors(
        &self,
        authority: &VerifiedServiceAuthority,
        peer_uid: u32,
        project_id: &str,
        workspace_id: &str,
        task_id: &str,
        task_revision: i64,
        submission_id: &str,
        workspace_descriptor: &OwnedFd,
        archive_descriptor: &OwnedFd,
    ) -> Result<BackupCheckpointRecord, ()> {
        self.commit_submission_snapshot(
            authority,
            peer_uid,
            project_id,
            workspace_id,
            task_id,
            task_revision,
            submission_id,
            workspace_descriptor,
            Some(archive_descriptor),
        )
    }

    #[cfg(unix)]
    #[allow(clippy::too_many_arguments)]
    fn commit_submission_snapshot(
        &self,
        authority: &VerifiedServiceAuthority,
        peer_uid: u32,
        project_id: &str,
        workspace_id: &str,
        task_id: &str,
        task_revision: i64,
        submission_id: &str,
        descriptor: &OwnedFd,
        archive_descriptor: Option<&OwnedFd>,
    ) -> Result<BackupCheckpointRecord, ()> {
        self.validate_workspace_descriptor(peer_uid, project_id, workspace_id, descriptor)?;
        let _backup_guard = self.backup_transition_lock.lock().map_err(|_| ())?;
        let project = self
            .registry
            .registered_projects()
            .map_err(|_| ())?
            .into_iter()
            .find(|project| project.project_id == project_id)
            .ok_or(())?;
        validate_project_identity(&project).map_err(|_| ())?;
        let project_root = self.projects_dir.join(&project.project_key);
        require_owned_directory(&project_root, authority.effective_uid()).map_err(|_| ())?;
        let database = ProjectDatabase::open_existing_at(&project_root).map_err(|_| ())?;
        require_single_project(&database, &project).map_err(|_| ())?;
        let state = database
            .load_canonical_state(project_id, workspace_id)
            .map_err(|_| ())?;
        let current_task = state
            .tasks
            .iter()
            .find(|task| task.task_id == task_id)
            .filter(|task| task.task_revision == task_revision)
            .ok_or(())?;
        if state.current_task_id.as_deref() != Some(current_task.task_id.as_str()) {
            return Err(());
        }
        let backup = crate::protected_git_backup::ProtectedGitBackup::initialize(
            &project_root,
            authority.effective_uid(),
        )
        .map_err(|_| ())?;
        if !backup.has_snapshot().map_err(|_| ())? {
            return Err(());
        }
        let occurred_at_unix = trusted_service_unix_time().map_err(|_| ())?;
        let metadata = crate::protected_git_backup::BackupCommitMetadata {
            transition: crate::protected_git_backup::BackupTransition::Submission,
            project_id: project_id.to_string(),
            workspace_id: workspace_id.to_string(),
            task_id: Some(task_id.to_string()),
            task_revision: Some(task_revision),
            state_revision: state.state_revision,
            occurred_at_unix,
            observed_compilation_fingerprint: None,
            transition_id: Some(submission_id.to_string()),
        };
        let receipt = match archive_descriptor {
            Some(archive) => backup.commit_snapshot_from_archive_descriptor(&metadata, archive),
            None => backup.commit_snapshot_from_descriptor(&metadata, descriptor),
        }
        .map_err(|_| ())?;
        backup_checkpoint_record(
            "submission",
            submission_id,
            project_id,
            workspace_id,
            Some(task_id),
            Some(task_revision),
            state.state_revision,
            &receipt,
        )
    }

    pub(crate) fn commit_review_snapshot(
        &self,
        authority: &VerifiedServiceAuthority,
        project_id: &str,
        workspace_id: &str,
        task_id: &str,
        submission_id: &str,
        review_id: &str,
    ) -> Result<BackupCheckpointRecord, ()> {
        let _backup_guard = self.backup_transition_lock.lock().map_err(|_| ())?;
        let project = self
            .registry
            .registered_projects()
            .map_err(|_| ())?
            .into_iter()
            .find(|project| project.project_id == project_id)
            .ok_or(())?;
        validate_project_identity(&project).map_err(|_| ())?;
        let project_root = self.projects_dir.join(&project.project_key);
        require_owned_directory(&project_root, authority.effective_uid()).map_err(|_| ())?;
        let database = ProjectDatabase::open_existing_at(&project_root).map_err(|_| ())?;
        require_single_project(&database, &project).map_err(|_| ())?;
        let state = database
            .load_canonical_state(project_id, workspace_id)
            .map_err(|_| ())?;
        let task = state
            .tasks
            .iter()
            .find(|task| task.task_id == task_id)
            .ok_or(())?;
        if state.current_task_id.as_deref() != Some(task.task_id.as_str()) {
            return Err(());
        }
        let backup = crate::protected_git_backup::ProtectedGitBackup::initialize(
            &project_root,
            authority.effective_uid(),
        )
        .map_err(|_| ())?;
        let occurred_at_unix = trusted_service_unix_time().map_err(|_| ())?;
        let receipt = backup
            .commit_snapshot_from_transition(
                &crate::protected_git_backup::BackupCommitMetadata {
                    transition: crate::protected_git_backup::BackupTransition::Review,
                    project_id: project_id.to_string(),
                    workspace_id: workspace_id.to_string(),
                    task_id: Some(task_id.to_string()),
                    task_revision: Some(task.task_revision),
                    state_revision: state.state_revision,
                    occurred_at_unix,
                    observed_compilation_fingerprint: None,
                    transition_id: Some(review_id.to_string()),
                },
                crate::protected_git_backup::BackupTransition::Submission,
                submission_id,
            )
            .map_err(|_| ())?;
        backup_checkpoint_record(
            "review",
            review_id,
            project_id,
            workspace_id,
            Some(task_id),
            Some(task.task_revision),
            state.state_revision,
            &receipt,
        )
    }

    pub(crate) fn install_generic_task_package(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        package: &CanonicalTaskPackage,
    ) -> Result<CanonicalMutationOutcome, ()> {
        let now = trusted_service_unix_time().map_err(|_| ())?;
        let authorized = capability.actor() == CapabilityActor::Operator
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::StateAdmin
            && capability.project_id().is_none()
            && capability.workspace_id().is_none()
            && capability.task_id().is_none()
            && self
                .registry
                .authorized_capability_is_current(capability, now)
                .map_err(|_| ())?;
        if !authorized {
            self.audit_rejected_authorized_request_at(capability, now)?;
            return Err(());
        }

        let database = self.open_registered_project(authority, &package.project_id)?;
        let mut fingerprint_material = Vec::new();
        fingerprint_material.extend_from_slice(package.contract_version.as_bytes());
        fingerprint_material.push(0);
        fingerprint_material.extend_from_slice(package.project_id.as_bytes());
        fingerprint_material.push(0);
        fingerprint_material.extend_from_slice(package.workspace_id.as_bytes());
        fingerprint_material.push(0);
        fingerprint_material.extend_from_slice(package.plan_id.as_bytes());
        fingerprint_material.extend_from_slice(&package.expected_state_revision.to_be_bytes());
        for task in &package.tasks {
            fingerprint_material.extend_from_slice(task.task_id.as_bytes());
            fingerprint_material.extend_from_slice(&task.task_revision.to_be_bytes());
            fingerprint_material.extend_from_slice(task.objective.as_bytes());
            for criterion in &task.acceptance_criteria {
                fingerprint_material.extend_from_slice(criterion.as_bytes());
                fingerprint_material.push(0);
            }
            for path in &task.relevant_paths {
                fingerprint_material.extend_from_slice(path.as_bytes());
                fingerprint_material.push(0);
            }
        }
        let fingerprint = lowercase_hex(&sha256(&fingerprint_material));
        let occurred_at = format!("unix:{now}");
        let event = NewAuditEvent {
            contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
            event_id: random_crockford_id("evt_").map_err(|_| ())?,
            project_id: package.project_id.clone(),
            workspace_id: Some(package.workspace_id.clone()),
            actor_kind: "operator".to_string(),
            actor_subject_id: "authorized-state-admin".to_string(),
            event_kind: "state_transitioned".to_string(),
            outcome: "succeeded".to_string(),
            occurred_at,
            fingerprint,
            correlation_id: random_crockford_id("ref_").map_err(|_| ())?,
            resource_id: Some(package.plan_id.clone()),
            payload: b"generic governance task package installed".to_vec(),
        };
        database
            .install_task_package(package, &event)
            .map_err(|_| ())
    }

    pub(crate) fn prepare_official_task_package(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        source: &crate::task_package_contracts::TaskGenerationSource,
    ) -> Result<
        (
            crate::task_package_contracts::TaskPackageProposal,
            crate::task_package_store::TaskPackageWriteOutcome,
        ),
        (),
    > {
        if !official_task_package_authority_matches(
            capability,
            CapabilityActor::Operator,
            CapabilityOperation::StateAdmin,
            &source.project_id,
            &source.workspace_id,
        ) {
            self.audit_rejected_authorized_request(capability)?;
            return Err(());
        }
        let resource = self.resolve_authorized_resource(authority, capability)?;
        let proposal = crate::task_package_generator::generate_task_package_proposal(source)
            .map_err(|_| ())?;
        let outcome =
            crate::task_package_store::store_task_package_proposal(resource.database(), &proposal)
                .map_err(|_| ())?;
        Ok((proposal, outcome))
    }

    pub(crate) fn inspect_official_task_package(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        request_id: &str,
        project_id: &str,
        workspace_id: &str,
        expected_state_revision: i64,
        proposal_id: &str,
    ) -> Result<
        (
            crate::task_package_contracts::TaskPackageProposal,
            crate::task_package_contracts::TaskPackageOperationReceipt,
        ),
        (),
    > {
        if !official_task_package_authority_matches(
            capability,
            CapabilityActor::Operator,
            CapabilityOperation::DiagnosticPrivate,
            project_id,
            workspace_id,
        ) {
            self.audit_rejected_authorized_request(capability)?;
            return Err(());
        }
        let resource = self.resolve_authorized_resource(authority, capability)?;
        crate::task_package_store::inspect_task_package_proposal(
            resource.database(),
            request_id,
            project_id,
            workspace_id,
            expected_state_revision,
            proposal_id,
        )
        .map_err(|_| ())
    }

    pub(crate) fn attest_official_task_package(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        attestation: &crate::task_package_contracts::TaskPackageAttestation,
    ) -> Result<crate::task_package_store::TaskPackageWriteOutcome, ()> {
        if !official_task_package_authority_matches(
            capability,
            CapabilityActor::Ci,
            CapabilityOperation::AttestationCreate,
            &attestation.project_id,
            &attestation.workspace_id,
        ) || attestation.verifier_subject_sha256
            != task_package_subject_sha256(capability.subject_uid())
        {
            self.audit_rejected_authorized_request(capability)?;
            return Err(());
        }
        let resource = self.resolve_authorized_resource(authority, capability)?;
        crate::task_package_store::store_task_package_attestation(resource.database(), attestation)
            .map_err(|_| ())
    }

    pub(crate) fn approve_official_task_package(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        approval: &crate::task_package_contracts::TaskPackageApproval,
    ) -> Result<crate::task_package_store::TaskPackageWriteOutcome, ()> {
        if !official_task_package_authority_matches(
            capability,
            CapabilityActor::Operator,
            CapabilityOperation::StateAdmin,
            &approval.project_id,
            &approval.workspace_id,
        ) || approval.operator_subject_sha256
            != task_package_subject_sha256(capability.subject_uid())
        {
            self.audit_rejected_authorized_request(capability)?;
            return Err(());
        }
        let resource = self.resolve_authorized_resource(authority, capability)?;
        crate::task_package_store::store_task_package_approval(resource.database(), approval)
            .map_err(|_| ())
    }

    pub(crate) fn advance_reviewed_task(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        advance: &CanonicalTaskAdvance,
        protected_gate_envelope: &[u8],
    ) -> Result<CanonicalTaskAdvanceOutcome, ()> {
        let now = trusted_service_unix_time().map_err(|_| ())?;
        let authorized = capability.actor() == CapabilityActor::Ci
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::StateTransition
            && capability.project_id() == Some(advance.project_id.as_str())
            && capability.workspace_id() == Some(advance.workspace_id.as_str())
            && capability.task_id() == Some(advance.task_id.as_str())
            && self
                .registry
                .authorized_capability_is_current(capability, now)
                .map_err(|_| ())?;
        if !authorized {
            self.audit_rejected_authorized_request_at(capability, now)?;
            return Err(());
        }

        let _backup_guard = self.backup_transition_lock.lock().map_err(|_| ())?;

        let database = self.open_registered_project(authority, &advance.project_id)?;
        let mut fingerprint_material = Vec::new();
        fingerprint_material.extend_from_slice(b"governance-task-advance-v1");
        fingerprint_material.push(0);
        fingerprint_material.extend_from_slice(advance.project_id.as_bytes());
        fingerprint_material.push(0);
        fingerprint_material.extend_from_slice(advance.workspace_id.as_bytes());
        fingerprint_material.push(0);
        fingerprint_material.extend_from_slice(advance.task_id.as_bytes());
        fingerprint_material.push(0);
        fingerprint_material.extend_from_slice(advance.review_id.as_bytes());
        fingerprint_material.extend_from_slice(&advance.expected_state_revision.to_be_bytes());
        let target_state_revision = advance.expected_state_revision.checked_add(1).ok_or(())?;
        let canonical_state = database
            .load_canonical_state(&advance.project_id, &advance.workspace_id)
            .map_err(|_| ())?;
        if canonical_state.state_revision != advance.expected_state_revision
            && canonical_state.state_revision != target_state_revision
        {
            return Err(());
        }
        let task = canonical_state
            .tasks
            .iter()
            .find(|task| task.task_id == advance.task_id)
            .ok_or(())?;
        if (canonical_state.state_revision == advance.expected_state_revision
            && (canonical_state.current_task_id.as_deref() != Some(advance.task_id.as_str())
                || task.status != "available"
                || task.review_status != "not_required"))
            || (canonical_state.state_revision == target_state_revision
                && (task.status != "completed" || task.review_status != "accepted"))
        {
            return Err(());
        }
        let task_revision = task.task_revision;
        let project = self
            .registry
            .registered_projects()
            .map_err(|_| ())?
            .into_iter()
            .find(|project| project.project_id == advance.project_id)
            .ok_or(())?;
        validate_project_identity(&project).map_err(|_| ())?;
        let project_root = self.projects_dir.join(&project.project_key);
        require_owned_directory(&project_root, authority.effective_uid()).map_err(|_| ())?;
        let backup = crate::protected_git_backup::ProtectedGitBackup::initialize(
            &project_root,
            authority.effective_uid(),
        )
        .map_err(|_| ())?;
        let review_checkpoint = database
            .backup_checkpoint(
                &advance.project_id,
                &advance.workspace_id,
                "review",
                &advance.review_id,
            )
            .map_err(|_| ())?
            .ok_or(())?;
        if review_checkpoint.task_id.as_deref() != Some(advance.task_id.as_str())
            || review_checkpoint.task_revision != Some(task_revision)
            || review_checkpoint.state_revision != advance.expected_state_revision
        {
            return Err(());
        }
        let protected_gate = self.protected_gate.as_ref().ok_or(())?;
        protected_gate
            .verify(
                protected_gate_envelope,
                &crate::protected_gate::ProtectedGateBinding {
                    project_id: &advance.project_id,
                    workspace_id: &advance.workspace_id,
                    task_id: &advance.task_id,
                    task_revision,
                    state_revision: advance.expected_state_revision,
                    review_id: &advance.review_id,
                    source_fingerprint: &review_checkpoint.source_fingerprint,
                    wrapper_sha256: protected_gate.trusted_wrapper_sha256(),
                },
            )
            .map_err(|_| ())?;
        let receipt = backup
            .commit_snapshot_from_transition(
                &crate::protected_git_backup::BackupCommitMetadata {
                    transition: crate::protected_git_backup::BackupTransition::Advance,
                    project_id: advance.project_id.clone(),
                    workspace_id: advance.workspace_id.clone(),
                    task_id: Some(advance.task_id.clone()),
                    task_revision: Some(task_revision),
                    state_revision: target_state_revision,
                    occurred_at_unix: now,
                    observed_compilation_fingerprint: None,
                    transition_id: Some(advance.review_id.clone()),
                },
                crate::protected_git_backup::BackupTransition::Review,
                &advance.review_id,
            )
            .map_err(|_| ())?;
        let checkpoint = backup_checkpoint_record(
            "advance",
            &advance.review_id,
            &advance.project_id,
            &advance.workspace_id,
            Some(&advance.task_id),
            Some(task_revision),
            target_state_revision,
            &receipt,
        )?;
        let event_digest = sha256(&fingerprint_material);
        let event_hex = lowercase_hex(&event_digest);
        let event = NewAuditEvent {
            contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
            event_id: format!("evt_{}", &event_hex[..26]),
            project_id: advance.project_id.clone(),
            workspace_id: Some(advance.workspace_id.clone()),
            actor_kind: "ci".to_string(),
            actor_subject_id: "authorized-state-transition".to_string(),
            event_kind: "state_transitioned".to_string(),
            outcome: "succeeded".to_string(),
            occurred_at: format!("unix:{}", receipt.occurred_at_unix),
            fingerprint: event_hex.clone(),
            correlation_id: format!("ref_{}", &event_hex[26..52]),
            resource_id: Some(advance.task_id.clone()),
            payload: b"accepted assigned review advanced canonical task".to_vec(),
        };
        database
            .advance_accepted_task_with_checkpoint(advance, &event, &checkpoint)
            .map_err(|_| ())
    }

    fn open_registered_project(
        &self,
        authority: &VerifiedServiceAuthority,
        project_id: &str,
    ) -> Result<ProjectDatabase, ()> {
        let mut matches = self
            .registry
            .registered_projects()
            .map_err(|_| ())?
            .into_iter()
            .filter(|project| project.project_id == project_id);
        let project = matches.next().ok_or(())?;
        if matches.next().is_some() {
            return Err(());
        }
        validate_project_identity(&project).map_err(|_| ())?;
        let project_root = self.projects_dir.join(&project.project_key);
        require_owned_directory(&project_root, authority.effective_uid()).map_err(|_| ())?;
        let database = ProjectDatabase::open_existing_at(&project_root).map_err(|_| ())?;
        require_single_project(&database, &project).map_err(|_| ())?;
        database.recover().map_err(|_| ())?;
        require_database_ownership(&database, authority.effective_uid()).map_err(|_| ())?;
        Ok(database)
    }

    pub(crate) fn submission_source_context(
        &self,
        authority: &VerifiedServiceAuthority,
        project_id: &str,
        workspace_id: &str,
        task_id: &str,
        submission_id: &str,
    ) -> Result<crate::protected_git_backup::ProtectedSourceContext, ()> {
        let _backup_guard = self.backup_transition_lock.lock().map_err(|_| ())?;
        let mut matches = self
            .registry
            .registered_projects()
            .map_err(|_| ())?
            .into_iter()
            .filter(|project| project.project_id == project_id);
        let project = matches.next().ok_or(())?;
        if matches.next().is_some() {
            return Err(());
        }
        validate_project_identity(&project).map_err(|_| ())?;
        let project_root = self.projects_dir.join(&project.project_key);
        require_owned_directory(&project_root, authority.effective_uid()).map_err(|_| ())?;
        let backup = crate::protected_git_backup::ProtectedGitBackup::initialize(
            &project_root,
            authority.effective_uid(),
        )
        .map_err(|_| ())?;
        let context = backup
            .source_context_for_transition(
                crate::protected_git_backup::BackupTransition::Submission,
                submission_id,
            )
            .map_err(|_| ())?;
        if context.project_id != project_id
            || context.workspace_id != workspace_id
            || context.task_id.as_deref() != Some(task_id)
            || context.transition_id.as_deref() != Some(submission_id)
        {
            return Err(());
        }
        Ok(context)
    }
}

pub(crate) fn task_package_subject_sha256(subject_uid: u32) -> String {
    // A new grant or actor label must not let the same OS principal approve
    // its own attestation. The UID comes from the authenticated capability.
    let mut material = b"task-package-subject-v1\0".to_vec();
    material.extend_from_slice(&subject_uid.to_be_bytes());
    lowercase_hex(&sha256(&material))
}

fn official_task_package_authority_matches(
    capability: &AuthorizedCapability,
    actor: CapabilityActor,
    operation: CapabilityOperation,
    project_id: &str,
    workspace_id: &str,
) -> bool {
    capability.actor() == actor
        && capability.channel() == CapabilityChannel::Administrative
        && capability.operation() == operation
        && capability.project_id() == Some(project_id)
        && capability.workspace_id() == Some(workspace_id)
        && capability.task_id().is_none()
}

pub(crate) fn initialize_configured_service_storage(
    authority: &VerifiedServiceAuthority,
) -> Result<ProtectedServiceStorage, ()> {
    let protected_gate =
        crate::protected_gate::ProtectedGateVerifier::from_installed_authority().map_err(|_| ())?;
    configured_state_directory()
        .and_then(|root| {
            ProtectedServiceStorage::initialize_with_protected_gate(
                authority,
                &root,
                Some(protected_gate),
            )
        })
        .map_err(|_| ())
}

fn configured_state_directory() -> Result<PathBuf, ProtectedStorageError> {
    let raw = std::env::var_os(STATE_DIRECTORY_ENV).ok_or(ProtectedStorageError::InvalidRoot)?;
    parse_state_directory(&raw)
}

fn parse_state_directory(raw: &OsStr) -> Result<PathBuf, ProtectedStorageError> {
    let value = raw.to_str().ok_or(ProtectedStorageError::InvalidRoot)?;
    if value.is_empty() || value.contains(':') {
        return Err(ProtectedStorageError::InvalidRoot);
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(ProtectedStorageError::InvalidRoot);
    }
    Ok(path)
}

impl ProtectedServiceStorage {
    #[allow(dead_code)]
    pub(crate) fn initialize(
        authority: &VerifiedServiceAuthority,
        root: &Path,
    ) -> Result<Self, ProtectedStorageError> {
        Self::initialize_with_protected_gate(authority, root, None)
    }

    pub(crate) fn initialize_with_protected_gate(
        authority: &VerifiedServiceAuthority,
        root: &Path,
        protected_gate: Option<crate::protected_gate::ProtectedGateVerifier>,
    ) -> Result<Self, ProtectedStorageError> {
        require_owned_directory(root, authority.effective_uid())?;
        let registry_dir = private_child_directory(root, REGISTRY_DIR_NAME, authority)?;
        let projects_dir = private_child_directory(root, PROJECTS_DIR_NAME, authority)?;
        let migration_staging_dir =
            private_child_directory(root, MIGRATION_STAGING_DIR_NAME, authority)?;

        let registry = ProjectDatabase::initialize_at(&registry_dir)?;
        let recovery = registry.recover()?;
        if recovery.health.schema_version != STORAGE_SCHEMA_VERSION {
            return Err(ProtectedStorageError::Storage);
        }
        require_database_ownership(&registry, authority.effective_uid())?;

        let storage = Self {
            projects_dir,
            migration_staging_dir,
            registry,
            bootstrap_lock: Mutex::new(()),
            backup_transition_lock: Mutex::new(()),
            protected_gate,
        };
        storage.recover_registered_projects(authority)?;
        Ok(storage)
    }

    pub(crate) fn stage_legacy_stream<R: std::io::Read>(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        inventory_document: &[u8],
        expected_inventory_sha256: &str,
        stream: &mut R,
    ) -> Result<crate::legacy_staging::LegacyStageReceipt, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        let authorized = capability.actor() == CapabilityActor::Operator
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::StateAdmin
            && capability.project_id().is_none()
            && capability.workspace_id().is_none()
            && capability.task_id().is_none()
            && self
                .registry
                .authorized_capability_is_current(capability, trusted_now_unix)
                .map_err(|_| ())?;
        if !authorized {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        }
        require_owned_directory(&self.migration_staging_dir, authority.effective_uid())
            .map_err(|_| ())?;
        crate::legacy_staging::stage_legacy_stream(
            &self.migration_staging_dir,
            inventory_document,
            expected_inventory_sha256,
            stream,
        )
        .map_err(|_| ())
    }

    pub(crate) fn normalize_legacy_migration(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        request: &crate::legacy_normalization::LegacyNormalizationRequest,
    ) -> Result<crate::legacy_normalization::LegacyNormalizationReceipt, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        let authorized = capability.actor() == CapabilityActor::Operator
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::StateAdmin
            && capability.project_id().is_none()
            && capability.workspace_id().is_none()
            && capability.task_id().is_none()
            && self
                .registry
                .authorized_capability_is_current(capability, trusted_now_unix)
                .map_err(|_| ())?;
        if !authorized {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        }
        require_owned_directory(&self.migration_staging_dir, authority.effective_uid())
            .map_err(|_| ())?;
        crate::legacy_normalization::normalize_staged_migration(
            &self.migration_staging_dir,
            request,
        )
        .map_err(|_| ())
    }

    pub(crate) fn dry_run_legacy_migration(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        request: &crate::legacy_dry_run::LegacyDryRunRequest,
    ) -> Result<crate::legacy_dry_run::LegacyDryRunReport, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        let authorized = capability.actor() == CapabilityActor::Operator
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::StateAdmin
            && capability.project_id().is_none()
            && capability.workspace_id().is_none()
            && capability.task_id().is_none()
            && self
                .registry
                .authorized_capability_is_current(capability, trusted_now_unix)
                .map_err(|_| ())?;
        if !authorized {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        }
        require_owned_directory(&self.migration_staging_dir, authority.effective_uid())
            .map_err(|_| ())?;
        crate::legacy_dry_run::dry_run_legacy_migration(
            &self.migration_staging_dir,
            &self.registry,
            request,
        )
        .map_err(|_| ())
    }

    pub(crate) fn convert_legacy_migration(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        request: &crate::legacy_conversion::LegacyConversionRequest,
    ) -> Result<crate::legacy_conversion::LegacyConversionReport, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        let authorized = capability.actor() == CapabilityActor::Operator
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::StateAdmin
            && capability.project_id().is_none()
            && capability.workspace_id().is_none()
            && capability.task_id().is_none()
            && self
                .registry
                .authorized_capability_is_current(capability, trusted_now_unix)
                .map_err(|_| ())?;
        if !authorized {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        }
        require_owned_directory(&self.migration_staging_dir, authority.effective_uid())
            .map_err(|_| ())?;
        crate::legacy_conversion::convert_legacy_migration(&self.migration_staging_dir, request)
            .map_err(|_| ())
    }

    pub(crate) fn attest_legacy_migration(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        request: &crate::legacy_attestation::LegacyAttestationRequest,
    ) -> Result<crate::legacy_attestation::LegacyAttestationReceipt, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        let authorized = capability.actor() == CapabilityActor::Ci
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::AttestationCreate
            && capability.project_id() == Some(request.project_id.as_str())
            && capability.workspace_id() == Some(request.workspace_id.as_str())
            && capability.task_id().is_none()
            && self
                .registry
                .authorized_capability_is_current(capability, trusted_now_unix)
                .map_err(|_| ())?;
        if !authorized {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        }
        require_owned_directory(&self.migration_staging_dir, authority.effective_uid())
            .map_err(|_| ())?;
        let mut subject_material = b"migration-verifier-v1\0".to_vec();
        subject_material.extend_from_slice(&capability.subject_uid().to_be_bytes());
        subject_material.extend_from_slice(capability.grant_id().as_bytes());
        let verifier_subject_sha256 =
            crate::legacy_staging::lowercase_hex(&sha256(&subject_material));
        crate::legacy_attestation::attest_legacy_candidate(
            &self.migration_staging_dir,
            request,
            &verifier_subject_sha256,
            trusted_now_unix,
        )
        .map_err(|_| ())
    }

    pub(crate) fn approve_legacy_migration(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        request: &crate::legacy_approval::LegacyApprovalRequest,
    ) -> Result<crate::legacy_approval::LegacyApprovalReceipt, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        let authorized = capability.actor() == CapabilityActor::Operator
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::StateAdmin
            && capability.project_id().is_none()
            && capability.workspace_id().is_none()
            && capability.task_id().is_none()
            && self
                .registry
                .authorized_capability_is_current(capability, trusted_now_unix)
                .map_err(|_| ())?;
        if !authorized {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        }
        require_owned_directory(&self.migration_staging_dir, authority.effective_uid())
            .map_err(|_| ())?;
        let mut subject_material = b"migration-operator-approval-v1\0".to_vec();
        subject_material.extend_from_slice(&capability.subject_uid().to_be_bytes());
        subject_material.extend_from_slice(capability.grant_id().as_bytes());
        let operator_subject_sha256 =
            crate::legacy_staging::lowercase_hex(&sha256(&subject_material));
        crate::legacy_approval::approve_legacy_candidate(
            &self.migration_staging_dir,
            request,
            &operator_subject_sha256,
            trusted_now_unix,
        )
        .map_err(|_| ())
    }

    pub(crate) fn cutover_legacy_migration(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        request: &crate::legacy_cutover::LegacyCutoverRequest,
    ) -> Result<crate::legacy_cutover::LegacyCutoverReceipt, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        let authorized = capability.actor() == CapabilityActor::Operator
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::StateAdmin
            && capability.project_id().is_none()
            && capability.workspace_id().is_none()
            && capability.task_id().is_none()
            && self
                .registry
                .authorized_capability_is_current(capability, trusted_now_unix)
                .map_err(|_| ())?;
        if !authorized {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        }
        let _guard = self.bootstrap_lock.lock().map_err(|_| ())?;
        require_owned_directory(&self.migration_staging_dir, authority.effective_uid())
            .map_err(|_| ())?;
        require_owned_directory(&self.projects_dir, authority.effective_uid()).map_err(|_| ())?;
        let mut subject_material = b"migration-cutover-service-v1\0".to_vec();
        subject_material.extend_from_slice(&authority.effective_uid().to_be_bytes());
        let service_subject_sha256 =
            crate::legacy_staging::lowercase_hex(&sha256(&subject_material));
        crate::legacy_cutover::cutover_legacy_candidate(
            &self.migration_staging_dir,
            &self.projects_dir,
            &self.registry,
            request,
            &service_subject_sha256,
            trusted_now_unix,
        )
        .map_err(|_| ())
    }

    pub(crate) fn grant_managed_capability(
        &self,
        capability: &AuthorizedCapability,
        request: ManagedCapabilityGrantRequest,
    ) -> Result<CapabilityGrantAuditReceipt, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        if !self.installation_admin_is_current(capability, trusted_now_unix)
            || !(MIN_MANAGED_CAPABILITY_LIFETIME_SECONDS..=MAX_MANAGED_CAPABILITY_LIFETIME_SECONDS)
                .contains(&request.lifetime_seconds)
        {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        }
        let Some(expires_at_unix) = trusted_now_unix.checked_add(request.lifetime_seconds) else {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        };
        let _guard = self.bootstrap_lock.lock().map_err(|_| ())?;
        let result = self.registry.grant_managed_capability(
            capability.subject_uid(),
            &NewCapabilityGrant {
                grant_id: request.grant_id,
                secret_hash: request.secret_hash,
                subject_uid: request.subject_uid,
                actor: request.actor,
                channel: request.channel,
                operation: request.operation,
                project_id: request.project_id,
                workspace_id: request.workspace_id,
                task_id: request.task_id,
                not_before_unix: trusted_now_unix,
                expires_at_unix,
                revocation_epoch: request.revocation_epoch,
                granted_at: format!("unix:{trusted_now_unix}"),
            },
            trusted_now_unix,
        );
        match result {
            Ok(receipt) => Ok(receipt),
            Err(_) => {
                self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
                Err(())
            }
        }
    }

    pub(crate) fn revoke_managed_capability(
        &self,
        capability: &AuthorizedCapability,
        grant_id: &str,
        revocation_epoch: u64,
    ) -> Result<CapabilityRevocationAuditReceipt, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        if !self.installation_admin_is_current(capability, trusted_now_unix) {
            self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
            return Err(());
        }
        let _guard = self.bootstrap_lock.lock().map_err(|_| ())?;
        let result = self.registry.revoke_managed_capability(
            capability.subject_uid(),
            grant_id,
            revocation_epoch,
            trusted_now_unix,
        );
        match result {
            Ok(receipt) => Ok(receipt),
            Err(_) => {
                self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
                Err(())
            }
        }
    }

    fn installation_admin_is_current(
        &self,
        capability: &AuthorizedCapability,
        trusted_now_unix: i64,
    ) -> bool {
        capability.actor() == CapabilityActor::Operator
            && capability.channel() == CapabilityChannel::Administrative
            && capability.operation() == CapabilityOperation::InstallationAdmin
            && capability.project_id().is_none()
            && capability.workspace_id().is_none()
            && capability.task_id().is_none()
            && self
                .registry
                .authorized_capability_is_current(capability, trusted_now_unix)
                .unwrap_or(false)
    }

    pub(crate) fn bootstrap_provisional(
        &self,
        authority: &VerifiedServiceAuthority,
        peer_uid: u32,
        project_identity_fingerprint: &[u8; 32],
        workspace_identity_fingerprint: &[u8; 32],
        requested_slug: &str,
    ) -> Result<ProvisionalBootstrapResult, ()> {
        if project_identity_fingerprint.iter().all(|byte| *byte == 0)
            || workspace_identity_fingerprint.iter().all(|byte| *byte == 0)
        {
            return Err(());
        }
        let _guard = self.bootstrap_lock.lock().map_err(|_| ())?;
        let _backup_guard = self.backup_transition_lock.lock().map_err(|_| ())?;
        let registration = match self
            .registry
            .provisional_registration(peer_uid, workspace_identity_fingerprint)
            .map_err(|_| ())?
        {
            Some(registration)
                if registration.project_identity_fingerprint == *project_identity_fingerprint =>
            {
                registration
            }
            Some(_) => return Err(()),
            None => {
                if self
                    .registry
                    .provisional_registration_count(peer_uid)
                    .map_err(|_| ())?
                    >= MAX_PROVISIONAL_PROJECTS_PER_UID
                {
                    return Err(());
                }
                self.create_provisional_registration(
                    authority,
                    peer_uid,
                    project_identity_fingerprint,
                    workspace_identity_fingerprint,
                    requested_slug,
                )?
            }
        };

        let now = trusted_service_unix_time().map_err(|_| ())?;
        let project_root = self.projects_dir.join(&registration.project_key);
        let backup = crate::protected_git_backup::ProtectedGitBackup::initialize(
            &project_root,
            authority.effective_uid(),
        )
        .map_err(|_| ())?;
        let backup_required = !backup
            .has_workspace_baseline(&registration.project_id, &registration.workspace_id)
            .map_err(|_| ())?;
        if !backup_required {
            let database = ProjectDatabase::open_existing_at(&project_root).map_err(|_| ())?;
            if database
                .backup_checkpoint_recovery_required(
                    &registration.project_id,
                    &registration.workspace_id,
                )
                .map_err(|_| ())?
            {
                let canonical_state = database
                    .load_canonical_state(&registration.project_id, &registration.workspace_id)
                    .map_err(|_| ())?;
                for recovered in backup.recover_checkpoint_chain().map_err(|_| ())? {
                    if recovered.metadata.project_id != registration.project_id {
                        return Err(());
                    }
                    if recovered.metadata.workspace_id != registration.workspace_id {
                        continue;
                    }
                    let transition_kind = recovered.metadata.transition.as_str();
                    let transition_id = recovered
                        .metadata
                        .transition_id
                        .as_deref()
                        .unwrap_or("baseline");
                    let checkpoint_exists = database
                        .backup_checkpoint(
                            &registration.project_id,
                            &registration.workspace_id,
                            transition_kind,
                            transition_id,
                        )
                        .map_err(|_| ())?
                        .is_some();
                    if !checkpoint_exists {
                        let domain_transition_exists = match recovered.metadata.transition {
                            crate::protected_git_backup::BackupTransition::Baseline => true,
                            crate::protected_git_backup::BackupTransition::Submission => database
                                .submission_assignment(&registration.project_id, transition_id)
                                .map_err(|_| ())?
                                .is_some_and(|assignment| {
                                    assignment.workspace_id == registration.workspace_id
                                        && recovered.metadata.task_id.as_deref()
                                            == Some(assignment.task_id.as_str())
                                }),
                            crate::protected_git_backup::BackupTransition::Review => database
                                .review_assignment(&registration.project_id, transition_id)
                                .map_err(|_| ())?
                                .is_some_and(|assignment| {
                                    assignment.workspace_id == registration.workspace_id
                                        && recovered.metadata.task_id.as_deref()
                                            == Some(assignment.task_id.as_str())
                                }),
                            crate::protected_git_backup::BackupTransition::Advance => {
                                canonical_state
                                    .tasks
                                    .iter()
                                    .find(|task| {
                                        recovered.metadata.task_id.as_deref()
                                            == Some(task.task_id.as_str())
                                    })
                                    .is_some_and(|task| {
                                        canonical_state.state_revision
                                            >= recovered.metadata.state_revision
                                            && task.status == "completed"
                                            && task.review_status == "accepted"
                                    })
                            }
                        };
                        if !domain_transition_exists {
                            continue;
                        }
                    }
                    persist_backup_checkpoint(
                        &database,
                        transition_kind,
                        transition_id,
                        &recovered.metadata.project_id,
                        &recovered.metadata.workspace_id,
                        recovered.metadata.task_id.as_deref(),
                        recovered.metadata.task_revision,
                        recovered.metadata.state_revision,
                        &recovered.receipt,
                    )?;
                }
            }
        }
        Ok(ProvisionalBootstrapResult {
            compile: self.issue_bootstrap_capability(
                peer_uid,
                &registration,
                CapabilityOperation::CompileObserve,
                now,
            )?,
            projection: self.issue_bootstrap_capability(
                peer_uid,
                &registration,
                CapabilityOperation::ProjectionRefresh,
                now,
            )?,
            project_id: registration.project_id,
            workspace_id: registration.workspace_id,
            backup_required,
        })
    }

    fn create_provisional_registration(
        &self,
        authority: &VerifiedServiceAuthority,
        peer_uid: u32,
        project_identity_fingerprint: &[u8; 32],
        workspace_identity_fingerprint: &[u8; 32],
        requested_slug: &str,
    ) -> Result<ProvisionalProjectRegistration, ()> {
        let existing_project = self
            .registry
            .provisional_project(peer_uid, project_identity_fingerprint)
            .map_err(|_| ())?;
        let project_id = match existing_project.as_ref() {
            Some(project) => project.project_id.clone(),
            None => random_crockford_id("prj_").map_err(|_| ())?,
        };
        let workspace_id = random_crockford_id("wsp_").map_err(|_| ())?;
        let project_token = project_id.strip_prefix("prj_").ok_or(())?;
        let workspace_token = workspace_id.strip_prefix("wsp_").ok_or(())?;
        let slug = sanitize_project_slug(requested_slug);
        let now = trusted_service_unix_time().map_err(|_| ())?;
        let created_at = format!("unix:{now}");
        let project = match existing_project {
            Some(project) => project,
            None => StorageProject {
                project_id: project_id.clone(),
                project_key: format!("{slug}--{project_token}"),
                created_at: created_at.clone(),
            },
        };
        let workspace = StorageWorkspace {
            workspace_id: workspace_id.clone(),
            project_id: project_id.clone(),
            workspace_key: format!("workspace--{workspace_token}"),
            created_at,
        };
        let database = self
            .materialize_project(authority, &project)
            .map_err(|_| ())?;
        CanonicalStateStore::register_workspace(&database, &workspace).map_err(|_| ())?;
        CanonicalStateStore::register_workspace(&self.registry, &workspace).map_err(|_| ())?;
        database
            .initialize_empty_governance_snapshot(
                &project_id,
                &workspace_id,
                &format!("unix:{now}"),
            )
            .map_err(|_| ())?;
        let registration = ProvisionalProjectRegistration {
            peer_uid,
            project_identity_fingerprint: *project_identity_fingerprint,
            workspace_identity_fingerprint: *workspace_identity_fingerprint,
            project_id,
            workspace_id,
            project_key: project.project_key,
            workspace_key: workspace.workspace_key,
            created_at_unix: now,
        };
        self.registry
            .store_provisional_registration(&registration)
            .map_err(|_| ())?;
        Ok(registration)
    }

    fn issue_bootstrap_capability(
        &self,
        peer_uid: u32,
        registration: &ProvisionalProjectRegistration,
        operation: CapabilityOperation,
        now: i64,
    ) -> Result<IssuedLimitedCapability, ()> {
        let capability_id = random_crockford_id("cap_").map_err(|_| ())?;
        let mut token = [0_u8; 32];
        fill_system_random(&mut token).map_err(|_| ())?;
        if token.iter().all(|byte| *byte == 0) {
            return Err(());
        }
        self.registry
            .store_capability_grant(&NewCapabilityGrant {
                grant_id: capability_id.clone(),
                secret_hash: sha256(&token),
                subject_uid: peer_uid,
                actor: CapabilityActor::Wrapper,
                channel: CapabilityChannel::Limited,
                operation,
                project_id: Some(registration.project_id.clone()),
                workspace_id: Some(registration.workspace_id.clone()),
                task_id: None,
                not_before_unix: now,
                expires_at_unix: now + BOOTSTRAP_CAPABILITY_LIFETIME_SECONDS,
                revocation_epoch: 0,
                granted_at: format!("unix:{now}"),
            })
            .map_err(|_| ())?;
        Ok(IssuedLimitedCapability {
            capability_id,
            token,
        })
    }

    // T4.5 is the first production caller. Channel acquisition in T4.3 deliberately
    // cannot materialize protected state from client-controlled bytes.
    #[allow(dead_code)]
    fn materialize_project(
        &self,
        authority: &VerifiedServiceAuthority,
        project: &StorageProject,
    ) -> Result<ProjectDatabase, ProtectedStorageError> {
        validate_project_identity(project)?;
        let registered_projects = self.registry.registered_projects()?;
        let mut is_registered = false;
        for registered in &registered_projects {
            if registered == project {
                is_registered = true;
            } else if registered.project_id == project.project_id
                || registered.project_key == project.project_key
            {
                return Err(ProtectedStorageError::Storage);
            }
        }

        let project_root = self.projects_dir.join(&project.project_key);
        let project_root_exists = match fs::symlink_metadata(&project_root) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(ProtectedStorageError::Storage),
        };
        if !project_root_exists {
            fs::create_dir(&project_root).map_err(|_| ProtectedStorageError::Storage)?;
        }
        require_owned_directory(&project_root, authority.effective_uid())?;

        let database_path = project_root.join("state/state.db");
        let database_exists = match fs::symlink_metadata(&database_path) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(ProtectedStorageError::Storage),
        };
        let database = if database_exists {
            ProjectDatabase::open_existing_at(&project_root)?
        } else if is_registered {
            return Err(ProtectedStorageError::Storage);
        } else {
            require_recoverable_empty_layout(&project_root)?;
            let database = ProjectDatabase::initialize_at(&project_root)?;
            database.ensure_project(project)?;
            database
        };
        require_single_project(&database, project)?;
        database.recover()?;
        require_database_ownership(&database, authority.effective_uid())?;
        crate::protected_git_backup::ProtectedGitBackup::initialize(
            &project_root,
            authority.effective_uid(),
        )
        .map_err(|_| ProtectedStorageError::Storage)?;
        self.registry.ensure_project(project)?;
        Ok(database)
    }

    #[allow(dead_code)]
    pub(crate) fn resolve_authorized_resource(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
    ) -> Result<ResolvedAuthorizedResource, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        match self.resolve_authorized_resource_inner(authority, capability, trusted_now_unix) {
            Ok(resource) => Ok(resource),
            Err(_) => {
                self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
                Err(())
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn audit_rejected_authorized_request(
        &self,
        capability: &AuthorizedCapability,
    ) -> Result<ServiceSecurityAuditReceipt, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        self.audit_rejected_authorized_request_at(capability, trusted_now_unix)
    }

    fn audit_rejected_authorized_request_at(
        &self,
        capability: &AuthorizedCapability,
        trusted_now_unix: i64,
    ) -> Result<ServiceSecurityAuditReceipt, ()> {
        self.registry
            .record_authorized_request_rejection(capability, trusted_now_unix)
            .map_err(|_| ())
    }

    fn resolve_authorized_resource_inner(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        trusted_now_unix: i64,
    ) -> Result<ResolvedAuthorizedResource, ProtectedStorageError> {
        let project_id = capability
            .project_id()
            .ok_or(ProtectedStorageError::ResourceUnavailable)?;
        if capability.task_id().is_some() && capability.workspace_id().is_none() {
            return Err(ProtectedStorageError::ResourceUnavailable);
        }
        if !self
            .registry
            .authorized_capability_is_current(capability, trusted_now_unix)?
        {
            return Err(ProtectedStorageError::ResourceUnavailable);
        }

        let mut matches = self
            .registry
            .registered_projects()?
            .into_iter()
            .filter(|project| project.project_id == project_id);
        let project = matches
            .next()
            .ok_or(ProtectedStorageError::ResourceUnavailable)?;
        if matches.next().is_some() {
            return Err(ProtectedStorageError::Storage);
        }
        validate_project_identity(&project)?;

        let project_root = self.projects_dir.join(&project.project_key);
        require_owned_directory(&project_root, authority.effective_uid())?;
        let database = ProjectDatabase::open_existing_at(&project_root)?;
        require_single_project(&database, &project)?;
        database.recover()?;
        require_database_ownership(&database, authority.effective_uid())?;
        if !database.contains_resource_scope(
            project_id,
            capability.workspace_id(),
            capability.task_id(),
        )? {
            return Err(ProtectedStorageError::ResourceUnavailable);
        }

        Ok(ResolvedAuthorizedResource {
            database,
            project_id: project_id.to_string(),
            workspace_id: capability.workspace_id().map(str::to_string),
            task_id: capability.task_id().map(str::to_string),
        })
    }

    fn recover_registered_projects(
        &self,
        authority: &VerifiedServiceAuthority,
    ) -> Result<(), ProtectedStorageError> {
        for project in self.registry.registered_projects()? {
            validate_project_identity(&project)?;
            let project_root = self.projects_dir.join(&project.project_key);
            require_owned_directory(&project_root, authority.effective_uid())?;
            let database = ProjectDatabase::open_existing_at(&project_root)?;
            require_single_project(&database, &project)?;
            database.recover()?;
            require_database_ownership(&database, authority.effective_uid())?;
            crate::protected_git_backup::ProtectedGitBackup::initialize(
                &project_root,
                authority.effective_uid(),
            )
            .map_err(|_| ProtectedStorageError::Storage)?;
        }
        Ok(())
    }
}

fn sanitize_project_slug(value: &str) -> String {
    let mut slug = String::with_capacity(value.len().min(48));
    let mut previous_dash = false;
    for byte in value.bytes() {
        let normalized = if byte.is_ascii_alphanumeric() {
            byte.to_ascii_lowercase()
        } else {
            b'-'
        };
        if normalized == b'-' {
            if slug.is_empty() || previous_dash {
                continue;
            }
            previous_dash = true;
        } else {
            previous_dash = false;
        }
        slug.push(normalized as char);
        if slug.len() == 48 {
            break;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "rust-project".to_string()
    } else {
        slug
    }
}

fn lowercase_hex(input: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(input.len() * 2);
    for byte in input {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn persist_backup_checkpoint(
    database: &ProjectDatabase,
    transition_kind: &str,
    transition_id: &str,
    project_id: &str,
    workspace_id: &str,
    task_id: Option<&str>,
    task_revision: Option<i64>,
    state_revision: i64,
    receipt: &crate::protected_git_backup::BackupCommitReceipt,
) -> Result<(), ()> {
    let checkpoint = backup_checkpoint_record(
        transition_kind,
        transition_id,
        project_id,
        workspace_id,
        task_id,
        task_revision,
        state_revision,
        receipt,
    )?;
    database
        .record_backup_checkpoint(&checkpoint)
        .map(|_| ())
        .map_err(|_| ())
}

#[allow(clippy::too_many_arguments)]
fn backup_checkpoint_record(
    transition_kind: &str,
    transition_id: &str,
    project_id: &str,
    workspace_id: &str,
    task_id: Option<&str>,
    task_revision: Option<i64>,
    state_revision: i64,
    receipt: &crate::protected_git_backup::BackupCommitReceipt,
) -> Result<BackupCheckpointRecord, ()> {
    Ok(BackupCheckpointRecord {
        transition_kind: transition_kind.to_string(),
        transition_id: transition_id.to_string(),
        project_id: project_id.to_string(),
        workspace_id: workspace_id.to_string(),
        task_id: task_id.map(str::to_string),
        task_revision,
        state_revision,
        commit_id: receipt.commit_id.clone(),
        parent_commit_id: receipt.parent_commit_id.clone(),
        source_fingerprint: receipt.source_fingerprint.clone(),
        entry_count: i64::try_from(receipt.entry_count).map_err(|_| ())?,
        byte_count: i64::try_from(receipt.byte_count).map_err(|_| ())?,
        recorded_at: format!("unix:{}", receipt.occurred_at_unix),
    })
}

fn backup_receipt_from_checkpoint(
    checkpoint: &BackupCheckpointRecord,
) -> Result<crate::protected_git_backup::BackupCommitReceipt, ()> {
    let occurred_at_unix = checkpoint
        .recorded_at
        .strip_prefix("unix:")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(())?;
    Ok(crate::protected_git_backup::BackupCommitReceipt {
        commit_id: checkpoint.commit_id.clone(),
        parent_commit_id: checkpoint.parent_commit_id.clone(),
        source_fingerprint: checkpoint.source_fingerprint.clone(),
        entry_count: u64::try_from(checkpoint.entry_count).map_err(|_| ())?,
        byte_count: u64::try_from(checkpoint.byte_count).map_err(|_| ())?,
        occurred_at_unix,
    })
}

fn trusted_service_unix_time() -> Result<i64, ProtectedStorageError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ProtectedStorageError::Storage)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| ProtectedStorageError::Storage)
}

fn require_database_ownership(
    database: &ProjectDatabase,
    expected_uid: u32,
) -> Result<(), ProtectedStorageError> {
    let state_dir = database
        .database_path()
        .parent()
        .ok_or(ProtectedStorageError::Storage)?;
    require_owned_directory(state_dir, expected_uid)?;
    for path in std::iter::once(database.database_path().to_path_buf()).chain(
        ["-journal", "-wal", "-shm"].into_iter().map(|suffix| {
            let mut path = database.database_path().as_os_str().to_os_string();
            path.push(suffix);
            PathBuf::from(path)
        }),
    ) {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(ProtectedStorageError::Storage);
            }
            Ok(metadata) => require_owner(&metadata, expected_uid)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ProtectedStorageError::Storage),
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn require_recoverable_empty_layout(project_root: &Path) -> Result<(), ProtectedStorageError> {
    let entries = fs::read_dir(project_root)
        .map_err(|_| ProtectedStorageError::Storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ProtectedStorageError::Storage)?;
    if entries.is_empty() {
        return Ok(());
    }
    if entries.len() != 1 || entries[0].file_name() != "state" {
        return Err(ProtectedStorageError::Storage);
    }
    let state_path = entries[0].path();
    let metadata = fs::symlink_metadata(&state_path).map_err(|_| ProtectedStorageError::Storage)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ProtectedStorageError::Storage);
    }
    if fs::read_dir(state_path)
        .map_err(|_| ProtectedStorageError::Storage)?
        .next()
        .is_some()
    {
        return Err(ProtectedStorageError::Storage);
    }
    Ok(())
}

fn require_single_project(
    database: &ProjectDatabase,
    expected: &StorageProject,
) -> Result<(), ProtectedStorageError> {
    let projects = database.registered_projects()?;
    if projects.len() != 1 || projects.first() != Some(expected) {
        return Err(ProtectedStorageError::Storage);
    }
    Ok(())
}

fn validate_project_identity(project: &StorageProject) -> Result<(), ProtectedStorageError> {
    let Some(token) = project.project_id.strip_prefix("prj_") else {
        return Err(ProtectedStorageError::InvalidProjectIdentity);
    };
    if !valid_crockford_token(token) {
        return Err(ProtectedStorageError::InvalidProjectIdentity);
    }
    let Some((slug, key_token)) = project.project_key.rsplit_once("--") else {
        return Err(ProtectedStorageError::InvalidProjectIdentity);
    };
    if key_token != token || !valid_project_slug(slug) {
        return Err(ProtectedStorageError::InvalidProjectIdentity);
    }
    Ok(())
}

fn valid_crockford_token(token: &str) -> bool {
    token.len() == 26
        && token
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z'))
}

fn valid_project_slug(slug: &str) -> bool {
    (1..=48).contains(&slug.len())
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn private_child_directory(
    parent: &Path,
    name: &str,
    authority: &VerifiedServiceAuthority,
) -> Result<PathBuf, ProtectedStorageError> {
    let path = parent.join(name);
    if !path.exists() {
        fs::create_dir(&path).map_err(|_| ProtectedStorageError::Storage)?;
    }
    require_owned_directory(&path, authority.effective_uid())?;
    Ok(path)
}

fn require_owned_directory(path: &Path, expected_uid: u32) -> Result<(), ProtectedStorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ProtectedStorageError::InvalidRoot)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ProtectedStorageError::InvalidRoot);
    }
    require_owner(&metadata, expected_uid)?;
    set_private_directory_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn require_owner(metadata: &fs::Metadata, expected_uid: u32) -> Result<(), ProtectedStorageError> {
    if metadata.uid() != expected_uid {
        return Err(ProtectedStorageError::WrongOwner);
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_owner(
    _metadata: &fs::Metadata,
    _expected_uid: u32,
) -> Result<(), ProtectedStorageError> {
    Err(ProtectedStorageError::WrongOwner)
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), ProtectedStorageError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| ProtectedStorageError::Storage)
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), ProtectedStorageError> {
    Err(ProtectedStorageError::Storage)
}

#[cfg(test)]
#[path = "tests/Service_Storage_test.rs"]
mod service_storage_tests;
