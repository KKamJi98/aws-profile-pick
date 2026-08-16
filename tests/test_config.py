"""Tests for the config module."""

import sys
from pathlib import Path
from unittest.mock import MagicMock, patch


from awspick.config import (
    display_profiles,
    get_aws_config_path,
    get_grouped_profiles,
    read_aws_profiles,
    validate_profile_selection,
)


def test_get_aws_config_path():
    """Test getting the AWS config path."""
    expected_path = Path.home() / ".aws" / "config"
    assert get_aws_config_path() == expected_path


@patch("awspick.config.get_aws_config_path")
@patch("configparser.ConfigParser")
def test_read_aws_profiles(mock_configparser, mock_get_path):
    """Test reading AWS profiles from config."""
    # Setup mock
    mock_config = MagicMock()
    mock_config.sections.return_value = ["default", "profile dev", "profile prod"]
    mock_configparser.return_value = mock_config

    mock_path = MagicMock()
    mock_path.exists.return_value = True
    mock_get_path.return_value = mock_path

    # Call function
    profiles = read_aws_profiles()

    # Assertions
    assert profiles == ["default", "dev", "prod"]
    mock_config.read.assert_called_once_with(mock_path)


@patch("awspick.config.get_aws_config_path")
def test_read_aws_profiles_no_config(mock_get_path):
    """Test reading AWS profiles when config doesn't exist."""
    # Setup mock
    mock_path = MagicMock()
    mock_path.exists.return_value = False
    mock_get_path.return_value = mock_path

    # Call function
    profiles = read_aws_profiles()

    # Assertions
    assert profiles == []


@patch("awspick.config.Console")
def test_display_profiles(mock_console_class):
    """Test displaying profiles using rich."""
    mock_console = MagicMock()
    mock_console_class.return_value = mock_console

    grouped_profiles = [("dev", "dev"), ("prod", "prod"), ("default", "others")]
    display_profiles(grouped_profiles)

    mock_console_class.assert_called_once_with(file=sys.stderr)
    mock_console.print.assert_called_once()


@patch("awspick.config.Console")
def test_display_profiles_empty(mock_console_class):
    """Test displaying profiles when no profiles are found."""
    mock_console = MagicMock()
    mock_console_class.return_value = mock_console

    display_profiles([])

    mock_console_class.assert_called_once_with(file=sys.stderr)
    mock_console.print.assert_called_once_with(
        "[bold red]No AWS profiles found in ~/.aws/config[/bold red]"
    )


def test_validate_profile_selection():
    """Test validating profile selection."""
    profiles = ["default", "dev", "prod"]

    # Test valid number
    assert validate_profile_selection("1", profiles) == "default"
    assert validate_profile_selection("2", profiles) == "dev"
    assert validate_profile_selection("3", profiles) == "prod"

    # Test valid name
    assert validate_profile_selection("default", profiles) == "default"
    assert validate_profile_selection("dev", profiles) == "dev"
    assert validate_profile_selection("prod", profiles) == "prod"

    # Test invalid number
    assert validate_profile_selection("0", profiles) is None
    assert validate_profile_selection("4", profiles) is None
    assert validate_profile_selection("999", profiles) is None

    # Test invalid name
    assert validate_profile_selection("staging", profiles) is None

    # Test empty input
    assert validate_profile_selection("", profiles) is None


def test_get_grouped_profiles_preprod_vs_prod():
    """preprod가 prod로 오분류되지 않아야 한다."""
    profiles = ["dev-main", "preprod-main", "prod-main", "misc"]
    grouped = dict(get_grouped_profiles(profiles))

    assert grouped["preprod-main"] == "preprod"
    assert grouped["prod-main"] == "prod"
    assert grouped["dev-main"] == "dev"
    assert grouped["misc"] == "others"
