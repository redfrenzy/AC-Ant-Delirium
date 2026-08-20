use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fixture_root(label: &str) -> PathBuf {
    let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/t71-installer-tests")
        .join(format!("{label}-{}-{sequence}", std::process::id()));
    fs::create_dir_all(&root).expect("create installer fixture");
    root
}

fn executable_source() -> PathBuf {
    std::env::current_exe().expect("current test executable")
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
        install_release_binary(&executable_source(), &missing),
        Ok(BinaryInstallOutcome::Installed)
    );
    assert!(missing.join("bin").join(INSTALLED_BINARY_NAME).is_file());
}
