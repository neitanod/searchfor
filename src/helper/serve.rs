//! The helper side of protocol v1 (spec §4.4): the loop a Rust helper runs. The example helper
//! (C5) and the Grammar Helper template (C8) use it. Each request is handled from scratch: a
//! handler keeps nothing between requests except what `make` built from `hello` (8458).
use super::protocol::{MatchPos, PROTOCOL, Reply, Request, WireName, line};
use crate::model::Trail;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Mutex;

/// What `hello` brought.
pub struct Hello {
    pub format: String,
    pub params: toml::Table,
    pub threads: usize,
}

pub trait Handler: Send + Sync {
    fn trails(&self, path: &Path, matches: &[MatchPos]) -> anyhow::Result<Vec<Trail>>;
    fn names(&self, path: &Path) -> anyhow::Result<Vec<WireName>>;
}

/// The helper's name and version, sent in the answer to `hello`.
#[derive(Clone, Copy)]
pub struct Info {
    pub name: &'static str,
    pub version: &'static str,
}

fn err(id: u64, e: impl std::fmt::Display) -> Reply {
    Reply {
        id,
        error: Some(e.to_string()),
        ..Default::default()
    }
}

/// Reads `hello`, builds the handler with `make`, answers it, then answers every request until
/// EOF. With `threads > 1` (concurrent mode) requests run on that many threads and each answer
/// is written when it is ready, in any order.
pub fn serve<H: Handler>(
    input: impl BufRead,
    output: impl Write + Send,
    info: Info,
    make: impl FnOnce(&Hello) -> anyhow::Result<H>,
) -> anyhow::Result<()> {
    let out = Mutex::new(output);
    let write = |r: &Reply| -> std::io::Result<()> {
        let mut o = out.lock().unwrap();
        o.write_all(line(r).as_bytes())?;
        o.flush()
    };
    let mut lines = input.lines();
    let first = lines.next().transpose()?.unwrap_or_default();
    let hello = match serde_json::from_str::<Request>(&first) {
        Ok(Request::Hello {
            protocol: PROTOCOL,
            format,
            params,
            threads,
            ..
        }) => {
            let params = match toml::Value::try_from(params)? {
                toml::Value::Table(t) => t,
                _ => toml::Table::new(),
            };
            Hello {
                format,
                params,
                threads: threads.max(1),
            }
        }
        Ok(Request::Hello { protocol, .. }) => {
            write(&err(
                0,
                format!("this helper speaks protocol {PROTOCOL}, not {protocol}"),
            ))?;
            anyhow::bail!("protocol {protocol}");
        }
        _ => {
            write(&err(0, "the first line must be hello"))?;
            anyhow::bail!("no hello");
        }
    };
    let handler = match make(&hello) {
        Ok(h) => h,
        Err(e) => {
            write(&err(0, format!("{e:#}")))?;
            return Err(e);
        }
    };
    write(&Reply {
        id: 0,
        protocol: Some(PROTOCOL),
        name: Some(info.name.into()),
        version: Some(info.version.into()),
        ..Default::default()
    })?;
    let answer = |l: &str| -> Reply {
        match serde_json::from_str::<Request>(l) {
            Ok(Request::Trails { id, path, matches }) => {
                match handler.trails(Path::new(&path), &matches) {
                    Ok(t) => Reply {
                        id,
                        trails: Some(t),
                        ..Default::default()
                    },
                    Err(e) => err(id, format!("{e:#}")),
                }
            }
            Ok(Request::Names { id, path }) => match handler.names(Path::new(&path)) {
                Ok(n) => Reply {
                    id,
                    names: Some(n),
                    ..Default::default()
                },
                Err(e) => err(id, format!("{e:#}")),
            },
            Ok(Request::Hello { id, .. }) => err(id, "hello was already received"),
            Err(e) => {
                let id = serde_json::from_str::<serde_json::Value>(l)
                    .ok()
                    .and_then(|v| v["id"].as_u64())
                    .unwrap_or(0);
                err(id, format!("not a request: {e}"))
            }
        }
    };
    if hello.threads == 1 {
        for l in lines {
            let l = l?;
            if !l.trim().is_empty() {
                write(&answer(&l))?;
            }
        }
        return Ok(());
    }
    let (tx, rx) = std::sync::mpsc::sync_channel::<String>(hello.threads);
    let rx = Mutex::new(rx);
    std::thread::scope(|s| -> anyhow::Result<()> {
        for _ in 0..hello.threads {
            s.spawn(|| {
                loop {
                    let next = rx.lock().unwrap().recv();
                    let Ok(l) = next else { break };
                    let _ = write(&answer(&l));
                }
            });
        }
        for l in lines {
            let l = l?;
            if !l.trim().is_empty() && tx.send(l).is_err() {
                break;
            }
        }
        drop(tx);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper::protocol::{MatchPos, Reply, Request, line};
    use std::time::Duration;

    struct Slow;

    impl Handler for Slow {
        /// Sleeps 200 ms for "slow" paths, so a concurrent run answers the fast one first.
        fn trails(&self, path: &Path, m: &[MatchPos]) -> anyhow::Result<Vec<crate::model::Trail>> {
            if path.to_str() == Some("slow") {
                std::thread::sleep(Duration::from_millis(200));
            }
            if path.to_str() == Some("bad") {
                anyhow::bail!("bad file");
            }
            Ok(vec![Vec::new(); m.len()])
        }
        fn names(&self, _: &Path) -> anyhow::Result<Vec<WireName>> {
            Ok(Vec::new())
        }
    }

    fn input(threads: usize, paths: &[&str]) -> Vec<u8> {
        let mut s = line(&Request::hello("x", &toml::Table::new(), threads));
        for (i, p) in paths.iter().enumerate() {
            s.push_str(&line(&Request::Trails {
                id: i as u64 + 1,
                path: (*p).into(),
                matches: vec![MatchPos { line: 1, byte: 0 }],
            }));
        }
        s.into_bytes()
    }

    fn replies(out: &[u8]) -> Vec<Reply> {
        std::str::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    const INFO: Info = Info {
        name: "test",
        version: "0",
    };

    #[test]
    fn answers_hello_then_each_request_in_order() {
        let mut out = Vec::new();
        serve(&input(1, &["slow", "fast"])[..], &mut out, INFO, |_| {
            Ok(Slow)
        })
        .unwrap();
        let r = replies(&out);
        assert_eq!(r.iter().map(|r| r.id).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(r[0].protocol, Some(1));
    }

    #[test]
    fn concurrent_answers_as_they_finish() {
        let mut out = Vec::new();
        serve(&input(2, &["slow", "fast"])[..], &mut out, INFO, |_| {
            Ok(Slow)
        })
        .unwrap();
        assert_eq!(
            replies(&out).iter().map(|r| r.id).collect::<Vec<_>>(),
            [0, 2, 1]
        );
    }

    #[test]
    fn errors_are_answers_and_the_loop_goes_on() {
        let mut out = Vec::new();
        let mut i = input(1, &["bad", "fine"]);
        i.extend_from_slice(b"not json\n");
        serve(&i[..], &mut out, INFO, |_| Ok(Slow)).unwrap();
        let r = replies(&out);
        assert_eq!(r[1].error.as_deref(), Some("bad file"));
        assert!(r[2].trails.is_some());
        assert!(r[3].error.as_deref().unwrap().starts_with("not a request"));
    }

    #[test]
    fn a_wrong_first_line_is_refused() {
        let mut out = Vec::new();
        assert!(
            serve(
                &b"{\"op\":\"names\",\"id\":1,\"path\":\"a\"}\n"[..],
                &mut out,
                INFO,
                |_| Ok(Slow)
            )
            .is_err()
        );
        assert!(replies(&out)[0].error.is_some());
    }

    #[test]
    fn hello_params_arrive_as_toml() {
        let mut params = toml::Table::new();
        params.insert("query".into(), toml::Value::String("(x) @segment".into()));
        let i = line(&Request::hello("lua", &params, 1));
        let mut out = Vec::new();
        serve(i.as_bytes(), &mut out, INFO, |h| {
            assert_eq!(h.params["query"].as_str(), Some("(x) @segment"));
            assert_eq!(h.format, "lua");
            Ok(Slow)
        })
        .unwrap();
    }
}
