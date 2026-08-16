"""Command-line interface for AWS Pick.

This module provides the main CLI functionality for the AWS Pick tool,
including user interaction, profile selection, and command execution.
"""

import argparse
import logging
import os
import sys
from typing import List, Optional

from awspick.config import (
    display_profiles,
    get_grouped_profiles,
    read_aws_profiles,
    validate_profile_selection,
)
from awspick.shell import (
    detect_shell,
    generate_export_command,
    get_current_profile,
    get_rc_path,
    update_aws_profile,
    write_shared_profile,
)

# Configure logging
logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
logger = logging.getLogger(__name__)


def parse_args(argv: Optional[List[str]] = None) -> argparse.Namespace:
    """Parse command-line arguments."""

    parser = argparse.ArgumentParser(description="AWS profile picker")
    # Deprecated option kept for backward compatibility but ignored
    parser.add_argument(
        "--export-command",
        action="store_true",
        help=argparse.SUPPRESS,
    )
    # Filtering and grouping options
    parser.add_argument(
        "-f",
        "--filter",
        action="append",
        help="Only show profiles that match any of these substrings (can repeat or comma-separate)",
    )
    parser.add_argument(
        "-x",
        "--exclude",
        action="append",
        help="Exclude profiles that match any of these substrings (can repeat or comma-separate)",
    )
    parser.add_argument(
        "-g",
        "--groups",
        help="Comma-separated group names to display (e.g., prod,dev)",
    )
    parser.add_argument(
        "--group-rules",
        help=(
            "Custom group rules, e.g. 'preprod=preprod;prod=prod,production;dev=dev' (order matters)"
        ),
    )
    parser.add_argument(
        "--regex",
        action="store_true",
        help="Treat filter/exclude as regular expressions",
    )
    parser.add_argument(
        "--case-sensitive",
        action="store_true",
        help="Make filter/exclude matching case-sensitive",
    )
    return parser.parse_args(argv)


def get_profile_selection(profiles: List[str]) -> Optional[str]:
    """
    Prompt the user to select a profile and validate the input.

    Args:
        profiles (List[str]): List of available profiles

    Returns:
        Optional[str]: Selected profile name or None if cancelled

    Note:
        The function will continue to prompt until a valid selection is made
        or the user explicitly cancels the operation.
    """
    while True:
        try:
            print("Enter profile number or name: ", end="", file=sys.stderr, flush=True)
            selection = input().strip()

            if not selection:
                print("No selection made.", file=sys.stderr)
                logger.info("Empty selection")
                return None

            # Allow user to cancel
            if selection.lower() in ("q", "quit", "exit"):
                logger.info("User cancelled profile selection")
                return None

            profile = validate_profile_selection(selection, profiles)
            if profile:
                return profile

            print(
                "Invalid selection. Please enter a valid profile number or name.",
                file=sys.stderr,
            )
        except KeyboardInterrupt:
            print("\nOperation cancelled.", file=sys.stderr)
            logger.info("Profile selection cancelled via keyboard interrupt")
            return None


def main(argv: Optional[List[str]] = None) -> int:
    """
    Main entry point for the CLI.

    Returns:
        int: Exit code (0 for success, 1 for failure)

    This function orchestrates the entire AWS profile switching process:
    1. Reads available AWS profiles from config
    2. Displays profiles to the user
    3. Gets user selection
    4. Updates shell configuration with the selected profile
    """

    try:
        # Read AWS profiles
        profiles = read_aws_profiles()
        if not profiles:
            logger.error("No AWS profiles found. Please check your AWS configuration.")
            return 1

        # Parse args and env for filtering
        args = parse_args(argv)

        def _split_csv_many(values: Optional[List[str]]) -> List[str]:
            parts: List[str] = []
            if not values:
                return parts
            for v in values:
                if not v:
                    continue
                parts.extend([p.strip() for p in v.split(",") if p.strip()])
            return parts

        env_filter = os.environ.get("AWSPICK_FILTER")
        env_exclude = os.environ.get("AWSPICK_EXCLUDE")
        env_groups = os.environ.get("AWSPICK_GROUPS_SHOW")
        env_rules = os.environ.get("AWSPICK_GROUP_RULES")
        env_regex = os.environ.get("AWSPICK_REGEX", "0").lower() in {"1", "true", "yes"}
        env_case = os.environ.get("AWSPICK_CASE_SENSITIVE", "0").lower() in {
            "1",
            "true",
            "yes",
        }

        include_patterns = _split_csv_many(args.filter) or (
            [p.strip() for p in env_filter.split(",")] if env_filter else []
        )
        exclude_patterns = _split_csv_many(args.exclude) or (
            [p.strip() for p in env_exclude.split(",")] if env_exclude else []
        )
        groups_to_show = (
            [p.strip() for p in args.groups.split(",") if p.strip()]
            if args and args.groups
            else ([p.strip() for p in env_groups.split(",")] if env_groups else None)
        )
        group_rules = args.group_rules if args and args.group_rules else env_rules
        use_regex = args.regex or env_regex
        case_sensitive = args.case_sensitive or env_case

        # Group and sort profiles for display, with optional filters
        grouped_profiles = get_grouped_profiles(
            profiles,
            group_rules=group_rules,
            show_groups=groups_to_show,
            include=include_patterns or None,
            exclude=exclude_patterns or None,
            regex=use_regex,
            case_sensitive=case_sensitive,
        )
        current_profile = get_current_profile()
        display_profiles(grouped_profiles, current_profile=current_profile)

        # The list of profiles for selection must match the display order
        selection_profiles = [p[0] for p in grouped_profiles]

        # Get user selection
        profile = get_profile_selection(selection_profiles)
        if not profile:
            logger.info("No profile selected, exiting")
            return 1

        print(f"Selected profile: {profile}", file=sys.stderr)

        # Detect shell and get RC path
        shell_name = detect_shell()
        rc_path, shell_config = get_rc_path(shell_name)

        # Update shell configuration
        success, backup_path = update_aws_profile(profile, shell_name)
        if not success:
            logger.error("Failed to update AWS profile.")
            return 1

        if backup_path:
            print(f"Backup created at {backup_path}", file=sys.stderr)

        print(f"Updated {rc_path} with AWS_PROFILE={profile}", file=sys.stderr)

        shared_path = write_shared_profile(profile)
        if shared_path:
            print(f"Updated shared profile at {shared_path}", file=sys.stderr)
        else:
            print(
                "Warning: failed to write shared profile file; other shells may not sync.",
                file=sys.stderr,
            )

        export_cmd = generate_export_command(profile, shell_name)

        # Always print the export command so the user can eval the output
        print(export_cmd)
        print(
            "Run 'eval \"$(awspick)\"' to apply in the current shell",
            file=sys.stderr,
        )

        return 0

    except Exception as e:
        logger.error(f"An unexpected error occurred: {e}", exc_info=True)
        return 1


if __name__ == "__main__":
    sys.exit(main())
