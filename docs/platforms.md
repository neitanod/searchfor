# Platforms

searchfor is built and tested natively on four systems (M5b spec §4.1 and §5.1). This page is
the inventory of acceptance criterion 34: every test that does not run on a system, and why.
A test is skipped only when the behaviour it checks does not exist on that system; a test that
only uses Unix tools (`sh`, `printf`) or `/`-paths is rewritten to run everywhere.

| System name | CI runner | Target | Reference rg archive |
|---|---|---|---|
| `linux` | `ubuntu-24.04` | `x86_64-unknown-linux-gnu` | `ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz` |
| `macos-arm64` | `macos-15` | `aarch64-apple-darwin` | `ripgrep-14.1.1-aarch64-apple-darwin.tar.gz` |
| `macos-x86_64` | `macos-15-intel` | `x86_64-apple-darwin` | `ripgrep-14.1.1-x86_64-apple-darwin.tar.gz` |
| `windows` | `windows-2022` | `x86_64-pc-windows-msvc` | `ripgrep-14.1.1-x86_64-pc-windows-msvc.zip` |

The CI job of each system (`.github/workflows/ci.yml`) downloads the reference rg 14.1.1 for
that system, checks its SHA-256 against the value written in the workflow, sets
`SEARCHFOR_RG`, and runs the whole suite with `SEARCHFOR_REQUIRE_REFERENCE=1`, so a comparison
whose reference is missing fails instead of passing. It then prints how many tests it listed,
how many were ignored and why, and how many this page excludes by `cfg`; a row of the first
table that disagrees with that system's `cargo test -- --list` fails the job.

## What each system lacks

- **Linux only.** GNU time (`/usr/bin/time -v`, the peak RSS of `tests/perf.rs`), strace, and
  `/proc`, the Linux way of observing processes (their memory maps, RSS, open descriptors and
  children). `/proc` is never the reason a macOS test is absent: the Ctrl-C tests h16, h17
  and h18 get a macOS variant (M5b plan, Task 2.3) that observes processes another way.
- **Unix only (Linux and macOS).** POSIX signals and process groups, FIFOs in the file system,
  descriptors named as paths (`/dev/stdin`, `/dev/fd`), POSIX permissions and the POSIX shell
  tools; file names of arbitrary bytes on Linux (APFS on macOS also requires UTF-8 names).
