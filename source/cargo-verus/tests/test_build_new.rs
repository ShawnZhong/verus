use cargo_verus::{
    ExecutionPlan,
    test_utils::{MockDep, MockPackage, MockWorkspace},
};

#[test]
fn lib_with_example_imports_own_lib() {
    let package_name = "mylib";
    let args_prefix = format!(" __VERUS_DRIVER_ARGS_FOR_{package_name}-0.1.0-");

    let project_dir =
        MockPackage::new(package_name).lib().example("foo").verify(true).materialize();

    let current_dir = Some(project_dir.path());
    let args = ["cargo-verus", "build"];
    let plan = cargo_verus::plan_execution(current_dir, args).expect("plan");

    let ExecutionPlan::RunCargo(cargo_plan) = plan else {
        panic!("expected `ExecutionPlan::RunCargo`");
    };

    let driver_args = cargo_plan.parse_driver_args_for_key_prefix(&args_prefix);
    assert!(
        driver_args.contains(&"import-dep-if-present=mylib"),
        "driver args should include the package's own lib: {:?}",
        driver_args,
    );
}

#[test]
fn bin_only_no_own_lib_import() {
    let package_name = "mybin";
    let args_prefix = format!(" __VERUS_DRIVER_ARGS_FOR_{package_name}-0.1.0-");

    let project_dir = MockPackage::new(package_name).bin("main").verify(true).materialize();

    let current_dir = Some(project_dir.path());
    let args = ["cargo-verus", "build"];
    let plan = cargo_verus::plan_execution(current_dir, args).expect("plan");

    let ExecutionPlan::RunCargo(cargo_plan) = plan else {
        panic!("expected `ExecutionPlan::RunCargo`");
    };

    let driver_args = cargo_plan.parse_driver_args_for_key_prefix(&args_prefix);
    assert!(
        !driver_args.contains(&"import-dep-if-present=mybin"),
        "driver args should not import a lib for a bin-only package: {:?}",
        driver_args,
    );
}

#[test]
fn workspace_workdir() {
    let optin = "optin";
    let optout = "optout";
    let unset = "unset";
    let hasdeps = "hasdeps";

    let workspace_dir = MockWorkspace::new()
        .members([
            MockPackage::new(optin).lib().verify(true),
            MockPackage::new(optout).lib().verify(false),
            MockPackage::new(unset).lib(),
            MockPackage::new(hasdeps).lib().deps([MockDep::workspace(optin)]).verify(true),
        ])
        .materialize();

    let verify_optin_prefix = format!("__VERUS_DRIVER_VERIFY_{optin}-0.1.0-");
    let verify_optout_prefix = format!("__VERUS_DRIVER_VERIFY_{optout}-0.1.0-");
    let verify_unset_prefix = format!("__VERUS_DRIVER_VERIFY_{unset}-0.1.0-");
    let verify_hasdeps_prefix = format!("__VERUS_DRIVER_VERIFY_{hasdeps}-0.1.0-");

    let current_dir = Some(workspace_dir.path());
    let args = ["cargo-verus", "build", "--release", "--", "--expand-errors", "--rlimit=100"];
    let plan = cargo_verus::plan_execution(current_dir, args).expect("plan");

    let ExecutionPlan::RunCargo(cargo_plan) = plan else {
        panic!("expected `ExecutionPlan::RunCargo`");
    };

    assert_eq!(cargo_plan.subcommand, "build");
    assert_eq!(cargo_plan.args, ["--release"]);

    let driver_args = cargo_plan.parse_driver_args(" __VERUS_DRIVER_ARGS__");
    assert!(
        !driver_args.contains(&"--expand-errors"),
        "forwarded Verus args should not be in __VERUS_DRIVER_ARGS__"
    );
    assert!(
        !driver_args.contains(&"--rlimit=100"),
        "forwarded Verus args should not be in __VERUS_DRIVER_ARGS__"
    );

    let optin_driver_args = cargo_plan
        .parse_driver_args_for_key_prefix(&format!(" __VERUS_DRIVER_ARGS_FOR_{optin}-0.1.0-"));
    assert!(optin_driver_args.contains(&"--expand-errors"));
    assert!(optin_driver_args.contains(&"--rlimit=100"));

    let hasdeps_driver_args = cargo_plan
        .parse_driver_args_for_key_prefix(&format!(" __VERUS_DRIVER_ARGS_FOR_{hasdeps}-0.1.0-"));
    assert!(hasdeps_driver_args.contains(&"--expand-errors"));
    assert!(hasdeps_driver_args.contains(&"--rlimit=100"));

    cargo_plan.assert_env_has("RUSTC_WRAPPER");
    cargo_plan.assert_env_sets("__CARGO_DEFAULT_LIB_METADATA", "verus");
    cargo_plan.assert_env_sets("__VERUS_DRIVER_VIA_CARGO__", "1");
    cargo_plan.assert_env_sets_key_prefix(&verify_optin_prefix, "1");
    cargo_plan.assert_env_sets_key_prefix(&verify_hasdeps_prefix, "1");
    cargo_plan.assert_env_has_no_key_prefix(&verify_optout_prefix);
    cargo_plan.assert_env_has_no_key_prefix(&verify_unset_prefix);
}
