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
