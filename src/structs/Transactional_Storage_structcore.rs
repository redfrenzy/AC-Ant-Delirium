use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use crate::libres::Rusqlite_unit::{
    AC_Connection, AC_Error, AC_OpenFlags, AC_OptionalExtension, AC_Transaction,
    AC_TransactionBehavior, AC_params,
};

use super::{
    reject_public_path_references, validate_serialized_public_document, AtomicRecoveryReport,
    EventWriteOutcome, NewAuditEvent, ProjectDatabase, ProjectStateRoot,
    ProvisionalProjectRegistration, StorageHealth, StorageProject, StorageRecoveryReport,
    TransactionalStorageError, TransactionalStorageResult, AUDIT_EVENT_CONTRACT_VERSION,
    STORAGE_BUSY_TIMEOUT_MILLIS, STORAGE_SCHEMA_VERSION,
};

const INITIAL_SCHEMA: &str = r#"
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
) STRICT;

CREATE TABLE projects (
    project_id TEXT PRIMARY KEY,
    project_key TEXT NOT NULL UNIQUE,
    logical_contract_version TEXT NOT NULL DEFAULT 'canonical-state-v1',
    state_revision INTEGER NOT NULL DEFAULT 0 CHECK (state_revision >= 0),
    active_plan_id TEXT,
    current_task_id TEXT,
    last_event_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_event_sequence >= 0),
    projection_generation INTEGER NOT NULL DEFAULT 0 CHECK (projection_generation >= 0),
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE workspaces (
    workspace_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    workspace_key TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'retired', 'tombstoned')),
    created_at TEXT NOT NULL,
    UNIQUE (project_id, workspace_key)
) STRICT;

CREATE TABLE plans (
    plan_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    plan_revision INTEGER NOT NULL DEFAULT 1 CHECK (plan_revision >= 1),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'tombstoned')),
    payload BLOB NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE tasks (
    task_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    plan_id TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE RESTRICT,
    task_revision INTEGER NOT NULL CHECK (task_revision >= 1),
    status TEXT NOT NULL CHECK (status IN ('blocked', 'available', 'submitted', 'accepted', 'completed', 'cancelled')),
    review_status TEXT NOT NULL CHECK (review_status IN ('not_required', 'pending', 'accepted', 'rejected')),
    payload BLOB NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE submissions (
    submission_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    workspace_id TEXT REFERENCES workspaces(workspace_id) ON DELETE RESTRICT,
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    payload BLOB NOT NULL,
    received_at TEXT NOT NULL,
    UNIQUE (project_id, request_id),
    UNIQUE (project_id, fingerprint)
) STRICT;

CREATE TABLE reviews (
    review_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    submission_id TEXT NOT NULL REFERENCES submissions(submission_id) ON DELETE RESTRICT,
    reviewer_id TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('accepted', 'rejected')),
    payload BLOB NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE compilation_observations (
    observation_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    workspace_id TEXT REFERENCES workspaces(workspace_id) ON DELETE RESTRICT,
    fingerprint TEXT NOT NULL,
    payload BLOB NOT NULL,
    observed_at TEXT NOT NULL,
    UNIQUE (project_id, fingerprint)
) STRICT;

CREATE TABLE attestations (
    attestation_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    observation_id TEXT REFERENCES compilation_observations(observation_id) ON DELETE RESTRICT,
    payload BLOB NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE events (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    sequence INTEGER NOT NULL CHECK (sequence >= 1),
    event_id TEXT NOT NULL UNIQUE,
    workspace_id TEXT REFERENCES workspaces(workspace_id) ON DELETE RESTRICT,
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('wrapper', 'agent', 'reviewer', 'operator', 'service', 'ci')),
    actor_subject_id TEXT NOT NULL,
    event_kind TEXT NOT NULL CHECK (event_kind IN ('compilation_observed', 'submission_recorded', 'review_recorded', 'state_transitioned', 'projection_rendered', 'capability_granted', 'capability_revoked', 'authorization_denied', 'migration_applied', 'recovery_performed')),
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'rejected', 'failed')),
    occurred_at TEXT NOT NULL,
    fingerprint TEXT NOT NULL CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
    correlation_id TEXT NOT NULL,
    resource_id TEXT,
    payload BLOB NOT NULL,
    PRIMARY KEY (project_id, sequence),
    UNIQUE (project_id, event_kind, fingerprint)
) STRICT;

CREATE TABLE projection_versions (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    artifact_kind TEXT NOT NULL CHECK (artifact_kind IN ('current_task', 'governance_errors')),
    generation INTEGER NOT NULL CHECK (generation >= 0),
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    fingerprint TEXT NOT NULL,
    rendered_at TEXT NOT NULL,
    PRIMARY KEY (project_id, artifact_kind)
) STRICT;

CREATE TABLE capability_grants (
    grant_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    subject_id TEXT NOT NULL,
    capability TEXT NOT NULL,
    scope TEXT NOT NULL,
    granted_at TEXT NOT NULL,
    expires_at TEXT
) STRICT;

CREATE TABLE revocations (
    revocation_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    grant_id TEXT NOT NULL REFERENCES capability_grants(grant_id) ON DELETE RESTRICT,
    revoked_at TEXT NOT NULL,
    reason TEXT NOT NULL
) STRICT;

CREATE INDEX events_correlation_idx ON events(project_id, correlation_id);
CREATE INDEX tasks_status_idx ON tasks(project_id, status, review_status);
CREATE INDEX submissions_task_idx ON submissions(project_id, task_id, received_at);
"#;

static ATOMIC_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const SQLITE_SIDECAR_SUFFIXES: [&str; 3] = ["-journal", "-wal", "-shm"];
const STORAGE_CONTENTION_RETRIES: usize = 4;

