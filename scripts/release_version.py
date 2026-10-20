#!/usr/bin/env python3
"""Product release spelling and its explicit bundled-qrate base version."""

import argparse
import re

PATTERN = r"(?P<base>(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*))(?P<beta>-beta(?:\.[1-9][0-9]*)?)?"


def qrate_version(product):
    match = re.fullmatch(PATTERN, product) if isinstance(product, str) else None
    if match is None:
        raise ValueError("Expected canonical MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-beta[.N] (N >= 1)")
    return match["base"]


def is_beta(product):
    qrate_version(product)
    return "-" in product


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    value = parser.parse_args().version
    try:
        print(qrate_version(value))
    except ValueError as error:
        parser.error(str(error))
