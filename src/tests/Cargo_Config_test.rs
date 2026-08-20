use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use toml_edit::value;

#[cfg(unix)]
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fixture_cargo_home(label: &str) -> PathBuf {
    let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("fixture clock after epoch")
        .as_nanos();
    let cargo_home = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/t72-config-tests")
        .join(format!("{label}-{}-{nonce}-{sequence}", std::process::id()))
        .join("cargo-home");
    fs::create_dir_all(&cargo_home).expect("create Cargo home fixture");
    cargo_home
}

fn insert_fixture_setting(document: &mut DocumentMut) -> Result<(), CargoConfigError> {
    document["antdelirium-fixture"]["enabled"] = value(true);
    Ok(())
}

fn expected_wrapper(cargo_home: &Path) -> String {
    cargo_home
        .join("bin")
        .join(INSTALLED_BINARY_NAME)
        .to_str()
        .expect("UTF-8 fixture path")
        .to_owned()
}

#[test]
fn configures_only_the_workspace_wrapper_and_preserves_other_build_settings() {
    let cargo_home = fixture_cargo_home("workspace-wrapper");
    let config = cargo_home.join(CONFIG_FILE_NAME);
    let original = b"# keep heading\n[build]\nrustc = \"custom-rustc\"\njobs = 3\n\n[target.x86_64-unknown-linux-gnu]\nrunner = \"custom-runner\"\n";
    fs::write(&config, original).expect("write original config");

    assert_eq!(
        configure_workspace_wrapper(&cargo_home),
        Ok(CargoConfigMergeOutcome::Updated)
    );
    let merged = fs::read_to_string(&config).expect("read configured Cargo file");
    let parsed = merged
        .parse::<DocumentMut>()
        .expect("parse configured Cargo file");
    assert_eq!(parsed["build"]["rustc"].as_str(), Some("custom-rustc"));
    assert_eq!(parsed["build"]["jobs"].as_integer(), Some(3));
    assert_eq!(
        parsed["target"]["x86_64-unknown-linux-gnu"]["runner"].as_str(),
        Some("custom-runner")
    );
    assert_eq!(
        parsed["build"]["rustc-workspace-wrapper"].as_str(),
        Some(expected_wrapper(&cargo_home).as_str())
    );
    assert!(parsed["build"].get("rustc-wrapper").is_none());
    assert_eq!(
        read_verified_config_backup(&cargo_home)
            .expect("verified wrapper backup")
            .original_bytes,
        original
    );
    assert_eq!(
        inspect_workspace_wrapper(&cargo_home),
        Ok(WorkspaceWrapperStatus::ActiveOwned)
    );
    #[cfg(unix)]
    {
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600))
            .expect("tamper installed config mode");
        assert_eq!(
            inspect_workspace_wrapper(&cargo_home),
            Ok(WorkspaceWrapperStatus::Conflict)
        );
    }
}

#[cfg(unix)]
#[test]
fn identical_preexisting_workspace_wrapper_is_an_unclaimed_no_op() {
    let cargo_home = fixture_cargo_home("workspace-wrapper-noop");
    let config = cargo_home.join(CONFIG_FILE_NAME);
    let configured = format!(
        "[build]\nrustc-workspace-wrapper = {:?}\n",
        expected_wrapper(&cargo_home)
    );
    fs::write(&config, configured.as_bytes()).expect("write matching wrapper");
    let before = fs::metadata(&config).expect("matching wrapper metadata");

    assert_eq!(
        configure_workspace_wrapper(&cargo_home),
        Ok(CargoConfigMergeOutcome::Unchanged)
    );
    let after = fs::metadata(&config).expect("matching wrapper metadata after no-op");
    assert_eq!(before.dev(), after.dev());
    assert_eq!(before.ino(), after.ino());
    assert_eq!(
        fs::read_to_string(&config).expect("matching wrapper bytes"),
        configured
    );
    assert!(!cargo_home.join(BACKUP_DIRECTORY_NAME).exists());
    assert_eq!(
        inspect_workspace_wrapper(&cargo_home),
        Ok(WorkspaceWrapperStatus::ActiveUnclaimed)
    );
}

