pub const CAPABILITY_SECRET_BYTES: usize = 32;
pub const CONNECTION_NONCE_BYTES: usize = 32;
pub const REQUEST_FINGERPRINT_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityChannel {
    Limited,
    Administrative,
}

impl CapabilityChannel {
    pub fn storage_value(self) -> &'static str {
        match self {
            Self::Limited => "limited",
            Self::Administrative => "administrative",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Option<Self> {
        match value {
            "limited" => Some(Self::Limited),
            "administrative" => Some(Self::Administrative),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityActor {
    Wrapper,
    Agent,
    Reviewer,
    Operator,
    Service,
    Ci,
}

impl CapabilityActor {
    pub fn storage_value(self) -> &'static str {
        match self {
            Self::Wrapper => "wrapper",
            Self::Agent => "agent",
            Self::Reviewer => "reviewer",
            Self::Operator => "operator",
            Self::Service => "service",
            Self::Ci => "ci",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Option<Self> {
        match value {
            "wrapper" => Some(Self::Wrapper),
            "agent" => Some(Self::Agent),
            "reviewer" => Some(Self::Reviewer),
            "operator" => Some(Self::Operator),
            "service" => Some(Self::Service),
            "ci" => Some(Self::Ci),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityOperation {
    CompileObserve,
    ProjectionRefresh,
    TaskCurrentRead,
    SubmissionCreate,
    SubmissionReadAssigned,
    ReviewDecideAssigned,
    AttestationCreate,
    StateTransition,
    StateAdmin,
    InstallationAdmin,
    DiagnosticPrivate,
}

impl CapabilityOperation {
    pub fn storage_value(self) -> &'static str {
        match self {
            Self::CompileObserve => "compile.observe",
            Self::ProjectionRefresh => "projection.refresh",
            Self::TaskCurrentRead => "task.current.read",
            Self::SubmissionCreate => "submission.create",
            Self::SubmissionReadAssigned => "submission.read.assigned",
            Self::ReviewDecideAssigned => "review.decide.assigned",
            Self::AttestationCreate => "attestation.create",
            Self::StateTransition => "state.transition",
            Self::StateAdmin => "state.admin",
            Self::InstallationAdmin => "installation.admin",
            Self::DiagnosticPrivate => "diagnostic.private",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Option<Self> {
        match value {
            "compile.observe" => Some(Self::CompileObserve),
            "projection.refresh" => Some(Self::ProjectionRefresh),
            "task.current.read" => Some(Self::TaskCurrentRead),
            "submission.create" => Some(Self::SubmissionCreate),
            "submission.read.assigned" => Some(Self::SubmissionReadAssigned),
            "review.decide.assigned" => Some(Self::ReviewDecideAssigned),
            "attestation.create" => Some(Self::AttestationCreate),
            "state.transition" => Some(Self::StateTransition),
            "state.admin" => Some(Self::StateAdmin),
            "installation.admin" => Some(Self::InstallationAdmin),
            "diagnostic.private" => Some(Self::DiagnosticPrivate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCapabilityGrant {
    pub grant_id: String,
    pub secret_hash: [u8; CAPABILITY_SECRET_BYTES],
    pub subject_uid: u32,
    pub actor: CapabilityActor,
    pub channel: CapabilityChannel,
    pub operation: CapabilityOperation,
    pub project_id: Option<String>,
    pub workspace_id: Option<String>,
    pub task_id: Option<String>,
    pub not_before_unix: i64,
    pub expires_at_unix: i64,
    pub revocation_epoch: u64,
    pub granted_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialCapabilityProvisioning {
    pub provisioning_id: String,
    pub manifest_sha256: [u8; 32],
    pub applied_at_unix: i64,
    pub grants: Vec<NewCapabilityGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialCapabilityProvisioningReceipt {
    pub provisioning_id: String,
    pub manifest_sha256: [u8; 32],
    pub applied_at_unix: i64,
    pub grant_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCapabilityRevocation {
    pub revocation_id: String,
    pub grant_id: String,
    pub revocation_epoch: u64,
    pub revoked_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityGrantAuditReceipt {
    pub grant_id: String,
    pub audit_sequence: i64,
    pub event_id: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRevocationAuditReceipt {
    pub grant_id: String,
    pub revocation_id: String,
    pub audit_sequence: i64,
    pub event_id: String,
    pub correlation_id: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CapabilityAuthorizationRequest {
    pub grant_id: String,
    pub presented_secret: [u8; CAPABILITY_SECRET_BYTES],
    pub operation: CapabilityOperation,
    pub project_id: Option<String>,
    pub workspace_id: Option<String>,
    pub task_id: Option<String>,
    pub presented_connection_nonce: [u8; CONNECTION_NONCE_BYTES],
    pub request_id: String,
    pub request_fingerprint: [u8; REQUEST_FINGERPRINT_BYTES],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedCapability {
    pub(crate) grant_id: String,
    pub(crate) subject_uid: u32,
    pub(crate) actor: CapabilityActor,
    pub(crate) channel: CapabilityChannel,
    pub(crate) operation: CapabilityOperation,
    pub(crate) project_id: Option<String>,
    pub(crate) workspace_id: Option<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) request_id: String,
    pub(crate) revocation_epoch: u64,
}

impl AuthorizedCapability {
    pub fn grant_id(&self) -> &str {
        &self.grant_id
    }

    pub fn subject_uid(&self) -> u32 {
        self.subject_uid
    }

    pub fn actor(&self) -> CapabilityActor {
        self.actor
    }

    pub fn channel(&self) -> CapabilityChannel {
        self.channel
    }

    pub fn operation(&self) -> CapabilityOperation {
        self.operation
    }

    pub fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn task_id(&self) -> Option<&str> {
        self.task_id.as_deref()
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn revocation_epoch(&self) -> u64 {
        self.revocation_epoch
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityAuthorizationOutcome {
    Fresh(AuthorizedCapability),
    Replay(AuthorizedCapability),
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServiceSecurityEventKind {
    AuthorizationDenied,
    RequestRejected,
}

impl ServiceSecurityEventKind {
    pub(crate) fn storage_value(self) -> &'static str {
        match self {
            Self::AuthorizationDenied => "authorization_denied",
            Self::RequestRejected => "request_rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewServiceSecurityEvent {
    pub(crate) event_id: String,
    pub(crate) correlation_id: String,
    pub(crate) event_kind: ServiceSecurityEventKind,
    pub(crate) peer_uid: u32,
    pub(crate) channel: CapabilityChannel,
    pub(crate) actor: Option<CapabilityActor>,
    pub(crate) operation: Option<CapabilityOperation>,
    pub(crate) project_id: Option<String>,
    pub(crate) workspace_id: Option<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) occurred_at_unix: i64,
}
