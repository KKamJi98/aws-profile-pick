"""
AWS config file parsing module.

This module handles reading and parsing AWS configuration files,
displaying available profiles, and validating user selections.
"""

import configparser
import logging
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from rich.console import Console
from rich.table import Table

logger = logging.getLogger(__name__)


def get_aws_config_path() -> Path:
    """
    Get the path to the AWS config file.

    Returns:
        Path: Path to the AWS config file (~/.aws/config)
    """
    return Path.home() / ".aws" / "config"


def read_aws_profiles() -> List[str]:
    """
    Read AWS profiles from the config file.

    Returns:
        List[str]: List of profile names sorted alphabetically

    Note:
        Returns an empty list if the config file doesn't exist or has no profiles.
        Handles both default profile and named profiles with "profile " prefix.
    """
    config_path = get_aws_config_path()
    if not config_path.exists():
        logger.error(f"AWS config file not found at {config_path}")
        return []

    try:
        config = configparser.ConfigParser()
        config.read(config_path)

        profiles = []
        for section in config.sections():
            # AWS config uses "profile name" format for non-default profiles
            if section == "default":
                profiles.append("default")
            elif section.startswith("profile "):
                profile_name = section[len("profile ") :]
                profiles.append(profile_name)

        logger.info(f"Found {len(profiles)} AWS profiles")
        return sorted(profiles)
    except configparser.Error as e:
        logger.error(f"Error parsing AWS config file: {e}")
        return []
    except Exception as e:
        logger.error(f"Unexpected error reading AWS profiles: {e}")
        return []


def _parse_group_rules(rules: Optional[str]) -> List[Tuple[str, List[str]]]:
    """Parse group rules string into ordered mapping.

    Format examples:
    - "preprod=preprod;prod=prod;stg=stg;dev=dev" (default order)
    - "prod=prod,production;dev=dev" (keywords list)
    - "others;tf=tf" (positional others: others on top, tf below)
    - "main=*;tf=tf" (renamed catch-all via wildcard)

    Special tokens:
    - ``others`` without ``=``: positional marker for unmatched profiles.
    - ``*`` keyword: catch-all wildcard (allows renaming the default group).

    Returns an ordered list of tuples [(group, [keywords...])].
    """
    if not rules:
        return [
            ("prod", ["prod"]),
            ("stg", ["stg"]),
            ("dev", ["dev"]),
            ("preprod", ["preprod"]),
        ]

    ordered: List[Tuple[str, List[str]]] = []
    for part in rules.split(";"):
        part = part.strip()
        if not part:
            continue
        if "=" in part:
            name, kws = part.split("=", 1)
            name = name.strip()
            keywords = [k.strip() for k in kws.split(",") if k.strip()]
        elif part.strip().lower() == "others":
            # Bare "others" → positional marker (no keywords, acts as catch-all)
            name = "others"
            keywords = ["*"]
        else:
            name = part
            keywords = [part]
        if name:
            ordered.append((name, keywords))
    return ordered


def _match_any(
    text: str, patterns: List[str], *, regex: bool, case_sensitive: bool
) -> bool:
    import re as _re

    flags = 0 if case_sensitive else _re.IGNORECASE
    if regex:
        return any(_re.search(p, text, flags=flags) is not None for p in patterns)
    if not case_sensitive:
        text = text.lower()
        patterns = [p.lower() for p in patterns]
    return any(p in text for p in patterns)


def _match_group_keyword(text: str, keyword: str) -> bool:
    """Match group keyword on token boundaries to avoid partial collisions.

    Examples:
    - Matches: "foo-prod", "prod", "prod-bar", "bar_prod" (prod as token)
    - Does not match: "preprod", "production" (prod is a substring only)
    """
    import re as _re

    pattern = rf"(^|\b|[-_]){_re.escape(keyword)}($|\b|[-_])"
    return _re.search(pattern, text, flags=_re.IGNORECASE) is not None


def filter_profiles(
    profiles: List[str],
    *,
    include: Optional[List[str]] = None,
    exclude: Optional[List[str]] = None,
    regex: bool = False,
    case_sensitive: bool = False,
) -> List[str]:
    """Filter profiles by include/exclude patterns.

    - include: only keep profiles matching any pattern
    - exclude: drop profiles matching any pattern
    """
    result = list(profiles)
    if include:
        result = [
            p
            for p in result
            if _match_any(p, include, regex=regex, case_sensitive=case_sensitive)
        ]
    if exclude:
        result = [
            p
            for p in result
            if not _match_any(p, exclude, regex=regex, case_sensitive=case_sensitive)
        ]
    return result


