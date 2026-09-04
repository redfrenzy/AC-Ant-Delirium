#![cfg(unix)]

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const WRAPPER: &str = env!("CARGO_BIN_EXE_antdelirium");
const HOST_TARGET: &str = "x86_64-unknown-linux-gnu";
const WASM_TARGET: &str = "wasm32-unknown-unknown";
static RUN_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn cargo_matrix_preserves_virtual_workspace_targets_and_parallel_builds() {
    let root = matrix_workspace();
    let direct_target = root.join("target-direct");
    let wrapped_target = root.join("target-wrapped");

    let wrapper_probe = run_cargo_with_depth(
        &root,
        &root.join("target-wrapper-proof"),
        "invalid",
        &[
            "check",
            "--locked",
            "--offline",
            "-p",
            "matrix-support",
            "--target",
            HOST_TARGET,
        ],
    );
    assert!(!wrapper_probe.status.success());
    assert!(
        wrapper_probe
            .stderr
            .windows(b"AD-CONFIG-0001".len())
            .any(|window| window == b"AD-CONFIG-0001"),
        "Cargo did not demonstrate participation of the configured wrapper"
    );

    let direct_run = run_cargo(
        &root,
        &direct_target,
        false,
        &[
            "run",
            "--quiet",
            "--locked",
            "--offline",
            "-p",
            "matrix-app",
            "--target",
            HOST_TARGET,
        ],
    );
    assert_success("direct native run", &direct_run);

    let wrapped_check = run_cargo(
        &root,
        &wrapped_target,
        true,
        &[
            "check",
            "--locked",
            "--offline",
            "--workspace",
            "--all-targets",
            "--target",
            HOST_TARGET,
            "-j",
            "4",
        ],
    );
    assert_success("parallel wrapped native check", &wrapped_check);

    let wrapped_cached_check = run_cargo(
        &root,
        &wrapped_target,
        true,
        &[
            "check",
            "--locked",
            "--offline",
            "--workspace",
            "--all-targets",
            "--target",
            HOST_TARGET,
            "-j",
            "4",
        ],
    );
    assert_success("cached wrapped native check", &wrapped_cached_check);

    let wrapped_test = run_cargo(
        &root,
        &wrapped_target,
        true,
        &[
            "test",
            "--quiet",
            "--locked",
            "--offline",
            "--workspace",
            "--target",
            HOST_TARGET,
            "-j",
            "4",
        ],
    );
    assert_success("wrapped workspace tests", &wrapped_test);

    let wrapped_run = run_cargo(
        &root,
        &wrapped_target,
        true,
        &[
            "run",
            "--quiet",
            "--locked",
            "--offline",
            "-p",
            "matrix-app",
            "--target",
            HOST_TARGET,
        ],
    );
    assert_success("wrapped native run", &wrapped_run);
    assert_eq!(wrapped_run.stdout, direct_run.stdout);
    assert_eq!(wrapped_run.stderr, direct_run.stderr);
    assert_eq!(wrapped_run.stdout, b"native:generated:derived\n");

    let wrapped_example = run_cargo(
        &root,
        &wrapped_target,
        true,
        &[
            "run",
            "--quiet",
            "--locked",
            "--offline",
            "-p",
            "matrix-app",
            "--example",
            "matrix-example",
            "--target",
            HOST_TARGET,
        ],
    );
    assert_success("wrapped example", &wrapped_example);
    assert_eq!(
        wrapped_example.stdout,
        b"example:native:generated:derived\n"
    );

    let wrapped_wasm = run_cargo(
        &root,
        &wrapped_target,
        true,
        &[
            "build",
            "--locked",
            "--offline",
            "--workspace",
            "--target",
            WASM_TARGET,
            "-j",
            "4",
        ],
    );
    assert_success("parallel wrapped WASM build", &wrapped_wasm);
    assert!(
        wrapped_target
            .join(WASM_TARGET)
            .join("debug/matrix-app.wasm")
            .is_file(),
        "wrapped WASM binary was not produced"
    );

    for candidate in [
        root.join("guard"),
        root.join("app/guard"),
        root.join("derive/guard"),
        root.join("support/guard"),
    ] {
        assert!(
            !candidate.exists(),
            "an unavailable limited service must not create local governance state"
        );
    }
}

fn matrix_workspace() -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/t66-cargo-matrix");
    let run_id = RUN_ID.fetch_add(1, Ordering::Relaxed);
    let destination = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/t66-cargo-matrix/{}-{run_id}",
        std::process::id()
    ));
    copy_fixture_tree(&source, &destination);
    destination
}

fn copy_fixture_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create matrix workspace");
    let mut entries = fs::read_dir(source)
        .expect("read matrix fixture")
        .map(|entry| entry.expect("matrix fixture entry"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type().expect("matrix fixture file type");
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_fixture_tree(&entry.path(), &target);
        } else {
            assert!(file_type.is_file(), "matrix fixture must not contain links");
            fs::copy(entry.path(), target).expect("copy matrix fixture file");
        }
    }
}

fn run_cargo(root: &Path, target: &Path, wrapped: bool, args: &[&str]) -> Output {
    cargo_command(root, target, wrapped, args)
        .output()
        .expect("execute Cargo matrix command")
}

fn run_cargo_with_depth(root: &Path, target: &Path, depth: &str, args: &[&str]) -> Output {
    cargo_command(root, target, true, args)
        .env("ANTDELIRIUM_WRAPPER_DEPTH", depth)
        .output()
        .expect("execute Cargo wrapper participation probe")
}

fn cargo_command(root: &Path, target: &Path, wrapped: bool, args: &[&str]) -> Command {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let isolated_cargo_home = target.join("cargo-home");
    fs::create_dir_all(&isolated_cargo_home).expect("create isolated Cargo home");
    let unavailable_endpoint = target.join("unavailable-agent.sock");
    let endpoint_value_override = format!(
        "env.ANTDELIRIUM_LIMITED_SOCKET.value=\"{}\"",
        unavailable_endpoint
            .to_str()
            .expect("matrix target path is UTF-8")
    );
    let mut command = Command::new(cargo);
    command
        .current_dir(root)
        .arg("--config")
        .arg(endpoint_value_override)
        .arg("--config")
        .arg("env.ANTDELIRIUM_LIMITED_SOCKET.force=true")
        .args(args)
        .env("CARGO_TARGET_DIR", target)
        .env("CARGO_HOME", isolated_cargo_home)
        .env_remove("RUSTC_WRAPPER")
        .env_remove("ANTDELIRIUM_WRAPPER_DEPTH")
        .env_remove("ANTDELIRIUM_LIMITED_SOCKET")
        .env_remove("ANTDELIRIUM_LIMITED_CAPABILITY_FD");
    if wrapped {
        command.env("RUSTC_WORKSPACE_WRAPPER", WRAPPER);
    } else {
        command.env_remove("RUSTC_WORKSPACE_WRAPPER");
    }
    command
}

fn assert_success(label: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{label} failed with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn cargo_matrix_fixture_is_a_locked_virtual_workspace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/t66-cargo-matrix");
    let workspace = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(workspace.contains("[workspace]"));
    assert!(!workspace.contains("[package]"));
    assert!(root.join("Cargo.lock").is_file());
    assert!(Path::new(WRAPPER).is_absolute());
    assert_eq!(
        Path::new(WRAPPER).file_name(),
        Some(OsStr::new("antdelirium"))
    );
}
