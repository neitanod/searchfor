//! `s4 --mcp`: an MCP server over stdin and stdout (spec §9; M5a plan, D17). It speaks
//! JSON-RPC 2.0, one message per line, as the Model Context Protocol's stdio transport, and
//! offers one tool, `search`, which runs `s4 --json` over the arguments it is given and answers
//! with its Matches: the path, the line, the Trail's names and the line's text, and with
//! `expand` the Section. Each search is a child process of the same binary, so a search that
//! fails or is slow never takes the server down. The child reads no configuration file and gets
//! only the flags the arguments ask for; what it prints is read and answered within fixed limits
//! of Matches and bytes (review r1 of the M5a plan: Codex #1, #2, #3, #6, #7; Fable #1, #2, #8),
//! saying why a Match was left out (review r2: Codex #5, #6; Fable #1, #2, #3), and its
//! messages paid for within the same limit (review r3: Codex #1, #2; Fable #1, #2).
use serde_json::{Map, Value, json};
use std::ffi::OsString;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

/// The protocol versions this server speaks, newest first; it answers a client with the
/// client's version when it is one of them, else with the newest.
pub const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// The Matches `search` returns when the call does not say.
pub const DEFAULT_MAX_RESULTS: u64 = 100;
/// The most Matches a call may ask for.
pub const MAX_RESULTS: u64 = 1000;
/// The longest result of a `tools/call`, in bytes, both copies of the Matches included. Its
/// line adds the JSON-RPC envelope: `ENVELOPE_BYTES` and the id as the client sent it, at most
/// a request's `MAX_REQUEST_BYTES` (review r2 of the M5a plan, Codex #6).
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;
/// The bytes of a response's line besides its result and its id:
/// `{"jsonrpc":"2.0","id":` `,"result":` `}`.
pub const ENVELOPE_BYTES: usize = 33;
/// The most bytes of a Match's `text`.
pub const MAX_TEXT_BYTES: usize = 2 * 1024;
/// The most bytes of a Section's `text`.
pub const MAX_SECTION_BYTES: usize = 16 * 1024;
/// The most bytes of a name of the Trail.
pub const MAX_NAME_BYTES: usize = 256;
/// The most names of a Trail a Match keeps: the innermost.
pub const MAX_TRAIL: usize = 32;
/// The longest message of `s4 --json` the server reads; a longer `match` is left out unread,
/// its file named in `errors`.
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
/// The most `match` messages too long to read whose file `errors` names; the rest are counted.
pub const MAX_LONG_LINES_NAMED: usize = 20;
/// The longest request the server reads.
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;
/// The most bytes of the search's stderr kept.
pub const MAX_STDERR_BYTES: usize = 4 * 1024;

/// The arguments of `search`, as its schema names them.
const ARGUMENTS: &[&str] = &[
    "pattern",
    "paths",
    "within",
    "expand",
    "glob",
    "case_insensitive",
    "max_results",
];

/// Serves MCP requests from stdin until it closes; `s4` is the binary each search runs.
pub fn serve(s4: &Path) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve_io(stdin.lock(), stdout.lock(), &Child(s4.to_path_buf()))
}

/// What starts a search: `s4` in a child process, or a stand-in in the tests.
pub trait Searcher {
    /// Starts `s4 ARGS`, or says why it could not.
    fn start(&self, args: &[OsString]) -> Result<Box<dyn Search + '_>, String>;
}

/// A search started.
pub trait Search {
    /// What it prints on stdout.
    fn output(&mut self) -> &mut dyn BufRead;
    /// Waits for it to end, ending it first when `stop`: how it ended, and its stderr (at most
    /// `MAX_STDERR_BYTES`, lossy).
    fn end(self: Box<Self>, stop: bool) -> (Ended, String);
}

/// How a search ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Ended {
    /// It exited with this code.
    Code(i32),
    /// The server ended it: the answer was full.
    Stopped,
    /// Anything else (a signal, a wait that failed), said in words.
    Other(String),
}