const STORAGE_SCHEMA_V2: &str = r#"
ALTER TABLE submissions ADD COLUMN task_revision INTEGER NOT NULL DEFAULT 1 CHECK (task_revision >= 1);
ALTER TABLE submissions ADD COLUMN base_revision TEXT;
ALTER TABLE submissions ADD COLUMN contract_version TEXT NOT NULL DEFAULT 'submission-v1' CHECK (contract_version = 'submission-v1');
ALTER TABLE events ADD COLUMN contract_version TEXT NOT NULL DEFAULT 'audit-event-v1' CHECK (contract_version = 'audit-event-v1');
ALTER TABLE projection_versions ADD COLUMN contract_version TEXT NOT NULL DEFAULT 'agent-projection-v1' CHECK (contract_version = 'agent-projection-v1');
"#;

const STORAGE_SCHEMA_V3: &str = r#"
ALTER TABLE revocations RENAME TO revocations_legacy_v1;
ALTER TABLE capability_grants RENAME TO capability_grants_legacy_v1;

CREATE TABLE capability_subject_epochs (
    subject_uid INTEGER NOT NULL CHECK (subject_uid >= 0),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('wrapper', 'agent', 'reviewer', 'operator', 'service', 'ci')),
    epoch INTEGER NOT NULL CHECK (epoch >= 0),
    PRIMARY KEY (subject_uid, actor_kind)
) STRICT;

CREATE TABLE capability_grants (
    grant_id TEXT PRIMARY KEY,
    secret_hash BLOB NOT NULL CHECK (length(secret_hash) = 32),
    subject_uid INTEGER NOT NULL CHECK (subject_uid >= 0),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('wrapper', 'agent', 'reviewer', 'operator', 'service', 'ci')),
    channel_kind TEXT NOT NULL CHECK (channel_kind IN ('limited', 'administrative')),
    operation TEXT NOT NULL CHECK (operation IN ('compile.observe', 'projection.refresh', 'task.current.read', 'submission.create', 'submission.read.assigned', 'review.decide.assigned', 'attestation.create', 'state.transition', 'state.admin', 'installation.admin', 'diagnostic.private')),
    project_id TEXT REFERENCES projects(project_id) ON DELETE RESTRICT,
    workspace_id TEXT,
    task_id TEXT,
    not_before_unix INTEGER NOT NULL,
    expires_at_unix INTEGER NOT NULL CHECK (expires_at_unix > not_before_unix),
    revocation_epoch INTEGER NOT NULL CHECK (revocation_epoch >= 0),
    granted_at TEXT NOT NULL,
    FOREIGN KEY (subject_uid, actor_kind) REFERENCES capability_subject_epochs(subject_uid, actor_kind) ON DELETE RESTRICT
) STRICT;

CREATE TABLE revocations (
    revocation_id TEXT PRIMARY KEY,
    grant_id TEXT NOT NULL UNIQUE REFERENCES capability_grants(grant_id) ON DELETE RESTRICT,
    revocation_epoch INTEGER NOT NULL CHECK (revocation_epoch >= 0),
    revoked_at TEXT NOT NULL,
    reason TEXT NOT NULL
) STRICT;

CREATE TABLE capability_request_receipts (
    grant_id TEXT NOT NULL REFERENCES capability_grants(grant_id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    request_fingerprint BLOB NOT NULL CHECK (length(request_fingerprint) = 32),
    authorized_at_unix INTEGER NOT NULL,
    PRIMARY KEY (grant_id, request_id)
) STRICT;

CREATE INDEX capability_scope_idx ON capability_grants(project_id, workspace_id, task_id);
CREATE INDEX capability_subject_idx ON capability_grants(subject_uid, actor_kind, revocation_epoch);
"#;

const STORAGE_SCHEMA_V4: &str = r#"
CREATE TABLE service_security_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    last_sequence INTEGER NOT NULL CHECK (last_sequence >= 0)
) STRICT;

INSERT INTO service_security_state(singleton, last_sequence) VALUES (1, 0);

CREATE TABLE service_security_events (
    sequence INTEGER PRIMARY KEY CHECK (sequence >= 1),
    event_id TEXT NOT NULL UNIQUE,
    correlation_id TEXT NOT NULL UNIQUE,
    event_kind TEXT NOT NULL CHECK (event_kind IN ('authorization_denied', 'request_rejected')),
    outcome TEXT NOT NULL CHECK (outcome = 'rejected'),
    peer_uid INTEGER NOT NULL CHECK (peer_uid >= 0),
    channel_kind TEXT NOT NULL CHECK (channel_kind IN ('limited', 'administrative')),
    actor_kind TEXT CHECK (actor_kind IN ('wrapper', 'agent', 'reviewer', 'operator', 'service', 'ci')),
    operation TEXT CHECK (operation IN ('compile.observe', 'projection.refresh', 'task.current.read', 'submission.create', 'submission.read.assigned', 'review.decide.assigned', 'attestation.create', 'state.transition', 'state.admin', 'installation.admin', 'diagnostic.private')),
    project_id TEXT,
    workspace_id TEXT,
    task_id TEXT,
    occurred_at_unix INTEGER NOT NULL CHECK (occurred_at_unix >= 0),
    CHECK (workspace_id IS NULL OR project_id IS NOT NULL),
    CHECK (task_id IS NULL OR (project_id IS NOT NULL AND workspace_id IS NOT NULL)),
    CHECK (
        (event_kind = 'authorization_denied' AND actor_kind IS NULL AND operation IS NULL AND project_id IS NULL AND workspace_id IS NULL AND task_id IS NULL)
        OR
        (event_kind = 'request_rejected' AND actor_kind IS NOT NULL AND operation IS NOT NULL)
    )
) STRICT;

