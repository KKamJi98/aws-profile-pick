# AWS Pick

A simple CLI tool to easily switch between AWS profiles in your shell environment.

```
INFO: Found 8 AWS profiles
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

AWS Pick (`awspick`) is a command-line utility that helps you quickly switch between different AWS profiles defined in your `~/.aws/config` file. It automatically updates your shell environment by modifying the `AWS_PROFILE` environment variable in your shell configuration file.



## Features

- Automatically groups and color-codes profiles by environment (dev, stg, prod, preprod) with visual dividers between groups.
- Lists all available AWS profiles from your `~/.aws/config` file with numbered options
- Allows selection by either number or profile name
- Validates input to ensure a valid profile is selected
- Highlights the current `AWS_PROFILE` in the table
- Supports multiple shells:
  - Bash (`~/.bashrc`)
  - Zsh (`~/.zshrc`)
  - Fish (`~/.config/fish/config.fish`)
- Updates your shell configuration file to set the selected profile as the default
- Writes the selected profile to a shared file for cross-shell sync
- Creates backup files before modifying your configuration (keeps the 2 most recent backups)
- Ensures idempotency (no duplicate modifications if selecting the same profile)
- Prints a shell command for immediate application
- Provides clear logging of operations
- Handles errors gracefully with informative messages
- Supports case-insensitive profile name matching
- Filtering and grouping via CLI flags or env vars

## How It Works

- Read profiles: Parses `~/.aws/config` and collects `default` and sections named `profile <name>`.
- Filter list: Applies include/exclude patterns from CLI flags or env vars. Patterns can be substrings or regular expressions (`--regex`), with optional case sensitivity (`--case-sensitive`).
- Group profiles: Groups names using ordered rules. Default order: `prod`, `stg`, `dev`, `preprod`. Unmatched profiles go to `others` (appended at end unless explicitly positioned). Supports `others` positional marker and `*` wildcard catch-all for custom ordering. Groups are separated by visual dividers.
- Display and select: Renders a numbered table via `rich` and highlights the current `AWS_PROFILE` in the "Current" column. Input accepts either the number (1-based, current display order) or the profile name (case-insensitive match supported).
- Apply to shell: Detects your shell (`bash`, `zsh`, `fish`) and writes or replaces a single `AWS_PROFILE="<name>"` line in the corresponding rc file. Creates a timestamped backup and avoids duplicate changes if the same profile is already set.
- Export command: Prints the exact shell command to stdout so you can run `eval "$(awspick)"` to apply immediately in the current session.
- Cross-shell sync: Writes the selected profile to `~/.config/awspick/profile` so other shells can pick it up on the next prompt.

## Installation

### Prerequisites

- Python 3.9 or higher
- uv (for development)

### Install with uv tool (global CLI)

Use uv's tool installation when you want `awspick` available outside any virtual environment.

The distribution is `aws-profile-pick` and the command it installs is `awspick`. They differ
because `aws-pick` on PyPI is an unrelated tool, and every separator variant of that name
normalises onto the same taken one.

```bash
# PyPI
uv tool install aws-profile-pick

# Straight from the repository
uv tool install git+https://github.com/KKamJi98/aws-profile-pick

# Install from the current checkout as a global tool
uv tool install .

# Keep it editable if you want code changes to take effect immediately
uv tool install --editable .

# Upgrade an existing installation (reinstall with latest changes)
uv tool install --upgrade .

# Editable + upgrade: reinstall in editable mode so code changes apply immediately
uv tool install --editable --upgrade .
```

After installation, add the bin directory printed by uv to your `PATH` so you can run `awspick`
from any shell.

### From source

```bash
git clone https://github.com/KKamJi98/aws-profile-pick.git
cd aws-profile-pick
uv venv .venv
uv pip install -e .[dev]
```

## Usage

Simply run the command:

```bash
awspick
```

Apply the selected profile immediately in the current shell:
- Installed via `uv tool install`: `eval "$(awspick)"`
- Running the script directly: `eval "$(python3 /path/to/awspick.py)"`

You can also invoke the launcher script directly:

```bash
python3 /path/to/awspick.py
```

All prompts and logs are printed to **stderr**, while the final
`export AWS_PROFILE="..."` command is printed to **stdout**. This
ensures the menu is visible when using command substitution.

Add a wrapper function to your shell to avoid typing `eval` each time. Use `command awspick` when
installed as a uv tool to avoid recursion:

```bash
function awspick_apply() {
    eval "$(command awspick "$@")"
}

function awspick_local() {
    eval "$(python3 /your/path/to/awspick.py "$@")"
}

alias ap='awspick_apply'
```

Use these helper functions to select and apply a profile in one step, depending on how you installed
the tool.

This will:
1. Display a list of available AWS profiles
2. Prompt you to select a profile by number or name
3. Update your shell configuration file to use the selected profile
4. Create a backup of your original configuration file

Example output:
```
Enter profile number or name: 5
Selected profile: acme-dev
Updated ~/.zshrc with AWS_PROFILE=acme-dev
Backup created at ~/.zshrc.bak-20250605060000
Configuration reloaded automatically.
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

This project uses [uv](https://github.com/astral-sh/uv) for dependency management.

### Setup development environment

```bash
# Clone the repository
git clone https://github.com/KKamJi98/aws-profile-pick.git
cd aws-profile-pick

# Install dependencies
uv venv .venv
uv pip install -e .[dev]

# Setup direnv (optional)
direnv allow
```

### Running tests

```bash
pytest
```

### Code formatting

```bash
black .
isort .
```

## CI

- Single workflow: `.github/workflows/ci.yml`
- Triggers:
  - `pull_request` and all branch `push`
- Behavior:
  - Runs tests and lint on every PR/push
  - Uses `STAGE=prod` on `main`, otherwise `STAGE=preprod`
  - Runs `googleapis/release-please-action@v4` only on `main`

## Project Structure

```
aws-profile-pick/
├── awspick.py       # Single-file launcher script
├── awspick/
│   ├── __init__.py
│   ├── cli.py          # Command-line interface
│   ├── config.py       # AWS config file parsing
│   └── shell.py        # Shell profile modification
├── pyproject.toml      # Project metadata and dependencies
├── README.md
└── LICENSE
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
