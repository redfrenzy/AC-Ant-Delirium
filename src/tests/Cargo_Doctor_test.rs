use super::*;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fixture_root(label: &str) -> PathBuf {
    let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("fixture clock after epoch")
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/t74-doctor-tests")
        .join(format!("{label}-{}-{nonce}-{sequence}", std::process::id()));
    fs::create_dir_all(&root).expect("create doctor fixture");
    root
}

#[test]
fn parses_stable_and_prerelease_tool_versions_against_declared_floors() {
    assert_eq!(
        parse_tool_version("cargo 1.93.1 (hash date)\n", "cargo"),
        Some(ToolVersion::new(1, 93, 1))
    );
    assert_eq!(
        parse_tool_version("rustc 1.88.0-nightly (hash date)\n", "rustc"),
        Some(ToolVersion::new(1, 88, 0))
    );
    assert_eq!(parse_tool_version("cargo 1.88\n", "cargo"), None);
    assert_eq!(parse_tool_version("rustc 1.88.0\n", "cargo"), None);
    assert!(ToolVersion::new(1, 50, 0) >= MINIMUM_CARGO_VERSION);
    assert!(ToolVersion::new(1, 88, 0) >= MINIMUM_RUSTC_VERSION);
}

#[test]
fn detects_every_compiler_override_even_when_its_value_is_empty() {
    let present = BTreeSet::from([
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_RUSTC",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    ]);
    assert_eq!(
        count_environment_overrides(|name| present.contains(name)),
        COMPILER_OVERRIDE_VARIABLES.len()
    );
    assert_eq!(count_environment_overrides(|_| false), 0);
}

#[test]
fn local_hierarchy_distinguishes_irrelevant_config_from_compiler_override() {
    let root = fixture_root("local-config");
    let project = root.join("project");
    let nested = project.join("src/nested");
    let cargo_directory = project.join(".cargo");
    let cargo_home = root.join("cargo-home");
    fs::create_dir_all(&nested).expect("create nested project path");
    fs::create_dir_all(&cargo_directory).expect("create local Cargo directory");
    fs::create_dir_all(&cargo_home).expect("create fixture Cargo home");
    let config = cargo_directory.join("config.toml");
    let inherited = inspect_local_configs(&root, Some(&cargo_home));
    assert!(matches!(
        inherited.kind,
        LocalConfigKind::Clear | LocalConfigKind::Present
    ));
    fs::write(&config, b"[net]\noffline = true\n").expect("write harmless local config");
    assert_eq!(
        inspect_local_configs(&nested, Some(&cargo_home)),
        LocalConfigStatus {
            kind: LocalConfigKind::Present,
            count: inherited.count + 1,
        }
    );

    fs::write(
        &config,
        b"[build]\nrustc-workspace-wrapper = \"another-wrapper\"\n",
    )
    .expect("write local override");
    assert_eq!(
        inspect_local_configs(&nested, Some(&cargo_home)),
        LocalConfigStatus {
            kind: LocalConfigKind::Override,
            count: inherited.count + 1,
        }
    );

    fs::write(
        &config,
        b"[env]\nANTDELIRIUM_LIMITED_SOCKET = { value = \"/different/agent.sock\", force = true }\n",
    )
    .expect("write local endpoint override");
    assert_eq!(
        inspect_local_configs(&nested, Some(&cargo_home)),
        LocalConfigStatus {
            kind: LocalConfigKind::Override,
            count: inherited.count + 1,
        }
    );
}

#[test]
fn legacy_and_preferred_local_configs_together_are_invalid() {
    let root = fixture_root("local-ambiguous");
    let project = root.join("project");
    let cargo_directory = project.join(".cargo");
    fs::create_dir_all(&cargo_directory).expect("create local Cargo directory");
    fs::write(cargo_directory.join("config"), b"[net]\nretry = 1\n").expect("write legacy config");
    fs::write(cargo_directory.join("config.toml"), b"[net]\nretry = 2\n")
        .expect("write preferred config");
    assert_eq!(
        inspect_local_configs(&project, None),
        LocalConfigStatus {
            kind: LocalConfigKind::Invalid,
            count: 2,
        }
    );
}

#[cfg(unix)]
#[test]
fn symlinked_local_cargo_directory_is_invalid_without_following_it() {
    use std::os::unix::fs::symlink;

    let root = fixture_root("local-symlink");
    let project = root.join("project");
    let external = root.join("external-cargo");
    fs::create_dir_all(&project).expect("create project");
    fs::create_dir_all(&external).expect("create external Cargo directory");
    fs::write(
        external.join("config.toml"),
        b"[build]\nrustc-workspace-wrapper = \"external\"\n",
    )
    .expect("write external Cargo config");
    symlink(&external, project.join(".cargo")).expect("symlink local Cargo directory");
    assert_eq!(
        inspect_local_configs(&project, None),
        LocalConfigStatus {
            kind: LocalConfigKind::Invalid,
            count: 0,
        }
    );
}

#[test]
fn service_requires_endpoint_and_active_service_or_both_socket_units() {
    assert_eq!(
        classify_service(Some(true), Some(false), Some(false), EndpointStatus::Ready),
        ServiceStatus::Ready
    );
    assert_eq!(
        classify_service(Some(false), Some(true), Some(true), EndpointStatus::Ready),
        ServiceStatus::Ready
    );
    assert_eq!(
        classify_service(None, None, None, EndpointStatus::Ready),
        ServiceStatus::Unknown
    );
    assert_eq!(
        classify_service(Some(true), Some(true), Some(true), EndpointStatus::Missing),
        ServiceStatus::Unavailable
    );
}

#[test]
fn report_is_path_free_and_requires_owned_config_without_overrides() {
    let ready = DoctorReport {
        cargo: ToolStatus::Ready(ToolVersion::new(1, 93, 1)),
        rustc: ToolStatus::Ready(ToolVersion::new(1, 93, 1)),
        binary: InstallationStatus::Ready,
        config: ConfigStatus::ActiveOwned,
        environment_override_count: 0,
        local_config: LocalConfigStatus {
            kind: LocalConfigKind::Present,
            count: 1,
        },
        service: ServiceStatus::Ready,
    };
    assert!(ready.is_ready());
    let rendered = ready.render();
    assert!(rendered.contains("result=ready"));
    assert!(rendered.contains("command_line_config=per-invocation-unobservable"));
    for forbidden in [
        "/home/",
        "/var/",
        "plugins/",
        "plan/",
        "config.toml",
        "RUSTC_WORKSPACE_WRAPPER",
        "ANTDELIRIUM_LIMITED_SOCKET",
    ] {
        assert!(!rendered.contains(forbidden));
    }

    let attention = DoctorReport {
        config: ConfigStatus::ActiveUnclaimed,
        environment_override_count: 1,
        ..ready
    };
    assert!(!attention.is_ready());
    assert!(attention.render().contains("result=attention"));
}
