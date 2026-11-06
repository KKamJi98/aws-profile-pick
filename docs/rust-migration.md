# Python -> Rust migration record

This file records what the migration from awspick 2.0.2 (Python, PyPI
`aws-profile-pick`) to the Rust rewrite checked, decided and changed. The plan
it follows lives outside the repository; where the plan and reality differed,
the difference and the evidence are written here.

## Phase 0. Spike results (2026-11-06)

### Reference implementation

- Golden reference: PyPI `aws-profile-pick==2.0.2` on CPython 3.13.5, the
  interpreter the installed `uv tool` copy on the maintainer's Mac uses.
  `scripts/gen_golden.py` pins both (PEP 723 header).
- boto3 is declared in 2.0.2 but never imported. rich 13.9.4 is the only
  runtime library in use.

### rich output (what the table renderer reproduces)

Measured by running 2.0.2 with `FORCE_COLOR` and different `TERM` /
`COLORTERM` / `NO_COLOR` values (cases `color_*` in the golden corpus):

| Condition | Escape codes |
|---|---|
| stderr not a terminal and no `FORCE_COLOR` | none |
| terminal and `TERM` is `dumb` or `unknown` | none |
| terminal and `NO_COLOR` set (any value) | bold and italic only, no colours |
| terminal, `COLORTERM` `truecolor`/`24bit` | colours; `orange3` (stg) as `38;5;172` |
| terminal, `TERM` ends in `-256color` or `-kitty` | same as above |
| any other terminal | `orange3` downgraded to `33` |

Each rich segment carries its own SGR and reset. Plain cell text and its
justification padding form one segment; text that came from rich markup in
2.x (the group name, the current marker) is a separate segment from its
padding. The empty-list message is split into three segments by rich's
highlighter (`No AWS profiles found in ~`, `/.aws/`, `config`).

Width: rich takes `COLUMNS`, else the size of the first terminal among
stdin/stdout/stderr, else 80. It only matters when the table is wider than
that: rich then folds columns. The Rust renderer does not fold; the case
`table_wide_80` records this as an allowed difference and
`table_wide_columns_200` checks the same table byte for byte.

### configparser

`tests/golden/configparser.json` holds a differential corpus: for each input,
the sections CPython 3.13 `configparser.ConfigParser` returned or the error it
raised (kind, line number, section, option). The unit test
`config::tests::matches_cpython_configparser` replays it against the Rust
parser.

### release-please

Checked against the sources of googleapis/release-please v17.11.2 and
release-please-action v5.0.0:

- The `rust` strategy updates `Cargo.toml` and `Cargo.lock` in the release
  PR (`src/strategies/rust.ts`, `CargoToml` and `CargoLock` updaters).
- **Plan vs reality:** the `rust` strategy names the component after the
  Cargo package, and `include-component-in-tag` defaults to true
  (`src/strategies/base.ts:152`, `getComponent`). Left alone, the tag would be
  `awspick-v3.0.0`, which the `release` environment (`v*` only) and the tag
  trigger would not accept. The python strategy has no default package name,
  which is why 2.x tags were plain `v2.0.2`. The workflow therefore sets
  `include-component-in-tag: false`, keeping `v3.0.0` and letting
  release-please find `v2.0.2` as the previous release.
- `Release-As: x.y.z` in the commit body sets the next version (README,
  "How do I change the version number?").

### Tag-triggered publishing