#[test]
fn refuses_cache_divergent_wrapper_and_malformed_build_without_writing() {
    let cases = [
        ("cache", "[build]\nrustc-wrapper = \"sccache\"\n"),
        (
            "divergent",
            "[build]\nrustc-workspace-wrapper = \"/opt/other-wrapper\"\n",
        ),
        ("wrong-type", "[build]\nrustc-workspace-wrapper = 7\n"),
        ("malformed-build", "build = \"not-a-table\"\n"),
    ];
    for (label, original) in cases {
        let cargo_home = fixture_cargo_home(label);
        let config = cargo_home.join(CONFIG_FILE_NAME);
        fs::write(&config, original.as_bytes()).expect("write conflicting config");
        assert_eq!(
            configure_workspace_wrapper(&cargo_home),
            Err(CargoConfigError::WrapperConflict),
            "case {label}"
        );
        assert_eq!(
            fs::read_to_string(&config).expect("read preserved conflicting config"),
            original,
            "case {label}"
        );
        assert!(!cargo_home.join(BACKUP_DIRECTORY_NAME).exists());
        assert_eq!(
            inspect_workspace_wrapper(&cargo_home),
            Ok(WorkspaceWrapperStatus::Conflict),
            "inspection case {label}"
        );
    }
}

#[test]
fn inspection_distinguishes_missing_workspace_wrapper() {
    let cargo_home = fixture_cargo_home("workspace-wrapper-missing");
    fs::write(
        cargo_home.join(CONFIG_FILE_NAME),
        b"[net]\noffline = true\n",
    )
    .expect("write config without wrapper");
    assert_eq!(
        inspect_workspace_wrapper(&cargo_home),
        Ok(WorkspaceWrapperStatus::Missing)
    );
}