CREATE INDEX service_security_time_idx ON service_security_events(occurred_at_unix, sequence);
CREATE INDEX service_security_scope_idx ON service_security_events(project_id, workspace_id, task_id);
"#;

const STORAGE_SCHEMA_V5: &str = r#"
CREATE TABLE governance_snapshots (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE RESTRICT,
    contract_version TEXT NOT NULL CHECK (contract_version = 'canonical-governance-snapshot-v1'),
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    fingerprint TEXT NOT NULL CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
    payload BLOB NOT NULL,
    observed_at TEXT NOT NULL,
    PRIMARY KEY (project_id, workspace_id)
) STRICT;

CREATE INDEX governance_snapshots_fingerprint_idx
ON governance_snapshots(project_id, fingerprint);
"#;

const STORAGE_SCHEMA_V6: &str = r#"
CREATE TABLE provisional_project_registrations (
    peer_uid INTEGER NOT NULL CHECK (peer_uid >= 0),
    identity_fingerprint BLOB NOT NULL CHECK (length(identity_fingerprint) = 32),
    project_id TEXT NOT NULL UNIQUE REFERENCES projects(project_id) ON DELETE RESTRICT,
    workspace_id TEXT NOT NULL UNIQUE REFERENCES workspaces(workspace_id) ON DELETE RESTRICT,
    project_key TEXT NOT NULL UNIQUE,
    workspace_key TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL CHECK (created_at_unix >= 0),
    PRIMARY KEY (peer_uid, identity_fingerprint)
) STRICT;

CREATE INDEX provisional_project_peer_idx
ON provisional_project_registrations(peer_uid, created_at_unix);
"#;

const STORAGE_SCHEMA_V7: &str = r#"
CREATE TABLE legacy_migration_records (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE RESTRICT,
    migration_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('history', 'problem', 'transition')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_key_sha256 TEXT NOT NULL CHECK (length(source_key_sha256) = 64 AND source_key_sha256 NOT GLOB '*[^0-9a-f]*'),
    payload_sha256 TEXT NOT NULL CHECK (length(payload_sha256) = 64 AND payload_sha256 NOT GLOB '*[^0-9a-f]*'),
    payload BLOB NOT NULL,
    PRIMARY KEY (project_id, migration_id, record_kind, ordinal)
) STRICT;

CREATE INDEX legacy_migration_records_digest_idx
ON legacy_migration_records(project_id, migration_id, record_kind, payload_sha256);
"#;

const STORAGE_SCHEMA_V8: &str = r#"
CREATE TABLE initial_capability_provisioning (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    provisioning_id TEXT NOT NULL UNIQUE,
    manifest_sha256 BLOB NOT NULL CHECK (length(manifest_sha256) = 32),
    applied_at_unix INTEGER NOT NULL CHECK (applied_at_unix >= 0),
    grant_count INTEGER NOT NULL CHECK (grant_count >= 1 AND grant_count <= 16)
) STRICT;
"#;

// Migration-scoped capabilities must exist before a project cutover can register
// the canonical project row. Keeping the opaque scope on the grant, but removing
// the premature project foreign key, breaks that attestation/cutover cycle without
// weakening resource resolution: handlers still resolve projects only after a
// freshly authorized capability reaches the protected service boundary.
const STORAGE_SCHEMA_V9: &str = r#"
DROP INDEX capability_scope_idx;
DROP INDEX capability_subject_idx;

ALTER TABLE capability_request_receipts RENAME TO capability_request_receipts_v8;
ALTER TABLE revocations RENAME TO revocations_v8;
ALTER TABLE capability_grants RENAME TO capability_grants_v8;

CREATE TABLE capability_grants (
    grant_id TEXT PRIMARY KEY,
    secret_hash BLOB NOT NULL CHECK (length(secret_hash) = 32),
    subject_uid INTEGER NOT NULL CHECK (subject_uid >= 0),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('wrapper', 'agent', 'reviewer', 'operator', 'service', 'ci')),
    channel_kind TEXT NOT NULL CHECK (channel_kind IN ('limited', 'administrative')),
    operation TEXT NOT NULL CHECK (operation IN ('compile.observe', 'projection.refresh', 'task.current.read', 'submission.create', 'submission.read.assigned', 'review.decide.assigned', 'attestation.create', 'state.transition', 'state.admin', 'installation.admin', 'diagnostic.private')),
    project_id TEXT,
    workspace_id TEXT,
    task_id TEXT,
    not_before_unix INTEGER NOT NULL,
    expires_at_unix INTEGER NOT NULL CHECK (expires_at_unix > not_before_unix),
    revocation_epoch INTEGER NOT NULL CHECK (revocation_epoch >= 0),
    granted_at TEXT NOT NULL,
    FOREIGN KEY (subject_uid, actor_kind) REFERENCES capability_subject_epochs(subject_uid, actor_kind) ON DELETE RESTRICT,
    CHECK (workspace_id IS NULL OR project_id IS NOT NULL),
    CHECK (task_id IS NULL OR (project_id IS NOT NULL AND workspace_id IS NOT NULL))
) STRICT;

CREATE TABLE revocations (
    revocation_id TEXT PRIMARY KEY,
    grant_id TEXT NOT NULL UNIQUE REFERENCES capability_grants(grant_id) ON DELETE RESTRICT,
    revocation_epoch INTEGER NOT NULL CHECK (revocation_epoch >= 0),
    revoked_at TEXT NOT NULL,
    reason TEXT NOT NULL
) STRICT;

