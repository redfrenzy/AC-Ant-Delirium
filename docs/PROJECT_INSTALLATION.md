# Ant-Delirium Project Installation

This document defines the supported integration contract for projects that want Ant-Delirium to run before compilation.

## What is supported

- Strict-by-default governance before `cargo check` and `cargo build`
- Automatic config discovery from the project root
- Compact terminal output plus `shadow_context` and manifest artifacts
- Hierarchical API queries such as `ant_delirium --api hierarchy.installation`

## Recommended layout

Keep the authoritative binary and config inside the project plugin folder. In this workspace the reference pattern is:

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

The runtime auto-discovers:

1. `plugins/<project>/ant_delirium_config`
2. `ant_delirium_config/type_1`
3. `delirium.toml`
4. Embedded defaults inside `ac_ant_delirium`

## Consumer build.rs template

Use this in the project being audited, not inside the Ant-Delirium source tree:

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

    println!("cargo:rerun-if-changed={}", config.join("default_hierarchy.toml").display());
    println!("cargo:rerun-if-changed={}", config.join("default_rules.toml").display());

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

Inside the `ac_ant_delirium` source crate itself, the bundled `build.rs` keeps the hook advisory by default to avoid deadlocking local development while the engine still audits its own legacy debt. Set `ANT_DELIRIUM_ENFORCE_BUILD_HOOK=1` when you want the source crate to fail hard as well.

## Expected runtime behavior

In strict mode:

- `build.rs` runs Ant-Delirium before compilation
- violations stop the build
- the terminal shows the compact summary
- the output points to `shadow_context`
- the output points to `--api <root>.installation`

## Simplified commands

From the project root:

```bash
ant_delirium --api hierarchy.installation
ant_delirium --api hierarchy.dependency_governance.allowed_crates
```

Manual commands should target the project-local plugin, for example `../plugins/My-Project/ac_ant_delirium --config ../plugins/My-Project/ant_delirium_config`.

## Security recommendations

- Keep `build.rs` write-protected for automation when possible
- Keep external `.toml` files outside the main code tree when practical
- Keep the authoritative binary and config in the project plugin folder
- Do not let agents silently remove or bypass the build hook
- Treat `shadow_context` and manifests as the detailed audit record
