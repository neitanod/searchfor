//! The run's output settings from the command line (M5a plan, D4): grep-printer's printers
//! configured as rg 14.1.1 configures them (hiargs.rs `printer_standard`, `printer_summary`,
//! `path_printer_builder`), for `--no-trail`, the summaries and `--files`; the `Look` of the
//! Trail layouts; and the `--stats` block.
use super::{Look, hyperlink::Hyperlinks, json::RunStats, replace::Replacer, style::Palette};
use crate::cli::Args;
use crate::matcher::PatternMatcher;
use grep_printer::{
    ColorSpecs, HyperlinkConfig, HyperlinkEnvironment, PathPrinter, PathPrinterBuilder, Standard,
    StandardBuilder, Summary, SummaryBuilder, SummaryKind, UserColorSpec,
};
use std::sync::Arc;
use termcolor::WriteColor;

/// rg's colours with the `--colors` specs of grep-printer's types (`trail:` is s4's own,
/// for the Trail layouts only).
pub fn color_specs(args: &Args) -> ColorSpecs {
    let mut specs = grep_printer::default_color_specs();
    specs.extend(
        args.colors
            .iter()
            .filter(|s| !s.starts_with("trail:"))
            .filter_map(|s| s.parse::<UserColorSpec>().ok()),
    );
    ColorSpecs::new(&specs)
}

/// rg's hyperlink configuration (hiargs.rs `take_hyperlink_config`): the format, the
/// hostname (`--hostname-bin`) and the WSL prefix.
pub fn hyperlink_config(args: &Args) -> HyperlinkConfig {
    let format = args
        .hyperlink_format
        .as_deref()
        .and_then(|f| f.parse().ok())
        .unwrap_or_default();
    let mut env = HyperlinkEnvironment::new();
    if args.hyperlink_format.is_some() {
        env.host(super::hyperlink::hostname(args.hostname_bin.as_deref()));
        env.wsl_prefix(super::hyperlink::wsl_prefix());
    }
    HyperlinkConfig::new(env, format)
}

/// rg's heading (hiargs.rs): asked for (`--heading`, `--tree`, `--pretty`) or on a terminal,
/// never with `--vimgrep`.
pub fn heading(args: &Args, tty: bool) -> bool {
    !args.vimgrep && (args.tree || (!args.flat && tty))
}

/// rg's `--column`: asked for, or with `--vimgrep` unless `--no-column` came last.
pub fn column(args: &Args) -> bool {
    if args.column {
        true
    } else if args.no_column {
        false
    } else {
        args.vimgrep
    }
}

/// rg's `-0`: a NUL after each path.
pub fn path_terminator(args: &Args) -> Option<u8> {
    args.null.then_some(0)
}

/// rg's standard printer for `--no-trail` (hiargs.rs `printer_standard`). `separator` is the
/// separator between files the printer writes itself, with one thread.
pub fn standard<W: WriteColor>(
    args: &Args,
    heading: bool,
    with_filename: bool,
    max_count: Option<u64>,
    separator: Option<Vec<u8>>,
    w: W,
) -> Standard<W> {
    StandardBuilder::new()
        .byte_offset(args.byte_offset)
        .color_specs(color_specs(args))
        .column(column(args))
        .heading(heading)
        .hyperlink(hyperlink_config(args))
        .max_columns_preview(args.max_columns_preview)
        .max_columns(args.max_columns)
        .max_matches(max_count)
        .only_matching(args.only_matching)
        .path(with_filename)
        .path_terminator(path_terminator(args))
        .per_match_one_line(true)
        .per_match(args.vimgrep)
        .replacement(args.replace.clone().map(String::into_bytes))
        .separator_context(args.context_separator.clone())
        .separator_field_context(args.field_context_separator.0.clone())
        .separator_field_match(args.field_match_separator.0.clone())
        .separator_path(args.path_separator.and_then(|s| s.0))
        .stats(args.stats)
        .trim_ascii(args.trim)
        .separator_search(separator)
        .build(w)
}

/// rg's summary printer (hiargs.rs `printer_summary`).
pub fn summary<W: WriteColor>(
    args: &Args,
    kind: SummaryKind,
    with_filename: bool,
    max_count: Option<u64>,
    w: W,
) -> Summary<W> {
    SummaryBuilder::new()
        .color_specs(color_specs(args))
        .exclude_zero(!args.include_zero)
        .hyperlink(hyperlink_config(args))
        .kind(kind)
        .max_matches(max_count)
        .path(with_filename)
        .path_terminator(path_terminator(args))
        .separator_field(b":".to_vec())
        .separator_path(args.path_separator.and_then(|s| s.0))
        .stats(args.stats)
        .build(w)
}

/// rg's printer of `--files` (hiargs.rs `path_printer_builder`).
pub fn path_printer<W: WriteColor>(args: &Args, w: W) -> PathPrinter<W> {
    PathPrinterBuilder::new()
        .color_specs(color_specs(args))
        .hyperlink(hyperlink_config(args))
        .separator(args.path_separator.and_then(|s| s.0))
        .terminator(path_terminator(args).unwrap_or(b'\n'))
        .build(w)
}

