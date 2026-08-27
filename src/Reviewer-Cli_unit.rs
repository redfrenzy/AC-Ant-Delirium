#[cfg(unix)]
use std::cell::Cell;
use std::ffi::{OsStr, OsString};

use ac_ant_delirium::libres::System_Random_unit::random_crockford_id;
use ac_ant_delirium::structs::{build_assigned_review_decision, ReviewDecision};

use crate::public_output::{OutputRouter, PublicError};

const LIMITED_SOCKET_ENV: &str = "ANTDELIRIUM_LIMITED_SOCKET";
const CAPABILITY_FD_ENV: &str = "ANTDELIRIUM_REVIEWER_CAPABILITY_FD";
const PROJECT_ID_ENV: &str = "ANTDELIRIUM_REVIEW_PROJECT_ID";
const WORKSPACE_ID_ENV: &str = "ANTDELIRIUM_REVIEW_WORKSPACE_ID";
const TASK_ID_ENV: &str = "ANTDELIRIUM_REVIEW_TASK_ID";

enum ReviewerCommand {
    Read {
        submission_id: String,
    },
    Decide {
        submission_id: String,
        decision: ReviewDecision,
        justification: String,
    },
}

struct ReviewerContext {
    endpoint: OsString,
    project_id: String,
    workspace_id: String,
    task_id: String,
    #[cfg(unix)]
    capability_fd: Cell<Option<i32>>,
}

pub(crate) fn run_reviewer_cli() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = ReviewerContext::from_environment().and_then(|context| execute(&args, &context));
    match result {
        Ok(output) => OutputRouter::write_success(&output),
        Err(error) => OutputRouter::exit(error),
    }
}

fn execute(args: &[OsString], context: &ReviewerContext) -> Result<String, PublicError> {
    match parse_command(args)? {
        ReviewerCommand::Read { submission_id } => {
            #[cfg(unix)]
            {
                let receipt = crate::limited_protocol::read_assigned_submission(
                    OsStr::new(&context.endpoint),
                    context.take_capability()?,
                    &context.project_id,
                    &context.workspace_id,
                    &context.task_id,
                    &submission_id,
                )
                .map_err(map_protocol_error)?;
                let payload =
                    String::from_utf8(receipt.payload).map_err(|_| PublicError::Protocol)?;
                return Ok(format!(
                    "antdelirium[AD-ASSIGNMENT]: {}\n{}",
                    receipt.submission_id, payload
                ));
            }
            #[cfg(not(unix))]
            Err(PublicError::ServiceUnavailable)
        }
        ReviewerCommand::Decide {
            submission_id,
            decision,
            justification,
        } => {
            let request_id =
                random_crockford_id("req_").map_err(|_| PublicError::ServiceUnavailable)?;
            let request = build_assigned_review_decision(
                &request_id,
                &submission_id,
                decision,
                &justification,
            )
            .map_err(|_| PublicError::Usage)?;
            #[cfg(unix)]
            {
                let receipt = crate::limited_protocol::decide_assigned_review(
                    OsStr::new(&context.endpoint),
                    context.take_capability()?,
                    &context.project_id,
                    &context.workspace_id,
                    &context.task_id,
                    &request,
                )
                .map_err(map_protocol_error)?;
                return Ok(format!("antdelirium[AD-REVIEW]: {}", receipt.review_id));
            }
            #[cfg(not(unix))]
            Err(PublicError::ServiceUnavailable)
        }
    }
}

fn parse_command(args: &[OsString]) -> Result<ReviewerCommand, PublicError> {
    let values = args
        .iter()
        .map(|value| value.to_str().ok_or(PublicError::Usage))
        .collect::<Result<Vec<_>, _>>()?;
    match values.as_slice() {
        ["review", "assigned", submission_id] if valid_id(submission_id, "sub_") => {
            Ok(ReviewerCommand::Read {
                submission_id: (*submission_id).to_string(),
            })
        }
        ["review", "decide", submission_id, decision, justification]
            if valid_id(submission_id, "sub_") && !justification.is_empty() =>
        {
            let decision = match *decision {
                "accepted" => ReviewDecision::Accepted,
                "rejected" => ReviewDecision::Rejected,
                _ => return Err(PublicError::Usage),
            };
            Ok(ReviewerCommand::Decide {
                submission_id: (*submission_id).to_string(),
                decision,
                justification: (*justification).to_string(),
            })
        }
        _ => Err(PublicError::Usage),
    }
}

impl ReviewerContext {
    fn from_environment() -> Result<Self, PublicError> {
        let endpoint =
            std::env::var_os(LIMITED_SOCKET_ENV).ok_or(PublicError::ServiceUnavailable)?;
        let project_id = take_id_environment(PROJECT_ID_ENV, "prj_")?;
        let workspace_id = take_id_environment(WORKSPACE_ID_ENV, "wsp_")?;
        let task_id = take_id_environment(TASK_ID_ENV, "tsk_")?;
        #[cfg(unix)]
        let capability_fd = std::env::var(CAPABILITY_FD_ENV)
            .ok()
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|value| *value >= 3)
            .ok_or(PublicError::Authorization)?;
        std::env::remove_var(LIMITED_SOCKET_ENV);
        std::env::remove_var(CAPABILITY_FD_ENV);
        Ok(Self {
            endpoint,
            project_id,
            workspace_id,
            task_id,
            #[cfg(unix)]
            capability_fd: Cell::new(Some(capability_fd)),
        })
    }

    #[cfg(unix)]
    fn take_capability(&self) -> Result<i32, PublicError> {
        self.capability_fd.take().ok_or(PublicError::Authorization)
    }
}

#[cfg(unix)]
impl Drop for ReviewerContext {
    fn drop(&mut self) {
        if let Some(raw_fd) = self.capability_fd.take() {
            unsafe { libc::close(raw_fd) };
        }
    }
}

fn take_id_environment(name: &str, prefix: &str) -> Result<String, PublicError> {
    let value = std::env::var(name).map_err(|_| PublicError::Authorization)?;
    std::env::remove_var(name);
    valid_id(&value, prefix)
        .then_some(value)
        .ok_or(PublicError::Authorization)
}

fn valid_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|token| {
        token.len() == 26
            && token.bytes().all(|byte| {
                byte.is_ascii_digit()
                    || matches!(byte, b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z')
            })
    })
}

fn map_protocol_error(error: crate::limited_protocol::LimitedProtocolError) -> PublicError {
    match error {
        crate::limited_protocol::LimitedProtocolError::Envelope
        | crate::limited_protocol::LimitedProtocolError::Framing => PublicError::Protocol,
        _ => PublicError::ServiceUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn reviewer_parser_is_closed_to_one_assignment_or_decision() {
        assert!(matches!(
            parse_command(&args(&[
                "review",
                "assigned",
                "sub_00000000000000000000000000"
            ])),
            Ok(ReviewerCommand::Read { .. })
        ));
        assert!(matches!(
            parse_command(&args(&[
                "review",
                "decide",
                "sub_00000000000000000000000000",
                "accepted",
                "Evidence satisfies the task."
            ])),
            Ok(ReviewerCommand::Decide { .. })
        ));
        for rejected in [
            args(&["review"]),
            args(&["review", "list"]),
            args(&["review", "decide", "sub_bad", "accepted", "ok"]),
            args(&[
                "review",
                "decide",
                "sub_00000000000000000000000000",
                "approved",
                "ok",
            ]),
        ] {
            assert!(matches!(parse_command(&rejected), Err(PublicError::Usage)));
        }
    }
}
