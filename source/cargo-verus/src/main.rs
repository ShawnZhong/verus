//
// Copyright (c) 2024 The Verus Contributors
// Copyright (c) 2014-2024 The Rust Project Developers
//
// SPDX-License-Identifier: MIT
//
// Derived, with significant modification, from:
// https://github.com/rust-lang/rust-clippy/blob/master/src/main.rs
//
use std::env;
use std::process::ExitCode;

use anyhow::Result;

mod cli;
mod metadata;
mod plan;
mod subcommands;
#[cfg(any(test, feature = "integration-tests"))]
pub mod test_utils;

use crate::plan::ExecutionPlan;

fn main() -> Result<ExitCode> {
    use ExecutionPlan::*;

    let plan = plan::plan_execution(env::args())?;

    match &plan {
        CreateNew(creation_plan) => subcommands::create_new_project(creation_plan),
        RunCargo(cargo_run_plan) => subcommands::run_cargo(cargo_run_plan),
    }
}