#[test]
fn merges_without_losing_comments_order_or_existing_keys_and_creates_exact_backup() {
    let cargo_home = fixture_cargo_home("merge");
    let config = cargo_home.join(CONFIG_FILE_NAME);
    let original = b"# user heading\n[net]\nretry = 3 # keep retry\n\n[build]\njobs = 2\ntarget-dir = \"custom-target\"\n\n[alias]\nxtask = \"run --package xtask\"\n";
    fs::write(&config, original).expect("write original config");
    #[cfg(unix)]
    fs::set_permissions(&config, fs::Permissions::from_mode(0o640)).expect("set original mode");

    assert_eq!(
        merge_cargo_config(&cargo_home, insert_fixture_setting),
        Ok(CargoConfigMergeOutcome::Updated)
    );
    let merged = fs::read_to_string(&config).expect("read merged config");
    assert!(merged.contains("# user heading"));
    assert!(merged.contains("retry = 3 # keep retry"));
    assert!(merged.contains("target-dir = \"custom-target\""));
    assert!(merged.contains("xtask = \"run --package xtask\""));
    let parsed = merged.parse::<DocumentMut>().expect("parse merged config");
    assert_eq!(parsed["net"]["retry"].as_integer(), Some(3));
    assert_eq!(parsed["build"]["jobs"].as_integer(), Some(2));
    assert_eq!(
        parsed["build"]["target-dir"].as_str(),
        Some("custom-target")
    );
    assert_eq!(
        parsed["antdelirium-fixture"]["enabled"].as_bool(),
        Some(true)
    );

    let backup = read_verified_config_backup(&cargo_home).expect("verified backup");
    assert!(backup.original_present);
    assert_eq!(backup.original_bytes, original);
    assert_eq!(backup.original_mode, Some(0o640));
    assert_eq!(
        backup.installed_sha256,
        lowercase_hex(&sha256(merged.as_bytes()))
    );
    #[cfg(unix)]
    {
        assert_eq!(
            fs::metadata(&config)
                .expect("merged metadata")
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        let backup_directory = cargo_home.join(BACKUP_DIRECTORY_NAME);
        assert_eq!(
            fs::metadata(&backup_directory)
                .expect("backup directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        for name in [BACKUP_PAYLOAD_NAME, BACKUP_RECEIPT_NAME] {
            assert_eq!(
                fs::metadata(backup_directory.join(name))
                    .expect("backup file metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn creates_new_config_with_recoverable_absence_record() {
    let cargo_home = fixture_cargo_home("absent");
    assert_eq!(
        merge_cargo_config(&cargo_home, insert_fixture_setting),
        Ok(CargoConfigMergeOutcome::Updated)
    );
    let config = cargo_home.join(CONFIG_FILE_NAME);
    let merged = fs::read_to_string(&config).expect("read new config");
    let parsed = merged.parse::<DocumentMut>().expect("parse new config");
    assert_eq!(
        parsed["antdelirium-fixture"]["enabled"].as_bool(),
        Some(true)
    );
    let backup = read_verified_config_backup(&cargo_home).expect("verified absence backup");
    assert!(!backup.original_present);
    assert!(backup.original_bytes.is_empty());
    assert_eq!(backup.original_mode, None);
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(config)
            .expect("new config metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn no_op_merge_preserves_inode_and_does_not_create_backup() {
    let cargo_home = fixture_cargo_home("noop");
    let config = cargo_home.join(CONFIG_FILE_NAME);
    fs::write(&config, b"[net]\noffline = true\n").expect("write original config");
    let before = fs::metadata(&config).expect("metadata before no-op");
    assert_eq!(
        merge_cargo_config(&cargo_home, |_| Ok(())),
        Ok(CargoConfigMergeOutcome::Unchanged)
    );
    assert_eq!(
        fs::read(&config).expect("read no-op config"),
        b"[net]\noffline = true\n"
    );
    assert!(!cargo_home.join(BACKUP_DIRECTORY_NAME).exists());
    #[cfg(unix)]
    {
        let after = fs::metadata(&config).expect("metadata after no-op");
        assert_eq!(before.dev(), after.dev());
        assert_eq!(before.ino(), after.ino());
    }
}

#[test]
fn invalid_or_legacy_configuration_fails_without_backup_or_rewrite() {
    let invalid_home = fixture_cargo_home("invalid");
    let invalid_config = invalid_home.join(CONFIG_FILE_NAME);
    fs::write(&invalid_config, b"[build\ninvalid =").expect("write invalid TOML");
    assert_eq!(
        merge_cargo_config(&invalid_home, insert_fixture_setting),
        Err(CargoConfigError::InvalidConfig)
    );
    assert_eq!(
        fs::read(&invalid_config).expect("read invalid TOML"),
        b"[build\ninvalid ="
    );
    assert!(!invalid_home.join(BACKUP_DIRECTORY_NAME).exists());

    let legacy_home = fixture_cargo_home("legacy");
    fs::write(
        legacy_home.join(LEGACY_CONFIG_FILE_NAME),
        b"[net]\nretry = 1\n",
    )
    .expect("write legacy config");
    assert_eq!(
        merge_cargo_config(&legacy_home, insert_fixture_setting),
        Err(CargoConfigError::LegacyConfigConflict)
    );
    assert!(!legacy_home.join(CONFIG_FILE_NAME).exists());
    assert!(!legacy_home.join(BACKUP_DIRECTORY_NAME).exists());
}

#[cfg(unix)]
#[test]
fn symlinked_config_is_rejected_without_touching_its_target() {
    let cargo_home = fixture_cargo_home("symlink");
    let external = cargo_home
        .parent()
        .expect("fixture parent")
        .join("external.toml");
    fs::write(&external, b"[net]\nretry = 9\n").expect("write external config");
    symlink(&external, cargo_home.join(CONFIG_FILE_NAME)).expect("symlink config");
    assert_eq!(
        merge_cargo_config(&cargo_home, insert_fixture_setting),
        Err(CargoConfigError::UnsafeFilesystemEntry)
    );
    assert_eq!(
        fs::read(external).expect("read external config"),
        b"[net]\nretry = 9\n"
    );
    assert!(!cargo_home.join(BACKUP_DIRECTORY_NAME).exists());
}

#[test]
fn existing_backup_and_concurrent_content_change_are_never_overwritten() {
    let conflict_home = fixture_cargo_home("backup-conflict");
    let conflict_config = conflict_home.join(CONFIG_FILE_NAME);
    fs::write(&conflict_config, b"[net]\nretry = 1\n").expect("write conflict config");
    fs::create_dir(conflict_home.join(BACKUP_DIRECTORY_NAME)).expect("create backup conflict");
    assert_eq!(
        merge_cargo_config(&conflict_home, insert_fixture_setting),
        Err(CargoConfigError::BackupConflict)
    );
    assert_eq!(
        fs::read(&conflict_config).expect("read preserved conflict config"),
        b"[net]\nretry = 1\n"
    );

    let concurrent_home = fixture_cargo_home("concurrent");
    let concurrent_config = concurrent_home.join(CONFIG_FILE_NAME);
    fs::write(&concurrent_config, b"[net]\nretry = 1\n").expect("write concurrent config");
    let path_for_mutation = concurrent_config.clone();
    let result = merge_cargo_config(&concurrent_home, move |document| {
        document["antdelirium-fixture"]["enabled"] = value(true);
        fs::write(&path_for_mutation, b"[net]\nretry = 2\n").map_err(|_| CargoConfigError::Io)
    });
    assert_eq!(result, Err(CargoConfigError::ConcurrentChange));
    assert_eq!(
        fs::read(&concurrent_config).expect("read concurrent config"),
        b"[net]\nretry = 2\n"
    );
    assert!(read_verified_config_backup(&concurrent_home).is_ok());
}
