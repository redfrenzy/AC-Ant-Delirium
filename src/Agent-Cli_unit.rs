#[cfg(unix)]
use std::cell::Cell;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use ac_ant_delirium::structs::{
    build_note_submission_request, parse_current_task_submission_binding,
    reject_public_path_references, validate_serialized_public_document, ProjectDiscovery,
    SubmissionCreateRequest, AGENT_PROJECTION_CONTRACT_VERSION,
};

use crate::public_output::{OutputRouter, PublicError};

type AgentCliError = PublicError;

const TASK_NOTIFICATION: &str = "antdelirium[AD-TASK]: leia guard/to-do_list.json";
const GOVERNANCE_NOTIFICATION: &str = "antdelirium[AD-GOV]: leia guard/governance_error.json";
const MAX_PROJECTION_BYTES: u64 = 4 * 1024 * 1024;
const MAX_EVIDENCE_CHARACTERS: usize = 8_192;
const LIMITED_SOCKET_ENV: &str = "ANTDELIRIUM_LIMITED_SOCKET";
const AGENT_CAPABILITY_FD_ENV: &str = "ANTDELIRIUM_AGENT_CAPABILITY_FD";

const AGENT_HELP: &str = "AntDelirium - perfil agente\n\
Uso:\n\
  antdelirium --api hierarchy.task.current\n\
  antdelirium --api hierarchy.rule.related\n\
  antdelirium next-action\n\
  antdelirium --api 'hierarchy.task.current.submit <evidência>'\n\
\n\
A submissão envia evidência para avaliação; ela não conclui nem aprova a tarefa.\n";