/// `serve` over any reader, writer and searcher. A request longer than `MAX_REQUEST_BYTES` is
/// read to its end and refused, with its id when the part read holds it; a line that is not
/// JSON (or not UTF-8) gets -32700; the server goes on after either.
pub fn serve_io(
    mut input: impl BufRead,
    mut output: impl Write,
    s: &dyn Searcher,
) -> anyhow::Result<()> {
    let mut line = Vec::new();
    while let Some(whole) = read_line_capped(&mut input, MAX_REQUEST_BYTES, &mut line)? {
        let answer = if !whole {
            Some(error(
                id_of_prefix(&line),
                -32600,
                &format!("a request is at most {MAX_REQUEST_BYTES} bytes"),
            ))
        } else if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        } else {
            handle(&line, s)
        };
        if let Some(answer) = answer {
            writeln!(output, "{answer}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// Reads one line of `r` into `buf`, its `\n` left out, keeping at most `cap` bytes: None at
/// the end of the input, else whether the line was whole (a longer one is read to its end and
/// the rest dropped, so it never takes more memory than `cap`).
fn read_line_capped(
    r: &mut dyn BufRead,
    cap: usize,
    buf: &mut Vec<u8>,
) -> std::io::Result<Option<bool>> {
    buf.clear();
    let mut whole = true;
    let mut any = false;
    loop {
        let chunk = r.fill_buf()?;
        if chunk.is_empty() {
            return Ok(any.then_some(whole));
        }
        any = true;
        let (take, done) = match memchr::memchr(b'\n', chunk) {
            Some(i) => (i, true),
            None => (chunk.len(), false),
        };
        let room = cap.saturating_sub(buf.len());
        whole &= take <= room;
        buf.extend_from_slice(&chunk[..take.min(room)]);
        r.consume(take + usize::from(done));
        if done {
            return Ok(Some(whole));
        }
    }
}

/// The JSON value that starts at `i` of `b` and where it ends, when `b` holds it whole.
fn value_at(b: &[u8], i: usize) -> Option<(Value, usize)> {
    let mut values = serde_json::Deserializer::from_slice(b.get(i..)?).into_iter::<Value>();
    let v = values.next()?.ok()?;
    let end = i + values.byte_offset();
    (end < b.len()).then_some((v, end))
}

/// The `id` of a request cut at `MAX_REQUEST_BYTES`, when the members of its object before the
/// cut hold it: read member by member, so an `"id"` inside `params` is never taken for it
/// (review r2 of the M5a plan, Fable #3). Null otherwise.
fn id_of_prefix(prefix: &[u8]) -> Value {
    let skip = |i: usize| {
        let rest = prefix.get(i..).unwrap_or_default();
        i + rest.iter().take_while(|c| c.is_ascii_whitespace()).count()
    };
    let mut i = skip(0);
    if prefix.get(i) != Some(&b'{') {
        return Value::Null;
    }
    i += 1;
    loop {
        let Some((Value::String(key), end)) = value_at(prefix, skip(i)) else {
            return Value::Null;
        };
        i = skip(end);
        if prefix.get(i) != Some(&b':') {
            return Value::Null;
        }
        let Some((v, end)) = value_at(prefix, i + 1) else {
            return Value::Null;
        };
        if key == "id" {
            return if v.is_string() || v.is_number() {
                v
            } else {
                Value::Null
            };
        }
        i = skip(end);
        if prefix.get(i) != Some(&b',') {
            return Value::Null;
        }
        i += 1;
    }
}

/// The answer to one message, None for a notification or a response. A message JSON-RPC 2.0
/// does not allow (not an object, a batch, no `"jsonrpc": "2.0"`, an `id` that is not a string
/// or a number, a `method` that is not a string, `params` that are not an object) gets -32600.
fn handle(line: &[u8], s: &dyn Searcher) -> Option<Value> {
    let msg: Value = match serde_json::from_slice(line) {
        Ok(v) => v,
        Err(e) => return Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
    };
    let Value::Object(obj) = msg else {
        let why = if msg.is_array() {
            "batches are not supported: send one request per line"
        } else {
            "a request is a JSON object"
        };
        return Some(error(Value::Null, -32600, why));
    };
    // A response to a request of the server's (it sends none): nothing to answer.
    if !obj.contains_key("method") && (obj.contains_key("result") || obj.contains_key("error")) {
        return None;
    }
    let id = match obj.get("id") {
        None => None,
        Some(v @ (Value::String(_) | Value::Number(_))) => Some(v.clone()),
        Some(_) => {
            return Some(error(Value::Null, -32600, "`id` is a string or a number"));
        }
    };
    let invalid = |why: &str| Some(error(id.clone().unwrap_or(Value::Null), -32600, why));
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return invalid("`jsonrpc` is \"2.0\"");
    }
    let method = match obj.get("method") {
        Some(Value::String(m)) => m.as_str(),
        Some(_) => return invalid("`method` is a string"),
        None => return invalid("a request has a `method`"),
    };
    // A notification (`notifications/initialized`, `notifications/cancelled`): nothing to
    // answer.
    let id = id?;
    let params = match obj.get("params") {
        None => Value::Object(Map::new()),
        Some(p @ Value::Object(_)) => p.clone(),
        Some(_) => return Some(error(id, -32602, "`params` is an object")),
    };
    Some(match method {
        "initialize" => result(id, initialize(&params)),
        "ping" => result(id, json!({})),
        "tools/list" => result(id, json!({ "tools": [search_tool()] })),
        "tools/call" => match call(&params, s) {
            Ok(r) => result(id, r),
            Err(e) => error(id, -32602, &e),
        },
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| VERSIONS.contains(v))
        .unwrap_or(VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "searchfor", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "searchfor (s4) searches like ripgrep and gives each Match its Trail: the \
            section, function, key, row, page or symbol it lies in. Use `search` to find text \
            and know where it sits without opening whole files; `within` keeps the Matches \
            under a Trail, `expand` returns the whole Section of each Match."
    })
}

/// The `search` tool (spec §9).
fn search_tool() -> Value {
    json!({
        "name": "search",
        "title": "Search with Trails",
        "description": "Search files like ripgrep (Rust regex syntax). Each Match comes with its \
            Trail: the headings, function, key path, table row, page or symbol it lies in. \
            Markdown, code, YAML/JSON/TOML, HTML/XML, CSV, subtitles, PDF and Office documents, \
            SQLite and object files. A line with Matches under two Trails is a Match per Trail. \
            Long texts come cut (`cut`), a Section repeated by a later Match comes once \
            (`same_as`), and `truncated` says the search found more than the answer holds, \
            `truncated_by` why: `max_results` (raise it, or narrow `paths`, `glob` or \
            `within`), `bytes` (the answer is full: narrow the search, or leave out `expand`), \
            or `long_line` (lines longer than 4 MiB, the files of the first 20 named in \
            `errors` and the rest counted: search them with s4 from a shell).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "A regular expression, as ripgrep reads it." },
                "paths": { "type": "array", "items": { "type": "string" },
                    "description": "Files or directories to search; the server's working directory when empty." },
                "within": { "type": "string",
                    "description": "Keep only the Matches whose Trail contains these Segment names, in order: \"Install › Linux\"." },
                "expand": { "type": "boolean", "description": "Return the whole Section of the last Segment of each Match." },
                "glob": { "type": "array", "items": { "type": "string" },
                    "description": "Include or exclude paths, as ripgrep's -g (\"*.md\", \"!vendor/**\")." },
                "case_insensitive": { "type": "boolean", "description": "Search case insensitively." },
                "max_results": { "type": "integer", "minimum": 1, "maximum": MAX_RESULTS,
                    "description": "The most Matches to return (100 when not given)." }
            },
            "required": ["pattern"],
            "additionalProperties": false
        },
        "outputSchema": {
            "type": "object",
            "properties": {
                "matches": { "type": "array", "items": { "type": "object" } },
                "truncated": { "type": "boolean" },
                "truncated_by": { "type": "string", "enum": ["max_results", "bytes", "long_line"] },
                "errors": { "type": "string" }
            },
            "required": ["matches", "truncated"]
        },
        "annotations": { "readOnlyHint": true, "openWorldHint": false }
    })
}

