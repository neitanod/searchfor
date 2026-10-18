//! Helper Processor protocol v1 (spec §4.4): the lines s4 and a helper exchange. The host
//! (`host.rs`) and the helper loop (`serve.rs`) both use these types; nothing builds JSON by hand.
use crate::model::{Name, Trail};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: u32 = 1;

/// A Match in a `trails` request: its 1-based line and its absolute byte offset in `path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchPos {
    pub line: u64,
    pub byte: u64,
}

/// One line from s4 to a helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    Hello {
        id: u64,
        protocol: u32,
        format: String,
        params: serde_json::Map<String, serde_json::Value>,
        threads: usize,
    },
    Trails {
        id: u64,
        path: String,
        matches: Vec<MatchPos>,
    },
    Names {
        id: u64,
        path: String,
    },
}

impl Request {
    /// The first line of every process (spec §4.4, 1). Id 0 is reserved for it.
    pub fn hello(format: &str, params: &toml::Table, threads: usize) -> Self {
        let params = match serde_json::to_value(params) {
            Ok(serde_json::Value::Object(m)) => m,
            _ => serde_json::Map::new(),
        };
        Request::Hello {
            id: 0,
            protocol: PROTOCOL,
            format: format.into(),
            params,
            threads,
        }
    }

    pub fn id(&self) -> u64 {
        match self {
            Request::Hello { id, .. } | Request::Trails { id, .. } | Request::Names { id, .. } => {
                *id
            }
        }
    }
}

/// One line from a helper to s4. Exactly one of `protocol` (to `hello`), `trails`, `names`
/// or `error` is expected; unknown fields are ignored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trails: Option<Vec<Trail>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<WireName>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A `names` entry on the wire: `pieces` as `[text_start, text_end, src_start, src_end]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireName {
    pub text: String,
    pub pieces: Vec<[usize; 4]>,
    pub anchor: usize,
    pub trail: Trail,
}

impl WireName {
    /// The model's Name. Labels are not on the wire: `Name::trail_at` then leaves out only the
    /// named Segment, as it documents.
    pub fn into_name(self) -> Name {
        Name {
            text: self.text,
            pieces: self
                .pieces
                .iter()
                .map(|p| (p[0]..p[1], p[2]..p[3]))
                .collect(),
            trail: self.trail,
            labels: Vec::new(),
            anchor: self.anchor,
        }
    }
}

impl From<&Name> for WireName {
    fn from(n: &Name) -> Self {
        WireName {
            text: n.text.clone(),
            pieces: n
                .pieces
                .iter()
                .map(|(t, s)| [t.start, t.end, s.start, s.end])
                .collect(),
            anchor: n.anchor,
            trail: n.trail.clone(),
        }
    }
}

/// `r` as one protocol line, `\n` included.
pub fn line<T: Serialize>(r: &T) -> String {
    let mut s = serde_json::to_string(r).expect("protocol types always serialize");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_one_json_object_per_line() {
        let r = Request::Trails {
            id: 7,
            path: "api/user.proto".into(),
            matches: vec![MatchPos {
                line: 42,
                byte: 1730,
            }],
        };
        assert_eq!(
            line(&r),
            "{\"op\":\"trails\",\"id\":7,\"path\":\"api/user.proto\",\"matches\":[{\"line\":42,\"byte\":1730}]}\n"
        );
        let mut params = toml::Table::new();
        params.insert("grammar".into(), toml::Value::String("lua".into()));
        let h = Request::hello("proto", &params, 8);
        assert_eq!(
            line(&h),
            "{\"op\":\"hello\",\"id\":0,\"protocol\":1,\"format\":\"proto\",\"params\":{\"grammar\":\"lua\"},\"threads\":8}\n"
        );
    }

    #[test]
    fn replies_parse_and_ignore_unknown_fields() {
        let r: Reply = serde_json::from_str(
            r#"{"id":7,"trails":[[{"kind":"message","name":"User","line":40,"end_line":55}]],"extra":true}"#,
        )
        .unwrap();
        assert_eq!(r.id, 7);
        let t = r.trails.unwrap();
        assert_eq!(t[0][0].name, "User");
        assert_eq!(t[0][0].level, None);
        let e: Reply = serde_json::from_str(r#"{"id":3,"error":"no such file"}"#).unwrap();
        assert_eq!(e.error.as_deref(), Some("no such file"));
        let h: Reply = serde_json::from_str(r#"{"id":0,"protocol":1,"name":"x"}"#).unwrap();
        assert_eq!(h.protocol, Some(1));
    }

    #[test]
    fn names_convert_both_ways() {
        let w: WireName = serde_json::from_str(
            r#"{"text":"User","pieces":[[0,4,1702,1706]],"anchor":1702,"trail":[]}"#,
        )
        .unwrap();
        let n = w.clone().into_name();
        assert_eq!(n.pieces, vec![(0..4, 1702..1706)]);
        assert!(n.labels.is_empty());
        assert_eq!(WireName::from(&n), w);
    }

    #[test]
    fn requests_parse_back_for_helpers() {
        let r = Request::Names {
            id: 2,
            path: "a.ini".into(),
        };
        let back: Request = serde_json::from_str(line(&r).trim_end()).unwrap();
        assert_eq!(back, r);
    }
}
