use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fixture_root(label: &str) -> PathBuf {
    let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("fixture clock after epoch")
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/t71-installer-tests")
        .join(format!("{label}-{}-{nonce}-{sequence}", std::process::id()));
    fs::create_dir_all(&root).expect("create installer fixture");
    root
}

fn executable_source() -> PathBuf {
    std::env::current_exe().expect("current test executable")
}

#[test]
fn integrated_install_activates_only_the_workspace_wrapper() {
    let root = fixture_root("integrated");
    let cargo_home = root.join("cargo-home");
    let bin = cargo_home.join("bin");
    fs::create_dir_all(&bin).expect("create Cargo bin");
    let cargo_proxy = bin.join("cargo");
    fs::write(&cargo_proxy, b"rustup proxy sentinel\n").expect("write proxy sentinel");
    fs::write(
        cargo_home.join("config.toml"),
        b"# preserve\n[net]\noffline = true\n",
    )
    .expect("write existing Cargo config");

    assert_eq!(
        install_release_and_configure(&executable_source(), &cargo_home),
        Ok(CargoInstallOutcome {
            binary: BinaryInstallOutcome::Installed,
            config: CargoConfigMergeOutcome::Updated,
        })
    );
    let config =
        fs::read_to_string(cargo_home.join("config.toml")).expect("read integrated Cargo config");
    let parsed = config
        .parse::<toml_edit::DocumentMut>()
        .expect("parse integrated Cargo config");
    assert_eq!(parsed["net"]["offline"].as_bool(), Some(true));
    assert_eq!(
        parsed["build"]["rustc-workspace-wrapper"].as_str(),
        cargo_home.join("bin").join(INSTALLED_BINARY_NAME).to_str()
    );
    assert!(parsed["build"].get("rustc-wrapper").is_none());
    assert_eq!(
        fs::read(cargo_proxy).expect("read preserved Cargo proxy"),
        b"rustup proxy sentinel\n"
    );
    assert!(cargo_home
        .join(".antdelirium-install/config-backup-v1.json")
        .is_file());
    assert_eq!(
        inspect_release_binary(&executable_source(), &cargo_home),
        Ok(BinaryInstallationStatus::Ready)
    );
}

#[test]
fn integrated_install_is_idempotent_after_owned_configuration() {
    let root = fixture_root("integrated-idempotent");
    let cargo_home = root.join("cargo-home");
    fs::create_dir_all(&cargo_home).expect("create Cargo home");
    let source = executable_source();
    assert!(install_release_and_configure(&source, &cargo_home).is_ok());
    let config = cargo_home.join("config.toml");
    let before = fs::metadata(&config).expect("configured metadata");

    assert_eq!(
        install_release_and_configure(&source, &cargo_home),
        Ok(CargoInstallOutcome {
            binary: BinaryInstallOutcome::AlreadyInstalled,
            config: CargoConfigMergeOutcome::Unchanged,
        })
    );
    #[cfg(unix)]
    {
        let after = fs::metadata(config).expect("configured metadata after retry");
        assert_eq!(before.dev(), after.dev());
        assert_eq!(before.ino(), after.ino());
    }
}

#[test]
fn integrated_uninstall_restores_config_and_preserves_cargo_proxy() {
    let root = fixture_root("integrated-uninstall");
    let cargo_home = root.join("cargo-home");
    let bin = cargo_home.join("bin");
    fs::create_dir_all(&bin).expect("create Cargo bin");
    let cargo_proxy = bin.join("cargo");
    let config = cargo_home.join("config.toml");
    let original = b"# original\n[net]\noffline = true\n";
    fs::write(&cargo_proxy, b"rustup proxy sentinel\n").expect("write Cargo proxy");
    fs::write(&config, original).expect("write original config");
    #[cfg(unix)]
    fs::set_permissions(&config, fs::Permissions::from_mode(0o640)).expect("set original mode");
    let source = executable_source();
    install_release_and_configure(&source, &cargo_home).expect("install fixture");

    assert_eq!(
        uninstall_release_and_configure(&source, &cargo_home),
        Ok(CargoUninstallOutcome {
            binary: BinaryUninstallOutcome::Removed,
            config: CargoConfigRollbackOutcome::Restored,
        })
    );
    assert_eq!(fs::read(&config).expect("read restored config"), original);
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(&config)
            .expect("restored config metadata")
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        fs::read(&cargo_proxy).expect("read preserved Cargo proxy"),
        b"rustup proxy sentinel\n"
    );
    assert!(!bin.join(INSTALLED_BINARY_NAME).exists());
    assert!(!cargo_home.join(".antdelirium-install").exists());
}

#[test]
fn integrated_uninstall_restores_original_absence() {
    let root = fixture_root("integrated-uninstall-absence");
    let cargo_home = root.join("cargo-home");
    fs::create_dir_all(&cargo_home).expect("create Cargo home");
    let source = executable_source();
    install_release_and_configure(&source, &cargo_home).expect("install fixture");

    assert!(uninstall_release_and_configure(&source, &cargo_home).is_ok());
    assert!(!cargo_home.join("config.toml").exists());
    assert!(!cargo_home.join("bin").join(INSTALLED_BINARY_NAME).exists());
    assert!(!cargo_home.join(".antdelirium-install").exists());
}

