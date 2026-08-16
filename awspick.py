#!/usr/bin/env python3
"""
AWS Pick - A simple CLI tool to easily switch between AWS profiles in your shell environment.

This is a launcher script that calls the main function from the awspick package.
It provides a convenient way to run the tool without installing it as a package.

Usage:
    ./awspick.py
"""

import sys

from awspick.cli import main

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
