"""
AWS Pick - A simple CLI tool to easily switch between AWS profiles in your shell environment.

This package provides functionality to list, select, and set AWS profiles
by modifying the AWS_PROFILE environment variable in your shell configuration.
"""

from importlib.metadata import PackageNotFoundError, version

# Read from the installed distribution. A second hardcoded copy only drifts:
# prjump shipped announcing the wrong number that way.
try:
    __version__ = version("aws-profile-pick")
except PackageNotFoundError:  # running from a source tree with nothing installed
    __version__ = "0.0.0+unknown"