CREATE TABLE capability_request_receipts (
    grant_id TEXT NOT NULL REFERENCES capability_grants(grant_id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    request_fingerprint BLOB NOT NULL CHECK (length(request_fingerprint) = 32),
    authorized_at_unix INTEGER NOT NULL,
    PRIMARY KEY (grant_id, request_id)
) STRICT;

INSERT INTO capability_grants SELECT * FROM capability_grants_v8;
INSERT INTO revocations SELECT * FROM revocations_v8;
INSERT INTO capability_request_receipts SELECT * FROM capability_request_receipts_v8;

DROP TABLE capability_request_receipts_v8;
DROP TABLE revocations_v8;
DROP TABLE capability_grants_v8;

CREATE INDEX capability_scope_idx ON capability_grants(project_id, workspace_id, task_id);
CREATE INDEX capability_subject_idx ON capability_grants(subject_uid, actor_kind, revocation_epoch);

CREATE TABLE capability_management_events (
    sequence INTEGER PRIMARY KEY CHECK (sequence >= 1),
    event_id TEXT NOT NULL UNIQUE,
    correlation_id TEXT NOT NULL UNIQUE,
    action_kind TEXT NOT NULL CHECK (action_kind IN ('grant', 'revoke')),
    operator_uid INTEGER NOT NULL CHECK (operator_uid >= 1),
    grant_id TEXT NOT NULL REFERENCES capability_grants(grant_id) ON DELETE RESTRICT,
    subject_uid INTEGER NOT NULL CHECK (subject_uid >= 1),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('wrapper', 'agent', 'reviewer', 'ci')),
    channel_kind TEXT NOT NULL CHECK (channel_kind IN ('limited', 'administrative')),
    operation TEXT NOT NULL CHECK (operation IN ('compile.observe', 'projection.refresh', 'task.current.read', 'submission.create', 'submission.read.assigned', 'review.decide.assigned', 'attestation.create')),
    project_id TEXT,
    workspace_id TEXT,
    task_id TEXT,
    occurred_at_unix INTEGER NOT NULL CHECK (occurred_at_unix >= 0),
    CHECK (workspace_id IS NULL OR project_id IS NOT NULL),
    CHECK (task_id IS NULL OR (project_id IS NOT NULL AND workspace_id IS NOT NULL))
) STRICT;

CREATE INDEX capability_management_time_idx
ON capability_management_events(occurred_at_unix, sequence);
CREATE INDEX capability_management_scope_idx
ON capability_management_events(project_id, workspace_id, task_id);
"#;

