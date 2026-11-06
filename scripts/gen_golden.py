# /// script
# requires-python = "==3.13.*"
# dependencies = ["aws-profile-pick==2.0.2"]
# ///
"""Generate the golden fixtures that pin the Rust port to awspick 2.0.2.

Run from the repository root:

    uv run scripts/gen_golden.py

The script defines the case corpus, writes it to tests/golden/cases.json,
runs the Python release (PyPI aws-profile-pick 2.0.2 on CPython 3.13) for
every case in a throwaway HOME, and writes what it observed to
tests/golden/expected/<case>.json: stdout, stderr and exit code of each run,
plus every file left under HOME with its mode and content.

tests/golden.rs replays the same cases against the Rust binary and compares.
Each case names the fields it compares. A case that compares less than
everything carries a `note` saying why; those are the allowed differences
listed in docs/rust-migration.md.

The generator only ever touches temporary directories. It never reads or
writes the real HOME.
"""

from __future__ import annotations

import base64
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GOLDEN = ROOT / "tests" / "golden"
AWSPICK = str(Path(sys.executable).parent / "awspick")
ALL = ["stdout", "stderr", "exit", "files"]

# --------------------------------------------------------------------------
# Corpus building blocks
# --------------------------------------------------------------------------

BASIC = (
    "[default]\nregion = ap-northeast-2\n\n"
    "[profile acme-prod]\n[profile acme-prod-admin]\n"
    "[profile acme-stg]\n[profile acme-stg-admin]\n"
    "[profile acme-dev]\n[profile acme-dev-admin]\n"
    "[profile sandbox]\n[sso-session corp]\nsso_region = us-east-1\n"
)
SMALL = "[default]\n[profile acme-prod]\n[profile acme-dev]\n"
INFRA = "[profile api-dev]\n[profile api-prod]\n[profile infra-dev]\n[profile infra-prod]\n"
MANY = "".join(f"[profile team{i:02d}-dev]\n" for i in range(1, 13))
WIDE = "[profile " + "a-very-long-profile-name-for-the-production-account-x" * 2 + "]\n"

# Ethan's real zsh hook shape: the export inside the function is indented,
# so the 2.x pattern (anchored at column 0) must leave it alone.
SYNC_HOOK = (
    'export AWS_PAGER=""\n'
    'export AWS_PROFILE="company"\n'
    "awspick_sync() {\n"
    '  local f="$HOME/.config/awspick/profile"\n'
    '  if [ -f "$f" ]; then\n'
    '    export AWS_PROFILE="$(cat "$f")"\n'
    "  fi\n"
    "}\n"
    "precmd_functions+=(awspick_sync)\n"
)

CASES: list[dict] = []


def case(
    name,
    *,
    files=None,
    env=None,
    runs=None,
    args=None,
    stdin="q\n",
    compare=None,
    note=None,
    same_second=False,
    modes=None,
):
    """Register a case. `files` maps HOME-relative paths to str, bytes,
    ("symlink", target) or ("dir",)."""
    enc = {}
    for path, value in (files or {}).items():
        if isinstance(value, tuple) and value[0] == "symlink":
            enc[path] = {"symlink": value[1]}
        elif isinstance(value, tuple) and value[0] == "dir":
            enc[path] = {"dir": True}
        elif isinstance(value, bytes):
            enc[path] = {"b64": base64.b64encode(value).decode()}
        else:
            enc[path] = value
    full_env = {"PATH": "/usr/bin:/bin", "SHELL": "/bin/zsh"}
    full_env.update(env or {})
    if runs is None:
        runs = [{"args": args or [], "stdin": stdin}]
    CASES.append(
        {
            "name": name,
            "files": enc,
            "modes": modes or {},
            "env": full_env,
            "runs": runs,
            "compare": ALL if compare is None else compare,
            "note": note,
            "same_second": same_second,
        }
    )


def run(args=None, stdin="q\n"):
    return {"args": args or [], "stdin": stdin}


# --------------------------------------------------------------------------
# The corpus
# --------------------------------------------------------------------------


