#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Sergio Martins
# SPDX-License-Identifier: MIT

"""Writes crates/rc_core/data/emojione.tsv from the emojione npm package.

Rocket.Chat validates and names reactions with emojione 4.5.0 (the latest release), not with gemoji,
which is what the `emojis` crate knows: about a third of gemoji's shortcodes are refused by the
server, and the same emoji has other canonical names. So the picker sends emojione's names.

One line per entry of `emojioneList` (skin tones included, they are needed to show a reaction):
canonical shortname, glyph, alternate shortnames (comma-separated), category. The glyph is
`uc_output`. Only the non-artwork files of the package are used, which are MIT.

Usage: scripts/update_emojione.py [output]
"""

import json
import re
import sys
import urllib.request

VERSION = "4.5.0"
URL = f"https://unpkg.com/emojione@{VERSION}/lib/js/emojione.js"
DEFAULT_OUTPUT = "crates/rc_core/data/emojione.tsv"


def main() -> None:
    output = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_OUTPUT
    with urllib.request.urlopen(URL) as response:
        source = response.read().decode("utf-8")
    found = re.search(r"emojioneList\s*=\s*(\{.*?\});", source, re.S)
    if not found:
        sys.exit("emojioneList not found")
    entries = json.loads(found.group(1))
    with open(output, "w", encoding="utf-8", newline="\n") as out:
        for name, entry in entries.items():
            glyph = "".join(chr(int(code, 16)) for code in entry["uc_output"].split("-"))
            out.write(f"{name}\t{glyph}\t{','.join(entry['shortnames'])}\t{entry['category']}\n")
    print(f"{len(entries)} emoji written to {output}")


if __name__ == "__main__":
    main()
