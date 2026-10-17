//! sfor-grammar: the Grammar Helper template (spec §7, D12). One build carries one Grammar,
//! chosen by a cargo feature, and serves it over protocol v1 with searchfor's own treesitter
//! code, in concurrent mode.
use searchfor::helper::protocol::{MatchPos, WireName};
use searchfor::helper::serve::{Handler, Hello, Info, serve};
use searchfor::model::Trail;
use searchfor::processors::treesitter::{Compiled, compile, outline, parse};
use searchfor::trail::{names_of, trail_for};
use std::path::Path;

#[cfg(feature = "lua")]
fn grammar() -> (&'static str, tree_sitter::Language) {
    ("lua", tree_sitter_lua::LANGUAGE.into())
}

struct Grammar(Compiled);

impl Handler for Grammar {
    fn trails(&self, path: &Path, matches: &[MatchPos]) -> anyhow::Result<Vec<Trail>> {
        let src = std::fs::read(path).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        let tree = parse(&self.0, &src)
            .ok_or_else(|| anyhow::anyhow!("{}: not parsed", path.display()))?;
        let anchors: Vec<usize> = matches.iter().map(|m| m.byte as usize).collect();
        let (lo, hi) = (
            anchors.iter().min().copied().unwrap_or(0),
            anchors.iter().max().copied().unwrap_or(0),
        );
        let o = outline(&self.0, &tree, &src, Some(lo..hi + 1));
        Ok(anchors.iter().map(|&a| trail_for(&o, a)).collect())
        // `src`, `tree` and `o` are dropped here: nothing outlives the request (8458).
    }

    fn names(&self, path: &Path) -> anyhow::Result<Vec<WireName>> {
        let src = std::fs::read(path).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        let tree = parse(&self.0, &src)
            .ok_or_else(|| anyhow::anyhow!("{}: not parsed", path.display()))?;
        Ok(names_of(&outline(&self.0, &tree, &src, None))
            .iter()
            .map(WireName::from)
            .collect())
    }
}

fn main() {
    let (name, language) = grammar();
    let helper = format!("sfor-grammar-{name}");
    let info = Info {
        name: Box::leak(helper.into_boxed_str()),
        version: env!("CARGO_PKG_VERSION"),
    };
    let r = serve(
        std::io::stdin().lock(),
        std::io::stdout(),
        info,
        |h: &Hello| {
            let asked = h
                .params
                .get("grammar")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if asked != name {
                anyhow::bail!(
                    "this Grammar Helper carries `{name}`, the Definition asks for `{asked}`"
                );
            }
            Ok(Grammar(compile(name, &language, &h.params)?))
        },
    );
    if let Err(e) = r {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}
