#![warn(clippy::pedantic)]

//! Command modules for the infs CLI.
//!
//! This module contains all subcommand implementations for the infs toolchain.
//!
//! ## Compilation Commands
//!
//! - [`build`] - Compile Inference source files
//! - [`version`] - Display version information
//!
//! ## Project Management Commands
//!
//! - [`new`] - Create a new Inference project
//! - [`init`] - Initialize an existing directory as an Inference project
//!
//! ## Toolchain Management Commands
//!
//! - [`install`] - Install toolchain versions
//! - [`uninstall`] - Remove toolchain versions
//! - [`list`] - List installed toolchains
//! - [`default`] - Set default toolchain version
//! - [`doctor`] - Check installation health
//! - [`self_cmd`] - Manage infs itself

pub mod build;
pub mod default;
pub mod doctor;
pub mod init;
pub mod install;
pub mod list;
pub mod new;
pub mod self_cmd;
pub mod uninstall;
pub mod version;