#[derive(Debug, Clone, PartialEq, Eq)]
enum AgentCommand {
    Help,
    Version,
    ReadCurrentTask,
    ReadRelatedRule,
    NextAction,
    SubmitEvidence(String),
    ForbiddenMutation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentNotification {
    CurrentTask,
    GovernanceErrors,
}

impl AgentNotification {
    fn output(self) -> &'static str {
        match self {
            Self::CurrentTask => TASK_NOTIFICATION,
            Self::GovernanceErrors => GOVERNANCE_NOTIFICATION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AgentCliSuccess {
    Help,
    Version(String),
    Notification(AgentNotification),
    Submission(String),
}

impl AgentCliSuccess {
    fn output(&self) -> &str {
        match self {
            Self::Help => AGENT_HELP,
            Self::Version(version) => version,
            Self::Notification(notification) => notification.output(),
            Self::Submission(notification) => notification,
        }
    }
}

pub(crate) fn run_agent_cli() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let transport = EnvironmentSubmissionTransport::from_environment();
    match execute(&args, &current_directory(), &transport) {
        Ok(output) => OutputRouter::write_success(output.output()),
        Err(error) => OutputRouter::exit(error),
    }
}

fn current_directory() -> Result<PathBuf, AgentCliError> {
    std::env::current_dir().map_err(|_| AgentCliError::ServiceUnavailable)
}

fn execute(
    args: &[OsString],
    current_directory: &Result<PathBuf, AgentCliError>,
    submission_transport: &impl AgentSubmissionTransport,
) -> Result<AgentCliSuccess, AgentCliError> {
    let command = parse_command(args)?;
    match command {
        AgentCommand::Help => Ok(AgentCliSuccess::Help),
        AgentCommand::Version => Ok(AgentCliSuccess::Version(render_public_version())),
        AgentCommand::SubmitEvidence(evidence) => {
            validate_evidence(&evidence)?;
            let project_root = discover_project_root(current_directory)?;
            let projection = read_projection(&project_root, ProjectionKind::CurrentTask)?;
            let binding = parse_current_task_submission_binding(&projection)
                .map_err(|_| AgentCliError::Protocol)?;
            let request_id = generate_request_id()?;
            let request = build_note_submission_request(&binding, &request_id, &evidence)
                .map_err(|_| AgentCliError::Protocol)?;
            let receipt = submission_transport.create_submission(&request)?;
            Ok(AgentCliSuccess::Submission(format!(
                "antdelirium[AD-SUBMISSION]: {}",
                receipt.submission_id
            )))
        }
        AgentCommand::ForbiddenMutation => Err(AgentCliError::Authorization),
        AgentCommand::ReadCurrentTask => {
            let project_root = discover_project_root(current_directory)?;
            read_projection(&project_root, ProjectionKind::CurrentTask)?;
            Ok(AgentCliSuccess::Notification(
                AgentNotification::CurrentTask,
            ))
        }
        AgentCommand::ReadRelatedRule => {
            let project_root = discover_project_root(current_directory)?;
            read_projection(&project_root, ProjectionKind::GovernanceErrors)?;
            Ok(AgentCliSuccess::Notification(
                AgentNotification::GovernanceErrors,
            ))
        }
        AgentCommand::NextAction => {
            let project_root = discover_project_root(current_directory)?;
            let governance = read_projection(&project_root, ProjectionKind::GovernanceErrors)?;
            if governance_has_violations(&governance) {
                return Ok(AgentCliSuccess::Notification(
                    AgentNotification::GovernanceErrors,
                ));
            }
            read_projection(&project_root, ProjectionKind::CurrentTask)?;
            Ok(AgentCliSuccess::Notification(
                AgentNotification::CurrentTask,
            ))
        }
    }
}

trait AgentSubmissionTransport {
    fn create_submission(
        &self,
        request: &SubmissionCreateRequest,
    ) -> Result<crate::limited_protocol::AgentSubmissionReceipt, AgentCliError>;
}

#[cfg(test)]
struct UnavailableSubmissionTransport;

#[cfg(test)]
impl AgentSubmissionTransport for UnavailableSubmissionTransport {
    fn create_submission(
        &self,
        _request: &SubmissionCreateRequest,
    ) -> Result<crate::limited_protocol::AgentSubmissionReceipt, AgentCliError> {
        Err(AgentCliError::ServiceUnavailable)
    }
}

struct EnvironmentSubmissionTransport {
    endpoint: Option<OsString>,
    #[cfg(unix)]
    capability_fd: Cell<Option<i32>>,
}

impl EnvironmentSubmissionTransport {
    fn from_environment() -> Self {
        let endpoint = std::env::var_os(LIMITED_SOCKET_ENV);
        std::env::remove_var(LIMITED_SOCKET_ENV);
        #[cfg(unix)]
        let capability_fd = std::env::var(AGENT_CAPABILITY_FD_ENV)
            .ok()
            .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|raw_fd| *raw_fd >= 3);
        std::env::remove_var(AGENT_CAPABILITY_FD_ENV);
        Self {
            endpoint,
            #[cfg(unix)]
            capability_fd: Cell::new(capability_fd),
        }
    }
}

impl AgentSubmissionTransport for EnvironmentSubmissionTransport {
    fn create_submission(
        &self,
        request: &SubmissionCreateRequest,
    ) -> Result<crate::limited_protocol::AgentSubmissionReceipt, AgentCliError> {
        #[cfg(unix)]
        {
            let endpoint = self
                .endpoint
                .as_deref()
                .ok_or(AgentCliError::ServiceUnavailable)?;
            let capability_fd = self
                .capability_fd
                .take()
                .ok_or(AgentCliError::Authorization)?;
            return crate::limited_protocol::create_agent_submission(
                endpoint,
                capability_fd,
                request,
            )
            .map_err(|error| match error {
                crate::limited_protocol::LimitedProtocolError::Envelope
                | crate::limited_protocol::LimitedProtocolError::Framing => AgentCliError::Protocol,
                crate::limited_protocol::LimitedProtocolError::Endpoint
                | crate::limited_protocol::LimitedProtocolError::Transport
                | crate::limited_protocol::LimitedProtocolError::Read
                | crate::limited_protocol::LimitedProtocolError::Write
                | crate::limited_protocol::LimitedProtocolError::Deadline
                | crate::limited_protocol::LimitedProtocolError::Entropy => {
                    AgentCliError::ServiceUnavailable
                }
            });
        }
        #[cfg(not(unix))]
        {
            let _ = request;
            Err(AgentCliError::ServiceUnavailable)
        }
    }
}

#[cfg(unix)]
impl Drop for EnvironmentSubmissionTransport {
    fn drop(&mut self) {
        if let Some(raw_fd) = self.capability_fd.take() {
            unsafe {
                libc::close(raw_fd);
            }
        }
    }
}

fn generate_request_id() -> Result<String, AgentCliError> {
    let mut entropy = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut entropy))
        .map_err(|_| AgentCliError::ServiceUnavailable)?;
    Ok(format!("req_{}", crockford_token(&entropy)))
}

