use std::ffi::{OsStr, OsString};
use std::path::Path;

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
}
