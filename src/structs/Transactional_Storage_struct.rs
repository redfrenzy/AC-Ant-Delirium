use std::fmt;
use std::path::PathBuf;

pub const STORAGE_SCHEMA_VERSION: i32 = 15;
pub const STORAGE_BUSY_TIMEOUT_MILLIS: u64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDatabase {
    pub(crate) database_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageProject {
    pub project_id: String,
    pub project_key: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionalProjectRegistration {
    pub peer_uid: u32,
    pub project_identity_fingerprint: [u8; 32],
    pub workspace_identity_fingerprint: [u8; 32],
    pub project_id: String,
    pub workspace_id: String,
    pub project_key: String,
    pub workspace_key: String,
    pub created_at_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAuditEvent {
    pub contract_version: String,
    pub event_id: String,
    pub project_id: String,
    pub workspace_id: Option<String>,
    pub actor_kind: String,
    pub actor_subject_id: String,
    pub event_kind: String,
    pub outcome: String,
    pub occurred_at: String,
    pub fingerprint: String,
    pub correlation_id: String,
    pub resource_id: Option<String>,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventWriteOutcome {
    Inserted { sequence: i64 },
    Duplicate { sequence: i64 },
}

impl EventWriteOutcome {
    pub fn sequence(self) -> i64 {
        match self {
            Self::Inserted { sequence } | Self::Duplicate { sequence } => sequence,
        }
    }

    pub fn inserted(self) -> bool {
        matches!(self, Self::Inserted { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageHealth {
    pub schema_version: i32,
    pub journal_mode: String,
    pub synchronous: i64,
    pub foreign_keys: bool,
    pub trusted_schema: bool,
    pub quick_check: String,
    pub foreign_key_violations: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRecoveryReport {
    pub health: StorageHealth,
    pub wal_busy: i64,
    pub wal_frames: i64,
    pub wal_frames_checkpointed: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSecurityAuditReceipt {
    pub(crate) sequence: i64,
    pub(crate) event_id: String,
    pub(crate) correlation_id: String,
}

impl ServiceSecurityAuditReceipt {
    pub fn sequence(&self) -> i64 {
        self.sequence
    }

    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtomicRecoveryReport {
    pub removed_temporary_files: usize,
    pub preserved_recent_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionalStorageError {
    Io(String),
    Database(String),
    Integrity(String),
    InvalidInput(String),
    UnsupportedSchema { found: i32, supported: i32 },
}

impl fmt::Display for TransactionalStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "storage I/O error: {message}"),
            Self::Database(message) => write!(formatter, "storage database error: {message}"),
            Self::Integrity(message) => write!(formatter, "storage integrity error: {message}"),
            Self::InvalidInput(message) => write!(formatter, "storage input error: {message}"),
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "unsupported storage schema version {found}; maximum supported version is {supported}"
            ),
        }
    }
}

impl std::error::Error for TransactionalStorageError {}

impl From<std::io::Error> for TransactionalStorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

pub type TransactionalStorageResult<T> = Result<T, TransactionalStorageError>;
