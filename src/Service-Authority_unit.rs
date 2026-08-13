use std::fs;

use crate::public_output::{OutputRouter, PublicError};

const SERVICE_ACCOUNT_NAME: &str = "antdelirium";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VerifiedServiceAuthority {
    effective_uid: u32,
}

impl VerifiedServiceAuthority {
    pub(crate) fn effective_uid(&self) -> u32 {
        self.effective_uid
    }

    #[cfg(test)]
    pub(crate) fn for_test(effective_uid: u32) -> Self {
        Self { effective_uid }
    }

    #[cfg(test)]
    pub(crate) fn for_current_test_process() -> Option<Self> {
        effective_uid().map(Self::for_test)
    }
}

trait ServiceAuthoritySource {
    fn effective_uid(&self) -> Option<u32>;
    fn account_uid(&self, account_name: &str) -> Option<u32>;
}

struct HostServiceAuthoritySource;

impl ServiceAuthoritySource for HostServiceAuthoritySource {
    fn effective_uid(&self) -> Option<u32> {
        effective_uid()
    }

    fn account_uid(&self, account_name: &str) -> Option<u32> {
        account_uid(account_name)
    }
}

pub(crate) fn run_service_mode() -> ! {
    match verify_service_authority(&HostServiceAuthoritySource) {
        Ok(authority) => run_verified_service(authority),
        Err(error) => OutputRouter::exit(error),
    }
}

fn verify_service_authority(
    source: &impl ServiceAuthoritySource,
) -> Result<VerifiedServiceAuthority, PublicError> {
    let effective_uid = source.effective_uid().ok_or(PublicError::Authorization)?;
    let service_uid = source
        .account_uid(SERVICE_ACCOUNT_NAME)
        .ok_or(PublicError::Authorization)?;
    if effective_uid == 0 || effective_uid != service_uid {
        return Err(PublicError::Authorization);
    }
    Ok(VerifiedServiceAuthority { effective_uid })
}

fn run_verified_service(authority: VerifiedServiceAuthority) -> ! {
    crate::service_channel::run_service_channels(authority)
}

#[cfg(unix)]
fn effective_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    parse_effective_uid(&status)
}

#[cfg(not(unix))]
fn effective_uid() -> Option<u32> {
    None
}

fn parse_effective_uid(status: &str) -> Option<u32> {
    let mut uid_lines = status.lines().filter(|line| line.starts_with("Uid:"));
    let line = uid_lines.next()?;
    if uid_lines.next().is_some() {
        return None;
    }
    let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 || fields[0] != "Uid:" {
        return None;
    }
    fields[2].parse().ok()
}

fn account_uid(account_name: &str) -> Option<u32> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    let mut matching_uids = passwd.lines().filter_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let _password = fields.next()?;
        let uid = fields.next()?;
        (name == account_name).then(|| uid.parse::<u32>().ok())?
    });
    let uid = matching_uids.next()?;
    if matching_uids.next().is_some() {
        return None;
    }
    Some(uid)
}

#[cfg(test)]
#[path = "tests/Service_Authority_test.rs"]
mod service_authority_tests;