- **Windows.** Its equivalents are tested natively instead: a console control event
  (`GenerateConsoleCtrlEvent`) for Ctrl-C, Job Objects for a reader's memory and for the
  children that must die with s4, and drive and `\` paths as rg 14.1.1 prints them.
- **macOS arm64.** PCRE2's JIT is unavailable on `aarch64-apple-darwin` (pcre2-sys disables
  it): `-P` works, slower, and `--pcre2-version` says the JIT is unavailable, as that system's
  rg. No test is skipped for it.

## Tests excluded by `cfg`

Counted from this table; each chunk that changes one of these tests sends its row before
merge. The owner is the chunk of the M5b plan that owns the file.

| Test | File | Skipped on | Missing feature | Owner |
|---|---|---|---|---|
| `a_worker_reaped_before_its_pipes_are_taken_is_an_error` | `src/docs/isolated.rs` | windows | runs `true` and `sh`, POSIX tools | C5 |
| `a_dead_workers_stderr_is_read_to_its_end_before_why` | `src/docs/isolated.rs` | windows | runs `sh`, a POSIX shell | C5 |
| `a_non_utf8_file_name_still_reaches_magic_and_shebang` | `src/formats/detector.rs` | windows | file names of arbitrary bytes (Windows names are UTF-16) | C3 |
| `the_system_key_is_os_dash_arch` | `src/grammars/index.rs` | macos-arm64, macos-x86_64, windows | one variant per system: this one checks the Linux x86-64 key | C8 |
| `the_system_key_on_apple_silicon` | `src/grammars/index.rs` | linux, macos-x86_64, windows | one variant per system: this one checks the macOS arm64 key | C8 |
| `a_reader_retired_while_the_run_is_cancelled_is_reaped_once` | `src/helper/host.rs` | windows | starts `sleep`, a POSIX tool | C2 |
| `pre_runs_the_command_with_the_path_and_the_file_on_stdin` | `src/preprocess.rs` | windows | a `--pre` script for `sh`, made executable with POSIX permissions | C8 |
| `a_failing_pre_command_names_itself_and_its_stderr` | `src/preprocess.rs` | windows | a `--pre` script for `sh`, made executable with POSIX permissions | C8 |
| `closing_a_command_read_in_part_gives_its_failure` | `src/preprocess.rs` | windows | a `--pre` script for `sh`, made executable with POSIX permissions | C8 |
| `the_real_ls_binary_parses` | `src/processors/object/mod.rs` | macos-arm64, macos-x86_64, windows | the system's own glibc ELF, `/bin/ls` | C10 |
| `t34_a_path_that_cannot_be_read_twice_is_read_as_stdin` | `tests/cli.rs` | windows | `/dev/stdin` (`-` is tested everywhere) | C4 |
| `t35_max_count_on_an_open_fifo_stops_at_the_limit` | `tests/cli.rs` | windows | FIFOs in the file system (`mkfifo`) | C4 |
| `m4_12_docs_reads_streams_once` | `tests/cli_m4.rs` | windows | FIFOs, `/dev/stdin`, `/dev/fd/0` | C5 |
| `m4_24_stdin_redirected_from_a_document_is_no_document` | `tests/cli_m4.rs` | windows | descriptors named as paths (`/dev/fd`, `/proc/self/fd`) | C5 |
| `m4_27_only_names_through_descriptors_are_no_documents` | `tests/cli_m4.rs` | macos-arm64, macos-x86_64, windows | `/proc` and `/dev/shm` | C5 |
| `m4_18_any_path_reaches_the_reader` | `tests/cli_m4.rs` | macos-arm64, macos-x86_64, windows | file names that are not UTF-8 (APFS and Windows refuse them) | C5 |
| `m4_19_a_signal_to_sfor_ends_its_readers` | `tests/cli_m4.rs` | macos-arm64, macos-x86_64, windows | POSIX signals and process groups; `/proc` to find the readers | C5 |
| `m4_26_a_signal_ignored_by_its_parent_is_ignored` | `tests/cli_m4.rs` | macos-arm64, macos-x86_64, windows | POSIX signals ignored by the parent; `/proc` to find the readers | C5 |
| `m4_28_a_signal_blocked_by_its_parent_is_no_stop` | `tests/cli_m4.rs` | macos-arm64, macos-x86_64, windows | POSIX signals blocked by the parent; `/proc` | C5 |
| `m4_21_a_document_past_the_readers_limit_ends_its_reader` | `tests/cli_m4.rs` | macos-arm64, macos-x86_64, windows | a FIFO that holds a reader, and `/proc` | C5 |
| `m4_25_a_command_that_fails_late_is_an_error_after_its_lines` | `tests/cli_m4.rs` | windows | a `--pre` script for `sh`, POSIX permissions | C5 |
| `m4_34_a_pipe_searches_documents_as_a_terminal_does` | `tests/cli_m4.rs` | macos-arm64, macos-x86_64, windows | a Linux PTY (`posix_openpt`, `ptsname_r`, `ioctl`) | C5 |
| `m4_35_with_follow_names_below_links_to_descriptors_are_no_documents` | `tests/cli_m4.rs` | macos-arm64, macos-x86_64, windows | links to `/dev/fd` and `/proc` | C5 |
| `m5_22_the_matches_of_a_line_or_a_file_cost_linear_time` | `tests/cli_m5.rs` | macos-arm64, macos-x86_64, windows | `wait4` CPU time of the child | C1 |
| `m5_19_no_mmap_maps_no_file` | `tests/cli_m5.rs` | macos-arm64, macos-x86_64, windows | `/proc/PID/maps` | C1 |
| `m5_21_mcp_memory_of_a_line_with_many_trails` | `tests/cli_m5.rs` | macos-arm64, macos-x86_64, windows | RSS from `/proc/PID/status` | C1 |
| `h16_ctrl_c_ends_every_helper` | `tests/helpers.rs` | windows | POSIX signals and a FIFO (the console Ctrl-C has its own test) | C2 |
| `h18_streams_named_on_the_command_line_go_through_a_temporary_file` | `tests/helpers.rs` | windows | `/dev` streams, a FIFO and a symlink to it | C2 |
| `a_signal_to_the_server_ends_its_search` | `tests/mcp_m5.rs` | macos-arm64, macos-x86_64, windows | POSIX signals, FIFOs and `/proc` (the whole file) | C6 |
| `a_search_dies_with_its_server` | `tests/mcp_m5.rs` | macos-arm64, macos-x86_64, windows | POSIX signals, FIFOs and `/proc` (the whole file) | C6 |
| `a_client_that_goes_away_ends_the_search` | `tests/mcp_m5.rs` | macos-arm64, macos-x86_64, windows | POSIX signals, FIFOs and `/proc` (the whole file) | C6 |
| `files_without_an_extension_cost_by_default_what_they_cost_without_documents` | `tests/perf.rs` | windows | GNU time and `getrusage` | C10 |
| `one_file_that_is_no_document_costs_what_it_costs_without_documents` | `tests/perf.rs` | windows | GNU time and `getrusage` | C10 |
| `an_installed_grammar_leaves_a_search_without_documents_unloaded` | `tests/perf.rs` | windows | strace (Linux); on macOS it passes without strace, checking nothing | C10 |
| `null_data_records_and_their_sections_cost_linear_time` | `tests/perf.rs` | windows | `getrusage` CPU time | C10 |
| `a_root_linked_to_a_directory_is_walked_as_ripgrep` | `tests/rg_compat.rs` | windows | symbolic links, which need a privilege on Windows | C3 |
| `memory_stays_flat_over_five_thousand_requests` | `helpers/example/tests/protocol.rs` | macos-arm64, macos-x86_64, windows | RSS from `/proc/PID/status` | C2 |
| `memory_stays_flat_over_five_thousand_requests` | `helpers/grammar/tests/protocol.rs` | macos-arm64, macos-x86_64, windows | RSS from `/proc/PID/status` | C8 |

## Tests ignored on a system

Counted from each system's test output (`cargo test -- --list --ignored`); the CI subtracts
the two sections below and prints what remains.

| Test | File | Skipped on | Missing feature | Owner |
|---|---|---|---|---|
| `inverted_search_on_a_big_plain_file_stays_under_the_ceilings` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `every_line_matching_in_a_big_plain_file_stays_under_the_ceilings` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `inverted_search_on_a_big_markdown_file_stays_under_the_ceilings` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `tree_output_of_a_big_markdown_file_stays_under_the_ceilings` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `walked_binary_files_are_not_read_before_the_search` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `an_explicit_markdown_file_without_matches_skips_the_processor` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `headings_inside_items_stay_under_the_ceiling` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `json_of_a_big_plain_file_stays_under_the_ceilings` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `json_of_a_big_markdown_file_stays_under_the_ceilings` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `json_without_trails_of_a_big_file_is_as_fast_as_ripgrep` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `the_binary_probe_of_in_headings_keeps_no_lines` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_walked_plain_file_is_not_read_whole_with_in_headings` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `keypath_json_scans_at_fifty_megabytes_a_second` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `treesitter_skips_files_over_max_bytes` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `broken_toml_recovers_in_bounded_time` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `yaml_beyond_the_tree_sitter_limit` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `many_arrays_of_tables_stay_linear` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `markup_many_distinct_sibling_tags_stay_linear` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `markup_keeps_no_index_of_the_lines` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `markup_deep_nesting_stays_linear` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `markup_end_tags_with_nothing_open_stay_linear` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `markup_implied_ends_at_one_byte_stay_linear` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `markup_html_walks_at_fifty_megabytes_a_second` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `markup_xml_walks_at_fifty_megabytes_a_second` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `csv_reads_at_fifty_megabytes_a_second` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `subtitles_read_at_fifty_megabytes_a_second` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `tsv_reads_at_fifty_megabytes_a_second` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `vtt_reads_at_fifty_megabytes_a_second` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_huge_header_cell_costs_no_memory_per_escape` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_million_nested_elements_cost_little_each` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_huge_name_costs_no_memory_per_piece` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_header_of_a_million_columns_names_only_the_columns_matched` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `the_rows_of_one_wide_line_cost_the_line_once` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `object_rows_of_a_big_elf_stay_under_rg_a` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `an_elf_with_two_hundred_thousand_symbols_stays_under_rg_a` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `documents_extract_within_their_ceilings` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_zip_bomb_is_refused_within_its_limit` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_pdf_bomb_is_refused_within_its_limit` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_zip_bomb_of_many_parts_is_refused_within_its_limit` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_pdf_page_of_millions_of_operations_is_refused_within_its_limit` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_pdf_object_stream_bomb_is_refused_within_its_limit` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_pdf_map_range_of_every_code_is_read_within_its_limit` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_pdf_map_of_long_text_is_read_within_its_limit` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `a_pdf_past_the_readers_memory_is_refused_within_its_limit` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `pcre2_and_multiline_without_trails_are_as_fast_as_ripgrep` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `files_lists_as_fast_as_ripgrep` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `output_flags_in_the_trail_layouts_stay_under_the_ceilings` | `tests/perf.rs` | macos-arm64, macos-x86_64, windows | GNU time `-v` (peak RSS), Linux only | C10 |
| `files_without_an_extension_cost_by_default_what_they_cost_without_documents` | `tests/perf.rs` | macos-arm64, macos-x86_64 | GNU time `-v` (peak RSS), Linux only | C10 |
| `one_file_that_is_no_document_costs_what_it_costs_without_documents` | `tests/perf.rs` | macos-arm64, macos-x86_64 | GNU time `-v` (peak RSS), Linux only | C10 |
| `rg_compat_doc` | `src/cli/rg_flags.rs` | linux, macos-arm64, macos-x86_64, windows | none: a generator that rewrites `docs/rg-compat.md`, run by hand | C8 |
| `query_share_on_the_cargo_registry` | `src/processors/treesitter/mod.rs` | linux, macos-arm64, macos-x86_64, windows | none: a measurement over `$HOME/.cargo/registry`, run by hand | C10 |

## Entry points invoked by a parent test

These `#[ignore]` tests are probes: a parent test runs each one in its own process, with the
environment or the console it needs, and checks what it did. They appear as ignored in every
system's `cargo test` output and are not skipped tests.

- `child_echo_environment`
- `orphan_probe`
- `windows_pre_resolution_probe`
- `windows_spawn_resolution_probe`
- `windows_batch_interpreter_probe`

## Tests selected explicitly in CI

On `linux` the CI runs the ignored tests of these files in a serial invocation of their own,
after the suite, without `SEARCHFOR_PERF_TIMING` (M5b plan, D18: time ratios are local, while
memory ceilings and the refusal of bombs are checked):

- every `#[ignore]` test of `tests/perf.rs`;
- `memory_stays_flat_over_five_thousand_requests` of `helpers/example/tests/protocol.rs` and
  `helpers/grammar/tests/protocol.rs`.

## Reference tools CI installs

With `SEARCHFOR_REQUIRE_REFERENCE=1` a missing reference fails its test. The CI installs, on
every system, Python with `sqlite3` (`SEARCHFOR_PYTHON`), qpdf (`SEARCHFOR_QPDF`) and
pdftotext (`SEARCHFOR_PDFTOTEXT`); on Windows the last two come from MSYS2's MinGW packages
and are named by those variables, never put on `PATH`. A local run without them prints a note
and skips the comparison.