def build_corpus():
    cfg = {".aws/config": BASIC}

    # Selection paths on the README profile set.
    case("select_by_number", files={**cfg, ".zshrc": "alias a=b\n"}, stdin="5\n")
    case("select_by_name", files={**cfg, ".zshrc": "alias a=b\n"}, stdin="acme-dev\n")
    case("select_case_insensitive", files={**cfg, ".zshrc": ""}, stdin="ACME-DEV\n")
    case("select_leading_zero", files={**cfg, ".zshrc": ""}, stdin="  007 \n")
    case("select_empty_line", files={**cfg, ".zshrc": ""}, stdin="\n")
    case("select_quit", files={**cfg, ".zshrc": ""}, stdin="QUIT\n")
    case("select_invalid_then_valid", files={**cfg, ".zshrc": ""}, stdin="zzz\n0\n99\n2\n")
    case(
        "select_eof",
        files={**cfg, ".zshrc": ""},
        stdin="",
        compare=["stdout", "exit", "files"],
        note="Python prints a traceback for EOFError; Rust prints the error line only",
    )
    case(
        "select_eof_after_invalid",
        files={**cfg, ".zshrc": ""},
        stdin="nope\n",
        compare=["stdout", "exit", "files"],
        note="Python prints a traceback for EOFError",
    )
    case(
        "select_fullwidth_digit",
        files={**cfg, ".zshrc": ""},
        stdin="２\nq\n",
        compare=[],
        note="Unicode digits: Python int() accepts U+FF12, Rust takes ASCII digits only",
    )
    case(
        "select_superscript_digit",
        files={**cfg, ".zshrc": ""},
        stdin="²\nq\n",
        compare=[],
        note="Unicode digits: Python crashes in int('²')",
    )

    # Current profile marker.
    case("current_from_env", files={**cfg, ".zshrc": ""}, env={"AWS_PROFILE": " ACME-STG "})
    case("current_from_rc", files={**cfg, ".zshrc": "export AWS_PROFILE='acme-dev-admin'\n"})
    case(
        "current_env_blank_falls_back_to_rc",
        files={**cfg, ".zshrc": "export AWS_PROFILE=sandbox\n"},
        env={"AWS_PROFILE": "   "},
    )

    # rc file editing.
    case("rc_missing_zsh", files={**cfg}, stdin="1\n")
    case("rc_missing_bash", files={**cfg}, env={"SHELL": "/bin/bash"}, stdin="1\n")
    case("rc_empty", files={**cfg, ".zshrc": ""}, stdin="1\n")
    case(
        "rc_same_profile_unchanged",
        files={**cfg, ".zshrc": 'export AWS_PROFILE="acme-dev"\n'},
        stdin="acme-dev\n",
    )
    case(
        "rc_replace",
        files={**cfg, ".zshrc": 'x=1\nexport AWS_PROFILE="acme-prod"\ny=2\n'},
        stdin="acme-dev\n",
    )
    case(
        "rc_replace_all_matches",
        files={**cfg, ".zshrc": "export AWS_PROFILE=a\nexport  AWS_PROFILE=b\n"},
        stdin="acme-dev\n",
    )
    case("rc_sync_hook_preserved", files={**cfg, ".zshrc": SYNC_HOOK}, stdin="acme-dev\n")
    case(
        "rc_inline_comment",
        files={**cfg, ".zshrc": "export AWS_PROFILE=acme-prod # pinned\n"},
        stdin="acme-dev\n",
    )
    case(
        "rc_unindented_dynamic_line",
        files={**cfg, ".zshrc": 'export AWS_PROFILE="$(cat ~/.config/awspick/profile)"\n'},
        stdin="acme-dev\n",
    )
    case(
        "rc_trailing_whitespace_append",
        files={**cfg, ".zshrc": "alias a=b\n\n\n  \t\n"},
        stdin="1\n",
    )
    case(
        "rc_crlf",
        files={**cfg, ".zshrc": 'alias a=b\r\nexport AWS_PROFILE="acme-prod"\r\n'},
        stdin="acme-dev\n",
    )
    case(
        "rc_non_utf8",
        files={**cfg, ".zshrc": b'alias caf\xe9=b\nexport AWS_PROFILE="acme-prod"\n'},
        stdin="acme-dev\n",
        compare=["stdout", "exit", "files"],
        note="Python logs the UnicodeDecodeError with a traceback",
    )
    case(
        "rc_symlink_mode_600",
        files={**cfg, "dotfiles/zshrc": "alias a=b\n", ".zshrc": ("symlink", "dotfiles/zshrc")},
        modes={"dotfiles/zshrc": "600"},
        stdin="1\n",
    )
    case(
        "rc_backup_rotation",
        files={
            **cfg,
            ".zshrc": "x\n",
            ".zshrc.bak-20240101000000": "OLD1\n",
            ".zshrc.bak-20250101000000": "OLD2\n",
            ".zshrc.bak-0-before-migration": "MANUAL\n",
        },
        stdin="1\n",
    )
    case(
        "rc_backup_same_second",
        files={**cfg, ".zshrc": "ORIGINAL\n"},
        same_second=True,
        runs=[run(stdin="1\n"), run(stdin="2\n"), run(stdin="3\n")],
    )
    case(
        "rc_shell_injection_kept",
        files={
            ".aws/config": '[profile x$(touch PWNED)]\n[profile y";touch PWN2;"]\n',
            ".zshrc": "",
        },
        stdin="2\n",
    )

    # Shells.
    case(
        "shell_bash",
        files={**cfg, ".bashrc": 'export AWS_PROFILE="acme-prod"\n'},
        env={"SHELL": "/opt/homebrew/bin/bash"},
        stdin="acme-dev\n",
    )
    case(
        "shell_fish_create", files={**cfg}, env={"SHELL": "/usr/local/bin/fish"}, stdin="acme-dev\n"
    )
    case(
        "shell_fish_repeat_appends",
        files={**cfg, ".config/fish/config.fish": "set -gx PATH x\n"},
        env={"SHELL": "/usr/local/bin/fish"},
        runs=[run(stdin="1\n"), run(stdin="2\n")],
        same_second=True,
    )
    case(
        "shell_fish_set_x_detected",
        files={**cfg, ".config/fish/config.fish": "set -x AWS_PROFILE 'acme-prod'\n"},
        env={"SHELL": "/usr/local/bin/fish"},
        stdin="acme-dev\n",
    )
    case(
        "shell_unknown_falls_back_to_bash",
        files={**cfg, ".bashrc": ""},
        env={"SHELL": "/usr/bin/nu"},
        stdin="1\n",
    )
    case(
        "shell_uppercase_name", files={**cfg, ".zshrc": ""}, env={"SHELL": "/bin/ZSH5"}, stdin="1\n"
    )
    case("shell_unset_uses_parent", files={**cfg, ".bashrc": ""}, env={"SHELL": None}, stdin="1\n")
    case("shell_empty", files={**cfg, ".bashrc": ""}, env={"SHELL": ""}, stdin="1\n")
    case(
        "shell_trailing_slash",
        files={**cfg, ".bashrc": ""},
        env={"SHELL": "/bin/zsh/"},
        stdin="1\n",
    )
    case(
        "shell_zdotdir_ignored",
        files={**cfg, "zdot/.zshrc": ""},
        env={"ZDOTDIR": "zdot"},
        stdin="1\n",
    )

    # Filtering and grouping.
    case("filter_substring", files={**cfg, ".zshrc": ""}, args=["-f", "admin"])
    case(
        "filter_repeat_and_csv",
        files={**cfg, ".zshrc": ""},
        args=["-f", "prod", "-f", "sandbox, ,dev"],
    )
    case("filter_attached_and_equals", files={**cfg, ".zshrc": ""}, args=["-fstg", "-x=admin"])
    case(
        "filter_case_sensitive",
        files={**cfg, ".zshrc": ""},
        args=["--case-sensitive", "-f", "ACME"],
    )
    case(
        "filter_regex",
        files={**cfg, ".zshrc": ""},
        args=["--regex", "-f", ".*-admin$", "-x", "stg"],
    )
    case("filter_regex_lookahead", files={**cfg, ".zshrc": ""}, args=["--regex", "-f", "^(?!acme)"])
    case(
        "filter_regex_invalid",
        files={**cfg, ".zshrc": ""},
        args=["--regex", "-f", "("],
        compare=["stdout", "exit", "files"],
        note="Python prints a traceback for re.error",
    )
    case("filter_no_match", files={**cfg, ".zshrc": ""}, args=["-f", "zzz"], stdin="1\nq\n")
    case("groups_show", files={**cfg, ".zshrc": ""}, args=["-g", "prod,dev"])
    case("groups_last_wins", files={**cfg, ".zshrc": ""}, args=["-g", "prod", "-g", "dev"])
    case("groups_only_commas", files={**cfg, ".zshrc": ""}, args=["-g", ",,"])
    case(
        "group_rules_others_first",
        files={".aws/config": INFRA, ".zshrc": ""},
        args=["--group-rules", "others;infra=infra"],
    )
    case(
        "group_rules_wildcard_rename",
        files={".aws/config": INFRA, ".zshrc": ""},
        args=["--group-rules", "app=*;infra=infra"],
    )
    case(
        "group_rules_duplicate_name",
        files={".aws/config": INFRA, ".zshrc": ""},
        args=["--group-rules", "a=api;a=infra"],
    )
    case(
        "group_rules_custom_names",
        files={".aws/config": INFRA, ".zshrc": ""},
        args=["--group-rules", "prd=prod;dv=dev"],
    )
    case(
        "abbreviated_long_options",
        files={**cfg, ".zshrc": ""},
        args=["--case", "--reg", "--fil", "acme-.*-admin"],
    )
    case("flag_repeated", files={**cfg, ".zshrc": ""}, args=["--regex", "--regex", "-f", "dev$"])
    case("export_command_ignored", files={**cfg, ".zshrc": ""}, args=["--export-command"])

    # Environment variables.
    case(
        "env_filter_exclude",
        files={**cfg, ".zshrc": ""},
        env={"AWSPICK_FILTER": "acme", "AWSPICK_EXCLUDE": "admin"},
    )
    case("env_filter_empty_parts", files={**cfg, ".zshrc": ""}, env={"AWSPICK_FILTER": "zzz,,"})
    case(
        "env_cli_overrides",
        files={**cfg, ".zshrc": ""},
        env={"AWSPICK_FILTER": "zzz"},
        args=["-f", "sandbox"],
    )
    case(
        "env_groups_rules",
        files={".aws/config": INFRA, ".zshrc": ""},
        env={"AWSPICK_GROUP_RULES": "others;infra=infra", "AWSPICK_GROUPS_SHOW": "infra"},
    )
    case(
        "env_regex_case",
        files={**cfg, ".zshrc": ""},
        env={
            "AWSPICK_REGEX": "TRUE",
            "AWSPICK_CASE_SENSITIVE": "yes",
            "AWSPICK_FILTER": "^acme-(dev|STG)$",
        },
    )

    # Usage errors and help (exit code only, see docs).
    case(
        "usage_ambiguous_option",
        files={**cfg},
        args=["--grou", "prod"],
        compare=["stdout", "exit", "files"],
        note="argparse and clap word usage errors differently",
    )
    case(
        "usage_ambiguous_hidden_option",
        files={**cfg},
        args=["--ex", "dev"],
        compare=["stdout", "exit", "files"],
        note="argparse and clap word usage errors differently",
    )
    case(
        "usage_positional",
        files={**cfg},
        args=["extra"],
        compare=["stdout", "exit", "files"],
        note="argparse and clap word usage errors differently",
    )
    case(
        "usage_version_unknown",
        files={**cfg},
        args=["--version"],
        compare=["stdout", "exit", "files"],
        note="argparse and clap word usage errors differently",
    )
    case(
        "usage_missing_value",
        files={**cfg},
        args=["-f"],
        compare=["stdout", "exit", "files"],
        note="argparse and clap word usage errors differently",
    )
    case(
        "help",
        files={**cfg},
        args=["-h"],
        compare=["exit", "files"],
        note="help text layout differs; options are checked by a separate test",
    )
    case("help_without_config", files={}, args=["--help"])
    case("bad_option_without_config", files={}, args=["--bogus"])

    # ~/.aws/config parsing.
    case("config_missing", files={".zshrc": ""})
    case("config_is_directory", files={".aws/config": ("dir",), ".zshrc": ""})
    case("config_empty", files={".aws/config": "", ".zshrc": ""})
    case("config_only_sso", files={".aws/config": "[sso-session a]\nx=1\n", ".zshrc": ""})
    case(
        "config_section_edge_cases",
        files={
            ".aws/config": (
                '[default]\n[profile default]\n[profile  two-spaces]\n[profile "quoted name"]\n'
                "[profileX]\n[ profile lead]\n[sso-session s]\n[services svc]\n"
                "[DEFAULT]\nregion=x\n[profile trail] ; comment\n[]]\n"
            ),
            ".zshrc": "",
        },
        stdin="q\n",
    )
    case(
        "config_select_spaced_name",
        files={".aws/config": "[profile  two-spaces]\n", ".zshrc": ""},
        stdin="1\n",
    )
    case(
        "config_comments_continuations",
        files={
            ".aws/config": (
                "# top comment\n; another\n[profile a]\nregion = x\n  [profile hidden]\n"
                "credential_process = cmd\n    --flag\n\n[profile b]\n  indented_option = y\n"
            ),
            ".zshrc": "",
        },
    )
    case(
        "config_crlf",
        files={".aws/config": "[profile a]\r\nregion=x\r\n[profile b]\r\n", ".zshrc": ""},
    )
    case("config_bom", files={".aws/config": "\ufeff[profile a]\n", ".zshrc": ""},
        compare=["stdout", "exit", "files"],
        note="configparser prints the offending line; Rust does not",
    )
    case(
        "config_duplicate_option",
        files={".aws/config": "[profile a]\nregion=x\nRegion = y\n", ".zshrc": ""},
    )
    case(
        "config_duplicate_section",
        files={".aws/config": "[profile a]\n[profile b]\n[profile a]\n", ".zshrc": ""},
    )
    case(
        "config_option_before_section",
        files={".aws/config": "region=x\n[profile a]\n", ".zshrc": ""},
        compare=["stdout", "exit", "files"],
        note="configparser prints the offending line; Rust does not",
    )
    case(
        "config_parse_error_secret_line",
        files={
            ".aws/config": (
                "[profile a]\nregion=x\nwJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\n=novalue\n"
            ),
            ".zshrc": "",
        },
        compare=["stdout", "exit", "files"],
        note="configparser prints the offending line; Rust does not",
    )
    case("config_non_utf8", files={".aws/config": b"[profile caf\xe9]\n", ".zshrc": ""})
    case(
        "config_empty_option_name_twice",
        files={".aws/config": "[profile a]\n=1\n=2\n", ".zshrc": ""},
    )

    # Table layout.
    case("table_many_profiles", files={".aws/config": MANY, ".zshrc": ""}, stdin="12\n")
    case(
        "table_unicode_names",
        files={
            ".aws/config": "[profile 한글-dev]\n[profile ＡＢ-prod]\n[profile café]\n",
            ".zshrc": "",
        },
    )
    case(
        "table_wide_80",
        files={".aws/config": WIDE, ".zshrc": ""},
        compare=["stdout", "exit", "files"],
        note="rich folds tables wider than the terminal; Rust does not",
    )
    case(
        "table_wide_columns_200", files={".aws/config": WIDE, ".zshrc": ""}, env={"COLUMNS": "200"}
    )
    case(
        "table_markup_like_names",
        files={
            ".aws/config": "[profile team[red]x]\n[profile a[/b]]\n[profile a[b]]\n[profile ok]\n",
            ".zshrc": "",
        },
        stdin="q\n",
        compare=["stdout", "exit", "files"],
        note="rich parsed cell text as markup (bug 9)",
    )
    case(
        "table_backslash_name",
        files={
            ".aws/config": "[profile a\\1]\n[profile c]\n",
            ".zshrc": 'export AWS_PROFILE="c"\n',
        },
        stdin="a\\1\n",
        compare=["stdout", "stderr", "exit"],
        note="re.sub expanded \\1 in the replacement (bug 2)",
    )

    # Colours (FORCE_COLOR makes rich treat the pipe as a terminal).
    for label, extra in [
        ("standard", {"TERM": "xterm"}),
        ("256color", {"TERM": "xterm-256color"}),
        ("truecolor", {"COLORTERM": "truecolor", "TERM": "xterm-256color"}),
        ("16color", {"TERM": "xterm-16color"}),
        ("kitty", {"TERM": "xterm-kitty"}),
        ("no_term", {}),
        ("no_color", {"TERM": "xterm", "NO_COLOR": ""}),
        ("dumb", {"TERM": "dumb"}),
    ]:
        env = {"FORCE_COLOR": "1", **extra}
        case(
            f"color_{label}",
            files={".aws/config": BASIC + "[profile x-preprod]\n[profile y-weird]\n", ".zshrc": ""},
            env={**env, "AWS_PROFILE": "acme-dev"},
            args=["--group-rules", "prod=prod;stg=stg;dev=dev;preprod=preprod;weird=weird"],
        )
    case(
        "color_empty_list",
        files={**cfg, ".zshrc": ""},
        env={"FORCE_COLOR": "1", "TERM": "xterm"},
        args=["-f", "zzz"],
    )
    case(
        "color_empty_list_no_color",
        files={**cfg, ".zshrc": ""},
        env={"FORCE_COLOR": "1", "TERM": "xterm", "NO_COLOR": "1"},
        args=["-f", "zzz"],
    )


