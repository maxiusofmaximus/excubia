mod support;

use excubia_core_crafting::{App, CraftError, Driver};
use excubia_core_test_dir::TestDir;
use support::{args, fake_tools, ok, ScriptedRunner};

#[test]
fn version_runs_the_cli_with_the_flag_all_three_accept() {
    let root = TestDir::new("craft-ver-ok");
    let tools = fake_tools(&root, &[App::WordCraft]);
    let runner = ScriptedRunner::new(|_| Ok(ok("wordcraft-cli 0.15.2\nmás texto\n")));

    let report = Driver::new(&tools, &runner, root.join("work"))
        .version(App::WordCraft)
        .unwrap();

    assert_eq!(args(&runner.calls()[0]), ["--version"]);
    assert_eq!(report.found, "wordcraft-cli 0.15.2");
    assert_eq!(report.tested, "0.15.2");
    assert!(report.matches);
}

#[test]
fn an_untested_version_is_noted_not_blocked() {
    let root = TestDir::new("craft-ver-other");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(|_| Ok(ok("pdfcraft-cli 0.9.0\n")));

    let report = Driver::new(&tools, &runner, root.join("work"))
        .version(App::PdfCraft)
        .unwrap();

    assert_eq!(report.found, "pdfcraft-cli 0.9.0");
    assert!(!report.matches);
}

#[test]
fn a_missing_app_has_no_version() {
    let root = TestDir::new("craft-ver-none");
    let tools = fake_tools(&root, &[]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));

    let result = Driver::new(&tools, &runner, root.join("work")).version(App::GridCraft);

    assert!(matches!(
        result,
        Err(CraftError::ToolMissing(App::GridCraft))
    ));
    assert!(runner.calls().is_empty());
}
