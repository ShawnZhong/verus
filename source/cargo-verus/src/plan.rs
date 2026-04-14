use anyhow::Result;

use crate::{
    cli::{CargoVerusCli, VerusSubcommand},
    subcommands::{self, CargoRunConfig, CargoRunPlan, NewCreationPlan},
};

pub enum ExecutionPlan {
    CreateNew(NewCreationPlan),
    RunCargo(CargoRunPlan),
}

pub fn plan_execution(args: impl Iterator<Item = String>) -> Result<ExecutionPlan> {
    let parsed_cli = CargoVerusCli::from_args(args)?;

    let cfg = match parsed_cli.command {
        VerusSubcommand::New(new_cmd) => {
            let creation_plan = match (new_cmd.bin, new_cmd.lib) {
                (Some(name), None) => NewCreationPlan { name, is_bin: true },
                (None, Some(name)) => NewCreationPlan { name, is_bin: false },
                _ => unreachable!("clap enforces exactly one of --bin/--lib"),
            };
            return Ok(ExecutionPlan::CreateNew(creation_plan));
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

    Ok(ExecutionPlan::RunCargo(cargo_run_plan))
}
