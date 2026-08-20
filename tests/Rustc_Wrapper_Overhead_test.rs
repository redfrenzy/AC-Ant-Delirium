#![cfg(unix)]

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const WRAPPER: &str = env!("CARGO_BIN_EXE_antdelirium");
const HOST_TARGET: &str = "x86_64-unknown-linux-gnu";
const CARGO_ROUNDS: usize = 3;
const RUSTC_ROUNDS: usize = 21;
const MAX_SINGLE_INVOCATION_OVERHEAD: Duration = Duration::from_millis(25);
const MAX_CLEAN_OVERHEAD: Duration = Duration::from_secs(2);
const MAX_INCREMENTAL_OVERHEAD: Duration = Duration::from_secs(1);
const MAX_NOOP_OVERHEAD: Duration = Duration::from_millis(250);
static RUN_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct ScenarioSamples {
    direct: Vec<Duration>,
    wrapped: Vec<Duration>,
}

impl ScenarioSamples {
    fn record(&mut self, direct: Duration, wrapped: Duration) {
        self.direct.push(direct);
        self.wrapped.push(wrapped);
    }

    fn medians(&self) -> (Duration, Duration, Duration) {
        let direct = median(&self.direct);
        let wrapped = median(&self.wrapped);
        (direct, wrapped, wrapped.saturating_sub(direct))
    }
}

#[test]
fn wrapper_overhead_stays_within_clean_incremental_noop_and_invocation_budgets() {
    let mut clean = ScenarioSamples::default();
    let mut incremental = ScenarioSamples::default();
    let mut noop = ScenarioSamples::default();

    for round in 0..CARGO_ROUNDS {
        let root = benchmark_workspace(round);
        let direct_target = root.join("target-direct");
        let wrapped_target = root.join("target-wrapped");

        let (direct_clean, wrapped_clean) = measure_pair(
            round % 2 == 0,
            || cargo_check(&root, &direct_target, false),
            || cargo_check(&root, &wrapped_target, true),
        );
        assert_equivalent("clean", &direct_clean.1, &wrapped_clean.1);
        clean.record(direct_clean.0, wrapped_clean.0);

        append_incremental_change(&root, round);
        let (direct_incremental, wrapped_incremental) = measure_pair(
            round % 2 != 0,
            || cargo_check(&root, &direct_target, false),
            || cargo_check(&root, &wrapped_target, true),
        );
        assert_equivalent("incremental", &direct_incremental.1, &wrapped_incremental.1);
        incremental.record(direct_incremental.0, wrapped_incremental.0);

        let (direct_noop, wrapped_noop) = measure_pair(
            round % 2 == 0,
            || cargo_check(&root, &direct_target, false),
            || cargo_check(&root, &wrapped_target, true),
        );
        assert_equivalent("no-op", &direct_noop.1, &wrapped_noop.1);
        noop.record(direct_noop.0, wrapped_noop.0);
    }

    let invocation = rustc_invocation_samples();
    let (direct_invocation, wrapped_invocation, invocation_overhead) = invocation.medians();
    let (direct_clean, wrapped_clean, clean_overhead) = clean.medians();
    let (direct_incremental, wrapped_incremental, incremental_overhead) = incremental.medians();
    let (direct_noop, wrapped_noop, noop_overhead) = noop.medians();

    eprintln!(
        "T6.7 medians_us invocation={}/{} delta={} clean={}/{} delta={} incremental={}/{} delta={} noop={}/{} delta={}",
        direct_invocation.as_micros(),
        wrapped_invocation.as_micros(),
        invocation_overhead.as_micros(),
        direct_clean.as_micros(),
        wrapped_clean.as_micros(),
        clean_overhead.as_micros(),
        direct_incremental.as_micros(),
        wrapped_incremental.as_micros(),
        incremental_overhead.as_micros(),
        direct_noop.as_micros(),
        wrapped_noop.as_micros(),
        noop_overhead.as_micros(),
    );

    assert_budget(
        "single rustc invocation",
        invocation_overhead,
        MAX_SINGLE_INVOCATION_OVERHEAD,
    );
    assert_budget("clean Cargo check", clean_overhead, MAX_CLEAN_OVERHEAD);
    assert_budget(
        "incremental Cargo check",
        incremental_overhead,
        MAX_INCREMENTAL_OVERHEAD,
    );
    assert_budget("no-op Cargo check", noop_overhead, MAX_NOOP_OVERHEAD);
}