/// The command line of a `search` call and its `max_results`, every argument checked against
/// the schema: `--no-config` first (the configuration file's `--pre`, `-z` or paths never
/// reach a call), `--json`, the flags the arguments ask for with each value joined to its flag
/// (`--within=-x` is a value, `--within -x` would be a flag), and the paths after `--`.
pub fn search_args(arguments: &Value) -> Result<(Vec<OsString>, u64), String> {
    let obj = arguments.as_object().ok_or("`arguments` is an object")?;
    if let Some(key) = obj.keys().find(|k| !ARGUMENTS.contains(&k.as_str())) {
        return Err(format!("unknown argument `{key}`"));
    }
    let text = |key: &str, v: &Value| -> Result<String, String> {
        let s = v.as_str().ok_or(format!("`{key}` is a string"))?;
        if s.contains('\0') {
            return Err(format!(
                "`{key}` holds a NUL, which a command line cannot carry"
            ));
        }
        Ok(s.to_string())
    };
    let texts = |key: &str| -> Result<Vec<String>, String> {
        match obj.get(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(v)) => v
                .iter()
                .map(|x| text(key, x).map_err(|_| format!("`{key}` is an array of strings")))
                .collect(),
            Some(_) => Err(format!("`{key}` is an array of strings")),
        }
    };
    let switch = |key: &str| -> Result<bool, String> {
        match obj.get(key) {
            None => Ok(false),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(format!("`{key}` is a boolean")),
        }
    };
    let pattern = text(
        "pattern",
        obj.get("pattern").ok_or("`pattern` is required")?,
    )?;
    let mut args: Vec<OsString> = ["--no-config", "--json", "--color", "never"]
        .map(OsString::from)
        .to_vec();
    if switch("case_insensitive")? {
        args.push("-i".into());
    }
    if switch("expand")? {
        args.push("--expand".into());
    }
    if let Some(w) = obj.get("within") {
        args.push(format!("--within={}", text("within", w)?).into());
    }
    for g in texts("glob")? {
        args.push(format!("--glob={g}").into());
    }
    args.push(format!("--regexp={pattern}").into());
    args.push("--".into());
    args.extend(texts("paths")?.into_iter().map(OsString::from));
    let max = match obj.get("max_results") {
        None => DEFAULT_MAX_RESULTS,
        // JSON Schema's integer is a number without a fraction: 100.0 is 100.
        Some(v) => v
            .as_u64()
            .or_else(|| {
                v.as_f64()
                    .filter(|f| f.fract() == 0.0 && *f >= 1.0)
                    .map(|f| f as u64)
            })
            .filter(|n| (1..=MAX_RESULTS).contains(n))
            .ok_or(format!(
                "`max_results` is an integer from 1 to {MAX_RESULTS}"
            ))?,
    };
    Ok((args, max))
}

