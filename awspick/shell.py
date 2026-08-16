"""
Shell profile modification module.

This module handles updating the shell configuration files
(~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish, etc.)
to set the AWS_PROFILE environment variable for the selected profile.
"""

import datetime
import logging
import os
import re
import shutil
import subprocess
from pathlib import Path
from typing import Dict, Optional, Tuple

logger = logging.getLogger(__name__)
BACKUP_RETENTION_COUNT = 2


class ShellConfig:
    """Shell configuration class to handle different shell types."""

    def __init__(self, name: str, rc_path: Path, export_format: str):
        """
        Initialize a shell configuration.

        Args:
            name (str): Name of the shell (e.g., "bash", "zsh", "fish")
            rc_path (Path): Path to the shell's rc file
            export_format (str): Format string for exporting variables in this shell
        """
        self.name = name
        self.rc_path = rc_path
        self.export_format = export_format

    def get_profile_line(self, profile_name: str) -> str:
        """
        Get the line to add to the shell config file.

        Args:
            profile_name (str): AWS profile name to set

        Returns:
            str: Formatted line for setting AWS_PROFILE
        """
        return self.export_format.format(profile_name=profile_name)

    def get_profile_pattern(self) -> re.Pattern:
        """
        Get the regex pattern to match AWS_PROFILE in this shell's syntax.

        Returns:
            re.Pattern: Compiled regex pattern
        """
        if self.name == "fish":
            return re.compile(
                r'^set -[gx] AWS_PROFILE\s+["\']?(.+?)["\']?\s*$', re.MULTILINE
            )
        else:
            return re.compile(r"^export\s+AWS_PROFILE=(.+)$", re.MULTILINE)


def detect_shell() -> str:
    """
    Detect the current shell.

    Returns:
        str: Name of the current shell (e.g., "bash", "zsh", "fish")
    """
    # Try to get from SHELL environment variable
    shell_path = os.environ.get("SHELL", "")
    if shell_path:
        shell_name = os.path.basename(shell_path)
        logger.info(f"Detected shell from SHELL env var: {shell_name}")
        return shell_name

    # Fallback to process inspection
    try:
        # Get the parent process name
        result = subprocess.run(
            ["ps", "-p", str(os.getppid()), "-o", "comm="],
            capture_output=True,
            text=True,
            check=True,
        )
        shell_name = result.stdout.strip()
        if "/" in shell_name:
            shell_name = os.path.basename(shell_name)
        logger.info(f"Detected shell from parent process: {shell_name}")
        return shell_name
    except Exception as e:
        logger.warning(f"Failed to detect shell: {e}")
        # Default to bash as fallback
        return "bash"


def get_shell_configs() -> Dict[str, ShellConfig]:
    """
    Get configurations for supported shells.

    Returns:
        Dict[str, ShellConfig]: Dictionary of shell configurations
    """
    home = Path.home()
    return {
        "bash": ShellConfig(
            "bash", home / ".bashrc", 'export AWS_PROFILE="{profile_name}"'
        ),
        "zsh": ShellConfig(
            "zsh", home / ".zshrc", 'export AWS_PROFILE="{profile_name}"'
        ),
        "fish": ShellConfig(
            "fish",
            home / ".config" / "fish" / "config.fish",
            'set -gx AWS_PROFILE "{profile_name}"',
        ),
        # Add more shells as needed
    }


def get_rc_path(shell_name: str = None) -> Tuple[Path, ShellConfig]:
    """
    Get the path to the shell's rc file.

    Args:
        shell_name (str, optional): Shell name. If None, auto-detect.

    Returns:
        Tuple[Path, ShellConfig]: Path to the rc file and shell config
    """
    if shell_name is None:
        shell_name = detect_shell()

    shell_configs = get_shell_configs()

    # Normalize shell name (remove version numbers, etc.)
    normalized_name = shell_name.lower()
    for name in shell_configs:
        if normalized_name.startswith(name):
            shell_config = shell_configs[name]
            logger.info(f"Using {name} configuration at {shell_config.rc_path}")
            return shell_config.rc_path, shell_config

    # Fallback to bash if shell not recognized
    logger.warning(f"Shell '{shell_name}' not recognized, falling back to bash")
    return shell_configs["bash"].rc_path, shell_configs["bash"]


def get_shared_profile_path() -> Path:
    """
    Get the shared profile path used for cross-shell synchronization.

    Returns:
        Path: Path to the shared profile file
    """
    return Path.home() / ".config" / "awspick" / "profile"


def write_shared_profile(profile_name: str) -> Optional[Path]:
    """
    Write the selected profile to a shared file for cross-shell sync.

    Args:
        profile_name (str): AWS profile name to set

    Returns:
        Optional[Path]: Path to the shared profile file if successful, otherwise None
    """
    shared_path = get_shared_profile_path()
    try:
        shared_path.parent.mkdir(parents=True, exist_ok=True)
        tmp_path = shared_path.with_suffix(".tmp")
        with open(tmp_path, "w") as f:
            f.write(f"{profile_name}\n")
        os.replace(tmp_path, shared_path)
        logger.info(f"Wrote shared profile to {shared_path}")
        return shared_path
    except Exception as e:
        logger.error(f"Failed to write shared profile file: {e}", exc_info=True)
        return None


