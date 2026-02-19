//! Toolchain module for the Inference compiler.
//!
//! This module provides build profile configuration that controls optimization
//! levels for different targets and compilation modes.

pub mod profile;

pub use profile::BuildProfile;