#[test]
fn integrated_uninstall_resumes_after_config_and_binary_were_already_reverted() {
    let root = fixture_root("integrated-uninstall-resume");
    let cargo_home = root.join("cargo-home");
    fs::create_dir_all(&cargo_home).expect("create Cargo home");
    let source = executable_source();
    install_release_and_configure(&source, &cargo_home).expect("install fixture");
    assert_eq!(
        rollback_owned_workspace_wrapper(&cargo_home),
        Ok(CargoConfigRollbackOutcome::Restored)
    );
    assert_eq!(
        remove_owned_release_binary(&source, &cargo_home),
        Ok(BinaryUninstallOutcome::Removed)
    );

    assert_eq!(
        uninstall_release_and_configure(&source, &cargo_home),
        Ok(CargoUninstallOutcome {
            binary: BinaryUninstallOutcome::AlreadyAbsent,
            config: CargoConfigRollbackOutcome::AlreadyRestored,
        })
    );
    assert!(!cargo_home.join("config.toml").exists());
    assert!(!cargo_home.join(".antdelirium-install").exists());
}

#[test]
fn integrated_uninstall_refuses_changed_config_or_binary_without_deleting_them() {
    let changed_config_root = fixture_root("uninstall-changed-config");
    let changed_config_home = changed_config_root.join("cargo-home");
    fs::create_dir_all(&changed_config_home).expect("create changed config Cargo home");
    let source = executable_source();
    install_release_and_configure(&source, &changed_config_home).expect("install config fixture");
    let config = changed_config_home.join("config.toml");
    fs::write(&config, b"[net]\nretry = 42\n").expect("change installed config");
    assert_eq!(
        uninstall_release_and_configure(&source, &changed_config_home),
        Err(CargoUninstallError::Config(
            CargoConfigError::RollbackConflict
        ))
    );
    assert_eq!(
        fs::read(&config).expect("read preserved changed config"),
        b"[net]\nretry = 42\n"
    );
    assert!(changed_config_home
        .join("bin")
        .join(INSTALLED_BINARY_NAME)
        .is_file());

    let changed_binary_root = fixture_root("uninstall-changed-binary");
    let changed_binary_home = changed_binary_root.join("cargo-home");
    fs::create_dir_all(&changed_binary_home).expect("create changed binary Cargo home");
    install_release_and_configure(&source, &changed_binary_home).expect("install binary fixture");
    let installed = changed_binary_home.join("bin").join(INSTALLED_BINARY_NAME);
    fs::write(&installed, b"changed binary\n").expect("change installed binary");
    let config_before = fs::read(changed_binary_home.join("config.toml"))
        .expect("read configured bytes before refusal");
    assert_eq!(
        uninstall_release_and_configure(&source, &changed_binary_home),
        Err(CargoUninstallError::Binary(
            BinaryInstallError::ExistingInstallationConflict
        ))
    );
    assert_eq!(
        fs::read(&installed).expect("read preserved changed binary"),
        b"changed binary\n"
    );
    assert_eq!(
        fs::read(changed_binary_home.join("config.toml")).expect("read preserved configured bytes"),
        config_before
    );
}

#[test]
fn wrapper_conflict_is_preserved_and_never_activated() {
    let root = fixture_root("integrated-wrapper-conflict");
    let cargo_home = root.join("cargo-home");
    fs::create_dir_all(&cargo_home).expect("create Cargo home");
    let config = cargo_home.join("config.toml");
    let conflicting = b"[build]\nrustc-wrapper = \"sccache\"\n";
    fs::write(&config, conflicting).expect("write cache wrapper conflict");

    assert_eq!(
        install_release_and_configure(&executable_source(), &cargo_home),
        Err(CargoInstallError::Config(CargoConfigError::WrapperConflict))
    );
    assert_eq!(
        fs::read(&config).expect("read preserved cache wrapper"),
        conflicting
    );
    assert!(!cargo_home.join(".antdelirium-install").exists());
    assert!(!cargo_home.join("bin").join(INSTALLED_BINARY_NAME).exists());
}