/// How the Trail layouts print rg's output flags (M5a plan, D4). A `-U` search whose pattern
/// cannot match a line terminator prints as a search of lines, as grep-printer's printers do
/// (`multi_line_with_matcher`): `-o` then prints an empty Match as an empty row (review r2 of
/// the M5a plan, Codex #4).
pub fn look(args: &Args, matcher: &PatternMatcher) -> Look {
    let multi_line = crate::search::searcher(args, false, true).multi_line_with_matcher(matcher);
    Look {
        column: column(args),
        byte_offset: args.byte_offset,
        only_matching: args.only_matching,
        per_match: args.vimgrep,
        trim: args.trim,
        max_columns_preview: args.max_columns_preview,
        replace: args.replace.as_ref().map(|r| {
            Arc::new(Replacer::new(
                matcher.clone(),
                r.clone().into_bytes(),
                crate::search::line_terminator(args),
                multi_line,
            ))
        }),
        sep_match: args.field_match_separator.0.clone(),
        sep_context: args.field_context_separator.0.clone(),
        sep_group: args.context_separator.clone(),
        path_terminator: path_terminator(args),
        path_separator: args.path_separator.and_then(|s| s.0),
        palette: Palette::new(&args.colors),
        hyperlinks: args
            .hyperlink_format
            .as_deref()
            .and_then(|f| Hyperlinks::new(f, args.hostname_bin.as_deref()))
            .map(Arc::new),
        crlf: args.crlf,
        null_data: args.null_data,
        multi_line,
    }
}

/// Whether the rows of a file can be printed as the searcher reports them (the streaming
/// printer, `output::stream`): not when a flag makes rows the searcher does not report (`-o`,
/// `--vimgrep`, `-r`, `-U`, `--null-data`, `--crlf`'s terminator), nor with `--passthru`,
/// which prints a file without a Match. `--stats` counts every file's search as it streams
/// (code review r1, Codex #1).
pub fn streams(look: &Look, args: &Args) -> bool {
    !(look.only_matching
        || look.per_match
        || look.replace.is_some()
        || look.multi_line
        || look.null_data
        || look.crlf
        || args.passthru)
}

/// rg 14.1.1's `--stats` block (main.rs `print_stats`), after the output.
pub fn stats_block(s: &RunStats) -> String {
    format!(
        "\n{} matches\n{} matched lines\n{} files contained matches\n{} files searched\n\
         {} bytes printed\n{} bytes searched\n{:0.6} seconds spent searching\n{:0.6} seconds\n",
        s.matches,
        s.matched_lines,
        s.searches_with_match,
        s.searches,
        s.bytes_printed,
        s.bytes_searched,
        s.elapsed.as_secs_f64(),
        s.elapsed_total.as_secs_f64(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Args {
        let mut argv = vec!["s4"];
        argv.extend_from_slice(v);
        argv.push("x");
        crate::cli::parse(argv.iter().map(Into::into).collect()).unwrap()
    }

    #[test]
    fn heading_and_column_as_ripgrep() {
        assert!(heading(&args(&[]), true));
        assert!(!heading(&args(&[]), false));
        assert!(heading(&args(&["--heading"]), false));
        assert!(!heading(&args(&["--no-heading"]), true));
        assert!(!heading(&args(&["--heading", "--vimgrep"]), true));
        assert!(heading(&args(&["-p"]), false));
        assert!(column(&args(&["--vimgrep"])));
        assert!(!column(&args(&["--vimgrep", "--no-column"])));
        assert!(column(&args(&["--column"])));
    }

    #[test]
    fn the_look_of_the_command_line() {
        let a = args(&[
            "-o",
            "--trim",
            "--field-match-separator",
            "=",
            "--no-context-separator",
            "-0",
            "--path-separator",
            r"\\",
            "-U",
        ]);
        // `-U` prints as several lines only when the pattern can match a line break (review r2
        // of the plan, Codex #4).
        let spans = crate::matcher::build(&a, &["x\\nx".into()]).unwrap();
        assert!(look(&a, &spans).multi_line);
        let m = crate::matcher::build(&a, &["x".into()]).unwrap();
        let l = look(&a, &m);
        assert!(l.only_matching && l.trim && !l.multi_line);
        assert_eq!(l.sep_match, b"=");
        assert_eq!(l.sep_group, None);
        assert_eq!(l.path_terminator, Some(0));
        assert_eq!(l.path_separator, Some(b'\\'));
        assert!(!streams(&l, &a));
        assert!(streams(
            &look(&args(&["--column"]), &m),
            &args(&["--column"])
        ));
    }

    #[test]
    fn stats_block_as_ripgrep_prints_it() {
        let s = RunStats {
            elapsed_total: std::time::Duration::from_micros(3961),
            elapsed: std::time::Duration::from_micros(57),
            searches: 2,
            searches_with_match: 2,
            bytes_searched: 152,
            bytes_printed: 106,
            matched_lines: 4,
            matches: 5,
        };
        assert_eq!(
            stats_block(&s),
            "\n5 matches\n4 matched lines\n2 files contained matches\n2 files searched\n\
             106 bytes printed\n152 bytes searched\n0.000057 seconds spent searching\n\
             0.003961 seconds\n"
        );
    }
}
