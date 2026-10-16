# `--json` is searchfor's own contract; `--json-rg` is ripgrep's

searchfor aims at full ripgrep flag compatibility, with one deliberate exception: `--json`
emits searchfor's own JSON Lines, one self-contained object per Match carrying its Trail,
instead of ripgrep's `begin`/`match`/`context`/`end`/`summary` messages. Agents are the main
consumers of `--json`, and an object per Match that already holds the Trail, the Fragment and the
context lines is what they can use without reassembling a stream. Tools that already parse
ripgrep's format use `--json-rg`, which emits exactly what `rg --json` does.

## Considered Options

- **ripgrep's messages with extra fields inside `match.data`**: existing `rg --json` consumers
  would keep working, but agents would have to stitch context lines back to their Match.
