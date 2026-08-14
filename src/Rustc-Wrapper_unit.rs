use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::public_output::{OutputRouter, PublicError};

pub(crate) const WRAPPER_DEPTH_ENV: &str = "ANTDELIRIUM_WRAPPER_DEPTH";

/// Determines whether governance may run before delegation. A recursive entry
/// is never allowed to enter governance again; T6.2 will use `DelegationOnly`
/// to execute the real compiler directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WrapperRoute {
    Governed,
    DelegationOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WrapperProtocolError {
    InvalidDepth,
}

/// Replaces the wrapper process with the compiler on Unix. Process replacement
/// keeps stdin/stdout/stderr, PID, exit status and signal semantics under the
/// compiler's control instead of emulating them in a supervising process.
pub(crate) fn run_wrapper(args: &[OsString], route: WrapperRoute) -> ! {
    let mut command = match compiler_command(args, route) {
        Some(command) => command,
        None => OutputRouter::exit(PublicError::Internal),
    };

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        let error = command.exec();
        OutputRouter::exit(toolchain_error(&error));
    }

    #[cfg(not(unix))]
    {
        match command.status() {
            Ok(status) => match status.code() {
                Some(code) => std::process::exit(code),
                None => OutputRouter::exit(PublicError::Internal),
            },
            Err(error) => OutputRouter::exit(toolchain_error(&error)),
        }
    }
}

fn compiler_command(args: &[OsString], route: WrapperRoute) -> Option<Command> {
    let (compiler, rustc_args) = args.split_first()?;
    let next_depth = match route {
        WrapperRoute::Governed | WrapperRoute::DelegationOnly => "1",
    };

    let mut command = Command::new(compiler);
    command
        .args(rustc_args)
        .env(WRAPPER_DEPTH_ENV, next_depth)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    Some(command)
}

fn toolchain_error(error: &io::Error) -> PublicError {
    match error.kind() {
        io::ErrorKind::NotFound => PublicError::ToolchainNotFound,
        _ => PublicError::ToolchainNotExecutable,
    }
}

/// Recognizes only Cargo's wrapper shape without resolving, opening or probing
/// the compiler path. An arbitrary first argument must not turn classification
/// into a filesystem oracle for paths outside the project.
pub(crate) fn detect_wrapper_invocation(
    args: &[OsString],
    wrapper_depth: Option<&OsStr>,
) -> Result<Option<WrapperRoute>, WrapperProtocolError> {
    if !has_unambiguous_wrapper_shape(args) {
        return Ok(None);
    }

    match wrapper_depth {
        None => Ok(Some(WrapperRoute::Governed)),
        Some(depth) if depth == OsStr::new("0") => Ok(Some(WrapperRoute::Governed)),
        Some(depth) if depth == OsStr::new("1") => Ok(Some(WrapperRoute::DelegationOnly)),
        Some(_) => Err(WrapperProtocolError::InvalidDepth),
    }
}

fn has_unambiguous_wrapper_shape(args: &[OsString]) -> bool {
    let Some(compiler) = args.first() else {
        return false;
    };
    if !is_qualified_rustc_path(compiler) || begins_with_antdelirium_command(args) {
        return false;
    }

    args.iter().skip(1).any(|arg| is_rustc_marker(arg))
}

fn is_qualified_rustc_path(value: &OsStr) -> bool {
    let path = Path::new(value);
    let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    if !matches!(file_name, "rustc" | "rustc.exe") {
        return false;
    }

    path.is_absolute() || path.components().count() > 1
}

fn begins_with_antdelirium_command(args: &[OsString]) -> bool {
    args.get(1).and_then(|arg| arg.to_str()).is_some_and(|arg| {
        matches!(
            arg,
            "service"
                | "daemon"
                | "review"
                | "reviewer"
                | "operator"
                | "install"
                | "uninstall"
                | "migrate-plugin"
                | "doctor"
                | "init"
                | "check"
                | "status"
                | "next-action"
                | "--api"
        )
    })
}

