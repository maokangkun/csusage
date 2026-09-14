# csusage

Token usage reports for [Claude Science](https://claude.com/science) — a standalone, read-only
CLI that reads Claude Science's local metadata database and prints daily, monthly, or
per-session token usage.

> [!NOTE]
> This project began as a [ccusage adapter PR](https://github.com/ccusage/ccusage/pull/1703).
> The ccusage maintainers (reasonably) declined to depend on an internal, undocumented
> SQLite schema, and suggested a downstream integration — which is this tool. Portions of
> the code are derived from ccusage (MIT).

## Install

```bash
cargo install csusage
```

Or with Homebrew:

```bash
brew tap maokangkun/csusage
brew install csusage
```

## Usage

```bash
csusage                  # daily report
csusage daily            # same as above
csusage monthly          # per-month report
csusage session          # per-session report

csusage --json           # machine-readable output
csusage --since 2026-09-01 --until 2026-09-30
csusage --timezone America/New_York
csusage --db /path/to/operon-cli.db
```

## How it works

- Claude Science records aggregate token usage per conversation **frame** (a root
  conversation or a delegated sub-agent turn) in a local SQLite database.
  `csusage` reads it **read-only** (`SELECT` only) and never writes.
- Frames roll up into sessions: sub-agent frames join their parent conversation
  via `root_frame_id`.
- Costs are Claude Science's own recorded cost estimates; frames without a
  recorded cost contribute $0.
- Database discovery: `--db` / `CLAUDE_SCIENCE_DB` (exclusive, comma-separated
  list allowed) or automatic discovery of well-known roots plus the daemon's
  org layout (`~/.claude-science/cs-switch-proxy/orgs/<org>/operon-cli.db`).
  Databases that don't match the expected schema are skipped.

## Caveats

Claude Science's database is an internal format that may change without notice.
`csusage` validates the schema before reading and fails soft (skips
non-matching databases), so a format change degrades to empty reports rather
than errors — but accuracy across app versions is not guaranteed.

## License

MIT
