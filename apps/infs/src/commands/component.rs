//! `infs component` — manage optional managed toolchain components.
//!
//! The sole component today is `wasm-opt` (Binaryen), the optimizer the
//! `[build.wasm-opt]` manifest table drives. `infs component add wasm-opt`
//! downloads a pinned, checksum-verified Binaryen into
//! `~/.inference/tools/binaryen/<version>/`; `list` reports install state; and
//! `remove` deletes it. Managed components are a distinct install tier from
//! toolchains — they live under `tools/`, never `toolchains/`, and are resolved
//! independently of `infc` (see [`crate::commands::wasm_opt`]).

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use crate::toolchain::binaryen;
use crate::toolchain::{Platform, ToolchainPaths};

/// The components `infs component` understands. The seam for future components;
/// unknown names are rejected against this list. Every entry must have a
/// matching dispatch arm in [`add`] and [`remove`] — those arms fail loudly on
/// a listed-but-unhandled name rather than operating on the wrong component.
const KNOWN_COMPONENTS: &[&str] = &[binaryen::COMPONENT_NAME];

/// Arguments for the `component` command.
#[derive(Args)]
pub struct ComponentArgs {
    /// The component operation to perform.
    #[command(subcommand)]
    pub command: ComponentCommand,
}

/// The `component` subcommands.
#[derive(Subcommand)]
pub enum ComponentCommand {
    /// Download and install a managed component.
    Add {
        /// Component name (currently only `wasm-opt`).
        name: String,
    },
    /// List managed components and their install state.
    List,
    /// Remove an installed managed component.
    Remove {
        /// Component name (currently only `wasm-opt`).
        name: String,
    },
}

/// Executes the `component` command.
///
/// # Errors
///
/// Returns an error if the component name is unknown, or if the underlying
/// install / remove operation fails.
pub async fn execute(args: &ComponentArgs) -> Result<()> {
    match &args.command {
        ComponentCommand::Add { name } => add(name).await,
        ComponentCommand::List => list(),
        ComponentCommand::Remove { name } => remove(name),
    }
}

/// Installs a component, dispatching on the validated name so a component
/// listed in [`KNOWN_COMPONENTS`] without a handler here fails loudly instead
/// of silently installing the wrong one.
async fn add(name: &str) -> Result<()> {
    ensure_known_component(name)?;
    match name {
        binaryen::COMPONENT_NAME => add_wasm_opt().await,
        other => bail!("component '{other}' has no install handler; this is a bug in infs"),
    }
}

/// Installs the managed Binaryen, then surfaces a precedence note if a
/// non-managed `wasm-opt` would shadow the managed copy at build time.
async fn add_wasm_opt() -> Result<()> {
    let platform = Platform::detect()?;
    let paths = ToolchainPaths::new()?;

    println!(
        "Installing component '{}' (Binaryen {}) for {platform}...",
        binaryen::COMPONENT_NAME,
        binaryen::BINARYEN_PIN
    );
    binaryen::install(&paths, platform).await.with_context(|| {
        format!(
            "Failed to install component '{}'. Install Binaryen manually and put \
             `wasm-opt` on PATH, or set WASM_OPT_PATH to its full path.",
            binaryen::COMPONENT_NAME
        )
    })?;

    print_precedence_note();
    Ok(())
}

/// Lists the managed components and whether each is installed.
fn list() -> Result<()> {
    let paths = ToolchainPaths::new()?;
    let status = binaryen::status(&paths);
    if status.installed {
        let location = binaryen::installed_wasm_opt(&paths)
            .map_or_else(|| "unknown".to_string(), |p| p.display().to_string());
        println!(
            "* {:<12}(installed: Binaryen {} at {location})",
            status.name, status.version
        );
    } else {
        println!(
            "  {:<12}(not installed; run 'infs component add {}')",
            status.name, status.name
        );
    }
    Ok(())
}

/// Removes a component's managed install, dispatching on the validated name
/// with the same listed-but-unhandled guard as [`add`].
fn remove(name: &str) -> Result<()> {
    ensure_known_component(name)?;
    match name {
        binaryen::COMPONENT_NAME => remove_wasm_opt(),
        other => bail!("component '{other}' has no remove handler; this is a bug in infs"),
    }
}

/// Removes the managed Binaryen install.
fn remove_wasm_opt() -> Result<()> {
    let paths = ToolchainPaths::new()?;
    binaryen::remove(&paths)?;
    println!(
        "Removed component '{}' (Binaryen {}).",
        binaryen::COMPONENT_NAME,
        binaryen::BINARYEN_PIN
    );
    Ok(())
}

/// Prints a note when `WASM_OPT_PATH` or a PATH `wasm-opt` would take precedence
/// over the copy just installed — both resolve ahead of the managed tier at
/// build time, so without this the managed copy could silently not take effect.
fn print_precedence_note() {
    if std::env::var_os("WASM_OPT_PATH").is_some() {
        println!(
            "Note: WASM_OPT_PATH is set; it takes precedence over the managed \
             copy at build time."
        );
    } else if which::which("wasm-opt").is_ok() {
        println!(
            "Note: a `wasm-opt` on your PATH takes precedence over the managed \
             copy at build time."
        );
    }
}

/// Validates `name` against [`KNOWN_COMPONENTS`].
///
/// # Errors
///
/// Bails when `name` is not a known component, listing the known ones.
fn ensure_known_component(name: &str) -> Result<()> {
    if KNOWN_COMPONENTS.contains(&name) {
        return Ok(());
    }
    bail!(
        "Unknown component '{name}'. Known components: {}",
        KNOWN_COMPONENTS.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_known_component_accepts_wasm_opt() {
        assert!(ensure_known_component("wasm-opt").is_ok());
    }

    #[test]
    fn ensure_known_component_rejects_unknown() {
        let err = ensure_known_component("wasm-optimizer").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Unknown component 'wasm-optimizer'")
                && msg.contains("Known components: wasm-opt"),
            "an unknown component must name it and list the known ones, got: {msg}"
        );
    }

    #[test]
    fn known_components_all_have_dispatch_handlers() {
        // The add/remove dispatch handles exactly the Binaryen component today.
        // A new KNOWN_COMPONENTS entry must come with matching dispatch arms;
        // this pins the current one-to-one state so growing the list without
        // touching the dispatch is caught here, not by a wrong-component
        // install.
        assert_eq!(KNOWN_COMPONENTS, &[binaryen::COMPONENT_NAME]);
    }
}
