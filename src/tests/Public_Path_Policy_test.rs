#[cfg(test)]
mod tests {
    use crate::structs::{
        reject_public_path_references, sanitize_public_text, validate_project_relative_path,
        validate_serialized_public_document, PUBLIC_PATH_REDACTION,
    };

    #[test]
    fn accepts_canonical_project_relative_source_paths() {
        for path in [
            "Cargo.toml",
            ".cargo/config.toml",
            "src/lib.rs",
            "crates/engine/src/main.rs",
            "assets/My File.txt",
            "src/..hidden/file.rs",
        ] {
            validate_project_relative_path(path, "fixture path")
                .unwrap_or_else(|error| panic!("valid relative path {path:?} failed: {error}"));
        }
    }

    #[test]
    fn rejects_noncanonical_or_externally_resolvable_paths() {
        for path in [
            "/home/operator/project/src/lib.rs",
            "//server/share/source.rs",
            "C:/private/source.rs",
            "D:\\private\\source.rs",
            "\\\\server\\share\\source.rs",
            "\\rooted\\source.rs",
            "~/private/source.rs",
            "~operator/private/source.rs",
            "$HOME/private/source.rs",
            "${CARGO_HOME}/antdelirium/state.db",
            "%USERPROFILE%\\private\\source.rs",
            "file:///var/lib/private/source.rs",
            "https://example.com/source.rs",
            "../plan/task.json",
            "src/../../plan/task.json",
            "src/./lib.rs",
            "src//lib.rs",
            "src/lib.rs/",
            "src/%2e%2e/plan.json",
            "src/%252fprivate/source.rs",
        ] {
            assert!(
                validate_project_relative_path(path, "fixture path").is_err(),
                "unsafe path was accepted: {path:?}"
            );
        }
    }

    #[test]
    fn sanitizes_physical_and_encoded_references_but_preserves_public_urls() {
        let value = concat!(
            "unix=/home/operator/private.rs; ",
            "drive=C:\\private\\source.rs; ",
            "unc=\\\\server\\share\\source.rs; ",
            "home=~/private; user=~operator/private; ",
            "env=$HOME/private; braced=${CARGO_HOME}/antdelirium; ",
            "percent=%USERPROFILE%\\private; uri=file:///var/lib/private; ",
            "traversal=../plan/task.json; nested=src/../../plan/task.json; ",
            "encoded=%2fvar%2flib; ",
            "public=https://docs.example.test/rules/type_logic; relative=src/lib.rs"
        );
        let sanitized = sanitize_public_text(value);

        for forbidden in [
            "/home/",
            "C:\\private",
            "\\\\server",
            "~/",
            "~operator/",
            "$HOME/",
            "${CARGO_HOME}",
            "%USERPROFILE%",
            "file:///",
            "../",
            "%2f",
        ] {
            assert!(
                !sanitized
                    .to_ascii_lowercase()
                    .contains(&forbidden.to_ascii_lowercase()),
                "private path representation survived: {forbidden} in {sanitized}"
            );
        }
        assert!(sanitized.matches(PUBLIC_PATH_REDACTION).count() >= 10);
        assert!(sanitized.contains("https://docs.example.test/rules/type_logic"));
        assert!(sanitized.contains("relative=src/lib.rs"));
    }

    #[test]
    fn public_text_validator_rejects_paths_and_accepts_urls_and_relative_hints() {
        for unsafe_text in [
            "Inspect /etc/passwd.",
            "Inspect C:\\private\\state.db.",
            "Inspect \\\\server\\share\\state.db.",
            "Inspect $CARGO_HOME/antdelirium/state.db.",
            "Inspect file:///var/lib/antdelirium/state.db.",
            "Inspect src/../../plan/task.json.",
            "Inspect %2fvar%2flib%2fstate.db.",
        ] {
            assert!(
                reject_public_path_references(unsafe_text, "fixture text").is_err(),
                "unsafe public text was accepted: {unsafe_text:?}"
            );
        }

        reject_public_path_references(
            "Read src/lib.rs and https://docs.example.test/rules/type_logic.",
            "fixture text",
        )
        .expect("relative source hints and public HTTPS URLs should remain valid");
    }

    #[test]
    fn serialized_document_validator_decodes_json_path_escapes() {
        for unsafe_document in [
            br#"{"path":"/home/operator/private.rs"}"#.as_slice(),
            br#"{"path":"\u002fvar\u002flib\u002fprivate.rs"}"#.as_slice(),
            br#"{"path":"C:\\private\\source.rs"}"#.as_slice(),
        ] {
            assert!(
                validate_serialized_public_document(unsafe_document, "fixture document").is_err(),
                "escaped private path survived: {}",
                String::from_utf8_lossy(unsafe_document)
            );
        }
        validate_serialized_public_document(
            br#"{"path":"src/lib.rs","docs":"https://docs.example.test/rules"}"#,
            "fixture document",
        )
        .expect("relative paths and HTTPS documentation should remain valid");
    }
}