fn is_rustc_marker(value: &OsStr) -> bool {
    let Some(value) = value.to_str() else {
        return false;
    };
    matches!(
        value,
        "--crate-name"
            | "--crate-type"
            | "--emit"
            | "--print"
            | "--target"
            | "--sysroot"
            | "--version"
            | "-V"
            | "-vV"
            | "--help"
            | "-h"
    ) || value.starts_with("--crate-name=")
        || value.starts_with("--crate-type=")
        || value.starts_with("--emit=")
        || value.starts_with("--print=")
        || value.starts_with("--target=")
        || value.starts_with("--sysroot=")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn recognizes_qualified_unix_windows_and_relative_rustc_paths() {
        for invocation in [
            args(&["/toolchain/bin/rustc", "--crate-name", "demo"]),
            args(&["C:/toolchain/bin/rustc.exe", "--print=sysroot"]),
            args(&["./rustc", "-vV"]),
        ] {
            assert_eq!(
                detect_wrapper_invocation(&invocation, None),
                Ok(Some(WrapperRoute::Governed))
            );
        }
    }

    #[test]
    fn recognizes_compilation_and_toolchain_query_markers() {
        for marker in [
            "--crate-name=demo",
            "--crate-type",
            "--emit=metadata",
            "--print",
            "--target=x86_64-unknown-linux-gnu",
            "--sysroot",
            "--version",
            "-V",
            "-vV",
            "--help",
            "-h",
        ] {
            assert_eq!(
                detect_wrapper_invocation(&args(&["/toolchain/bin/rustc", marker]), None),
                Ok(Some(WrapperRoute::Governed)),
                "marker was not recognized: {marker}"
            );
        }
    }

    #[test]
    fn rejects_ambiguous_compiler_shapes_without_filesystem_access() {
        for invocation in [
            args(&[]),
            args(&["rustc", "--crate-name", "demo"]),
            args(&["/toolchain/bin/rustc", "input.rs"]),
            args(&["/toolchain/bin/compiler", "--crate-name", "demo"]),
            args(&["--api", "/toolchain/bin/rustc", "--crate-name"]),
        ] {
            assert_eq!(detect_wrapper_invocation(&invocation, None), Ok(None));
        }
    }

    #[test]
    fn rejects_antdelirium_subcommands_in_the_wrapper_command_position() {
        for command in [
            "service",
            "review",
            "operator",
            "install",
            "uninstall",
            "migrate-plugin",
            "doctor",
            "init",
            "check",
            "status",
            "next-action",
            "--api",
        ] {
            let invocation = args(&["/toolchain/bin/rustc", command, "--crate-name=demo"]);
            assert_eq!(detect_wrapper_invocation(&invocation, None), Ok(None));
        }
    }

    #[test]
    fn depth_zero_is_governed_and_depth_one_is_delegation_only() {
        let invocation = args(&["/toolchain/bin/rustc", "--emit=metadata"]);
        assert_eq!(
            detect_wrapper_invocation(&invocation, Some(OsStr::new("0"))),
            Ok(Some(WrapperRoute::Governed))
        );
        assert_eq!(
            detect_wrapper_invocation(&invocation, Some(OsStr::new("1"))),
            Ok(Some(WrapperRoute::DelegationOnly))
        );
    }

    #[test]
    fn invalid_or_excessive_depth_fails_closed_for_wrapper_invocations() {
        let invocation = args(&["/toolchain/bin/rustc", "--emit=metadata"]);
        for depth in ["", "01", "2", "999", "-1", "governed"] {
            assert_eq!(
                detect_wrapper_invocation(&invocation, Some(OsStr::new(depth))),
                Err(WrapperProtocolError::InvalidDepth)
            );
        }
    }

    #[test]
    fn depth_cannot_turn_an_agent_or_operator_command_into_wrapper_mode() {
        for invocation in [
            args(&["--api", "hierarchy.task.current"]),
            args(&["install"]),
        ] {
            assert_eq!(
                detect_wrapper_invocation(&invocation, Some(OsStr::new("1"))),
                Ok(None)
            );
        }
    }

    #[test]
    fn compiler_command_preserves_program_and_argument_order() {
        let invocation = args(&[
            "/toolchain/bin/rustc",
            "--crate-name",
            "demo",
            "source with spaces.rs",
            "--emit=metadata,link",
        ]);
        let command = compiler_command(&invocation, WrapperRoute::Governed).expect("command");

        assert_eq!(command.get_program(), OsStr::new("/toolchain/bin/rustc"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            invocation
                .iter()
                .skip(1)
                .map(OsString::as_os_str)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn both_routes_set_exactly_one_internal_depth_for_the_compiler() {
        let invocation = args(&["/toolchain/bin/rustc", "--version"]);
        for route in [WrapperRoute::Governed, WrapperRoute::DelegationOnly] {
            let command = compiler_command(&invocation, route).expect("command");
            let depth = command
                .get_envs()
                .find(|(name, _)| *name == OsStr::new(WRAPPER_DEPTH_ENV));
            assert_eq!(
                depth,
                Some((OsStr::new(WRAPPER_DEPTH_ENV), Some(OsStr::new("1"))))
            );
        }
    }

    #[test]
    fn missing_and_other_exec_failures_use_only_public_toolchain_errors() {
        assert_eq!(
            toolchain_error(&io::Error::from(io::ErrorKind::NotFound)),
            PublicError::ToolchainNotFound
        );
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::InvalidInput,
            io::ErrorKind::OutOfMemory,
        ] {
            assert_eq!(
                toolchain_error(&io::Error::from(kind)),
                PublicError::ToolchainNotExecutable
            );
        }
    }
}