impl From<AC_Error> for TransactionalStorageError {
    fn from(error: AC_Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl ProjectDatabase {
    pub fn initialize(state_root: &ProjectStateRoot) -> TransactionalStorageResult<Self> {
        Self::initialize_at(state_root.root())
    }

    pub fn initialize_at(state_root: &Path) -> TransactionalStorageResult<Self> {
        Self::open_at(state_root, true)
    }

    pub fn open_existing_at(state_root: &Path) -> TransactionalStorageResult<Self> {
        Self::open_at(state_root, false)
    }

    fn open_at(state_root: &Path, allow_create: bool) -> TransactionalStorageResult<Self> {
        let state_dir = state_root.join("state");
        match fs::symlink_metadata(&state_dir) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(TransactionalStorageError::Integrity(
                    "storage directory is not a regular directory".to_string(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && allow_create => {
                fs::create_dir_all(&state_dir)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(TransactionalStorageError::Integrity(
                    "registered project storage directory is unavailable".to_string(),
                ));
            }
            Err(error) => return Err(error.into()),
        }
        set_directory_permissions(&state_dir)?;

        let database = Self {
            database_path: state_dir.join("state.db"),
        };
        let database_exists = match fs::symlink_metadata(&database.database_path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(TransactionalStorageError::Integrity(
                    "storage database is not a regular file".to_string(),
                ));
            }
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        if !allow_create && !database_exists {
            return Err(TransactionalStorageError::Integrity(
                "registered project database is unavailable".to_string(),
            ));
        }
        let mut connection = database.open_initial_connection_with_retry(allow_create)?;
        database.apply_migrations_with_retry(&mut connection)?;
        let health = database.health_for_connection(&connection)?;
        validate_health(&health)?;
        database.harden_database_files()?;
        Ok(database)
    }

    pub fn registered_projects(&self) -> TransactionalStorageResult<Vec<StorageProject>> {
        let connection = self.open_connection(false)?;
        let mut statement = connection.prepare(
            "SELECT project_id, project_key, created_at FROM projects ORDER BY project_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(StorageProject {
                project_id: row.get(0)?,
                project_key: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        let mut projects = Vec::new();
        for row in rows {
            projects.push(row?);
        }
        Ok(projects)
    }

    pub fn provisional_registration(
        &self,
        peer_uid: u32,
        identity_fingerprint: &[u8; 32],
    ) -> TransactionalStorageResult<Option<ProvisionalProjectRegistration>> {
        let connection = self.open_connection(false)?;
        connection
            .query_row(
                "SELECT project_id, workspace_id, project_key, workspace_key, created_at_unix FROM provisional_project_registrations WHERE peer_uid = ?1 AND identity_fingerprint = ?2",
                AC_params![i64::from(peer_uid), identity_fingerprint.as_slice()],
                |row| {
                    Ok(ProvisionalProjectRegistration {
                        peer_uid,
                        identity_fingerprint: *identity_fingerprint,
                        project_id: row.get(0)?,
                        workspace_id: row.get(1)?,
                        project_key: row.get(2)?,
                        workspace_key: row.get(3)?,
                        created_at_unix: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn provisional_registration_count(&self, peer_uid: u32) -> TransactionalStorageResult<i64> {
        let connection = self.open_connection(false)?;
        Ok(connection.query_row(
            "SELECT count(*) FROM provisional_project_registrations WHERE peer_uid = ?1",
            AC_params![i64::from(peer_uid)],
            |row| row.get(0),
        )?)
    }

    pub fn store_provisional_registration(
        &self,
        registration: &ProvisionalProjectRegistration,
    ) -> TransactionalStorageResult<()> {
        if registration
            .identity_fingerprint
            .iter()
            .all(|byte| *byte == 0)
            || registration.created_at_unix < 0
        {
            return Err(TransactionalStorageError::InvalidInput(
                "invalid provisional registration".to_string(),
            ));
        }
        self.with_immediate_transaction(|transaction| {
            transaction.execute(
                "INSERT INTO provisional_project_registrations(peer_uid, identity_fingerprint, project_id, workspace_id, project_key, workspace_key, created_at_unix) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                AC_params![
                    i64::from(registration.peer_uid),
                    registration.identity_fingerprint.as_slice(),
                    registration.project_id,
                    registration.workspace_id,
                    registration.project_key,
                    registration.workspace_key,
                    registration.created_at_unix,
                ],
            )?;
            Ok(())
        })
    }

    pub fn contains_resource_scope(
        &self,
        project_id: &str,
        workspace_id: Option<&str>,
        task_id: Option<&str>,
    ) -> TransactionalStorageResult<bool> {
        validate_required("project_id", project_id)?;
        if task_id.is_some() && workspace_id.is_none() {
            return Ok(false);
        }
        let connection = self.open_connection(false)?;
        let project_count: i64 = connection.query_row(
            "SELECT count(*) FROM projects WHERE project_id = ?1",
            AC_params![project_id],
            |row| row.get(0),
        )?;
        if project_count != 1 {
            return Ok(false);
        }
        if let Some(workspace_id) = workspace_id {
            validate_required("workspace_id", workspace_id)?;
            let workspace_count: i64 = connection.query_row(
                "SELECT count(*) FROM workspaces WHERE workspace_id = ?1 AND project_id = ?2 AND status = 'active'",
                AC_params![workspace_id, project_id],
                |row| row.get(0),
            )?;
            if workspace_count != 1 {
                return Ok(false);
            }
        }
        if let Some(task_id) = task_id {
            validate_required("task_id", task_id)?;
            let task_count: i64 = connection.query_row(
                "SELECT count(*) FROM tasks WHERE task_id = ?1 AND project_id = ?2",
                AC_params![task_id, project_id],
                |row| row.get(0),
            )?;
            if task_count != 1 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn health(&self) -> TransactionalStorageResult<StorageHealth> {
        let connection = self.open_connection(false)?;
        let health = self.health_for_connection(&connection)?;
        validate_health(&health)?;
        self.harden_database_files()?;
        Ok(health)
    }

    pub fn recover(&self) -> TransactionalStorageResult<StorageRecoveryReport> {
        let connection = self.open_connection(false)?;
        let health = self.health_for_connection(&connection)?;
        validate_health(&health)?;
        let checkpoint = connection.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        self.harden_database_files()?;
        Ok(StorageRecoveryReport {
            health,
            wal_busy: checkpoint.0,
            wal_frames: checkpoint.1,
            wal_frames_checkpointed: checkpoint.2,
        })
    }

    pub fn ensure_project(&self, project: &StorageProject) -> TransactionalStorageResult<()> {
        validate_required("project_id", &project.project_id)?;
        validate_required("project_key", &project.project_key)?;
        validate_required("created_at", &project.created_at)?;
        self.with_immediate_transaction(|transaction| {
            transaction.execute(
                "INSERT INTO projects(project_id, project_key, created_at) VALUES (?1, ?2, ?3) ON CONFLICT(project_id) DO NOTHING",
                AC_params![project.project_id, project.project_key, project.created_at],
            )?;
            let persisted_key: String = transaction.query_row(
                "SELECT project_key FROM projects WHERE project_id = ?1",
                AC_params![project.project_id],
                |row| row.get(0),
            )?;
            if persisted_key != project.project_key {
                return Err(TransactionalStorageError::Integrity(
                    "project_id is already bound to another project_key".to_string(),
                ));
            }
            Ok(())
        })
    }

    pub fn record_event(
        &self,
        event: &NewAuditEvent,
    ) -> TransactionalStorageResult<EventWriteOutcome> {
        validate_event(event)?;
        self.with_immediate_transaction(|transaction| {
            self.record_event_in_transaction(transaction, event)
        })
    }

    pub fn record_event_in_transaction(
        &self,
        transaction: &AC_Transaction<'_>,
        event: &NewAuditEvent,
    ) -> TransactionalStorageResult<EventWriteOutcome> {
        validate_event(event)?;
        let existing = transaction
            .query_row(
                "SELECT sequence FROM events WHERE project_id = ?1 AND event_kind = ?2 AND fingerprint = ?3",
                AC_params![event.project_id, event.event_kind, event.fingerprint],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(sequence) = existing {
            return Ok(EventWriteOutcome::Duplicate { sequence });
        }

        let last_sequence: i64 = transaction.query_row(
            "SELECT last_event_sequence FROM projects WHERE project_id = ?1",
            AC_params![event.project_id],
            |row| row.get(0),
        )?;
        let sequence = last_sequence.checked_add(1).ok_or_else(|| {
            TransactionalStorageError::Integrity(
                "audit event sequence exhausted its integer range".to_string(),
            )
        })?;

        transaction.execute(
            "INSERT INTO events(project_id, sequence, contract_version, event_id, workspace_id, actor_kind, actor_subject_id, event_kind, outcome, occurred_at, fingerprint, correlation_id, resource_id, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            AC_params![
                event.project_id,
                sequence,
                event.contract_version,
                event.event_id,
                event.workspace_id,
                event.actor_kind,
                event.actor_subject_id,
                event.event_kind,
                event.outcome,
                event.occurred_at,
                event.fingerprint,
                event.correlation_id,
                event.resource_id,
                event.payload,
            ],
        )?;
        let updated = transaction.execute(
            "UPDATE projects SET last_event_sequence = ?2 WHERE project_id = ?1 AND last_event_sequence = ?3",
            AC_params![event.project_id, sequence, last_sequence],
        )?;
        if updated != 1 {
            return Err(TransactionalStorageError::Integrity(
                "audit sequence update did not affect exactly one project".to_string(),
            ));
        }
        Ok(EventWriteOutcome::Inserted { sequence })
    }

    pub fn with_immediate_transaction<T, F>(&self, operation: F) -> TransactionalStorageResult<T>
    where
        F: FnOnce(&AC_Transaction<'_>) -> TransactionalStorageResult<T>,
    {
        let mut connection = self.open_connection(false)?;
        let transaction =
            connection.transaction_with_behavior(AC_TransactionBehavior::Immediate)?;
        let output = match operation(&transaction) {
            Ok(output) => output,
            Err(error) => {
                drop(transaction);
                self.harden_database_files()?;
                return Err(error);
            }
        };
        let commit_result = transaction.commit();
        self.harden_database_files()?;
        commit_result?;
        Ok(output)
    }

    pub fn event_count(&self, project_id: &str) -> TransactionalStorageResult<i64> {
        let connection = self.open_connection(false)?;
        Ok(connection.query_row(
            "SELECT count(*) FROM events WHERE project_id = ?1",
            AC_params![project_id],
            |row| row.get(0),
        )?)
    }

    pub fn last_event_sequence(&self, project_id: &str) -> TransactionalStorageResult<i64> {
        let connection = self.open_connection(false)?;
        Ok(connection.query_row(
            "SELECT last_event_sequence FROM projects WHERE project_id = ?1",
            AC_params![project_id],
            |row| row.get(0),
        )?)
    }

    pub(crate) fn open_connection(
        &self,
        allow_create: bool,
    ) -> TransactionalStorageResult<AC_Connection> {
        self.validate_database_sidecars(allow_create)?;
        let mut flags = AC_OpenFlags::SQLITE_OPEN_READ_WRITE;
        if allow_create {
            flags |= AC_OpenFlags::SQLITE_OPEN_CREATE;
        }
        let connection = AC_Connection::open_with_flags(&self.database_path, flags)?;
        connection.busy_timeout(Duration::from_millis(STORAGE_BUSY_TIMEOUT_MILLIS))?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA foreign_keys = ON;
             PRAGMA trusted_schema = OFF;",
        )?;
        self.harden_database_files()?;
        Ok(connection)
    }

    fn open_initial_connection_with_retry(
        &self,
        allow_create: bool,
    ) -> TransactionalStorageResult<AC_Connection> {
        for attempt in 0..=STORAGE_CONTENTION_RETRIES {
            let creating_database = if allow_create {
                match fs::symlink_metadata(&self.database_path) {
                    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                        return Err(TransactionalStorageError::Integrity(
                            "storage database is not a regular file".to_string(),
                        ));
                    }
                    Ok(_) => false,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                    Err(error) => return Err(error.into()),
                }
            } else {
                false
            };
            match self.open_connection(creating_database) {
                Ok(connection) => return Ok(connection),
                Err(error)
                    if attempt < STORAGE_CONTENTION_RETRIES
                        && is_transient_storage_contention(&error) =>
                {
                    std::thread::sleep(Duration::from_millis(25 * (attempt as u64 + 1)));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("bounded initial connection retry always returns")
    }

    fn apply_migrations(&self, connection: &mut AC_Connection) -> TransactionalStorageResult<()> {
        let transaction =
            connection.transaction_with_behavior(AC_TransactionBehavior::Immediate)?;
        let mut version: i32 =
            transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > STORAGE_SCHEMA_VERSION {
            return Err(TransactionalStorageError::UnsupportedSchema {
                found: version,
                supported: STORAGE_SCHEMA_VERSION,
            });
        }
        if version == STORAGE_SCHEMA_VERSION {
            transaction.commit()?;
            return Ok(());
        }

        if version == 0 {
            transaction.execute_batch(INITIAL_SCHEMA)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (1, ?1)",
                AC_params!["storage-v1"],
            )?;
            transaction.pragma_update(None, "user_version", 1)?;
            version = 1;
        }
        if version == 1 {
            transaction.execute_batch(STORAGE_SCHEMA_V2)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (2, ?1)",
                AC_params!["storage-v2-separated-contract-stores"],
            )?;
            transaction.pragma_update(None, "user_version", 2)?;
            version = 2;
        }
        if version == 2 {
            transaction.execute_batch(STORAGE_SCHEMA_V3)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (3, ?1)",
                AC_params!["storage-v3-capability-authorization"],
            )?;
            transaction.pragma_update(None, "user_version", 3)?;
            version = 3;
        }
        if version == 3 {
            transaction.execute_batch(STORAGE_SCHEMA_V4)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (4, ?1)",
                AC_params!["storage-v4-service-security-journal"],
            )?;
            transaction.pragma_update(None, "user_version", 4)?;
            version = 4;
        }
        if version == 4 {
            transaction.execute_batch(STORAGE_SCHEMA_V5)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (5, ?1)",
                AC_params!["storage-v5-canonical-governance-snapshots"],
            )?;
            transaction.pragma_update(None, "user_version", 5)?;
            version = 5;
        }
        if version == 5 {
            transaction.execute_batch(STORAGE_SCHEMA_V6)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (6, ?1)",
                AC_params!["storage-v6-provisional-bootstrap"],
            )?;
            transaction.pragma_update(None, "user_version", 6)?;
            version = 6;
        }
        if version == 6 {
            transaction.execute_batch(STORAGE_SCHEMA_V7)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (7, ?1)",
                AC_params!["storage-v7-protected-legacy-migration-records"],
            )?;
            transaction.pragma_update(None, "user_version", 7)?;
            version = 7;
        }
        if version == 7 {
            transaction.execute_batch(STORAGE_SCHEMA_V8)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (8, ?1)",
                AC_params!["storage-v8-initial-capability-provisioning"],
            )?;
            transaction.pragma_update(None, "user_version", 8)?;
            version = 8;
        }
        if version == 8 {
            transaction.execute_batch(STORAGE_SCHEMA_V9)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (9, ?1)",
                AC_params!["storage-v9-pre-cutover-capability-management"],
            )?;
            transaction.pragma_update(None, "user_version", 9)?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn apply_migrations_with_retry(
        &self,
        connection: &mut AC_Connection,
    ) -> TransactionalStorageResult<()> {
        for attempt in 0..=STORAGE_CONTENTION_RETRIES {
            match self.apply_migrations(connection) {
                Ok(()) => return Ok(()),
                Err(error)
                    if attempt < STORAGE_CONTENTION_RETRIES
                        && is_transient_storage_contention(&error) =>
                {
                    std::thread::sleep(Duration::from_millis(25 * (attempt as u64 + 1)));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("bounded migration retry always returns")
    }

    fn health_for_connection(
        &self,
        connection: &AC_Connection,
    ) -> TransactionalStorageResult<StorageHealth> {
        let schema_version = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let journal_mode =
            connection.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))?;
        let synchronous =
            connection.query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))?;
        let foreign_keys =
            connection.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))? != 0;
        let trusted_schema =
            connection.query_row("PRAGMA trusted_schema", [], |row| row.get::<_, i64>(0))? != 0;
        let quick_check =
            connection.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))?;
        let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
        let mut rows = statement.query([])?;
        let mut foreign_key_violations = 0usize;
        while rows.next()?.is_some() {
            foreign_key_violations += 1;
        }
        Ok(StorageHealth {
            schema_version,
            journal_mode,
            synchronous,
            foreign_keys,
            trusted_schema,
            quick_check,
            foreign_key_violations,
        })
    }

    fn harden_database_files(&self) -> TransactionalStorageResult<()> {
        for path in std::iter::once(self.database_path.clone()).chain(
            SQLITE_SIDECAR_SUFFIXES
                .into_iter()
                .map(|suffix| path_with_suffix(&self.database_path, suffix)),
        ) {
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                    return Err(TransactionalStorageError::Integrity(
                        "storage file is not a regular file".to_string(),
                    ));
                }
                Ok(_) => set_file_permissions(&path)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn validate_database_sidecars(
        &self,
        creating_database: bool,
    ) -> TransactionalStorageResult<()> {
        for suffix in SQLITE_SIDECAR_SUFFIXES {
            let sidecar = path_with_suffix(&self.database_path, suffix);
            match fs::symlink_metadata(sidecar) {
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                    return Err(TransactionalStorageError::Integrity(
                        "storage sidecar is not a regular file".to_string(),
                    ));
                }
                Ok(_) if creating_database => {
                    match fs::symlink_metadata(&self.database_path) {
                        Ok(database)
                            if !database.file_type().is_symlink() && database.is_file() =>
                        {
                            // Another initializer won the create race. From this
                            // point the sidecar belongs to an existing database.
                        }
                        Ok(_) => {
                            return Err(TransactionalStorageError::Integrity(
                                "storage database is not a regular file".to_string(),
                            ));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            return Err(TransactionalStorageError::Integrity(
                                "new storage has an orphan SQLite sidecar".to_string(),
                            ));
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

fn is_transient_storage_contention(error: &TransactionalStorageError) -> bool {
    matches!(
        error,
        TransactionalStorageError::Database(message)
            if message.contains("database is locked") || message.contains("database is busy")
    )
}

pub fn atomic_replace(path: &Path, contents: &[u8]) -> TransactionalStorageResult<()> {
    let parent = path.parent().ok_or_else(|| {
        TransactionalStorageError::InvalidInput(
            "atomic destination must have a parent directory".to_string(),
        )
    })?;
    fs::create_dir_all(parent)?;
    let temporary_path = unique_temporary_path(path)?;
    let result = write_and_replace(path, &temporary_path, contents);
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

pub(crate) fn atomic_replace_if_changed(
    path: &Path,
    contents: &[u8],
) -> TransactionalStorageResult<bool> {
    if unchanged_regular_file(path, contents)? {
        return Ok(false);
    }
    atomic_replace(path, contents)?;
    Ok(true)
}

fn unchanged_regular_file(path: &Path, contents: &[u8]) -> TransactionalStorageResult<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() != contents.len() as u64 {
        return Ok(false);
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o777 != 0o600 {
        return Ok(false);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(_) => return Ok(false),
    };
    let opened = file.metadata()?;
    if !opened.file_type().is_file() || opened.len() != contents.len() as u64 {
        return Ok(false);
    }
    #[cfg(unix)]
    if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
        return Ok(false);
    }
    #[cfg(unix)]
    if opened.permissions().mode() & 0o777 != 0o600 {
        return Ok(false);
    }
    let limit = u64::try_from(contents.len())
        .map_err(|_| {
            TransactionalStorageError::InvalidInput("projection is too large".to_string())
        })?
        .saturating_add(1);
    let mut existing = Vec::with_capacity(contents.len());
    file.take(limit).read_to_end(&mut existing)?;
    if existing != contents {
        return Ok(false);
    }
    let current = match fs::symlink_metadata(path) {
        Ok(current) if current.file_type().is_file() => current,
        _ => return Ok(false),
    };
    #[cfg(unix)]
    if current.dev() != opened.dev() || current.ino() != opened.ino() {
        return Ok(false);
    }
    Ok(true)
}

pub fn recover_atomic_temporary_files(
    destination: &Path,
    minimum_age: Duration,
) -> TransactionalStorageResult<AtomicRecoveryReport> {
    let parent = destination.parent().ok_or_else(|| {
        TransactionalStorageError::InvalidInput(
            "atomic destination must have a parent directory".to_string(),
        )
    })?;
    if !parent.is_dir() {
        return Ok(AtomicRecoveryReport {
            removed_temporary_files: 0,
            preserved_recent_files: 0,
        });
    }
    let prefix = temporary_prefix(destination)?;
    let now = SystemTime::now();
    let mut report = AtomicRecoveryReport {
        removed_temporary_files: 0,
        preserved_recent_files: 0,
    };
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(&prefix) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_file() {
            continue;
        }
        let old_enough = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .map(|age| age >= minimum_age)
            .unwrap_or(false);
        if old_enough {
            fs::remove_file(entry.path())?;
            report.removed_temporary_files += 1;
        } else {
            report.preserved_recent_files += 1;
        }
    }
    Ok(report)
}

fn validate_health(health: &StorageHealth) -> TransactionalStorageResult<()> {
    if health.schema_version != STORAGE_SCHEMA_VERSION {
        return Err(TransactionalStorageError::UnsupportedSchema {
            found: health.schema_version,
            supported: STORAGE_SCHEMA_VERSION,
        });
    }
    if !health.journal_mode.eq_ignore_ascii_case("wal")
        || health.synchronous != 2
        || !health.foreign_keys
        || health.trusted_schema
    {
        return Err(TransactionalStorageError::Integrity(
            "SQLite connection policy does not match storage-v1".to_string(),
        ));
    }
    if health.quick_check != "ok" || health.foreign_key_violations != 0 {
        return Err(TransactionalStorageError::Integrity(format!(
            "SQLite health check failed: quick_check={}, foreign_key_violations={}",
            health.quick_check, health.foreign_key_violations
        )));
    }
    Ok(())
}

pub(crate) fn validate_event(event: &NewAuditEvent) -> TransactionalStorageResult<()> {
    if event.contract_version != AUDIT_EVENT_CONTRACT_VERSION {
        return Err(TransactionalStorageError::InvalidInput(
            "audit event contract_version must be audit-event-v1".to_string(),
        ));
    }
    for (field, value) in [
        ("event_id", event.event_id.as_str()),
        ("project_id", event.project_id.as_str()),
        ("actor_kind", event.actor_kind.as_str()),
        ("actor_subject_id", event.actor_subject_id.as_str()),
        ("event_kind", event.event_kind.as_str()),
        ("outcome", event.outcome.as_str()),
        ("occurred_at", event.occurred_at.as_str()),
        ("correlation_id", event.correlation_id.as_str()),
    ] {
        validate_required(field, value)?;
        reject_public_path_references(value, field)?;
    }
    if let Some(workspace_id) = event.workspace_id.as_deref() {
        reject_public_path_references(workspace_id, "workspace_id")?;
    }
    if let Some(resource_id) = event.resource_id.as_deref() {
        reject_public_path_references(resource_id, "resource_id")?;
    }
    if event.payload.is_empty() || event.payload.len() > 65_536 {
        return Err(TransactionalStorageError::InvalidInput(
            "audit payload violates its bounded contract".to_string(),
        ));
    }
    let payload = std::str::from_utf8(&event.payload).map_err(|_| {
        TransactionalStorageError::InvalidInput(
            "audit payload must be valid UTF-8 without physical paths".to_string(),
        )
    })?;
    reject_public_path_references(payload, "audit payload")?;
    if matches!(payload.trim_start().chars().next(), Some('{' | '[')) {
        validate_serialized_public_document(&event.payload, "audit payload")?;
    }
    if event.fingerprint.len() != 64
        || !event
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(TransactionalStorageError::InvalidInput(
            "fingerprint must contain exactly 64 lowercase hexadecimal characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_required(field: &str, value: &str) -> TransactionalStorageResult<()> {
    if value.trim().is_empty() {
        return Err(TransactionalStorageError::InvalidInput(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

fn unique_temporary_path(destination: &Path) -> TransactionalStorageResult<PathBuf> {
    let parent = destination.parent().ok_or_else(|| {
        TransactionalStorageError::InvalidInput(
            "atomic destination must have a parent directory".to_string(),
        )
    })?;
    let prefix = temporary_prefix(destination)?;
    for _ in 0..128 {
        let sequence = ATOMIC_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!("{prefix}{}-{sequence}", std::process::id()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(TransactionalStorageError::Io(
        "unable to allocate an atomic temporary file".to_string(),
    ))
}

fn temporary_prefix(destination: &Path) -> TransactionalStorageResult<String> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            TransactionalStorageError::InvalidInput(
                "atomic destination must have a UTF-8 file name".to_string(),
            )
        })?;
    Ok(format!(".{name}.antdelirium-tmp-"))
}

fn write_and_replace(
    destination: &Path,
    temporary: &Path,
    contents: &[u8],
) -> TransactionalStorageResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    set_file_permissions(temporary)?;
    file.write_all(contents)?;
    file.flush()?;
    file.sync_all()?;
    fs::rename(temporary, destination)?;
    set_file_permissions(destination)?;
    if let Some(parent) = destination.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

#[cfg(unix)]
fn set_directory_permissions(path: &Path) -> std::io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_directory_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions(path: &Path) -> std::io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