# --------------------------------------------------------------------------
# configparser differential corpus: section names, or the error raised.
# --------------------------------------------------------------------------

CONFIG_CORPUS = [
    "",
    "[default]\n",
    "[default]\n[profile a]\nregion = x\n",
    "[profile a[b]]\n[]]\n[]\n",
    "[ profile spaced ]\n[profile  two]\n[profile trail] ; c\n",
    "[DEFAULT]\nregion=x\n[DEFAULT]\n[profile a]\n",
    "[profile a]\n[profile a]\n",
    "[profile a]\nregion=x\nREGION=y\n",
    "[profile a]\nregion=x\n[profile b]\nregion=y\n",
    "region=x\n[profile a]\n",
    "# c\n; c\n\n[profile a]\n",
    "[profile a]\nno delimiter here\nalso bad\n",
    "[profile a]\n=value\n",
    "[profile a]\n=1\n=2\n",
    "[profile a]\nkey = v\n  [profile hidden]\n  more\n[profile b]\n",
    "[profile a]\n  indented = 1\n  [profile c]\n",
    "[profile a]\nk: v\nk2 :v\n",
    "[profile a]\nbad\n[profile a]\n",
    "\ufeff[profile a]\n",
    "  [profile indented-header]\n",
    "[profile a]\nk=v\n\n  continued\n[profile b]\n",
    "[profile a]\n# comment\n  # indented comment\nk=1\n",
    "[profile \u00e9]\n[profile \u0130]\nK\u0130=1\nk\u0130=2\n",
    "[profile a]\n\u00a0k=1\n",
    "[profile a]\nk=1\n\x1c\n",
]