fn crockford_token(entropy: &[u8; 16]) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
    let mut token = String::with_capacity(26);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in entropy {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            token.push(ALPHABET[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        token.push(ALPHABET[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }
    token
}

fn parse_command(args: &[OsString]) -> Result<AgentCommand, AgentCliError> {
    let values = args
        .iter()
        .map(|value| value.to_str().ok_or(AgentCliError::Usage))
        .collect::<Result<Vec<_>, _>>()?;

    match values.as_slice() {
        [] | ["--help"] | ["-h"] => Ok(AgentCommand::Help),
        ["--version"] => Ok(AgentCommand::Version),
        ["next-action"] => Ok(AgentCommand::NextAction),
        ["--api", "hierarchy.task.current"] => Ok(AgentCommand::ReadCurrentTask),
        ["--api", "hierarchy.rule.related"] => Ok(AgentCommand::ReadRelatedRule),
        ["--api", query] if *query == "hierarchy.task.current.submit" => Err(AgentCliError::Usage),
        ["--api", query] if query.starts_with("hierarchy.task.current.submit ") => {
            parse_submission_query(query)
        }
        ["--api", _] => Ok(AgentCommand::ForbiddenMutation),
        ["--api"] | ["next-action", ..] | ["--help", ..] | ["-h", ..] | ["--version", ..] => {
            Err(AgentCliError::Usage)
        }
        _ => Ok(AgentCommand::ForbiddenMutation),
    }
}

fn render_public_version() -> String {
    let commit = option_env!("ANT_DELIRIUM_BUILD_GIT_HASH")
        .filter(|value| (7..=64).contains(&value.len()))
        .filter(|value| value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or("unknown");
    format!(
        "antdelirium {}\ncommit: {}\nschema: {}\n",
        env!("CARGO_PKG_VERSION"),
        commit,
        AGENT_PROJECTION_CONTRACT_VERSION
    )
}

fn parse_submission_query(query: &str) -> Result<AgentCommand, AgentCliError> {
    const PREFIX: &str = "hierarchy.task.current.submit ";
    let evidence = query.strip_prefix(PREFIX).ok_or(AgentCliError::Usage)?;
    validate_evidence(evidence)?;
    Ok(AgentCommand::SubmitEvidence(evidence.to_string()))
}

fn validate_evidence(evidence: &str) -> Result<(), AgentCliError> {
    let length = evidence.chars().count();
    if length == 0
        || length > MAX_EVIDENCE_CHARACTERS
        || evidence.chars().any(char::is_control)
        || evidence.trim() != evidence
    {
        return Err(AgentCliError::Usage);
    }
    reject_public_path_references(evidence, "submission evidence").map_err(|_| AgentCliError::Usage)
}

fn discover_project_root(
    current_directory: &Result<PathBuf, AgentCliError>,
) -> Result<PathBuf, AgentCliError> {
    let current_directory = current_directory.as_ref().map_err(|error| *error)?;
    ProjectDiscovery::discover(current_directory, None)
        .map(|discovery| discovery.project_root)
        .map_err(|_| AgentCliError::ServiceUnavailable)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectionKind {
    CurrentTask,
    GovernanceErrors,
}

impl ProjectionKind {
    fn file_name(self) -> &'static str {
        match self {
            Self::CurrentTask => "to-do_list.json",
            Self::GovernanceErrors => "governance_error.json",
        }
    }

    fn artifact_marker(self) -> &'static str {
        match self {
            Self::CurrentTask => "\"artifact_kind\": \"current_task\"",
            Self::GovernanceErrors => "\"artifact_kind\": \"governance_errors\"",
        }
    }
}

fn read_projection(project_root: &Path, kind: ProjectionKind) -> Result<Vec<u8>, AgentCliError> {
    let guard = project_root.join("guard");
    require_regular_directory(&guard)?;
    let projection = guard.join(kind.file_name());
    let metadata =
        fs::symlink_metadata(&projection).map_err(|_| AgentCliError::ServiceUnavailable)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_PROJECTION_BYTES
    {
        return Err(AgentCliError::Protocol);
    }

    let document = fs::read(&projection).map_err(|_| AgentCliError::ServiceUnavailable)?;
    if document.len() as u64 > MAX_PROJECTION_BYTES {
        return Err(AgentCliError::Protocol);
    }
    validate_serialized_public_document(&document, "agent projection")
        .map_err(|_| AgentCliError::Protocol)?;
    let text = std::str::from_utf8(&document).map_err(|_| AgentCliError::Protocol)?;
    if !text.contains("\"contract_version\": \"agent-projection-v1\"")
        || !text.contains(kind.artifact_marker())
    {
        return Err(AgentCliError::Protocol);
    }
    Ok(document)
}

fn require_regular_directory(path: &Path) -> Result<(), AgentCliError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| AgentCliError::ServiceUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AgentCliError::Protocol);
    }
    Ok(())
}

