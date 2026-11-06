"""Record golden cases whose Rust output deliberately differs from 2.0.2.

usage: python3 scripts/golden-change.py "<reason>" <case> [<case> ...]

Adds the cases to tests/golden/changes.json with the reason. Then run
`GOLDEN_BLESS=1 cargo test --test golden` to pin their current output.
"""

import json
import sys
from pathlib import Path

path = Path(__file__).resolve().parent.parent / "tests" / "golden" / "changes.json"
reason, cases = sys.argv[1], sys.argv[2:]
data = json.loads(path.read_text())
for case in cases:
    data[case] = f"{data[case]}; {reason}" if case in data and reason not in data[case] else reason
path.write_text(json.dumps(dict(sorted(data.items())), indent=1, ensure_ascii=False) + "\n")
