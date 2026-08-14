#![cfg(unix)]

use std::ffi::OsStr;
use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;

const WRAPPER: &str = env!("CARGO_BIN_EXE_antdelirium");

fn real_rustc() -> PathBuf {
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .expect("query rustc sysroot");
    assert!(output.status.success(), "rustc sysroot query failed");
    let sysroot = String::from_utf8(output.stdout).expect("utf8 sysroot");
    PathBuf::from(sysroot.trim()).join("bin/rustc")
}

fn direct_output(rustc: &OsStr, args: &[&str]) -> Output {
    Command::new(rustc)
        .args(args)
        .output()
        .expect("run rustc directly")
}

fn wrapper_output(rustc: &OsStr, args: &[&str], depth: Option<&str>) -> Output {
    let mut command = Command::new(WRAPPER);
    command.arg(rustc).args(args);
    if let Some(depth) = depth {
        command.env("ANTDELIRIUM_WRAPPER_DEPTH", depth);
    }
    command.output().expect("run rustc through wrapper")
}

fn output_with_stdin(mut command: Command, input: &[u8]) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn compiler");
    child
        .stdin
        .take()
        .expect("compiler stdin")
        .write_all(input)
        .expect("write compiler stdin");
    child.wait_with_output().expect("wait for compiler")
}

fn assert_same_output(direct: &Output, wrapped: &Output) {
    assert_eq!(wrapped.status.code(), direct.status.code());
    assert_eq!(wrapped.status.signal(), direct.status.signal());
    assert_eq!(wrapped.stdout, direct.stdout);
    assert_eq!(wrapped.stderr, direct.stderr);
}

#[test]
fn wrapper_preserves_success_stdout_stderr_and_exit() {
    let rustc = real_rustc();
    let direct = direct_output(rustc.as_os_str(), &["--version"]);
    let wrapped = wrapper_output(rustc.as_os_str(), &["--version"], None);

    assert_same_output(&direct, &wrapped);
    assert!(wrapped.status.success());
}

#[test]
fn wrapper_preserves_compiler_failure_bytes_and_exit() {
    let rustc = real_rustc();
    let args = ["--crate-name", "probe", "--invalid-antdelirium-option"];
    let direct = direct_output(rustc.as_os_str(), &args);
    let wrapped = wrapper_output(rustc.as_os_str(), &args, None);

    assert_same_output(&direct, &wrapped);
    assert!(!wrapped.status.success());
}

#[test]
fn wrapper_inherits_stdin_without_touching_compiler_output() {
    let rustc = real_rustc();
    let args = [
        "-",
        "--crate-name",
        "stdin_probe",
        "--crate-type=lib",
        "--emit=metadata=-",
    ];
    let source = b"pub fn stdin_probe() -> bool { true }\n";

    let mut direct_command = Command::new(&rustc);
    direct_command.args(args);
    let direct = output_with_stdin(direct_command, source);

    let mut wrapper_command = Command::new(WRAPPER);
    wrapper_command.arg(&rustc).args(args);
    let wrapped = output_with_stdin(wrapper_command, source);

    assert_same_output(&direct, &wrapped);
    assert!(wrapped.status.success());
}

#[test]
fn delegation_only_depth_is_also_transparent() {
    let rustc = real_rustc();
    let direct = direct_output(rustc.as_os_str(), &["-vV"]);
    let wrapped = wrapper_output(rustc.as_os_str(), &["-vV"], Some("1"));

    assert_same_output(&direct, &wrapped);
    assert!(wrapped.status.success());
}

#[test]
fn missing_compiler_uses_the_public_127_contract() {
    let output = wrapper_output(
        OsStr::new("/antdelirium-missing-toolchain/rustc"),
        &["--version"],
        None,
    );

    assert_eq!(output.status.code(), Some(127));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        "antdelirium[AD-TOOLCHAIN-0002]: compilador configurado não foi encontrado\n".as_bytes()
    );
}

#[test]
fn wrapper_process_becomes_rustc_and_preserves_sigterm() {
    let rustc = real_rustc();
    let mut child = Command::new(WRAPPER)
        .arg(&rustc)
        .args([
            "-",
            "--crate-name",
            "signal_probe",
            "--crate-type=lib",
            "--emit=metadata",
            "-o",
            "/dev/null",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn wrapper");
    let pid = child.id();
    let process_exe = PathBuf::from(format!("/proc/{pid}/exe"));

    let mut became_rustc = false;
    for _ in 0..2_000 {
        if let Ok(executable) = std::fs::read_link(&process_exe) {
            if executable.file_name() == Some(OsStr::new("rustc")) {
                became_rustc = true;
                break;
            }
        }
        if let Some(status) = child.try_wait().expect("inspect wrapper child") {
            panic!("compiler exited before signal probe: {status}");
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(became_rustc, "wrapper process was not replaced by rustc");

    let kill_result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    assert_eq!(kill_result, 0, "failed to signal compiler process");
    let status = child.wait().expect("wait for signaled rustc");
    assert_eq!(status.signal(), Some(libc::SIGTERM));
}