fn measure_pair<D, W>(
    direct_first: bool,
    direct: D,
    wrapped: W,
) -> ((Duration, Output), (Duration, Output))
where
    D: FnOnce() -> (Duration, Output),
    W: FnOnce() -> (Duration, Output),
{
    if direct_first {
        (direct(), wrapped())
    } else {
        let wrapped_result = wrapped();
        let direct_result = direct();
        (direct_result, wrapped_result)
    }
}

fn cargo_check(root: &Path, target: &Path, wrapped: bool) -> (Duration, Output) {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .current_dir(root)
        .args([
            "check",
            "--quiet",
            "--locked",
            "--offline",
            "--workspace",
            "--all-targets",
            "--target",
            HOST_TARGET,
            "-j",
            "4",
        ])
        .env("CARGO_TARGET_DIR", target)
        .env_remove("RUSTC_WRAPPER")
        .env_remove("ANTDELIRIUM_WRAPPER_DEPTH")
        .env_remove("ANTDELIRIUM_LIMITED_SOCKET")
        .env_remove("ANTDELIRIUM_LIMITED_CAPABILITY_FD");
    if wrapped {
        command.env("RUSTC_WORKSPACE_WRAPPER", WRAPPER);
    } else {
        command.env_remove("RUSTC_WORKSPACE_WRAPPER");
    }
    timed_output(command, "Cargo benchmark command")
}

fn rustc_invocation_samples() -> ScenarioSamples {
    let rustc = real_rustc();
    let mut samples = ScenarioSamples::default();
    for round in 0..RUSTC_ROUNDS {
        let direct = || {
            let mut command = Command::new(&rustc);
            command.arg("--version");
            timed_output(command, "direct rustc benchmark")
        };
        let wrapped = || {
            let mut command = Command::new(WRAPPER);
            command.arg(&rustc).arg("--version");
            timed_output(command, "wrapped rustc benchmark")
        };
        let (direct_result, wrapped_result) = measure_pair(round % 2 == 0, direct, wrapped);
        assert_equivalent("rustc invocation", &direct_result.1, &wrapped_result.1);
        samples.record(direct_result.0, wrapped_result.0);
    }
    samples
}

fn real_rustc() -> PathBuf {
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .expect("query rustc sysroot");
    assert!(output.status.success());
    let sysroot = String::from_utf8(output.stdout).expect("UTF-8 rustc sysroot");
    PathBuf::from(sysroot.trim()).join("bin/rustc")
}

fn timed_output(mut command: Command, label: &str) -> (Duration, Output) {
    let started = Instant::now();
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("{label}: {error}"));
    (started.elapsed(), output)
}

fn assert_equivalent(scenario: &str, direct: &Output, wrapped: &Output) {
    assert_eq!(wrapped.status.code(), direct.status.code(), "{scenario}");
    assert_eq!(wrapped.stdout, direct.stdout, "{scenario} stdout");
    assert_eq!(wrapped.stderr, direct.stderr, "{scenario} stderr");
    assert!(direct.status.success(), "{scenario} direct command failed");
}

fn assert_budget(scenario: &str, measured: Duration, budget: Duration) {
    assert!(
        measured <= budget,
        "{scenario} overhead {:?} exceeded budget {:?}",
        measured,
        budget,
    );
}

fn append_incremental_change(root: &Path, round: usize) {
    let mut source = OpenOptions::new()
        .append(true)
        .open(root.join("support/src/lib.rs"))
        .expect("open incremental source");
    writeln!(source, "// t67 incremental round {round}").expect("write incremental source");
}

fn benchmark_workspace(round: usize) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/t66-cargo-matrix");
    let run_id = RUN_ID.fetch_add(1, Ordering::Relaxed);
    let destination = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/t67-wrapper-overhead/{}-{run_id}-{round}",
        std::process::id()
    ));
    copy_fixture_tree(&source, &destination);
    destination
}

fn copy_fixture_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create benchmark workspace");
    let mut entries = fs::read_dir(source)
        .expect("read benchmark fixture")
        .map(|entry| entry.expect("benchmark fixture entry"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type().expect("benchmark fixture file type");
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_fixture_tree(&entry.path(), &target);
        } else {
            assert!(
                file_type.is_file(),
                "benchmark fixture must not contain links"
            );
            fs::copy(entry.path(), target).expect("copy benchmark fixture file");
        }
    }
}

fn median(samples: &[Duration]) -> Duration {
    assert!(!samples.is_empty());
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}
