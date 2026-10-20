//! The data and configuration directories `--help` names on each system (M5b spec §4.2): the
//! ones `dirs` gives, never a Linux path elsewhere. Nothing is written to a real user directory.
use assert_cmd::Command;

#[test]
fn m5b_help_names_the_native_data_and_config_directories() {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("s4").unwrap();
    cmd.arg("--help")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("xdg-config"))
        .env("XDG_DATA_HOME", home.path().join("xdg-data"))
        .env("APPDATA", home.path().join("appdata"))
        .env("LOCALAPPDATA", home.path().join("localappdata"))
        .env_remove("SEARCHFOR_FORMATS_DIR")
        .env_remove("SEARCHFOR_GRAMMARS_DIR")
        .env_remove("SEARCHFOR_CONFIG_PATH")
        .env_remove("RIPGREP_CONFIG_PATH");
    let out = String::from_utf8(cmd.output().unwrap().stdout).unwrap();
    // Unix: the overrides above decide `dirs`. Windows resolves Known Folders
    // and ignores them; the directory is only printed there, never written.
    let (data, config) = if cfg!(windows) {
        (dirs::data_dir().unwrap(), dirs::config_dir().unwrap())
    } else if cfg!(target_os = "macos") {
        let support = home.path().join("Library").join("Application Support");
        (support.clone(), support)
    } else {
        (home.path().join("xdg-data"), home.path().join("xdg-config"))
    };
    assert!(
        out.contains(
            &data
                .join("searchfor")
                .join("grammars")
                .display()
                .to_string()
        ),
        "{out}"
    );
    assert!(
        out.contains(
            &config
                .join("searchfor")
                .join("formats")
                .display()
                .to_string()
        ),
        "{out}"
    );
}