fn governance_has_violations(document: &[u8]) -> bool {
    std::str::from_utf8(document).is_ok_and(|document| document.contains("\"violation_id\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::time::{SystemTime, UNIX_EPOCH};

    use ac_ant_delirium::structs::SubmissionEvidence;

    #[derive(Default)]
    struct CapturingSubmissionTransport {
        request: RefCell<Option<SubmissionCreateRequest>>,
    }

    impl AgentSubmissionTransport for CapturingSubmissionTransport {
        fn create_submission(
            &self,
            request: &SubmissionCreateRequest,
        ) -> Result<crate::limited_protocol::AgentSubmissionReceipt, AgentCliError> {
            self.request.replace(Some(request.clone()));
            Ok(crate::limited_protocol::AgentSubmissionReceipt {
                submission_id: "sub_11111111111111111111111111".to_string(),
            })
        }
    }

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn fixture() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "antdelirium-agent-cli-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("guard")).expect("fixture guard directory");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"agent-cli-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("fixture Cargo manifest");
        write_task_projection(&root);
        write_governance_projection(&root, false);
        root
    }

    fn write_task_projection(root: &Path) {
        fs::write(
            root.join("guard/to-do_list.json"),
            "{\n  \"contract_version\": \"agent-projection-v1\",\n  \"artifact_kind\": \"current_task\",\n  \"project_ref\": \"prj_00000000000000000000000000\",\n  \"workspace_ref\": \"wsp_00000000000000000000000000\",\n  \"source_revision\": 1,\n  \"task\": {\"task_id\": \"tsk_00000000000000000000000000\", \"task_revision\": 1, \"objective\": \"Implement current task.\", \"allowed_action\": \"implement\", \"acceptance_criteria\": [], \"relevant_paths\": []},\n  \"queue_status\": \"task_available\",\n  \"submission\": {\"contract_version\": \"submission-v1\", \"instruction\": \"Submit evidence.\"}\n}\n",
        )
        .expect("task projection");
    }

    fn write_governance_projection(root: &Path, with_violation: bool) {
        let violations = if with_violation {
            "{\"violation_id\":\"vio_public\",\"rule_id\":\"rule_public\",\"summary\":\"fix source\",\"path\":\"src/lib.rs\",\"line\":1,\"quick_fix\":null,\"public_rule_reference\":\"rule_public\"}"
        } else {
            ""
        };
        fs::write(
            root.join("guard/governance_error.json"),
            format!(
                "{{\n  \"contract_version\": \"agent-projection-v1\",\n  \"artifact_kind\": \"governance_errors\",\n  \"project_ref\": \"prj_0123456789abcdefghjkmnpq\",\n  \"workspace_ref\": \"wsp_0123456789abcdefghjkmnpq\",\n  \"source_revision\": 1,\n  \"violations\": [{violations}]\n}}\n"
            ),
        )
        .expect("governance projection");
    }

    fn execute_in(root: &Path, values: &[&str]) -> Result<AgentCliSuccess, AgentCliError> {
        execute(
            &args(values),
            &Ok(root.to_path_buf()),
            &UnavailableSubmissionTransport,
        )
    }

    #[test]
    fn help_exposes_only_the_four_agent_actions() {
        let help = execute(
            &[],
            &Err(AgentCliError::ServiceUnavailable),
            &UnavailableSubmissionTransport,
        )
        .expect("help");
        let help = help.output();

        for public_action in [
            "hierarchy.task.current",
            "hierarchy.rule.related",
            "next-action",
            "hierarchy.task.current.submit",
        ] {
            assert!(help.contains(public_action));
        }
        for forbidden in [
            "install",
            "doctor",
            "config",
            "override",
            "implemented",
            "task_reviewed",
            "grade",
            "plugins/",
            "Cargo.toml",
            "plan/",
            "/home/",
        ] {
            assert!(!help.contains(forbidden), "help leaked {forbidden}");
        }
    }

    #[test]
    fn parser_accepts_only_the_closed_agent_grammar() {
        assert_eq!(
            parse_command(&args(&["next-action"])),
            Ok(AgentCommand::NextAction)
        );
        assert_eq!(
            parse_command(&args(&["--api", "hierarchy.task.current"])),
            Ok(AgentCommand::ReadCurrentTask)
        );
        assert_eq!(
            parse_command(&args(&["--api", "hierarchy.rule.related"])),
            Ok(AgentCommand::ReadRelatedRule)
        );
        assert_eq!(
            parse_command(&args(&["--version"])),
            Ok(AgentCommand::Version)
        );
        assert_eq!(
            parse_command(&args(&[
                "--api",
                "hierarchy.task.current.submit cargo check passou"
            ])),
            Ok(AgentCommand::SubmitEvidence(
                "cargo check passou".to_string()
            ))
        );

        for rejected in [
            args(&["--api"]),
            args(&["next-action", "--json"]),
            args(&["--api", "hierarchy.task.current.submit"]),
            args(&["--help", "extra"]),
        ] {
            assert_eq!(parse_command(&rejected), Err(AgentCliError::Usage));
        }
    }

    #[test]
    fn administrative_mutations_are_denied_as_authorization_errors() {
        let forbidden = [
            args(&["--api", "hierarchy.override.active_plan"]),
            args(&["--api", "hierarchy.override.mode"]),
            args(&["--api", "hierarchy.task.current.implemented=true"]),
            args(&["--api", "hierarchy.task.current.task_reviewed=true"]),
            args(&["--api", "hierarchy.task.current.review_status=accepted"]),
            args(&["--api", "hierarchy.task.current.grade=10"]),
            args(&["--api", "hierarchy.task.current.add_task"]),
            args(&["config", "new_task"]),
            args(&["--submit-accept", "sub_public"]),
            args(&["--manager-task-done", "task_public"]),
            args(&["--prune-stale-agents"]),
            args(&["--api", "hierarchy.unknown"]),
            args(&["--project-root", "project"]),
            args(&["--config", "config"]),
            args(&["--next-action"]),
            args(&["invented-administrative-command"]),
        ];
        for arguments in forbidden {
            assert_eq!(
                parse_command(&arguments),
                Ok(AgentCommand::ForbiddenMutation)
            );
            assert_eq!(
                execute(
                    &arguments,
                    &Err(AgentCliError::ServiceUnavailable),
                    &UnavailableSubmissionTransport,
                ),
                Err(AgentCliError::Authorization)
            );
        }
    }

    #[test]
    fn task_and_rule_queries_return_only_local_projection_notifications() {
        let root = fixture();
        assert_eq!(
            execute_in(&root, &["--api", "hierarchy.task.current"]),
            Ok(AgentCliSuccess::Notification(
                AgentNotification::CurrentTask
            ))
        );
        assert_eq!(
            execute_in(&root, &["--api", "hierarchy.rule.related"]),
            Ok(AgentCliSuccess::Notification(
                AgentNotification::GovernanceErrors
            ))
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn next_action_prioritizes_governance_then_current_task() {
        let root = fixture();
        assert_eq!(
            execute_in(&root, &["next-action"]),
            Ok(AgentCliSuccess::Notification(
                AgentNotification::CurrentTask
            ))
        );

        write_governance_projection(&root, true);
        assert_eq!(
            execute_in(&root, &["next-action"]),
            Ok(AgentCliSuccess::Notification(
                AgentNotification::GovernanceErrors
            ))
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn unavailable_projection_fails_without_revealing_its_location() {
        let root = fixture();
        fs::remove_file(root.join("guard/to-do_list.json")).expect("remove task projection");
        let error = execute_in(&root, &["--api", "hierarchy.task.current"])
            .expect_err("missing projection must fail");

        assert_eq!(error, AgentCliError::ServiceUnavailable);
        assert!(!error
            .public_message()
            .contains(root.to_string_lossy().as_ref()));
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn projection_symlinks_and_private_paths_are_rejected() {
        let root = fixture();
        fs::write(
            root.join("guard/to-do_list.json"),
            "{\"contract_version\": \"agent-projection-v1\", \"artifact_kind\": \"current_task\", \"objective\": \"/home/operator/private\"}",
        )
        .expect("replace task projection");
        assert_eq!(
            execute_in(&root, &["--api", "hierarchy.task.current"]),
            Err(AgentCliError::Protocol)
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let external = root.with_extension("external");
            fs::write(&external, "external").expect("external fixture");
            fs::remove_file(root.join("guard/to-do_list.json")).expect("remove projection");
            symlink(&external, root.join("guard/to-do_list.json")).expect("projection symlink");
            assert_eq!(
                execute_in(&root, &["--api", "hierarchy.task.current"]),
                Err(AgentCliError::Protocol)
            );
            fs::remove_file(external).expect("external cleanup");
        }
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn submission_builds_a_typed_request_without_writing_project_state() {
        let root = fixture();
        let before = fs::read_dir(&root)
            .expect("fixture root")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            execute_in(
                &root,
                &[
                    "--api",
                    "hierarchy.task.current.submit cargo check e testes passaram"
                ]
            ),
            Err(AgentCliError::ServiceUnavailable)
        );
        let after = fs::read_dir(&root)
            .expect("fixture root")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(before, after);

        let transport = CapturingSubmissionTransport::default();
        assert_eq!(
            execute(
                &args(&[
                    "--api",
                    "hierarchy.task.current.submit cargo check e testes passaram"
                ]),
                &Ok(root.clone()),
                &transport,
            ),
            Ok(AgentCliSuccess::Submission(
                "antdelirium[AD-SUBMISSION]: sub_11111111111111111111111111".to_string()
            ))
        );
        let request = transport
            .request
            .take()
            .expect("transport should receive one typed request");
        assert!(request.request_id.starts_with("req_"));
        assert_eq!(request.request_id.len(), 30);
        assert_eq!(request.project_id, "prj_00000000000000000000000000");
        assert_eq!(request.workspace_id, "wsp_00000000000000000000000000");
        assert_eq!(request.task_id, "tsk_00000000000000000000000000");
        assert_eq!(request.task_revision, 1);
        assert_eq!(
            request.evidence,
            vec![SubmissionEvidence::Note {
                text: "cargo check e testes passaram".to_string()
            }]
        );
        let final_entries = fs::read_dir(&root)
            .expect("fixture root")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(before, final_entries);
        assert_eq!(
            parse_command(&args(&[
                "--api",
                "hierarchy.task.current.submit /home/operator/proof"
            ])),
            Err(AgentCliError::Usage)
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn generated_request_ids_follow_the_limited_protocol_shape() {
        let first = generate_request_id().expect("request id");
        let second = generate_request_id().expect("request id");
        for request_id in [&first, &second] {
            assert!(request_id.starts_with("req_"));
            assert_eq!(request_id.len(), 30);
            assert!(request_id[4..].bytes().all(|byte| {
                byte.is_ascii_digit()
                    || matches!(
                        byte,
                        b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z'
                    )
            }));
        }
        assert_ne!(first, second);
        assert_eq!(crockford_token(&[0; 16]), "00000000000000000000000000");
    }
}

#[cfg(test)]
#[path = "tests/Agent_Surface_Non_Disclosure_test.rs"]
mod agent_surface_non_disclosure_tests;

#[cfg(test)]
#[path = "tests/Version_Output_test.rs"]
mod version_output_tests;