The `release` environment admits only `v*` tags, so publishing moves to a
workflow started by the tag push. release-please creates the tag with the
GitHub App token; events from an App installation token start workflows,
unlike those from `GITHUB_TOKEN` (GitHub Actions docs, "Triggering a workflow
from a workflow"). This cannot be run before the first release. If the tag
push does not start `release.yml`, the fallback is `gh workflow run
release.yml --ref v3.0.0`: `workflow_dispatch` on a tag ref runs with
`GITHUB_REF=refs/tags/v3.0.0`, which the environment policy accepts, and
crates.io Trusted Publishing refuses only `pull_request_target` and
`workflow_run`. `release.yml` therefore also accepts `workflow_dispatch` and
refuses to run on anything but a `v*` tag.

### Release tooling

prjump's Phase 0 result was not available when this was written. This repo
uses a hand-written workflow either way: release-please creates the GitHub
release, and cargo-dist's default flow wants to create it itself.

## Phase 1. Behaviour-equivalent port

### Golden fixtures

`uv run scripts/gen_golden.py` writes the corpus to `tests/golden/cases.json`
and the 2.0.2 results to `tests/golden/expected/`. `tests/golden.rs` replays
every case against the Rust binary. A case compares stdout, stderr, exit code
of each run and the full file tree left in the throwaway HOME (paths, types,
modes, contents, symlink targets). Backup timestamps created during a run are
normalised to `<TS1>`, `<TS2>`, ... on both sides; HOME becomes `<HOME>`.

Allowed differences (cases whose `compare` list omits a field, each with a
`note`):

1. Paths where Python printed a traceback (EOF on stdin, invalid `--regex`,
   undecodable rc file): stderr not compared. Exit code, stdout and files are.
2. Usage errors and `-h`: argparse and clap word these differently. Exit code
   (2, or 0 for help), stdout and files are compared; `tests/cli.rs` checks
   that help lists every option.
3. configparser errors that quote the offending line (parsing errors, missing
   section header, BOM): Rust prints the path and line number only, because
   the line can be a secret.
4. Unicode digits at the prompt (`２`, `²`): Rust accepts ASCII digits only.
5. Tables wider than the terminal: not folded.
6. Profile names that rich read as markup (`a[/b]` crashed 2.x, `team[red]x`
   lost text) and names with a backslash that `re.sub` expanded (`a\1`
   corrupted the rc file). Rust prints names literally and replaces
   literally. These are bugs 9 and 2 of the plan; the plan applies the literal
   replacement already in Phase 1, so the golden case compares stdout,
   stderr and exit code but not the corrupted file.

Other known differences, not in the corpus:

- An undecodable `~/.aws/config` over 8 KiB: CPython reports the byte
  position relative to its 8 KiB decode chunk, Rust the absolute position.
- `HOME` set to an empty string: Python falls back to `/`, Rust to the
  password database.
- stdout closed before the export line is written (`awspick | true`): Python
  exits 120 after a "BrokenPipeError" message at interpreter shutdown, Rust
  exits 0 silently.
- Ctrl-C outside the prompt: Python dies from SIGINT after a traceback, Rust
  exits 130.

### Tests ported from pytest

The 34 pytest tests mocked `open` and `Path.exists`. They are replaced by
`tests/cli.rs`, which runs the binary in a throwaway HOME, plus unit tests in
`src/`. Several Python tests that checked one function each collapse into one
binary-level test. `test_update_aws_profile_fish` only checked that the new
line was present, which hid bug 1; its port keeps that weak assertion in
Phase 1 and is tightened in Phase 3.

## Phase 3. Bug fixes

Each bug from the plan is one commit with a test in `tests/cli.rs` that
failed before the fix. Where a fix changes a golden case, the case is listed
in `tests/golden/changes.json` with the reason, and its new result is pinned
in `tests/golden/changed/` (`python3 scripts/golden-change.py "<reason>"
<case>...`, then `GOLDEN_BLESS=1 cargo test --test golden`). The 2.0.2
results in `tests/golden/expected/` stay as they were, so every departure
from 2.0.2 is visible in one file.

| Bug | Change | Golden cases |
|---|---|---|
| 1 | fish: `set` with any mix of `-g`, `-x`, `-gx`, `-xg`, `--global`, `--export` is the managed line; switching replaces it instead of appending another block | `shell_fish_repeat_appends`, `shell_fish_set_x_detected` |
| 2 | names are written literally (already true since Phase 1; now pinned) | `table_backslash_name` |
| 3 | names outside `[A-Za-z0-9._@+=,:/-]` are single-quoted with the shell's escaping (POSIX `'\''`, fish `\\` `\'`); plain names keep `"name"` | `config_select_spaced_name`, `rc_shell_injection_kept`, `table_backslash_name` |
| 4 | a backup in the same second as the previous one gets `-1`, `-2`, ... | `rc_backup_same_second`, `shell_fish_repeat_appends` |
| 5 | rotation removes only `<rc>.bak-<14 digits>[-N]` | `rc_backup_rotation` |
| 6 | end of input at the prompt cancels ("No selection made.", exit 1) | `select_eof`, `select_eof_after_invalid` |
| 7 | invalid `--regex`: `ERROR: invalid regex '<pattern>': <reason>`, exit 1 | `filter_regex_invalid` |
| 8 | only ASCII digits select by number | `select_fullwidth_digit`, `select_superscript_digit` |
| 9 | table cells are printed literally (pinned) | `table_markup_like_names` |
| 10 | config errors name file and line, never the line text (pinned) | `config_bom`, `config_option_before_section`, `config_parse_error_secret_line` |
| 11 | only literal assignments are managed (one quoted or bare word, optional `# comment`, comment kept); `$`/backtick values are left alone | `rc_inline_comment`, `rc_unindented_dynamic_line` |
| 12 | arguments are parsed before the config is read | `bad_option_without_config`, `help_without_config` |
| 13 | `AWS_CONFIG_FILE` is honoured; `[profile ...]` names are shlex-split like botocore; `[default]` and `[profile default]` are one profile. `tests/cli.rs` cross-checks with `aws configure list-profiles` when the AWS CLI is installed and otherwise uses the values recorded from AWS CLI 2 | `config_section_edge_cases`, `config_select_spaced_name`, `rc_shell_injection_kept`, `table_backslash_name` |
| 14 | zsh uses `$ZDOTDIR/.zshrc` when `ZDOTDIR` is set | `shell_zdotdir_ignored` |
| 15 | the rc file is read and written as bytes: CRLF stays CRLF (an appended block uses the file's line ending), non-UTF-8 bytes are kept, a line that is not UTF-8 is never managed | `rc_crlf`, `rc_non_utf8` |

Also fixed, outside the numbered list:

- Ctrl-C at the prompt prints the same cancel lines as an empty answer and
  exits 1. Elsewhere it exits 130 (2.x: traceback).
- Atomic rc write. The plan could not reproduce the risk with Python
  (`ulimit -f` broke the interpreter start). The Rust binary starts under
  `ulimit -f`, so `failed_write_leaves_rc_intact` reproduces it: with the
  2.x-style truncate the rc was cut at the size limit; now the rc is
  unchanged and the error is `[Errno 27] File too large`. The new contents go
  to `.<name>.awspick-<pid>-<n>.tmp` beside the real file (after following a
  symlink), get the file's owner, group and mode, are synced and renamed
  over it. The rc is first opened for writing so a read-only rc is still
  refused (`[Errno 13]`) instead of being replaced. Hard links, a different
  owner (fchown fails), an unwritable directory or a failed rename fall back
  to the in-place rewrite. Extended attributes and ACLs on the rc are not
  copied to the new file.

## Phase 4. P1 features

| Feature | Notes | Golden cases |
|---|---|---|
| `-V`, `--version` | `awspick <CARGO_PKG_VERSION>`, exit 0 | `usage_version_unknown` |
| `-v`, `--verbose` | INFO lines are off by default. `tests/golden.rs` runs every case with `-v` so stderr still compares with 2.0.2; clap echoes that `-v` in usage errors and help | `bad_option_without_config`, `help_without_config` |
| `awspick <profile>` | no table, no prompt; exact name, else the single case-insensitive match; several case-insensitive matches or none: `ERROR: ...`, exit 1. Matched against all profiles, not the filtered list, so `AWSPICK_FILTER` does not get in the way | `usage_positional` |
| credentials profiles | section names of `AWS_SHARED_CREDENTIALS_FILE` (expanded like `AWS_CONFIG_FILE`) or `~/.aws/credentials`, literally and without `DEFAULT`, as botocore does (checked against AWS CLI 2.39.5 `configure list-profiles`). Values are parsed for syntax only and never kept. A broken credentials file is skipped with a `WARNING` (the AWS CLI fails instead), so a file 2.x ignored cannot stop awspick | none (the corpus has no credentials file) |
| `--completions <bash\|zsh\|fish>` | `clap_complete` 4.6.11 (the version prjump uses); an option, not a subcommand, so it cannot clash with a profile name. bash's script lists the hidden `--export-command` too | `help_without_config` |

P2 and P3 are recorded in docs/ROADMAP.md.

## Final review (before the PR left draft)

Fixed:

- Rotation could delete the backup it had just made when older backups sort
  after it (names are local time: time zone change, DST fall-back). The new
  backup is now never a rotation candidate (`rotation_keeps_the_new_backup`).
  Backups are also created owner-only until the rc's mode is applied.
- `scripts/release/render-formula.sh` exited 0 with an empty `sha256` when a
  checksum file was missing; the sums are now computed before the heredoc.
- The `homebrew` job runs in the tag-only `release` environment, like
  `crates-io`.

Known, not changed (inherited from 2.x, low impact):

- "already set" compares only the first managed line; an rc with two managed
  lines `"b"` then `"a"` is left alone by `awspick b`.
- Ctrl-C in the middle of the rc write can leave the
  `.<rc>.awspick-<pid>-<n>.tmp` file, or, in the in-place fallback, a
  truncated rc (the backup exists by then).
- `~/.config/awspick/profile` is written through a fixed `profile.tmp`
  without fsync; two concurrent runs can race.
- A non-UTF-8 HOME makes the backup, and so the update, fail (safely).
