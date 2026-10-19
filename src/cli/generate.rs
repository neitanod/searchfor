//! `--generate`: the man page and the shell completions of s4 (M5a plan, D16), made from s4's
//! own command line (`cli::command`) by clap_mangen and clap_complete, so they list s4's flags
//! with their help, ripgrep's and searchfor's alike. ripgrep writes its own from its flag
//! definitions; the kinds are its five.
use clap_complete::Shell;
use std::io::Write;

/// Writes what `kind` names (`man`, `complete-bash`, `complete-zsh`, `complete-fish`,
/// `complete-powershell`) to `w`.
pub fn generate(kind: &str, w: &mut dyn Write) -> std::io::Result<()> {
    let mut cmd = super::command();
    let shell = match kind {
        "man" => return clap_mangen::Man::new(cmd).render(w),
        "complete-bash" => Shell::Bash,
        "complete-zsh" => Shell::Zsh,
        "complete-fish" => Shell::Fish,
        "complete-powershell" => Shell::PowerShell,
        other => {
            return Err(std::io::Error::other(format!(
                "choice '{other}' is unrecognized"
            )));
        }
    };
    clap_complete::generate(shell, &mut cmd, "s4", w);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(kind: &str) -> String {
        let mut v = Vec::new();
        generate(kind, &mut v).unwrap();
        String::from_utf8(v).unwrap()
    }

    #[test]
    fn man_page_lists_rgs_and_s4s_flags() {
        let man = out("man");
        assert!(
            man.starts_with(".ie \\n(.g .ds Aq \\(aq") || man.contains(".TH s4"),
            "{man}"
        );
        assert!(man.contains(".TH s4"));
        assert!(man.contains("pcre2"));
        assert!(man.contains("within"));
    }

    #[test]
    fn completions_for_each_shell() {
        assert!(out("complete-bash").contains("_s4()"));
        assert!(out("complete-zsh").starts_with("#compdef s4"));
        assert!(out("complete-fish").contains("complete -c s4"));
        assert!(out("complete-powershell").contains("Register-ArgumentCompleter"));
        for kind in ["complete-bash", "complete-zsh"] {
            let text = out(kind);
            assert!(
                text.contains("--within") && text.contains("--pcre2"),
                "{kind}"
            );
        }
        let fish = out("complete-fish");
        assert!(fish.contains("-l within") && fish.contains("-l pcre2"));
    }

    #[test]
    fn an_unknown_kind_is_an_error() {
        assert!(generate("complete-tcsh", &mut Vec::new()).is_err());
    }
}
