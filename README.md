# AWS Pick

A simple CLI tool to easily switch between AWS profiles in your shell environment.

```
           AWS Profiles
┏━━━━━┳━━━━━━━━━━━━━━━━━┳━━━━━━━━┳━━━━━━━━━┓
┃ No. ┃ Profile         ┃ Group  ┃ Current ┃
┡━━━━━╇━━━━━━━━━━━━━━━━━╇━━━━━━━━╇━━━━━━━━━┩
│   1 │ acme-prod       │ prod   │         │
│   2 │ acme-prod-admin │ prod   │         │
├─────┼─────────────────┼────────┼─────────┤
│   3 │ acme-stg        │ stg    │         │
│   4 │ acme-stg-admin  │ stg    │         │
├─────┼─────────────────┼────────┼─────────┤
│   5 │ acme-dev        │ dev    │    *    │
│   6 │ acme-dev-admin  │ dev    │         │
├─────┼─────────────────┼────────┼─────────┤
│   7 │ default         │ others │         │
│   8 │ sandbox         │ others │         │
└─────┴─────────────────┴────────┴─────────┘
Enter profile number or name:
```

## Overview

AWS Pick (`awspick`) is a command-line utility that helps you quickly switch between different AWS profiles defined in your `~/.aws/config` and `~/.aws/credentials` files. It automatically updates your shell environment by modifying the `AWS_PROFILE` environment variable in your shell configuration file.



## Features

- Automatically groups and color-codes profiles by environment (dev, stg, prod, preprod) with visual dividers between groups.
- Lists all available AWS profiles with numbered options: the same names as `aws configure list-profiles`, from `~/.aws/config` (`AWS_CONFIG_FILE`) and `~/.aws/credentials` (`AWS_SHARED_CREDENTIALS_FILE`; only section names are read)
- Allows selection by either number or profile name, or directly with `awspick <profile>`
- Validates input to ensure a valid profile is selected
- Highlights the current `AWS_PROFILE` in the table
- Supports multiple shells:
  - Bash (`~/.bashrc`)
  - Zsh (`~/.zshrc`, or `$ZDOTDIR/.zshrc` when `ZDOTDIR` is set)
  - Fish (`~/.config/fish/config.fish`)
- Updates your shell configuration file to set the selected profile as the default
- Writes the selected profile to a shared file for cross-shell sync
- Creates backup files before modifying your configuration (keeps the 2 most recent backups; only removes backups it created)
- Rewrites the rc file atomically and keeps its other bytes and line endings (LF or CRLF) as they were
- Ensures idempotency (no duplicate modifications if selecting the same profile)
- Prints a shell command for immediate application
- Logs each step with `-v/--verbose`
- Shell completions for bash, zsh and fish
- Handles errors gracefully with informative messages
- Supports case-insensitive profile name matching
- Filtering and grouping via CLI flags or env vars

## How It Works

- Read profiles: Parses `~/.aws/config` and collects `default` and sections named `profile <name>` (names are split like the AWS CLI does, so `[profile "a b"]` is `a b`), then adds the section names of `~/.aws/credentials`.
- Filter list: Applies include/exclude patterns from CLI flags or env vars. Patterns can be substrings or regular expressions (`--regex`), with optional case sensitivity (`--case-sensitive`).
- Group profiles: Groups names using ordered rules. Default order: `prod`, `stg`, `dev`, `preprod`. Unmatched profiles go to `others` (appended at end unless explicitly positioned). Supports `others` positional marker and `*` wildcard catch-all for custom ordering. Groups are separated by visual dividers.
- Display and select: Renders a numbered table and highlights the current `AWS_PROFILE` in the "Current" column. Input accepts either the number (1-based, current display order) or the profile name (case-insensitive match supported).
- Apply to shell: Detects your shell (`bash`, `zsh`, `fish`) and writes or replaces the `AWS_PROFILE="<name>"` line in the corresponding rc file. Only lines at the start of a line that set a literal value (`export AWS_PROFILE="x"`, `set -gx AWS_PROFILE x`, optionally followed by a `# comment`, which is kept) are managed; a computed value such as `export AWS_PROFILE="$(cat ...)"` is left alone. Names with characters outside `A-Za-z0-9._@+=,:/-` are single-quoted so the shell never expands them. Creates a timestamped backup and avoids duplicate changes if the same profile is already set.
- Export command: Prints the exact shell command to stdout so you can run `eval "$(awspick)"` to apply immediately in the current session.
- Cross-shell sync: Writes the selected profile to `~/.config/awspick/profile` so other shells can pick it up on the next prompt.

## Installation

awspick is a single static binary. Python is no longer needed.

### Homebrew (macOS, Linux)

```bash
brew install KKamJi98/tap/awspick
```

### Cargo

```bash
cargo install --locked awspick
```

### GitHub Release

