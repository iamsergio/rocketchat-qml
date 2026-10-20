# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

"""Turns the summary of `llvm-cov export -summary-only` into a markdown table: one row per crate
with its total, then the files of each crate. Used by scripts/coverage.sh.

Usage: coverage_summary.py summary.json source_root
"""

import json
import sys
from collections import defaultdict
from pathlib import Path


def percent(counts):
    return f"{100 * counts['covered'] / counts['count']:.1f}%" if counts["count"] else "-"


def row(name, summary):
    lines, functions, regions = (summary[k] for k in ("lines", "functions", "regions"))
    return (
        f"| {name} | {percent(lines)} | {lines['covered']} / {lines['count']} "
        f"| {percent(functions)} | {percent(regions)} |"
    )


def add(total, summary):
    for kind in ("lines", "functions", "regions"):
        for key in ("count", "covered"):
            total[kind][key] += summary[kind][key]


def main():
    data = json.loads(Path(sys.argv[1]).read_text())["data"][0]
    root = Path(sys.argv[2])
    crates = defaultdict(list)
    totals = defaultdict(lambda: {k: {"count": 0, "covered": 0} for k in ("lines", "functions", "regions")})
    for entry in data["files"]:
        path = Path(entry["filename"]).relative_to(root)
        crate = path.parts[1]
        crates[crate].append((str(Path(*path.parts[2:])), entry["summary"]))
        add(totals[crate], entry["summary"])
        add(totals[""], entry["summary"])

    header = "| | Lines | Lines covered | Functions | Regions |\n|---|---:|---:|---:|---:|"
    print("## Rust code coverage\n")
    print(header)
    print(row("**Total**", totals[""]))
    for crate in sorted(crates):
        print(row(f"**{crate}**", totals[crate]))
    for crate in sorted(crates):
        print(f"\n<details><summary>{crate}</summary>\n\n{header}")
        for name, summary in sorted(crates[crate]):
            print(row(f"`{name}`", summary))
        print("\n</details>")


main()
