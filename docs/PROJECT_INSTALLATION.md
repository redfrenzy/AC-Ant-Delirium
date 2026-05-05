# AC-Ant-Delirium Pre-Binary Installation

This document keeps only the information needed before the binary exists:

- how to compile the project
- where to place the generated binary
- how to configure `build.rs`
- how to configure project `.toml`
- how to reach the terminal API after the binary exists

Everything else should be discovered from the binary itself through `--installation`, `--plugin.help`, and `--api`.

## 1. Build the binary

From the source repository root:

```bash
cargo build --release --target x86_64-unknown-linux-gnu
```

The generated binary will be:

```text
target/x86_64-unknown-linux-gnu/release/ac_ant_delirium
```

If you want a project-local plugin layout, copy the binary to:

```text
plugins/<project>/ac_ant_delirium
```

## 2. Recommended project layout

```text
plugins/My-Project/
  ac_ant_delirium
  ant_delirium_config/
    default_hierarchy.toml
    default_rules.toml
my_project/
  build.rs
  src/
```

The runtime resolves configuration in this order:

1. `plugins/<project>/ant_delirium_config`
2. `ant_delirium_config/type_1`
3. `delirium.toml`
4. embedded defaults inside the engine

## 3. Generate the initial config

After the binary exists, the simplest bootstrap is:

```bash
plugins/My-Project/ac_ant_delirium config init
```

This materializes project-local templates such as:

- `plugins/My-Project/ant_delirium_config/default_hierarchy.toml`
- `plugins/My-Project/ant_delirium_config/default_rules.toml`
- `AC_Ant-Delirium_props/ant_delirium_build_guard.rs`

## 4. Configure build.rs in the consumer project

Use this in the project being audited, not inside the AC-Ant-Delirium source repository:

```rust
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ANT_DELIRIUM_BIN");
    println!("cargo:rerun-if-env-changed=ANT_DELIRIUM_CONFIG");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let project_plugin = manifest_dir.join("../plugins/My-Project");
    let plugin = env::var("ANT_DELIRIUM_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| project_plugin.join("ac_ant_delirium"));
    let config = env::var("ANT_DELIRIUM_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| project_plugin.join("ant_delirium_config"));

    println!(
        "cargo:rerun-if-changed={}",
        config.join("default_hierarchy.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        config.join("default_rules.toml").display()
    );

    let mut cmd = Command::new(&plugin);
    cmd.current_dir(&manifest_dir);
    cmd.arg("--project-root").arg(&manifest_dir);
    cmd.arg("--config").arg(&config);

    let status = cmd.status().expect("failed to run ant_delirium");
    if !status.success() {
        panic!("build blocked by ant_delirium violations");
    }
}
```

Inside the AC-Ant-Delirium source crate itself, the bundled `build.rs` stays advisory by default. Set `ANT_DELIRIUM_ENFORCE_BUILD_HOOK=1` if you want the source crate to fail hard too.

## 5. Configure the project TOML

You can either:

- keep `plugins/<project>/ant_delirium_config/default_hierarchy.toml` and `default_rules.toml` as the canonical source
- or provide `delirium.toml` for legacy/simple compatibility

The project-local plugin config is the preferred contract.

## 6. Access the terminal API after build

Once the binary exists, use the binary itself as the source of truth:

```bash
plugins/My-Project/ac_ant_delirium --installation
plugins/My-Project/ac_ant_delirium --plugin.help
plugins/My-Project/ac_ant_delirium --api
plugins/My-Project/ac_ant_delirium --api hierarchy.installation
```

Do not maintain a parallel manual API document when the binary can answer the same surface directly.
