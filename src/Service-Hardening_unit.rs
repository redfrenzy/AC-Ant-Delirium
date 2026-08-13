use std::fs;
use std::os::fd::RawFd;

use crate::service_authority::VerifiedServiceAuthority;

const FIRST_NON_STANDARD_DESCRIPTOR: u32 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct VerifiedServiceHardening {
    _private: (),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServiceHardeningError {
    #[cfg(not(target_os = "linux"))]
    UnsupportedPlatform,
    ProcessPolicy,
    TracedProcess,
    DescriptorPolicy,
}

pub(crate) fn harden_service_process(
    _authority: &VerifiedServiceAuthority,
) -> Result<VerifiedServiceHardening, ServiceHardeningError> {
    harden_linux_service_process()?;
    Ok(VerifiedServiceHardening { _private: () })
}

#[cfg(target_os = "linux")]
fn harden_linux_service_process() -> Result<(), ServiceHardeningError> {
    require_untraced_process()?;
    set_no_new_privileges()?;
    disable_process_dumping()?;
    disable_core_dumps()?;
    unsafe {
        libc::umask(0o077);
    }
    mark_inherited_descriptors_close_on_exec()?;
    verify_process_policy()?;
    require_untraced_process()
}

#[cfg(not(target_os = "linux"))]
fn harden_linux_service_process() -> Result<(), ServiceHardeningError> {
    Err(ServiceHardeningError::UnsupportedPlatform)
}

#[cfg(target_os = "linux")]
fn set_no_new_privileges() -> Result<(), ServiceHardeningError> {
    let result = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if result != 0 {
        return Err(ServiceHardeningError::ProcessPolicy);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn disable_process_dumping() -> Result<(), ServiceHardeningError> {
    let result = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
    if result != 0 {
        return Err(ServiceHardeningError::ProcessPolicy);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn disable_core_dumps() -> Result<(), ServiceHardeningError> {
    let limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
        return Err(ServiceHardeningError::ProcessPolicy);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn verify_process_policy() -> Result<(), ServiceHardeningError> {
    let dumpable = unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) };
    let no_new_privileges = unsafe { libc::prctl(libc::PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) };
    let mut core_limit: libc::rlimit = unsafe { std::mem::zeroed() };
    if dumpable != 0
        || no_new_privileges != 1
        || unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut core_limit) } != 0
        || core_limit.rlim_cur != 0
        || core_limit.rlim_max != 0
    {
        return Err(ServiceHardeningError::ProcessPolicy);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn mark_inherited_descriptors_close_on_exec() -> Result<(), ServiceHardeningError> {
    let result = unsafe {
        libc::syscall(
            libc::SYS_close_range,
            FIRST_NON_STANDARD_DESCRIPTOR,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    };
    if result != 0 {
        return Err(ServiceHardeningError::DescriptorPolicy);
    }
    Ok(())
}

pub(crate) fn set_descriptor_close_on_exec(raw_fd: RawFd) -> Result<(), ServiceHardeningError> {
    if raw_fd < 0 {
        return Err(ServiceHardeningError::DescriptorPolicy);
    }
    let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(raw_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return Err(ServiceHardeningError::DescriptorPolicy);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn require_untraced_process() -> Result<(), ServiceHardeningError> {
    let status = fs::read_to_string("/proc/self/status")
        .map_err(|_| ServiceHardeningError::ProcessPolicy)?;
    match parse_tracer_pid(&status) {
        Some(0) => Ok(()),
        Some(_) => Err(ServiceHardeningError::TracedProcess),
        None => Err(ServiceHardeningError::ProcessPolicy),
    }
}

fn parse_tracer_pid(status: &str) -> Option<u32> {
    let mut tracer_lines = status.lines().filter(|line| line.starts_with("TracerPid:"));
    let line = tracer_lines.next()?;
    if tracer_lines.next().is_some() {
        return None;
    }
    let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() != 2 || fields[0] != "TracerPid:" {
        return None;
    }
    fields[1].parse().ok()
}

#[cfg(test)]
#[path = "tests/Service_Hardening_test.rs"]
mod service_hardening_tests;