Download `awspick-<version>-<target>.tar.gz` for your platform from the
[releases page](https://github.com/KKamJi98/aws-profile-pick/releases), check it
against `SHA256SUMS`, and put `awspick` on your `PATH`. Builds exist for macOS
(arm64, x86_64) and Linux (x86_64, aarch64; statically linked with musl). Each
release carries a build provenance attestation:

```bash
gh attestation verify awspick-<version>-<target>.tar.gz --repo KKamJi98/aws-profile-pick
```

Windows is not supported: awspick edits bash, zsh and fish rc files.

### Moving from the Python version (2.x)

Versions up to 2.0.2 were published to PyPI as `aws-profile-pick`. 3.0.0 is a
Rust rewrite with the same command, options and files, and PyPI gets no
further releases. What changes for 2.x users:

- `INFO:` log lines are printed only with `-v/--verbose`. stdout (the
  `export` line) and the summary lines are unchanged, so `eval "$(awspick)"`
  wrappers keep working.
- New: `awspick <profile>`, `-V/--version`, `--completions <shell>`, profiles
  from `~/.aws/credentials`, `AWS_CONFIG_FILE`, `ZDOTDIR`.
- Bug fixes: fish lines are replaced instead of appended, unusual profile names
  are quoted for the shell, backups in the same second no longer overwrite each
  other, other `*.bak-*` files are no longer deleted, Ctrl-C/EOF and invalid
  regexes no longer print tracebacks, `--help` works without a config, config
  errors no longer echo the offending line, rc files keep CRLF and non-UTF-8
  bytes, and the rc write is atomic. docs/rust-migration.md has the full list.

Install the new binary first, then remove the old one:

```bash
brew install KKamJi98/tap/awspick      # or: cargo install --locked awspick
uv tool uninstall aws-profile-pick
command -v awspick                     # should now point at brew or ~/.cargo/bin
```

To go back, `brew uninstall awspick` and `uv tool install aws-profile-pick==2.0.2`.

## Usage

Simply run the command:

```bash
awspick
```

Or name the profile to skip the table and the prompt (exact name first, then a
unique case-insensitive match; filters and groups do not apply):

```bash
awspick acme-dev
```

Other options:

```bash
awspick -v                     # also print INFO log lines
awspick --version              # awspick <version>
awspick --completions zsh      # print a completion script (bash, zsh, fish)
```

Completion scripts, for example:

```bash
awspick --completions bash > ~/.local/share/bash-completion/completions/awspick
awspick --completions zsh > "${fpath[1]}/_awspick"
awspick --completions fish > ~/.config/fish/completions/awspick.fish
```

Apply the selected profile immediately in the current shell:

```bash
eval "$(awspick)"
```

All prompts, summary lines and logs are printed to **stderr**, while the final
`export AWS_PROFILE="..."` command is printed to **stdout**. This
ensures the menu is visible when using command substitution.

Add a wrapper function to your shell to avoid typing `eval` each time. `command awspick` avoids
recursion if you name the function `awspick`:

```bash
function ap() {
    eval "$(command awspick "$@")"
}
```

This will:
1. Display a list of available AWS profiles
2. Prompt you to select a profile by number or name
3. Update your shell configuration file to use the selected profile
4. Create a backup of your original configuration file

Example output (stderr):
```
Enter profile number or name: 5
Selected profile: acme-dev
Backup created at /home/me/.zshrc.bak-20250605060000
Updated /home/me/.zshrc with AWS_PROFILE=acme-dev
Updated shared profile at /home/me/.config/awspick/profile
Run 'eval "$(awspick)"' to apply in the current shell
```

## Sync across tabs and shells (recommended)

By default, each shell session has its own environment. To keep multiple tabs or splits in sync,
`awspick` writes the selected profile to `~/.config/awspick/profile`. Add a small hook so each
shell reads that file on every prompt. The change will apply on the next prompt.

### Bash

```bash
# ~/.bashrc
awspick_sync() {
  local f="$HOME/.config/awspick/profile"
  if [ -f "$f" ]; then
    export AWS_PROFILE="$(cat "$f")"
  fi
}
PROMPT_COMMAND="awspick_sync${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
```

### Zsh

```bash
# ~/.zshrc
awspick_sync() {
  local f="$HOME/.config/awspick/profile"
  if [ -f "$f" ]; then
    export AWS_PROFILE="$(cat "$f")"
  fi
}
precmd_functions+=(awspick_sync)
```

### Fish

```fish
# ~/.config/fish/config.fish
function awspick_sync --on-event fish_prompt
    set -l f ~/.config/awspick/profile
    if test -f $f
        set -gx AWS_PROFILE (cat $f)
    end
end
```

## Filtering and Grouping

You can control which profiles are shown and how they are grouped.

- `-f, --filter`: Only show profiles matching any given substring.
- `-x, --exclude`: Exclude profiles matching any given substring.
- `-g, --groups`: Only display specific groups (e.g., `prod,dev`).
- `--group-rules`: Customize grouping rules. Order determines display order.
- `--regex`: Treat `--filter`/`--exclude` as regular expressions.
- `--case-sensitive`: Make matches case-sensitive.

### Group rules syntax

Rules are semicolon-separated: `name=keyword,keyword;name2=keyword`.
Keywords match on token boundaries (`-`, `_`) so `prod` won't match `preprod`.

| Syntax | Meaning |
|---|---|
| `tf=tf` | Profiles containing token `tf` → group `tf` |
| `prod=prod,production` | Multiple keywords for one group |
| `others` | Positional marker - unmatched profiles appear here |
| `main=*` | Catch-all wildcard - same as `others` but with custom name |

**Display order** follows the rule order. Unmatched profiles go to the catch-all group
(`others` by default, appended at the end if not explicitly placed).

**Matching priority**: explicit keyword rules are always evaluated before `*`/`others`,
regardless of position. First matching explicit rule wins.

Groups are visually separated by a divider line when the group changes.

### Examples

```bash
# Show only prod and dev groups
awspick --groups prod,dev

# Show profiles containing "tooling" but not "legacy"
awspick -f tooling -x legacy

# Regex example: include profiles ending with -admin, exclude sandbox
awspick --regex -f '.*-admin$' -x sandbox

# Custom grouping: order ensures first match wins
awspick --group-rules 'preprod=preprod;prod=prod,production;stg=stg;dev=dev'

# Place unmatched profiles on top, infra profiles below
awspick --group-rules 'others;infra=infra'

# Same but rename the catch-all group to "app"
awspick --group-rules 'app=*;infra=infra'
```

```
# Result of 'others;infra=infra' with profiles: api-dev, api-prod, infra-dev, infra-prod
┏━━━━━┳━━━━━━━━━━━━┳━━━━━━━━┳━━━━━━━━━┓
┃ No. ┃ Profile    ┃ Group  ┃ Current ┃
┡━━━━━╇━━━━━━━━━━━━╇━━━━━━━━╇━━━━━━━━━┩
│   1 │ api-dev    │ others │    *    │
│   2 │ api-prod   │ others │         │
├─────┼────────────┼────────┼─────────┤
│   3 │ infra-dev  │ infra  │         │
│   4 │ infra-prod │ infra  │         │
└─────┴────────────┴────────┴─────────┘
```

### Environment variables

```bash
export AWSPICK_FILTER="tooling,admin"
export AWSPICK_EXCLUDE="legacy"
export AWSPICK_GROUPS_SHOW="prod,dev"
export AWSPICK_GROUP_RULES='others;infra=infra'
export AWSPICK_REGEX=0             # 1/true to enable regex
export AWSPICK_CASE_SENSITIVE=0    # 1/true for case-sensitive
```

## Development

```bash
git clone https://github.com/KKamJi98/aws-profile-pick.git
cd aws-profile-pick
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

`tests/golden` pins the behaviour of the Python release this binary replaced:
`scripts/gen_golden.py` runs PyPI `aws-profile-pick` 2.0.2 over a corpus of
configs, rc files, shells, options and inputs, and `tests/golden.rs` checks
that the Rust binary produces the same stdout, stderr, exit code and files.
Regenerate it with `uv run scripts/gen_golden.py`. docs/rust-migration.md
lists the differences the corpus allows.

## CI

- `.github/workflows/ci.yml`: format, clippy and tests on Linux and macOS, `cargo deny`, and `cargo publish --dry-run`, for pull requests and branches other than `main`.
- `.github/workflows/release-please.yml`: on every push to `main`, the same checks, then release-please (`release-type: rust`). Merging its release PR tags `v<version>` and creates the GitHub release.
- `.github/workflows/release.yml`: on that tag, builds the four targets, attaches archives, `SHA256SUMS` and attestations to the release, publishes to crates.io through Trusted Publishing (environment `release`), and updates `Formula/awspick.rb` in KKamJi98/homebrew-tap.
- Repository settings it relies on:
  - variable `RELEASE_APP_CLIENT_ID` and secret `RELEASE_APP_PRIVATE_KEY`: the GitHub App release-please runs as
  - secret `HOMEBREW_TAP_DEPLOY_KEY`: a deploy key with write access to KKamJi98/homebrew-tap
  - environment `release`, limited to `v*` tags, named in the crates.io Trusted Publisher

## Project Structure

```
aws-profile-pick/
├── src/
│   ├── main.rs         # Entry point
│   ├── cli.rs          # Options and the overall flow
│   ├── config.rs       # ~/.aws/config parsing, filtering, grouping
│   ├── shell.rs        # Shell detection and rc file updates
│   ├── table.rs        # The profile table
│   ├── out.rs          # stdout/stderr and log lines
│   └── py.rs           # Python behaviour the port has to match
├── tests/              # Binary tests and golden fixtures
├── scripts/            # Golden generator, Homebrew formula renderer
├── docs/
│   ├── rust-migration.md   # Port record, golden differences, bug fixes
│   └── ROADMAP.md          # Planned features
├── Cargo.toml
└── README.md
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes following the commit convention
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Commit Convention

This project follows the Conventional Commits specification:

- `feat`: A new feature
- `fix`: A bug fix
- `docs`: Documentation only changes
- `style`: Changes that do not affect the meaning of the code
- `refactor`: A code change that neither fixes a bug nor adds a feature
- `test`: Adding missing tests or correcting existing tests
- `chore`: Changes to the build process or auxiliary tools

Example: `feat(cli): implement number and name selection logic`
