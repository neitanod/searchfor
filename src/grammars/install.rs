//! `--install-grammar` and `--list-grammars` (spec §7).
use super::index::{Index, fetch, system, valid_name};
use super::{Installed, binary_name};
use sha2::Digest;
use std::path::Path;

pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Downloads `lang`'s Grammar Helper for this system, checks its SHA-256, and only then writes
/// `<dir>/<lang>/` (binary, Definition, installed.json), replacing an older install.
pub fn install(index: &Index, lang: &str, dir: &Path) -> anyhow::Result<Installed> {
    if !valid_name(lang) {
        anyhow::bail!("`{lang}` is not a Grammar name (lowercase letters, digits and _)");
    }
    let e = index.get(lang).ok_or_else(|| {
        anyhow::anyhow!("the Grammar index has no `{lang}` (see --list-grammars)")
    })?;
    // D37: the Definition must describe this Grammar and nothing else. Checked before the
    // download, which a refused index does not need.
    let def = crate::formats::definition::FormatDefinition::parse(
        &e.definition,
        crate::processors::Processors::known,
    )
    .map_err(|err| anyhow::anyhow!("the index's Definition for `{lang}` is invalid: {err}"))?;
    let grammar = def.params.get("grammar").and_then(|v| v.as_str());
    if def.name != lang || def.processor != "treesitter" || grammar != Some(lang) {
        anyhow::bail!(
            "the index's Definition for `{lang}` must have name = \"{lang}\", grammar = \"{lang}\" and processor = \"treesitter\"; nothing was installed"
        );
    }
    let sys = system();
    let b = e
        .binaries
        .get(&sys)
        .ok_or_else(|| anyhow::anyhow!("the Grammar index has no `{lang}` build for {sys}"))?;
    let bytes = fetch(&b.url)?;
    let got = hex(&sha2::Sha256::digest(&bytes));
    if !got.eq_ignore_ascii_case(&b.sha256) {
        anyhow::bail!(
            "{}: SHA-256 is {got}, the index says {}; nothing was installed",
            b.url,
            b.sha256
        );
    }
    std::fs::create_dir_all(dir)?;
    // One install of a Grammar at a time (std's File::lock, Rust ≥ 1.89). The lock file stays:
    // removing it would let two installers lock different inodes.
    let lock = std::fs::File::create(dir.join(format!(".{lang}.lock")))?;
    lock.lock()?;
    // A staging directory of its own, in the same filesystem so the renames are atomic; it is
    // deleted when it goes out of scope unless `replace` moved it into place.
    let part = tempfile::Builder::new()
        .prefix(&format!(".{lang}.part-"))
        .tempdir_in(dir)?;
    let binary = part.path().join(binary_name(lang));
    std::fs::write(&binary, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::write(part.path().join(format!("{lang}.toml")), &e.definition)?;
    let meta = serde_json::json!({"name": lang, "version": e.version, "sha256": got, "source": e.source, "license": e.license});
    std::fs::write(part.path().join("installed.json"), meta.to_string())?;
    replace(dir, lang, part.path())?;
    let _ = part.keep(); // already renamed: nothing for TempDir to delete
    drop(lock);
    let target = dir.join(lang);
    super::installed(dir)
        .into_iter()
        .find(|i| i.name == lang)
        .ok_or_else(|| anyhow::anyhow!("{}: the install is incomplete", target.display()))
}

/// Moves `staging` to `<dir>/<lang>`, keeping the previous install in a unique backup
/// directory until the rename succeeds, and putting it back when it fails (D37). Runs under
/// the Grammar's lock.
fn replace(dir: &Path, lang: &str, staging: &Path) -> anyhow::Result<()> {
    let target = dir.join(lang);
    // A unique name for the backup: created, kept and removed, so the rename can take it.
    let old = tempfile::Builder::new()
        .prefix(&format!(".{lang}.old-"))
        .tempdir_in(dir)?
        .keep();
    std::fs::remove_dir(&old)?;
    let had_old = target.exists();
    if had_old {
        std::fs::rename(&target, &old)?;
    }
    if let Err(err) = std::fs::rename(staging, &target) {
        // Put the previous install back before reporting.
        if had_old {
            let _ = std::fs::rename(&old, &target);
        }
        return Err(anyhow::anyhow!(
            "{}: {err}; the previous install was kept",
            target.display()
        ));
    }
    if had_old {
        let _ = std::fs::remove_dir_all(&old);
    }
    Ok(())
}

/// The text of `--list-grammars`: one line per Grammar, `name  state  detail`. The warning for
/// an unreadable index is the caller's, which passes `Err(reason)`.
pub fn list(compiled_in: &[&str], installed: &[Installed], index: Result<&Index, &str>) -> String {
    let mut out = String::new();
    for g in compiled_in {
        out.push_str(&format!("{g:<14} compiled in\n"));
    }
    for i in installed {
        out.push_str(&format!(
            "{:<14} installed {:<10} {}\n",
            i.name,
            i.version,
            i.dir.display()
        ));
    }
    if let Ok(index) = index {
        let sys = system();
        for e in &index.grammars {
            let known = compiled_in.contains(&e.name.as_str())
                || installed.iter().any(|i| i.name == e.name);
            if !known && e.binaries.contains_key(&sys) {
                out.push_str(&format!(
                    "{:<14} available {:<10} {}\n",
                    e.name, e.version, e.source
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::hex;
    use super::*;
    use crate::grammars::{index::Index, installed};
    use sha2::Digest;

    /// An index offering `lua` for this system, whose binary is `bin` with checksum `sha`.
    fn index(dir: &std::path::Path, bin: &[u8], sha: Option<&str>) -> Index {
        let p = dir.join("payload");
        std::fs::write(&p, bin).unwrap();
        let sha = sha
            .map(String::from)
            .unwrap_or_else(|| hex(&sha2::Sha256::digest(bin)));
        let json = serde_json::json!({"version":1,"grammars":[{"name":"lua","version":"0.2.0","source":"s","license":"MIT",
            "definition":"name = 'lua'\nextensions = ['lua']\nprocessor = 'treesitter'\n[params]\ngrammar = 'lua'\nquery = '(x) @segment'\n",
            "binaries":{ (crate::grammars::index::system()): {"url": format!("file://{}", p.display()), "sha256": sha} }}]});
        Index::parse(json.to_string().as_bytes()).unwrap()
    }

    /// The entries of `dir`, sorted.
    fn entries(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn installs_binary_definition_and_metadata() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let got = install(&index(src.path(), b"#!fake", None), "lua", dir.path()).unwrap();
        assert_eq!(std::fs::read(&got.binary).unwrap(), b"#!fake");
        assert!(
            std::fs::read_to_string(&got.definition)
                .unwrap()
                .contains("processor = 'treesitter'")
        );
        assert_eq!(installed(dir.path()), vec![got.clone()]);
        assert_eq!(got.version, "0.2.0");
        let meta: serde_json::Value =
            serde_json::from_slice(&std::fs::read(got.dir.join("installed.json")).unwrap())
                .unwrap();
        assert_eq!(meta["sha256"], hex(&sha2::Sha256::digest(b"#!fake")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&got.binary).unwrap().permissions().mode() & 0o111,
                0o111
            );
        }
    }

    #[test]
    fn a_checksum_mismatch_writes_nothing() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let e = install(
            &index(src.path(), b"#!fake", Some("deadbeef")),
            "lua",
            dir.path(),
        )
        .unwrap_err();
        assert!(e.to_string().contains("SHA-256"), "{e}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn the_checksum_ignores_case() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let upper = hex(&sha2::Sha256::digest(b"x")).to_uppercase();
        install(&index(src.path(), b"x", Some(&upper)), "lua", dir.path()).unwrap();
        assert_eq!(installed(dir.path()).len(), 1);
    }

    #[test]
    fn a_download_that_fails_writes_nothing() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut i = index(src.path(), b"x", None);
        for url in [
            format!("file://{}", src.path().join("absent").display()),
            "http://127.0.0.1:9/sfor-grammar-lua".to_string(),
        ] {
            i.grammars[0].binaries.values_mut().next().unwrap().url = url;
            assert!(install(&i, "lua", dir.path()).is_err());
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn unknown_grammars_and_systems_are_errors() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let i = index(src.path(), b"x", None);
        assert!(
            install(&i, "cobol", dir.path())
                .unwrap_err()
                .to_string()
                .contains("cobol")
        );
        let mut other = i.clone();
        other.grammars[0].binaries = [(
            "plan9-mips".to_string(),
            other.grammars[0].binaries.values().next().unwrap().clone(),
        )]
        .into();
        assert!(
            install(&other, "lua", dir.path())
                .unwrap_err()
                .to_string()
                .contains(&crate::grammars::index::system())
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn an_invalid_name_is_refused_before_anything_else() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let i = index(src.path(), b"x", None);
        for bad in ["../lua", "Lua", ""] {
            assert!(
                install(&i, bad, dir.path())
                    .unwrap_err()
                    .to_string()
                    .contains("not a Grammar name"),
                "{bad}"
            );
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn installing_again_replaces() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        install(&index(src.path(), b"one", None), "lua", dir.path()).unwrap();
        let got = install(&index(src.path(), b"two", None), "lua", dir.path()).unwrap();
        assert_eq!(std::fs::read(&got.binary).unwrap(), b"two");
        // The lock stays (removing it would let two installers lock different inodes); no
        // staging or backup directory does (review r2, Codex #4, Fable #3).
        assert_eq!(entries(dir.path()), [".lua.lock", "lua"]);
        assert_eq!(installed(dir.path()).len(), 1);
    }

    #[test]
    fn a_definition_for_another_format_is_refused() {
        // D37: the index cannot install a Definition for `php`, or an `external` command.
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        for bad in [
            "name = 'php'\nprocessor = 'treesitter'\n[params]\ngrammar = 'lua'\nquery = '(x) @segment'\n",
            "name = 'lua'\nprocessor = 'external'\n[params]\ncommand = ['sh']\n",
            "name = 'lua'\nprocessor = 'treesitter'\n[params]\ngrammar = 'php'\nquery = '(x) @segment'\n",
            "name = 'lua'\nprocessor = 'treesitter'\n",
            "not toml",
        ] {
            let mut i = index(src.path(), b"x", None);
            i.grammars[0].definition = bad.into();
            let e = install(&i, "lua", dir.path()).unwrap_err();
            assert!(e.to_string().contains("Definition"), "{e}");
            assert_eq!(
                std::fs::read_dir(dir.path())
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                    .count(),
                0
            );
        }
    }

    #[test]
    fn a_failed_reinstall_keeps_the_previous_one() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        install(&index(src.path(), b"one", None), "lua", dir.path()).unwrap();
        assert!(
            install(
                &index(src.path(), b"two", Some("deadbeef")),
                "lua",
                dir.path()
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read(installed(dir.path())[0].binary.clone()).unwrap(),
            b"one"
        );
    }

    #[test]
    fn a_failed_final_rename_restores_the_previous_install() {
        // The rename of the staging directory onto `<dir>/lua` fails; the backup goes back.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("lua");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("marker"), b"old").unwrap();
        let e = replace(dir.path(), "lua", &dir.path().join("no-such-staging")).unwrap_err();
        assert!(e.to_string().contains("previous install was kept"), "{e}");
        assert_eq!(std::fs::read(target.join("marker")).unwrap(), b"old");
        assert_eq!(entries(dir.path()), ["lua"]);
    }

    #[test]
    fn two_installs_at_once_leave_one_complete_install() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let i = index(src.path(), b"same", None);
        std::thread::scope(|s| {
            for _ in 0..2 {
                s.spawn(|| install(&i, "lua", dir.path()).unwrap());
            }
        });
        let got = installed(dir.path());
        assert_eq!(got.len(), 1);
        assert_eq!(std::fs::read(&got[0].binary).unwrap(), b"same");
        // No staging or backup directory is left behind.
        let left: Vec<_> = entries(dir.path())
            .into_iter()
            .filter(|n| n != "lua" && !n.ends_with(".lock"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn list_shows_compiled_installed_and_available() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let i = index(src.path(), b"x", None);
        let before = list(&["php", "rust"], &installed(dir.path()), Ok(&i));
        assert!(
            before.contains("php") && before.contains("compiled in"),
            "{before}"
        );
        assert!(
            before.contains("lua") && before.contains("available 0.2.0"),
            "{before}"
        );
        install(&i, "lua", dir.path()).unwrap();
        let after = list(&["php"], &installed(dir.path()), Err("offline"));
        assert!(after.contains("installed 0.2.0"), "{after}");
        assert!(!after.contains("available"), "{after}");
        // An installed Grammar is listed once even when the index still offers it.
        let again = list(&["php"], &installed(dir.path()), Ok(&i));
        assert_eq!(
            again.lines().filter(|l| l.starts_with("lua ")).count(),
            1,
            "{again}"
        );
    }

    #[test]
    fn list_leaves_out_what_this_system_cannot_install() {
        let src = tempfile::tempdir().unwrap();
        let mut i = index(src.path(), b"x", None);
        i.grammars[0].binaries.clear();
        assert!(!list(&["php"], &[], Ok(&i)).contains("lua"));
    }
}