/// A Match of `s4 --json`, as the tool returns it (D17): `path`, `line`, the Trail's names, the
/// line's `text` without its terminator, and the Format.
fn compact(d: &Value) -> Value {
    let names: Vec<Value> = d["trail"]
        .as_array()
        .map(|t| t.iter().map(|s| s["name"].clone()).collect())
        .unwrap_or_default();
    let mut text = d["lines"]["text"].as_str().unwrap_or_default().to_string();
    if text.ends_with('\n') {
        text.pop();
        if text.ends_with('\r') {
            text.pop();
        }
    }
    json!({
        "path": d["path"]["text"],
        "line": d["line_number"],
        "trail": names,
        "text": text,
        "format": d["format"]
    })
}

fn call(params: &Value, s: &dyn Searcher) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or("`name` is a string")?;
    if name != "search" {
        return Err(format!("unknown tool `{name}`"));
    }
    let empty = json!({});
    let (args, max) = search_args(params.get("arguments").unwrap_or(&empty))?;
    Ok(run(s, &args, max))
}

/// Runs a search and answers with what it found (D17): at most `max` Matches (the search is
/// ended at the first Match past it: `truncated`, and `truncated_by` says so), and s4's
/// messages, if any, as `errors`. A search that exits 2 with no Match, ends with any other
/// code, or does not exit (a signal) is a tool error with what s4 printed, which the model can
/// read.
fn run(s: &dyn Searcher, args: &[OsString], max: u64) -> Value {
    let mut search = match s.start(args) {
        Ok(search) => search,
        Err(e) => return tool_error(&e),
    };
    let mut matches = Vec::new();
    let mut truncated_by = None;
    let mut stop = false;
    let mut line = Vec::new();
    // A failed read ends the loop like the end of the output; how the search ended says why.
    while let Ok(Some(whole)) = read_line_capped(search.output(), MAX_MESSAGE_BYTES, &mut line) {
        if !whole {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if v["type"] != "match" {
            continue;
        }
        if matches.len() as u64 == max {
            truncated_by = Some("max_results");
            stop = true;
            break;
        }
        matches.push(compact(&v["data"]));
    }
    let (ended, stderr) = search.end(stop);
    let messages = stderr.trim_end();
    let says = |what: String| {
        if messages.is_empty() {
            what
        } else {
            format!("{what}: {messages}")
        }
    };
    let errors = match ended {
        Ended::Stopped | Ended::Code(0 | 1) => messages.to_string(),
        Ended::Code(2) if !matches.is_empty() => {
            if messages.is_empty() {
                "s4 exited with code 2".to_string()
            } else {
                messages.to_string()
            }
        }
        Ended::Code(2) if !messages.is_empty() => return tool_error(messages),
        Ended::Code(c) => return tool_error(&says(format!("s4 ended with exit code {c}"))),
        Ended::Other(why) => return tool_error(&says(format!("s4 {why}"))),
    };
    let mut structured = json!({ "matches": matches, "truncated": truncated_by.is_some() });
    if let Some(why) = truncated_by {
        structured["truncated_by"] = why.into();
    }
    if !errors.is_empty() {
        structured["errors"] = Value::String(errors);
    }
    json!({
        "content": [{ "type": "text", "text": structured.to_string() }],
        "structuredContent": structured,
        "isError": false
    })
}

/// A tool error the model can read (a pattern that does not parse, a missing path), as MCP
/// asks, not a protocol error.
fn tool_error(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

/// `s4` in a child process.
struct Child(PathBuf);

/// The command of a search: `s4 ARGS` with stdin closed, and neither `SEARCHFOR_CONFIG_PATH`
/// nor `RIPGREP_CONFIG_PATH` in its environment (D17), besides `--no-config` in `ARGS`.
pub fn command(s4: &Path, args: &[OsString]) -> std::process::Command {
    let mut c = std::process::Command::new(s4);
    c.args(args)
        .env_remove(crate::cli::config::ENV)
        .env_remove("RIPGREP_CONFIG_PATH")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    c
}

struct Running {
    child: std::process::Child,
    out: std::io::BufReader<std::process::ChildStdout>,
    errors: std::thread::JoinHandle<String>,
}

impl Searcher for Child {
    fn start(&self, args: &[OsString]) -> Result<Box<dyn Search + '_>, String> {
        let mut child = command(&self.0, args)
            .spawn()
            .map_err(|e| format!("s4 did not start: {e}"))?;
        let mut stderr = child.stderr.take().expect("piped");
        // Read to its end so the child never blocks on it, keeping the first bytes.
        let errors = std::thread::spawn(move || {
            let mut kept = Vec::new();
            let _ = (&mut stderr)
                .take(MAX_STDERR_BYTES as u64)
                .read_to_end(&mut kept);
            let more = std::io::copy(&mut stderr, &mut std::io::sink()).unwrap_or(0) > 0;
            let mut s = String::from_utf8_lossy(&kept).into_owned();
            if more {
                s.push_str(" […]");
            }
            s
        });
        let out = std::io::BufReader::new(child.stdout.take().expect("piped"));
        Ok(Box::new(Running { child, out, errors }))
    }
}

impl Search for Running {
    fn output(&mut self) -> &mut dyn BufRead {
        &mut self.out
    }

    fn end(self: Box<Self>, stop: bool) -> (Ended, String) {
        let Running {
            mut child,
            out,
            errors,
        } = *self;
        if stop {
            let _ = child.kill();
        }
        drop(out);
        let ended = match child.wait() {
            Err(e) => Ended::Other(format!("could not be waited for: {e}")),
            Ok(status) => match status.code() {
                Some(c) => Ended::Code(c),
                None if stop => Ended::Stopped,
                None => Ended::Other(format!("ended without an exit code ({status})")),
            },
        };
        (ended, errors.join().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// A searcher that answers every search with the same output and end, and keeps the
    /// arguments and whether the server stopped it.
    struct Canned {
        out: Vec<u8>,
        ended: Ended,
        stderr: String,
        seen: RefCell<Vec<Vec<OsString>>>,
        stopped: Cell<Option<bool>>,
    }

    struct CannedSearch<'a> {
        out: &'a [u8],
        c: &'a Canned,
    }

    impl Searcher for Canned {
        fn start(&self, args: &[OsString]) -> Result<Box<dyn Search + '_>, String> {
            self.seen.borrow_mut().push(args.to_vec());
            Ok(Box::new(CannedSearch {
                out: &self.out,
                c: self,
            }))
        }
    }

    impl Search for CannedSearch<'_> {
        fn output(&mut self) -> &mut dyn BufRead {
            &mut self.out
        }
        fn end(self: Box<Self>, stop: bool) -> (Ended, String) {
            self.c.stopped.set(Some(stop));
            let ended = if stop {
                Ended::Stopped
            } else {
                self.c.ended.clone()
            };
            (ended, self.c.stderr.clone())
        }
    }

    fn guide() -> Value {
        json!({"type":"match","data":{"path":{"text":"md/guide.md"},
            "lines":{"text":"apt install needle-pkg\n"},"line_number":12,"absolute_offset":96,
            "submatches":[{"match":{"text":"needle"},"start":12,"end":18}],"format":"markdown",
            "trail":[{"kind":"heading","name":"Install","line":5,"end_line":34,"level":1},
                     {"kind":"heading","name":"Linux","line":9,"end_line":18,"level":2}],
            "fragment":null}})
    }

    fn with(messages: &[Value], ended: Ended, stderr: &str) -> Canned {
        let mut out = br#"{"type":"begin","data":{"path":{"text":"md/guide.md"}}}"#.to_vec();
        out.push(b'\n');
        for m in messages {
            out.extend_from_slice(m.to_string().as_bytes());
            out.push(b'\n');
        }
        Canned {
            out,
            ended,
            stderr: stderr.into(),
            seen: Default::default(),
            stopped: Cell::new(None),
        }
    }

    fn canned() -> Canned {
        with(&[guide(), guide()], Ended::Code(0), "")
    }

    fn session(lines: &[Value], s: &dyn Searcher) -> Vec<Value> {
        let input: String = lines.iter().map(|l| format!("{l}\n")).collect();
        raw_session(input.as_bytes(), s)
    }

    fn raw_session(input: &[u8], s: &dyn Searcher) -> Vec<Value> {
        let mut out = Vec::new();
        serve_io(input, &mut out, s).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// One `tools/call` of `search` with these arguments, and its result.
    fn call_with(arguments: Value, s: &dyn Searcher) -> Value {
        let out = session(
            &[json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":"search","arguments":arguments}})],
            s,
        );
        out[0]["result"].clone()
    }

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn initialize_list_and_call() {
        let s = canned();
        let out = session(
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
                json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"pattern":"needle","within":"Install › Linux","max_results":1}}}),
                json!({"jsonrpc":"2.0","id":4,"method":"ping"}),
            ],
            &s,
        );
        assert_eq!(out.len(), 4, "the notification gets no answer: {out:?}");
        assert_eq!(out[0]["id"], 1);
        assert_eq!(out[0]["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(out[0]["result"]["serverInfo"]["name"], "searchfor");
        assert!(out[0]["result"]["capabilities"]["tools"].is_object());
        let tools = &out[1]["result"]["tools"];
        assert_eq!(tools[0]["name"], "search");
        assert_eq!(tools[0]["inputSchema"]["required"], json!(["pattern"]));
        let r = &out[2]["result"];
        assert_eq!(r["isError"], false);
        assert_eq!(r["structuredContent"]["truncated"], true);
        assert_eq!(
            s.stopped.get(),
            Some(true),
            "the search is ended at the limit"
        );
        assert_eq!(
            r["structuredContent"]["matches"],
            json!([{"path":"md/guide.md","line":12,"trail":["Install","Linux"],
                    "text":"apt install needle-pkg","format":"markdown"}])
        );
        let text: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, r["structuredContent"]);
        assert_eq!(out[3]["result"], json!({}));
        assert_eq!(
            strings(&s.seen.borrow()[0]),
            [
                "--no-config",
                "--json",
                "--color",
                "never",
                "--within=Install › Linux",
                "--regexp=needle",
                "--"
            ]
        );
    }

    #[test]
    fn an_unknown_version_gets_the_newest() {
        let out = session(
            &[
                json!({"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"1999-01-01"}}),
            ],
            &canned(),
        );
        assert_eq!(out[0]["id"], "a");
        assert_eq!(out[0]["result"]["protocolVersion"], VERSIONS[0]);
    }

    #[test]
    fn errors_of_the_protocol_and_of_the_tool() {
        let s = with(&[], Ended::Code(2), "s4: regex parse error\n");
        let out = session(
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"grep","arguments":{}}}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"pattern":7}}}),
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search","arguments":{"pattern":"x","bogus":1}}}),
                json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"search","arguments":{"pattern":"("}}}),
            ],
            &s,
        );
        assert_eq!(out[0]["error"]["code"], -32601);
        assert_eq!(out[1]["error"]["code"], -32602);
        assert_eq!(out[2]["error"]["code"], -32602);
        assert!(
            out[3]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("bogus")
        );
        assert_eq!(out[4]["result"]["isError"], true);
        assert_eq!(
            out[4]["result"]["content"][0]["text"],
            "s4: regex parse error"
        );
        let out = raw_session(b"{not json}\n\n", &s);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["error"]["code"], -32700);
    }

    /// Codex r1 #6, Fable r1 #8: what JSON-RPC 2.0 does not allow gets -32600 (id null when
    /// the id cannot be read), a line that is not UTF-8 -32700, a request past
    /// `MAX_REQUEST_BYTES` -32600; the server answers the next request after each.
    #[test]
    fn invalid_requests_are_answered() {
        let s = canned();
        let mut input = Vec::new();
        for l in [
            json!({}),
            json!([{"jsonrpc":"2.0","id":1,"method":"ping"}]),
            json!(7),
            json!({"jsonrpc":"2.0","id":{},"method":"ping"}),
            json!({"jsonrpc":"2.0","id":null,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":2,"method":7}),
            json!({"id":3,"method":"ping"}),
            json!({"jsonrpc":"1.0","id":4,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":5}),
            json!({"jsonrpc":"2.0","id":6,"method":"ping","params":[1]}),
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"search","arguments":[]}}),
        ] {
            input.extend_from_slice(format!("{l}\n").as_bytes());
        }
        // Answered with nothing: a notification with params of any kind, and a response.
        input.extend_from_slice(
            b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/x\",\"params\":3}\n",
        );
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{}}\n");
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":\"\xff\",\"method\":\"ping\"}\n");
        input.extend_from_slice(&vec![b' '; MAX_REQUEST_BYTES + 1]);
        input.extend_from_slice(b"\n{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"ping\"}\n");
        // Longer than a request may be: answered with its id when the id comes before what
        // is cut (review r2 of the M5a plan, Fable #3), else with null.
        let long = "x".repeat(MAX_REQUEST_BYTES);
        let early = json!({"jsonrpc":"2.0","id":"r-11","method":"tools/call","params":{"name":"search","arguments":{"pattern":long}}});
        input.extend_from_slice(format!("{early}\n").as_bytes());
        input.extend_from_slice(
            format!("{{\"params\":{{\"note\":\"{long}\"}},\"id\":12,\"jsonrpc\":\"2.0\"}}\n")
                .as_bytes(),
        );
        let out = raw_session(&input, &s);
        let codes: Vec<(Value, Value)> = out
            .iter()
            .map(|a| (a["id"].clone(), a["error"]["code"].clone()))
            .collect();
        assert_eq!(
            codes,
            [
                (Value::Null, json!(-32600)),
                (Value::Null, json!(-32600)),
                (Value::Null, json!(-32600)),
                (Value::Null, json!(-32600)),
                (Value::Null, json!(-32600)),
                (json!(2), json!(-32600)),
                (json!(3), json!(-32600)),
                (json!(4), json!(-32600)),
                (json!(5), json!(-32600)),
                (json!(6), json!(-32602)),
                (json!(7), json!(-32602)),
                (Value::Null, json!(-32700)),
                (Value::Null, json!(-32600)),
                (json!(10), Value::Null),
                (json!("r-11"), json!(-32600)),
                (Value::Null, json!(-32600)),
            ]
        );
        assert!(
            out[1]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("batch")
        );
        assert_eq!(out[13]["result"], json!({}));
        assert!(s.seen.borrow().is_empty());
    }

    #[test]
    fn search_arguments_become_flags() {
        let (args, max) = search_args(&json!({
            "pattern": "-x",
            "paths": ["a", "-b"],
            "expand": true,
            "glob": ["*.md", "!vendor/**", "-g"],
            "within": "--files",
            "case_insensitive": true
        }))
        .unwrap();
        // Every value joined to its flag, the paths after `--`: nothing the client sends is
        // read as a flag of s4 (Codex r1 #1).
        assert_eq!(
            strings(&args),
            [
                "--no-config",
                "--json",
                "--color",
                "never",
                "-i",
                "--expand",
                "--within=--files",
                "--glob=*.md",
                "--glob=!vendor/**",
                "--glob=-g",
                "--regexp=-x",
                "--",
                "a",
                "-b"
            ]
        );
        assert_eq!(max, DEFAULT_MAX_RESULTS);
        let max = |v: Value| search_args(&json!({"pattern":"x","max_results":v})).map(|a| a.1);
        assert_eq!(max(json!(100.0)), Ok(100));
        assert_eq!(max(json!(MAX_RESULTS)), Ok(MAX_RESULTS));
        for bad in [
            json!(0),
            json!(-1),
            json!(2.5),
            json!(MAX_RESULTS + 1),
            json!("5"),
            Value::Null,
        ] {
            assert!(max(bad.clone()).is_err(), "{bad}");
        }
        // Each argument has its schema's type; null is not one of them.
        for bad in [
            json!({"pattern":"x","glob":"*.md"}),
            json!({"pattern":"x","paths":[1]}),
            json!({"pattern":"x","paths":null}),
            json!({"pattern":"x","expand":"true"}),
            json!({"pattern":"x","case_insensitive":1}),
            json!({"pattern":"x","within":7}),
            json!({"pattern":"x","within":null}),
            json!({"pattern":"a\u{0}b"}),
            json!({"pattern":"x","paths":["a\u{0}"]}),
            json!({"paths":["a"]}),
            json!(["x"]),
        ] {
            assert!(search_args(&bad).is_err(), "{bad}");
        }
    }

    /// Fable r1 #1, Codex r1 #1: the search reads no configuration file, so neither its
    /// `--pre`, `-z` nor paths reach a call.
    #[test]
    fn the_search_runs_without_configuration() {
        let (args, _) = search_args(&json!({"pattern":"x"})).unwrap();
        assert_eq!(args[0], "--no-config");
        assert!(!args.iter().any(|a| a == "--pre" || a == "-z"));
        let c = command(Path::new("s4"), &args);
        let envs: Vec<_> = c.get_envs().collect();
        for var in ["SEARCHFOR_CONFIG_PATH", "RIPGREP_CONFIG_PATH"] {
            assert!(
                envs.contains(&(std::ffi::OsStr::new(var), None)),
                "{var}: {envs:?}"
            );
        }
    }

    /// Codex r1 #3: every way a search ends. Exit 2 with Matches gives them and s4's errors;
    /// exit 2 without Matches, another code or a signal is a tool error with s4's message.
    #[test]
    fn how_a_search_ends() {
        let found = |ended: Ended, stderr: &str| {
            let s = with(&[guide()], ended, stderr);
            call_with(json!({"pattern":"needle"}), &s)
        };
        let r = found(
            Ended::Code(2),
            "s4: nope: No such file or directory (os error 2)\n",
        );
        assert_eq!(r["isError"], false);
        assert_eq!(
            r["structuredContent"]["matches"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            r["structuredContent"]["errors"],
            "s4: nope: No such file or directory (os error 2)"
        );
        assert_eq!(
            found(Ended::Code(2), "")["structuredContent"]["errors"],
            "s4 exited with code 2"
        );
        let r = found(Ended::Code(0), "");
        assert_eq!(
            (&r["isError"], &r["structuredContent"]["errors"]),
            (&json!(false), &Value::Null)
        );
        let r = found(Ended::Code(101), "thread 'main' panicked\n");
        assert_eq!(r["isError"], true);
        assert_eq!(
            r["content"][0]["text"],
            "s4 ended with exit code 101: thread 'main' panicked"
        );
        let r = found(
            Ended::Other("ended without an exit code (signal: 9 (SIGKILL))".into()),
            "",
        );
        assert_eq!(r["isError"], true);
        assert_eq!(
            r["content"][0]["text"],
            "s4 ended without an exit code (signal: 9 (SIGKILL))"
        );
        let s = with(&[], Ended::Code(2), "s4: regex parse error\n");
        let r = call_with(json!({"pattern":"("}), &s);
        assert_eq!(
            (&r["isError"], &r["content"][0]["text"]),
            (&json!(true), &json!("s4: regex parse error"))
        );
        assert_eq!(s.stopped.get(), Some(false));
    }
}
