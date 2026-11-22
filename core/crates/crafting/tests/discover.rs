use excubia_crafting::{App, CraftError, Tools, CONFIG_FILE};
use excubia_test_dir::TestDir;
use std::env;
use std::fs;
use std::path::Path;

fn exe(app: App) -> String {
    format!("{}{}", app.cli(), env::consts::EXE_SUFFIX)
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"").unwrap();
}

fn write_config(data: &Path, json: &str) {
    fs::create_dir_all(data).unwrap();
    fs::write(data.join(CONFIG_FILE), json).unwrap();
}

#[test]
fn the_config_path_wins_over_tools_and_path() {
    let root = TestDir::new("craft-disc-cfg");
    let data = root.join("data");
    let cfg_exe = root.join("cfg").join(exe(App::GridCraft));
    touch(&cfg_exe);
    touch(&data.join("tools/gridcraft").join(exe(App::GridCraft)));
    touch(&root.join("path").join(exe(App::GridCraft)));
    let config = format!(
        r#"{{"gridcraft_cli": {:?}}}"#,
        cfg_exe.display().to_string()
    );
    write_config(&data, &config);
    let path_var = env::join_paths([root.join("path")]).unwrap();

    let tools = Tools::discover(&data, &data.join("tools"), Some(&path_var)).unwrap();

    assert_eq!(tools.path(App::GridCraft), Some(cfg_exe.as_path()));
}

#[test]
fn a_config_path_that_does_not_exist_falls_through() {
    let root = TestDir::new("craft-disc-miss");
    let data = root.join("data");
    let tool = data.join("tools/gridcraft").join(exe(App::GridCraft));
    touch(&tool);
    let config = format!(
        r#"{{"gridcraft_cli": {:?}}}"#,
        root.join("nope").display().to_string()
    );
    write_config(&data, &config);

    let tools = Tools::discover(&data, &data.join("tools"), None).unwrap();

    assert_eq!(tools.path(App::GridCraft), Some(tool.as_path()));
}

#[test]
fn the_tools_folder_is_searched_one_level_down() {
    let root = TestDir::new("craft-disc-tools");
    let data = root.join("data");
    let direct = data.join("tools/wordcraft").join(exe(App::WordCraft));
    touch(&direct);
    let nested = data
        .join("tools/gridcraft/gridcraft-0.16.1-windows-x64-portable")
        .join(exe(App::GridCraft));
    touch(&nested);

    let tools = Tools::discover(&data, &data.join("tools"), None).unwrap();

    assert_eq!(tools.path(App::WordCraft), Some(direct.as_path()));
    assert_eq!(tools.path(App::GridCraft), Some(nested.as_path()));
}

#[test]
fn the_tools_folder_beats_the_path() {
    let root = TestDir::new("craft-disc-order");
    let data = root.join("data");
    let tool = data.join("tools/pdfcraft").join(exe(App::PdfCraft));
    touch(&tool);
    touch(&root.join("path").join(exe(App::PdfCraft)));
    let path_var = env::join_paths([root.join("path")]).unwrap();

    let tools = Tools::discover(&data, &data.join("tools"), Some(&path_var)).unwrap();

    assert_eq!(tools.path(App::PdfCraft), Some(tool.as_path()));
}

#[test]
fn the_path_is_the_last_resort() {
    let root = TestDir::new("craft-disc-path");
    let found = root.join("path").join(exe(App::WordCraft));
    touch(&found);
    let path_var = env::join_paths([root.join("empty"), root.join("path")]).unwrap();

    let tools = Tools::discover(
        &root.join("data"),
        &root.join("data").join("tools"),
        Some(&path_var),
    )
    .unwrap();

    assert_eq!(tools.path(App::WordCraft), Some(found.as_path()));
    assert!(tools.available(App::WordCraft));
}

#[test]
fn missing_apps_are_reported_unavailable_without_running_anything() {
    let root = TestDir::new("craft-disc-none");

    let tools =
        Tools::discover(&root.join("data"), &root.join("data").join("tools"), None).unwrap();

    for app in App::ALL {
        assert_eq!(tools.path(app), None);
        assert!(!tools.available(app));
    }
}

#[test]
fn a_broken_config_is_an_error_not_a_silent_default() {
    let root = TestDir::new("craft-disc-bad");
    write_config(&root.join("data"), "{no es json");

    let result = Tools::discover(&root.join("data"), &root.join("data").join("tools"), None);

    assert!(matches!(result, Err(CraftError::BadConfig)));
}

#[test]
fn the_tools_folder_the_host_names_is_the_one_searched() {
    let root = TestDir::new("craft-disc-given");
    let data = root.join("data");
    let given = root
        .join("apps")
        .join("gridcraft")
        .join(exe(App::GridCraft));
    touch(&given);
    touch(&data.join("tools/gridcraft").join(exe(App::GridCraft)));

    let tools = Tools::discover(&data, &root.join("apps"), None).unwrap();

    assert_eq!(tools.path(App::GridCraft), Some(given.as_path()));
}
