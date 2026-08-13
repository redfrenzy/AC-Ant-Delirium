use super::*;

#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::process::Command;

const CHILD_MARKER: &str = "ANTDELIRIUM_HARDENING_CHILD_TEST";

#[test]
fn tracer_status_requires_one_exact_numeric_field() {
    assert_eq!(
        parse_tracer_pid("Name:\tantdelirium\nTracerPid:\t0\n"),
        Some(0)
    );
    assert_eq!(parse_tracer_pid("TracerPid:\t731\n"), Some(731));
    for malformed in [
        "Name:\tantdelirium\n",
        "TracerPid:\n",
        "TracerPid:\tnot-a-pid\n",
        "TracerPid:\t0\textra\n",
        "TracerPid:\t0\nTracerPid:\t0\n",
    ] {
        assert_eq!(parse_tracer_pid(malformed), None);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn service_bootstrap_applies_process_and_descriptor_hardening_in_a_child() {
    if std::env::var_os(CHILD_MARKER).is_some_and(|value| value == "verify-exec") {
        let raw_fd = std::env::var("ANTDELIRIUM_HARDENING_TEST_FD")
            .expect("inherited descriptor number")
            .parse::<libc::c_int>()
            .expect("numeric descriptor");
        assert_eq!(unsafe { libc::fcntl(raw_fd, libc::F_GETFD) }, -1);
        return;
    }
    if std::env::var_os(CHILD_MARKER).is_some_and(|value| value == "harden") {
        let inherited = File::open(env!("CARGO_MANIFEST_DIR")).expect("directory descriptor");
        let raw_fd = inherited.as_raw_fd();
        let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFD) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(raw_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) },
            0
        );

        let authority = VerifiedServiceAuthority::for_current_test_process()
            .expect("test process effective uid");
        let _hardening = harden_service_process(&authority).expect("service hardening");

        let hardened_flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFD) };
        assert_ne!(hardened_flags & libc::FD_CLOEXEC, 0);
        assert_eq!(unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) }, 0);
        assert_eq!(
            unsafe { libc::prctl(libc::PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) },
            1
        );
        let mut limit: libc::rlimit = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut limit) }, 0);
        assert_eq!((limit.rlim_cur, limit.rlim_max), (0, 0));

        let exec_probe = Command::new(std::env::current_exe().expect("current test executable"))
            .arg("--exact")
            .arg("service_hardening::service_hardening_tests::service_bootstrap_applies_process_and_descriptor_hardening_in_a_child")
            .arg("--nocapture")
            .env(CHILD_MARKER, "verify-exec")
            .env("ANTDELIRIUM_HARDENING_TEST_FD", raw_fd.to_string())
            .output()
            .expect("descriptor inheritance probe");
        assert!(exec_probe.status.success());
        return;
    }

    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--exact")
        .arg("service_hardening::service_hardening_tests::service_bootstrap_applies_process_and_descriptor_hardening_in_a_child")
        .arg("--nocapture")
        .env(CHILD_MARKER, "harden")
        .output()
        .expect("hardened child test");
    assert!(
        output.status.success(),
        "child hardening failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn system_unit_reinforces_proc_core_dump_and_namespace_isolation() {
    let unit = include_str!("../../packaging/systemd/antdelirium.service.in");
    for required in [
        "NoNewPrivileges=yes",
        "ProtectProc=invisible",
        "ProcSubset=pid",
        "LimitCORE=0",
        "UMask=0077",
        "RestrictNamespaces=yes",
        "RestrictRealtime=yes",
        "KeyringMode=private",
    ] {
        assert!(unit.lines().any(|line| line == required));
    }
}