def configparser_results():
    import configparser
    import io

    out = []
    for text in CONFIG_CORPUS:
        cp = configparser.ConfigParser()
        try:
            cp.read_file(io.StringIO(text, newline=None))
            out.append({"text": text, "sections": cp.sections()})
        except configparser.MissingSectionHeaderError as e:
            out.append({"text": text, "error": "MissingSectionHeader", "lineno": e.lineno})
        except configparser.DuplicateSectionError as e:
            out.append({"text": text, "error": "DuplicateSection", "section": e.section, "lineno": e.lineno})
        except configparser.DuplicateOptionError as e:
            out.append({"text": text, "error": "DuplicateOption", "section": e.section,
                        "option": e.option, "lineno": e.lineno})
        except configparser.ParsingError as e:
            out.append({"text": text, "error": "Parsing", "linenos": [n for n, _ in e.errors]})
    return out

# --------------------------------------------------------------------------
# Running
# --------------------------------------------------------------------------


def materialize(home: Path, c: dict) -> None:
    for rel, value in c["files"].items():
        p = home / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(value, dict) and "symlink" in value:
            os.symlink(value["symlink"], p)
        elif isinstance(value, dict) and value.get("dir"):
            p.mkdir(parents=True, exist_ok=True)
        elif isinstance(value, dict):
            p.write_bytes(base64.b64decode(value["b64"]))
        else:
            p.write_bytes(value.encode())
    for rel, mode in c["modes"].items():
        os.chmod(home / rel, int(mode, 8))