#[test]
fn installs_exact_binary_without_touching_cargo_proxy_or_configuration() {
    let root = fixture_root("install");
    let cargo_home = root.join("cargo-home");
    let bin = cargo_home.join("bin");
    fs::create_dir_all(&bin).expect("create Cargo bin");
    let cargo_proxy = bin.join("cargo");
    let config = cargo_home.join("config.toml");
    fs::write(&cargo_proxy, b"rustup proxy sentinel\n").expect("write proxy sentinel");
    fs::write(&config, b"[net]\noffline = true\n").expect("write config sentinel");
    let proxy_before = fs::metadata(&cargo_proxy).expect("proxy metadata");
    let config_before = fs::metadata(&config).expect("config metadata");
    let source = executable_source();

    assert_eq!(
        install_release_binary(&source, &cargo_home),
        Ok(BinaryInstallOutcome::Installed)
    );
    let installed = bin.join(INSTALLED_BINARY_NAME);
    assert_eq!(
        fs::read(&installed).expect("read installed binary"),
        fs::read(&source).expect("read source binary")
    );
    assert_eq!(
        fs::read(&cargo_proxy).expect("read proxy sentinel"),
        b"rustup proxy sentinel\n"
    );
    assert_eq!(
        fs::read(&config).expect("read config sentinel"),
        b"[net]\noffline = true\n"
    );
    #[cfg(unix)]
    {
        assert_eq!(
            fs::metadata(&installed)
                .expect("installed metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        let proxy_after = fs::metadata(&cargo_proxy).expect("proxy metadata after install");
        assert_eq!(proxy_before.dev(), proxy_after.dev());
        assert_eq!(proxy_before.ino(), proxy_after.ino());
        let config_after = fs::metadata(&config).expect("config metadata after install");
        assert_eq!(config_before.dev(), config_after.dev());
        assert_eq!(config_before.ino(), config_after.ino());
    }
}

#[test]
fn identical_install_is_idempotent() {
    let root = fixture_root("idempotent");
    let cargo_home = root.join("cargo-home");
    fs::create_dir_all(&cargo_home).expect("create Cargo home");
    let source = executable_source();

    assert_eq!(
        install_release_binary(&source, &cargo_home),
        Ok(BinaryInstallOutcome::Installed)
    );
    let destination = cargo_home.join("bin").join(INSTALLED_BINARY_NAME);
    let before = fs::metadata(&destination).expect("installed metadata");
    assert_eq!(
        install_release_binary(&source, &cargo_home),
        Ok(BinaryInstallOutcome::AlreadyInstalled)
    );
    #[cfg(unix)]
    {
        let after = fs::metadata(&destination).expect("installed metadata after retry");
        assert_eq!(before.dev(), after.dev());
        assert_eq!(before.ino(), after.ino());
    }
}

#[test]
fn refuses_to_overwrite_an_unknown_existing_binary() {
    let root = fixture_root("conflict");
    let cargo_home = root.join("cargo-home");
    let bin = cargo_home.join("bin");
    fs::create_dir_all(&bin).expect("create Cargo bin");
    let destination = bin.join(INSTALLED_BINARY_NAME);
    fs::write(&destination, b"unowned installation\n").expect("write conflict");
    #[cfg(unix)]
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o755))
        .expect("make conflict executable");

    assert_eq!(
        install_release_binary(&executable_source(), &cargo_home),
        Err(BinaryInstallError::ExistingInstallationConflict)
    );
    assert_eq!(
        fs::read(destination).expect("read preserved conflict"),
        b"unowned installation\n"
    );
    assert_eq!(
        inspect_release_binary(&executable_source(), &cargo_home),
        Ok(BinaryInstallationStatus::Conflict)
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_bin_directory_and_destination() {
    use std::os::unix::fs::symlink;

    let root = fixture_root("symlink");
    let cargo_home = root.join("cargo-home");
    let external = root.join("external");
    fs::create_dir_all(&cargo_home).expect("create Cargo home");
    fs::create_dir_all(&external).expect("create external directory");
    symlink(&external, cargo_home.join("bin")).expect("symlink bin");
    assert_eq!(
        install_release_binary(&executable_source(), &cargo_home),
        Err(BinaryInstallError::UnsafeFilesystemEntry)
    );
    assert!(!external.join(INSTALLED_BINARY_NAME).exists());

    let second_home = root.join("second-cargo-home");
    let second_bin = second_home.join("bin");
    fs::create_dir_all(&second_bin).expect("create second Cargo bin");
    let external_file = root.join("external-binary");
    fs::write(&external_file, b"external sentinel\n").expect("write external file");
    symlink(&external_file, second_bin.join(INSTALLED_BINARY_NAME)).expect("symlink destination");
    assert_eq!(
        install_release_binary(&executable_source(), &second_home),
        Err(BinaryInstallError::ExistingInstallationConflict)
    );
    assert_eq!(
        fs::read(external_file).expect("read external file"),
        b"external sentinel\n"
    );
}

#[test]
fn rejects_relative_cargo_home_and_creates_one_missing_level() {
    assert_eq!(
        install_release_binary(&executable_source(), Path::new("relative-cargo-home")),
        Err(BinaryInstallError::InvalidCargoHome)
    );
    let missing = fixture_root("missing").join("missing-cargo-home");
    assert_eq!(
        inspect_release_binary(&executable_source(), &missing),
        Ok(BinaryInstallationStatus::Missing)
    );
    assert_eq!(
        install_release_binary(&executable_source(), &missing),
        Ok(BinaryInstallOutcome::Installed)
    );
    assert!(missing.join("bin").join(INSTALLED_BINARY_NAME).is_file());
}