def backup_rc_file(rc_path: Path) -> Path:
    """
    Create a backup of the shell rc file.

    Args:
        rc_path (Path): Path to the rc file

    Returns:
        Path: Path to the backup file

    Note:
        Creates a timestamped backup with format ~/.rcfile.bak-YYYYMMDDHHMMSS
        Preserves file permissions and metadata using shutil.copy2
    """
    try:
        timestamp = datetime.datetime.now().strftime("%Y%m%d%H%M%S")
        backup_path = Path(f"{rc_path}.bak-{timestamp}")

        shutil.copy2(rc_path, backup_path)
        logger.info(f"Backup created at {backup_path}")

        # Rotate old backups, keep only BACKUP_RETENTION_COUNT
        backups = sorted(rc_path.parent.glob(f"{rc_path.name}.bak-*"))
        if len(backups) > BACKUP_RETENTION_COUNT:
            for old_backup in backups[:-BACKUP_RETENTION_COUNT]:
                try:
                    old_backup.unlink()
                    logger.info(f"Removed old backup {old_backup}")
                except OSError as e:
                    logger.warning(f"Failed to remove old backup {old_backup}: {e}")

        return backup_path
    except Exception as e:
        logger.error(f"Failed to create backup: {e}")
        raise


def get_current_profile(shell_name: str = None) -> Optional[str]:
    """
    Get the current AWS profile from the environment or shell rc file.

    Args:
        shell_name (str, optional): Shell name. If None, auto-detect.

    Returns:
        Optional[str]: Current AWS profile name if found
    """
    env_profile = os.environ.get("AWS_PROFILE")
    if env_profile:
        env_profile = env_profile.strip()
        if env_profile:
            logger.info("Using AWS_PROFILE from environment")
            return env_profile

    rc_path, shell_config = get_rc_path(shell_name)
    if not rc_path.exists():
        return None

    try:
        with open(rc_path, "r") as f:
            content = f.read()

        match = shell_config.get_profile_pattern().search(content)
        if not match:
            return None

        profile = match.group(1).strip().strip("\"'")
        return profile or None
    except Exception as e:
        logger.warning(f"Failed to read current AWS_PROFILE from {rc_path}: {e}")
        return None


def update_aws_profile(
    profile_name: str, shell_name: str = None
) -> Tuple[bool, Optional[Path]]:
    """
    Update the AWS_PROFILE environment variable in the shell rc file.

    Args:
        profile_name (str): AWS profile name to set
        shell_name (str, optional): Shell name. If None, auto-detect.

    Returns:
        Tuple[bool, Optional[Path]]: Success status and backup path if created

    Note:
        - Returns (True, None) if profile is already set (no changes made)
        - Returns (True, Path) if profile was updated successfully
        - Returns (False, None) if an error occurred

    This function ensures idempotency by checking if the profile is already set
    before making any changes to the rc file.
    """
    rc_path, shell_config = get_rc_path(shell_name)

    if not rc_path.exists():
        # Create parent directories if they don't exist (especially for fish)
        if shell_config.name == "fish":
            rc_path.parent.mkdir(parents=True, exist_ok=True)
            with open(rc_path, "w") as f:
                f.write("# Created by AWS Pick\n\n")
            logger.info(f"Created new config file at {rc_path}")
        else:
            logger.error(f"Shell config file not found at {rc_path}")
            return False, None

    try:
        # Read the current content
        with open(rc_path, "r") as f:
            content = f.read()

        # Check if AWS_PROFILE is already set to the same value
        aws_profile_pattern = shell_config.get_profile_pattern()
        match = aws_profile_pattern.search(content)

        # Extract current profile value if it exists
        current_profile = None
        if match:
            current_profile = match.group(1).strip("\"'")

        if current_profile == profile_name:
            logger.info(f"AWS_PROFILE already set to {profile_name}, no changes needed")
            return True, None

        # Create backup
        backup_path = backup_rc_file(rc_path)

        # Update or add AWS_PROFILE
        if match:
            # Replace existing AWS_PROFILE
            new_content = aws_profile_pattern.sub(
                shell_config.get_profile_line(profile_name), content
            )
            logger.info(
                f"Replacing existing AWS_PROFILE={current_profile} with {profile_name}"
            )
        else:
            # Add AWS_PROFILE at the end
            new_content = (
                content.rstrip()
                + f"\n\n# Added by AWS Pick\n{shell_config.get_profile_line(profile_name)}\n"
            )
            logger.info(f"Adding new AWS_PROFILE={profile_name} entry")

        # Write the updated content
        with open(rc_path, "w") as f:
            f.write(new_content)

        logger.info(f"Successfully updated {rc_path} with AWS_PROFILE={profile_name}")
        return True, backup_path

    except Exception as e:
        logger.error(f"Failed to update AWS profile: {e}", exc_info=True)
        return False, None


def source_rc_file(shell_config: ShellConfig) -> bool:
    """Source the rc file for the detected shell."""

    try:
        subprocess.run(
            [shell_config.name, "-c", f"source {shell_config.rc_path}"],
            check=True,
        )
        logger.info(f"Sourced {shell_config.rc_path} using {shell_config.name}")
        return True
    except Exception as e:
        logger.error(f"Failed to source rc file: {e}")
        return False


def generate_export_command(profile_name: str, shell_name: str = None) -> str:
    """Return the shell command to export AWS_PROFILE for the given shell."""

    _, shell_config = get_rc_path(shell_name)
    return shell_config.get_profile_line(profile_name)
