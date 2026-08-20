use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};

use toml_edit::DocumentMut;

use crate::cargo_config::{inspect_workspace_wrapper, WorkspaceWrapperStatus};
use crate::cargo_installer::{
    cargo_home_from_environment, inspect_release_binary, BinaryInstallationStatus,
};
use crate::public_output::OutputRouter;

const MINIMUM_CARGO_VERSION: ToolVersion = ToolVersion::new(1, 50, 0);
const MINIMUM_RUSTC_VERSION: ToolVersion = ToolVersion::new(1, 88, 0);
const MAXIMUM_LOCAL_CONFIG_BYTES: u64 = 4 * 1024 * 1024;
const COMPILER_OVERRIDE_VARIABLES: [&str; 6] = [
    "CARGO_BUILD_RUSTC",
    "RUSTC",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
];
const LIMITED_ENDPOINT_VARIABLE: &str = "ANTDELIRIUM_LIMITED_SOCKET";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ToolVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

impl ToolVersion {
    const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    fn render(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolStatus {
    Ready(ToolVersion),
    Unsupported(ToolVersion),
    Unavailable,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallationStatus {
    Ready,
    Missing,
    Conflict,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigStatus {
    ActiveOwned,
    ActiveUnclaimed,
    Missing,
    Conflict,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalConfigKind {
    Clear,
    Present,
    Override,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LocalConfigStatus {
    kind: LocalConfigKind,
    count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndpointStatus {
    Ready,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceStatus {
    Ready,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorReport {
    cargo: ToolStatus,
    rustc: ToolStatus,
    binary: InstallationStatus,
    config: ConfigStatus,
    environment_override_count: usize,
    local_config: LocalConfigStatus,
    service: ServiceStatus,
}

impl DoctorReport {
    fn is_ready(&self) -> bool {
        matches!(self.cargo, ToolStatus::Ready(_))
            && matches!(self.rustc, ToolStatus::Ready(_))
            && self.binary == InstallationStatus::Ready
            && self.config == ConfigStatus::ActiveOwned
            && self.environment_override_count == 0
            && matches!(
                self.local_config.kind,
                LocalConfigKind::Clear | LocalConfigKind::Present
            )
            && self.service == ServiceStatus::Ready
    }

    fn render(&self) -> String {
        let mut output = String::from("antdelirium doctor v1\n");
        output.push_str(&render_tool_status(
            "cargo",
            self.cargo,
            MINIMUM_CARGO_VERSION,
        ));
        output.push_str(&render_tool_status(
            "rustc",
            self.rustc,
            MINIMUM_RUSTC_VERSION,
        ));
        output.push_str(&format!("binary={}\n", render_installation(self.binary)));
        output.push_str(&format!("cargo_config={}\n", render_config(self.config)));
        if self.environment_override_count == 0 {
            output.push_str("environment_overrides=clear\n");
        } else {
            output.push_str(&format!(
                "environment_overrides=detected count={}\n",
                self.environment_override_count
            ));
        }
        output.push_str(&format!(
            "local_config={} count={}\n",
            render_local_config(self.local_config.kind),
            self.local_config.count
        ));
        output.push_str(&format!("service={}\n", render_service(self.service)));
        output.push_str("command_line_config=per-invocation-unobservable\n");
        output.push_str(if self.is_ready() {
            "result=ready\n"
        } else {
            "result=attention\n"
        });
        output
    }
}

pub(crate) fn run_doctor() {
    let report = collect_report();
    let ready = report.is_ready();
    OutputRouter::write_success(&report.render());
    if !ready {
        std::process::exit(78);
    }
}

fn collect_report() -> DoctorReport {
    let cargo = inspect_tool(OsStr::new("cargo"), "cargo", MINIMUM_CARGO_VERSION);
    let rustc_program = effective_rustc_program();
    let rustc = inspect_tool(&rustc_program, "rustc", MINIMUM_RUSTC_VERSION);
    let cargo_home = cargo_home_from_environment().ok();
    let binary = inspect_binary(cargo_home.as_deref());
    let config = inspect_config(cargo_home.as_deref());
    let environment_override_count =
        count_environment_overrides(|name| std::env::var_os(name).is_some());
    let local_config = std::env::current_dir()
        .ok()
        .map(|cwd| inspect_local_configs(&cwd, cargo_home.as_deref()))
        .unwrap_or(LocalConfigStatus {
            kind: LocalConfigKind::Invalid,
            count: 0,
        });
    let endpoint = inspect_limited_endpoint(std::env::var_os(LIMITED_ENDPOINT_VARIABLE));
    let service = classify_service(
        system_unit_active("antdelirium.service"),
        system_unit_active("antdelirium-agent.socket"),
        system_unit_active("antdelirium-admin.socket"),
        endpoint,
    );
    DoctorReport {
        cargo,
        rustc,
        binary,
        config,
        environment_override_count,
        local_config,
        service,
    }
}

fn effective_rustc_program() -> OsString {
    std::env::var_os("CARGO_BUILD_RUSTC")
        .or_else(|| std::env::var_os("RUSTC"))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OsString::from("rustc"))
}

fn inspect_tool(program: &OsStr, name: &str, minimum: ToolVersion) -> ToolStatus {
    let output = match Command::new(program)
        .arg("--version")
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(_) => return ToolStatus::Unavailable,
        Err(_) => return ToolStatus::Unavailable,
    };
    if output.stdout.len() > 4096 {
        return ToolStatus::Invalid;
    }
    let version = match std::str::from_utf8(&output.stdout)
        .ok()
        .and_then(|text| parse_tool_version(text, name))
    {
        Some(version) => version,
        None => return ToolStatus::Invalid,
    };
    if version >= minimum {
        ToolStatus::Ready(version)
    } else {
        ToolStatus::Unsupported(version)
    }
}

fn parse_tool_version(output: &str, expected_name: &str) -> Option<ToolVersion> {
    let mut fields = output.split_ascii_whitespace();
    if fields.next()? != expected_name {
        return None;
    }
    let version_field = fields.next()?;
    let version = version_field.split('-').next()?;
    let mut numbers = version.split('.');
    let parsed = ToolVersion::new(
        numbers.next()?.parse().ok()?,
        numbers.next()?.parse().ok()?,
        numbers.next()?.parse().ok()?,
    );
    if numbers.next().is_some() {
        return None;
    }
    Some(parsed)
}

fn inspect_binary(cargo_home: Option<&Path>) -> InstallationStatus {
    let Some(cargo_home) = cargo_home else {
        return InstallationStatus::Invalid;
    };
    let source = match std::env::current_exe() {
        Ok(source) => source,
        Err(_) => return InstallationStatus::Invalid,
    };
    match inspect_release_binary(&source, cargo_home) {
        Ok(BinaryInstallationStatus::Ready) => InstallationStatus::Ready,
        Ok(BinaryInstallationStatus::Missing) => InstallationStatus::Missing,
        Ok(BinaryInstallationStatus::Conflict) => InstallationStatus::Conflict,
        Err(_) => InstallationStatus::Invalid,
    }
}

fn inspect_config(cargo_home: Option<&Path>) -> ConfigStatus {
    let Some(cargo_home) = cargo_home else {
        return ConfigStatus::Invalid;
    };
    match inspect_workspace_wrapper(cargo_home) {
        Ok(WorkspaceWrapperStatus::ActiveOwned) => ConfigStatus::ActiveOwned,
        Ok(WorkspaceWrapperStatus::ActiveUnclaimed) => ConfigStatus::ActiveUnclaimed,
        Ok(WorkspaceWrapperStatus::Missing) => ConfigStatus::Missing,
        Ok(WorkspaceWrapperStatus::Conflict) => ConfigStatus::Conflict,
        Err(_) => ConfigStatus::Invalid,
    }
}

fn count_environment_overrides(mut is_present: impl FnMut(&str) -> bool) -> usize {
    COMPILER_OVERRIDE_VARIABLES
        .iter()
        .filter(|name| is_present(name))
        .count()
}

fn inspect_local_configs(cwd: &Path, cargo_home: Option<&Path>) -> LocalConfigStatus {
    if !cwd.is_absolute() {
        return LocalConfigStatus {
            kind: LocalConfigKind::Invalid,
            count: 0,
        };
    }
    let mut count = 0;
    let mut found_override = false;
    for ancestor in cwd.ancestors() {
        let cargo_directory = ancestor.join(".cargo");
        if cargo_home.is_some_and(|home| home == cargo_directory) {
            continue;
        }
        let metadata = match fs::symlink_metadata(&cargo_directory) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return invalid_local_config(count),
            Ok(metadata) => metadata,
        };
        if !metadata.file_type().is_dir() {
            return invalid_local_config(count);
        }
        let legacy = cargo_directory.join("config");
        let preferred = cargo_directory.join("config.toml");
        let legacy_present = path_entry_exists(&legacy);
        let preferred_present = path_entry_exists(&preferred);
        if legacy_present && preferred_present {
            return invalid_local_config(count + 2);
        }
        for path in [legacy, preferred] {
            if !path_entry_exists(&path) {
                continue;
            }
            count += 1;
            let document = match read_local_config(&path) {
                Ok(document) => document,
                Err(_) => return invalid_local_config(count),
            };
            match local_config_has_compiler_override(&document) {
                Ok(has_override) => found_override |= has_override,
                Err(()) => return invalid_local_config(count),
            }
        }
    }
    let kind = if found_override {
        LocalConfigKind::Override
    } else if count > 0 {
        LocalConfigKind::Present
    } else {
        LocalConfigKind::Clear
    };
    LocalConfigStatus { kind, count }
}

fn invalid_local_config(count: usize) -> LocalConfigStatus {
    LocalConfigStatus {
        kind: LocalConfigKind::Invalid,
        count,
    }
}

fn path_entry_exists(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(_) => true,
    }
}

fn read_local_config(path: &Path) -> Result<DocumentMut, ()> {
    let before = fs::symlink_metadata(path).map_err(|_| ())?;
    if !before.file_type().is_file() || before.len() > MAXIMUM_LOCAL_CONFIG_BYTES {
        return Err(());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options.open(path).map_err(|_| ())?;
    let opened = file.metadata().map_err(|_| ())?;
    if !opened.file_type().is_file() || !same_file(&before, &opened) {
        return Err(());
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    Read::by_ref(&mut file)
        .take(MAXIMUM_LOCAL_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAXIMUM_LOCAL_CONFIG_BYTES {
        return Err(());
    }
    let after = file.metadata().map_err(|_| ())?;
    let current = fs::symlink_metadata(path).map_err(|_| ())?;
    if !same_snapshot(&opened, &after) || !same_snapshot(&after, &current) {
        return Err(());
    }
    let source = std::str::from_utf8(&bytes).map_err(|_| ())?;
    source.parse::<DocumentMut>().map_err(|_| ())
}

fn local_config_has_compiler_override(document: &DocumentMut) -> Result<bool, ()> {
    if document.get("include").is_some() {
        return Ok(true);
    }
    let Some(build_item) = document.get("build") else {
        return Ok(false);
    };
    let build = build_item.as_table_like().ok_or(())?;
    Ok(["rustc", "rustc-wrapper", "rustc-workspace-wrapper"]
        .iter()
        .any(|key| build.get(key).is_some()))
}

fn inspect_limited_endpoint(value: Option<OsString>) -> EndpointStatus {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return EndpointStatus::Missing;
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return EndpointStatus::Invalid;
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return EndpointStatus::Missing,
    };
    #[cfg(unix)]
    if metadata.file_type().is_socket() {
        return EndpointStatus::Ready;
    }
    EndpointStatus::Invalid
}

fn system_unit_active(name: &str) -> Option<bool> {
    Command::new("systemctl")
        .args(["is-active", "--quiet", name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .map(|status| status.success())
}

fn classify_service(
    service_active: Option<bool>,
    agent_socket_active: Option<bool>,
    admin_socket_active: Option<bool>,
    endpoint: EndpointStatus,
) -> ServiceStatus {
    if endpoint != EndpointStatus::Ready {
        return ServiceStatus::Unavailable;
    }
    if service_active == Some(true)
        || (agent_socket_active == Some(true) && admin_socket_active == Some(true))
    {
        return ServiceStatus::Ready;
    }
    if service_active.is_none() && agent_socket_active.is_none() && admin_socket_active.is_none() {
        ServiceStatus::Unknown
    } else {
        ServiceStatus::Unavailable
    }
}

fn render_tool_status(name: &str, status: ToolStatus, minimum: ToolVersion) -> String {
    match status {
        ToolStatus::Ready(version) => format!(
            "{name}=ready version={} minimum={}\n",
            version.render(),
            minimum.render()
        ),
        ToolStatus::Unsupported(version) => format!(
            "{name}=unsupported version={} minimum={}\n",
            version.render(),
            minimum.render()
        ),
        ToolStatus::Unavailable => format!("{name}=unavailable minimum={}\n", minimum.render()),
        ToolStatus::Invalid => format!("{name}=invalid minimum={}\n", minimum.render()),
    }
}

fn render_installation(status: InstallationStatus) -> &'static str {
    match status {
        InstallationStatus::Ready => "ready",
        InstallationStatus::Missing => "missing",
        InstallationStatus::Conflict => "conflict",
        InstallationStatus::Invalid => "invalid",
    }
}

fn render_config(status: ConfigStatus) -> &'static str {
    match status {
        ConfigStatus::ActiveOwned => "active-owned",
        ConfigStatus::ActiveUnclaimed => "active-unclaimed",
        ConfigStatus::Missing => "missing",
        ConfigStatus::Conflict => "conflict",
        ConfigStatus::Invalid => "invalid",
    }
}

fn render_local_config(status: LocalConfigKind) -> &'static str {
    match status {
        LocalConfigKind::Clear => "clear",
        LocalConfigKind::Present => "present",
        LocalConfigKind::Override => "override-detected",
        LocalConfigKind::Invalid => "invalid",
    }
}

fn render_service(status: ServiceStatus) -> &'static str {
    match status {
        ServiceStatus::Ready => "ready",
        ServiceStatus::Unavailable => "unavailable",
        ServiceStatus::Unknown => "unknown",
    }
}

#[cfg(unix)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn same_snapshot(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    same_file(left, right)
        && left.len() == right.len()
        && left.mode() == right.mode()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

#[cfg(not(unix))]
fn same_snapshot(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
#[path = "tests/Cargo_Doctor_test.rs"]
mod cargo_doctor_tests;