def manifest(home: Path) -> list[dict]:
    entries = []
    for dirpath, dirnames, filenames in os.walk(home):
        dirnames.sort()
        for name in sorted(dirnames + filenames):
            p = Path(dirpath) / name
            rel = str(p.relative_to(home))
            st = os.lstat(p)
            if stat.S_ISLNK(st.st_mode):
                entries.append({"path": rel, "type": "symlink", "target": os.readlink(p)})
            elif stat.S_ISDIR(st.st_mode):
                entries.append({"path": rel, "type": "dir", "mode": oct(st.st_mode & 0o7777)[2:]})
            else:
                data = p.read_bytes()
                try:
                    content = data.decode()
                except UnicodeDecodeError:
                    content = {"b64": base64.b64encode(data).decode()}
                entries.append(
                    {
                        "path": rel,
                        "type": "file",
                        "mode": oct(st.st_mode & 0o7777)[2:],
                        "content": content,
                    }
                )
    entries.sort(key=lambda e: e["path"])
    return entries


def wait_for_fresh_second() -> None:
    while time.time() % 1 > 0.2:
        time.sleep(0.01)


def run_case(c: dict) -> dict:
    with tempfile.TemporaryDirectory(prefix="awspick-golden-") as tmp:
        home = Path(tmp) / "home"
        home.mkdir()
        materialize(home, c)
        env = {k: v for k, v in c["env"].items() if v is not None}
        env["HOME"] = str(home)
        if "ZDOTDIR" in env:
            env["ZDOTDIR"] = str(home / env["ZDOTDIR"])
        if c["same_second"]:
            wait_for_fresh_second()
        results = []
        for r in c["runs"]:
            # sh is the parent, so the ps fallback sees the same process name
            # for Python and Rust.
            proc = subprocess.run(
                ["/bin/sh", "-c", '"$@"; exit $?', "sh", AWSPICK, *r["args"]],
                input=r["stdin"].encode(),
                capture_output=True,
                env=env,
                cwd=home,
            )
            results.append(
                {
                    "stdout": proc.stdout.decode(errors="replace").replace(str(home), "<HOME>"),
                    "stderr": proc.stderr.decode(errors="replace").replace(str(home), "<HOME>"),
                    "exit": proc.returncode,
                }
            )
        return {"runs": results, "files": manifest(home)}


def main() -> int:
    build_corpus()
    names = [c["name"] for c in CASES]
    assert len(names) == len(set(names)), "duplicate case names"
    (GOLDEN / "cases.json").write_text(json.dumps(CASES, indent=1, ensure_ascii=False) + "\n")
    expected = GOLDEN / "expected"
    if expected.exists():
        shutil.rmtree(expected)
    expected.mkdir(parents=True)
    for c in CASES:
        result = run_case(c)
        (expected / f"{c['name']}.json").write_text(
            json.dumps(result, indent=1, ensure_ascii=False) + "\n"
        )
    (GOLDEN / "configparser.json").write_text(
        json.dumps(configparser_results(), indent=1, ensure_ascii=False) + "\n"
    )
    print(f"wrote {len(CASES)} cases to {GOLDEN.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
