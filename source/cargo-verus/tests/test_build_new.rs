use cargo_verus::{ExecutionPlan, test_utils::MockPackage};

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