def get_grouped_profiles(
    profiles: List[str],
    *,
    group_rules: Optional[str] = None,
    show_groups: Optional[List[str]] = None,
    include: Optional[List[str]] = None,
    exclude: Optional[List[str]] = None,
    regex: bool = False,
    case_sensitive: bool = False,
) -> List[Tuple[str, str]]:
    """
    Group profiles by user-defined rules and return list of (profile, group).

    Args:
        profiles: List of profile names
        group_rules: Rules string (e.g., "prod=prod;dev=dev"). Order matters.
        show_groups: If provided, only include these group names.
        include: Include-only patterns (substring or regex)
        exclude: Exclude patterns (substring or regex)
        regex: Treat include/exclude as regex if True
        case_sensitive: Case sensitivity for matching
    """
    # Apply include/exclude filtering first
    filtered = filter_profiles(
        profiles,
        include=include,
        exclude=exclude,
        regex=regex,
        case_sensitive=case_sensitive,
    )

    ordered_rules = _parse_group_rules(group_rules)
    group_order = [name for name, _ in ordered_rules]

    # Separate explicit keyword rules from catch-all (*) rule.
    # Explicit rules are matched first; unmatched profiles go to catch-all.
    explicit_rules = [(n, kws) for n, kws in ordered_rules if "*" not in kws]
    catchall_name = next((n for n, kws in ordered_rules if "*" in kws), "others")

    groups: Dict[str, List[str]] = {name: [] for name in group_order}
    groups.setdefault(catchall_name, [])

    for profile in filtered:
        assigned = False
        for name, keywords in explicit_rules:
            if any(_match_group_keyword(profile, kw) for kw in keywords):
                groups[name].append(profile)
                assigned = True
                break
        if not assigned:
            groups[catchall_name].append(profile)

    # Limit to requested groups if specified
    allowed_groups = set(g.strip() for g in show_groups) if show_groups else None

    # Ensure catch-all group is in display order
    if catchall_name not in group_order:
        group_order.append(catchall_name)

    grouped_profiles: List[Tuple[str, str]] = []
    for grp in group_order:
        if allowed_groups is not None and grp not in allowed_groups:
            continue
        for profile in sorted(groups[grp]):
            grouped_profiles.append((profile, grp))
    return grouped_profiles


def display_profiles(
    grouped_profiles: List[Tuple[str, str]],
    current_profile: Optional[str] = None,
) -> None:
    """
    Display available AWS profiles in a tabulated format using rich.

    Args:
        grouped_profiles (List[Tuple[str, str]]): List of (profile, group) tuples
        current_profile (Optional[str]): Current AWS profile name, if available
    """
    console = Console(file=sys.stderr)

    if not grouped_profiles:
        console.print("[bold red]No AWS profiles found in ~/.aws/config[/bold red]")
        return

    group_colors = {
        "prod": "bold red",
        "preprod": "bold green",
        "stg": "bold orange3",
        "dev": "bold blue",
        "others": "bold white",
    }

    table = Table(title="AWS Profiles", style="bold blue")
    table.add_column("No.", style="cyan", justify="right")
    table.add_column("Profile", style="white")
    table.add_column("Group", style="white")
    table.add_column("Current", style="white", justify="center")

    current_profile_norm = current_profile.lower().strip() if current_profile else None

    prev_group = None
    for i, (profile, group_name) in enumerate(grouped_profiles):
        if prev_group is not None and group_name != prev_group:
            table.add_section()
        prev_group = group_name
        color = group_colors.get(group_name, "white")
        is_current = (
            current_profile_norm is not None and profile.lower() == current_profile_norm
        )
        current_marker = "[bold green]*[/bold green]" if is_current else ""
        table.add_row(
            str(i + 1),
            profile,
            f"[{color}]{group_name}[/{color}]",
            current_marker,
        )

    console.print(table)


def validate_profile_selection(selection: str, profiles: List[str]) -> Optional[str]:
    """
    Validate the user's profile selection.

    Args:
        selection (str): User input (number or profile name)
        profiles (List[str]): List of available profiles

    Returns:
        Optional[str]: Selected profile name or None if invalid

    Note:
        Handles both numeric selection (by index) and direct profile name input.
        Returns None for any invalid input with appropriate error logging.
    """
    if not selection:
        logger.error("Empty selection")
        return None

    # Check if selection is a number
    if selection.isdigit():
        index = int(selection) - 1
        if 0 <= index < len(profiles):
            return profiles[index]
        else:
            logger.error(
                f"Invalid profile number: {selection}. Valid range is 1-{len(profiles)}"
            )
            return None

    # Check if selection is a profile name
    if selection in profiles:
        return selection

    # Check for case-insensitive match as a fallback
    for profile in profiles:
        if profile.lower() == selection.lower():
            logger.info(f"Found case-insensitive match for '{selection}': '{profile}'")
            return profile

    logger.error(f"Profile '{selection}' not found in available profiles")
    return None
