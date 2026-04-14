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
mod subcommands;
#[cfg(any(test, feature = "integration-tests"))]
pub mod test_utils;

use crate::{
    cli::{CargoVerusCli, VerusSubcommand},
    subcommands::{CargoRunConfig, CargoRunPlan, NewCreationPlan},
};

fn main() -> Result<ExitCode> {
    use ExecutionPlan::*;

    let plan = plan_execution(env::args())?;

    match &plan {
        CreateNew(creation_plan) => subcommands::create_new_project(creation_plan),
        RunCargo(cargo_run_plan) => subcommands::run_cargo(cargo_run_plan),
    }
}

pub enum ExecutionPlan {
    CreateNew(NewCreationPlan),
    RunCargo(CargoRunPlan),
}

pub fn plan_execution(args: impl Iterator<Item = String>) -> Result<ExecutionPlan> {
    use ExecutionPlan::*;

    let parsed_cli = CargoVerusCli::from_args(args)?;

    let cfg = match parsed_cli.command {
        VerusSubcommand::New(new_cmd) => {
            let creation_plan = match (new_cmd.bin, new_cmd.lib) {
                (Some(name), None) => NewCreationPlan { name, is_bin: true },
                (None, Some(name)) => NewCreationPlan { name, is_bin: false },
                _ => unreachable!("clap enforces exactly one of --bin/--lib"),
            };
            return Ok(CreateNew(creation_plan));
        }
        VerusSubcommand::Verify(options) => CargoRunConfig {
            subcommand: "check",
            options,
            compile_primary: false,
            verify_deps: true,
            warn_if_nothing_verified: true,
        },
        VerusSubcommand::Focus(options) => CargoRunConfig {
            subcommand: "check",
            options,
            compile_primary: false,
            verify_deps: false,
            warn_if_nothing_verified: true,
        },
        VerusSubcommand::Build(options) => CargoRunConfig {
            subcommand: "build",
            options,
            compile_primary: true,
            verify_deps: true,
            warn_if_nothing_verified: false,
        },
        VerusSubcommand::Check(options) => CargoRunConfig {
            subcommand: "check",
            options,
            compile_primary: false,
            verify_deps: true,
            warn_if_nothing_verified: true,
        },
    };

    let cargo_run_plan = subcommands::plan_cargo_run(cfg)?;

    Ok(RunCargo(cargo_run_plan))
}
