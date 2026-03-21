#!/usr/bin/env python3
"""
Prepare the BGL supercomputer log dataset for fishy.

Downloads the dataset, splits lines by anomaly label and node rack,
and writes one log file per rack into baseline/ and test/ directories.

Usage:
    python3 scripts/prep_bgl.py           # full dataset (~708 MB download)
    python3 scripts/prep_bgl.py --sample  # 2k-line sample (fast, no big download)

Output:
    data/bgl/baseline/<RACK>.log   — normal lines (label == '-')
    data/bgl/test/<RACK>.log       — anomalous lines (label != '-')

Then encode and run fishy:
    cargo run --bin encoder -- build-dict data/bgl/baseline/ -o data/bgl/dict.json --format bgl
    cargo run --bin encoder -- encode data/bgl/baseline/ --dict data/bgl/dict.json -o data/bgl/collections/baseline/ --format bgl
    cargo run --bin encoder -- encode data/bgl/test/     --dict data/bgl/dict.json -o data/bgl/collections/test/     --format bgl
    cargo run --bin fishy   -- -b data/bgl/collections/baseline/ -c data/bgl/collections/test/
"""

import os
import re
import sys
import zipfile
import urllib.request
from collections import defaultdict

FULL_URL   = "https://zenodo.org/records/8196385/files/BGL.zip?download=1"
SAMPLE_URL = "https://raw.githubusercontent.com/logpai/loghub/master/BGL/BGL_2k.log"
ZIP_PATH   = "data/bgl/BGL.zip"

# Minimum events a rack must have in BOTH sets to be included.
# Fishy's spectral/co-occurrence methods need ≥32 events per source.
MIN_EVENTS = 32


def rack_of(node: str) -> str:
    m = re.match(r"^(R\d+)", node)
    return m.group(1) if m else "OTHER"


def load_lines(sample: bool) -> list[str]:
    if sample:
        print("Downloading 2k sample from loghub…")
        with urllib.request.urlopen(SAMPLE_URL) as r:
            return r.read().decode(errors="replace").splitlines()

    os.makedirs("data/bgl", exist_ok=True)
    if not os.path.exists(ZIP_PATH):
        print("Downloading BGL.zip (~708 MB) from Zenodo…")
        urllib.request.urlretrieve(FULL_URL, ZIP_PATH,
            reporthook=lambda b, bs, t: print(f"\r  {b*bs/1e6:.0f}/{t/1e6:.0f} MB", end="", flush=True))
        print()
    print("Extracting…")
    with zipfile.ZipFile(ZIP_PATH) as z:
        log_name = next(n for n in z.namelist() if n.endswith(".log"))
        return z.read(log_name).decode(errors="replace").splitlines()


def main() -> None:
    sample = "--sample" in sys.argv

    lines = load_lines(sample)
    print(f"Loaded {len(lines):,} lines")

    baseline: dict[str, list[str]] = defaultdict(list)
    test:     dict[str, list[str]] = defaultdict(list)

    for line in lines:
        # BGL columns: LABEL UNIX_TS DATE NODE ...
        parts = line.split(None, 4)
        if len(parts) < 4:
            continue
        label, node = parts[0], parts[3]
        rack = rack_of(node)
        if label == "-":
            baseline[rack].append(line)
        else:
            test[rack].append(line)

    # Keep only racks with enough events in both sets.
    common = {r for r in set(baseline) & set(test)
              if len(baseline[r]) >= MIN_EVENTS and len(test[r]) >= MIN_EVENTS}

    if not common:
        # Relax threshold for the 2k sample.
        common = set(baseline) & set(test)
        print(f"Note: relaxed MIN_EVENTS threshold (sample mode or sparse data)")

    print(f"Racks with ≥{MIN_EVENTS} events in both sets: {len(common)}")

    os.makedirs("data/bgl/baseline", exist_ok=True)
    os.makedirs("data/bgl/test",     exist_ok=True)

    for rack in sorted(common):
        with open(f"data/bgl/baseline/{rack}.log", "w") as f:
            f.write("\n".join(baseline[rack]))
        with open(f"data/bgl/test/{rack}.log", "w") as f:
            f.write("\n".join(test[rack]))

    total_b = sum(len(baseline[r]) for r in common)
    total_t = sum(len(test[r])     for r in common)
    print(f"Baseline: {total_b:,} events across {len(common)} racks")
    print(f"Test:     {total_t:,} events across {len(common)} racks")
    print()
    print("Next steps:")
    print("  cargo run --bin encoder -- build-dict data/bgl/baseline/ -o data/bgl/dict.json --format bgl")
    print("  cargo run --bin encoder -- encode data/bgl/baseline/ --dict data/bgl/dict.json -o data/bgl/collections/baseline/ --format bgl")
    print("  cargo run --bin encoder -- encode data/bgl/test/     --dict data/bgl/dict.json -o data/bgl/collections/test/     --format bgl")
    print("  cargo run --bin fishy   -- -b data/bgl/collections/baseline/ -c data/bgl/collections/test/ --duration-tolerance 0.0 -v")


if __name__ == "__main__":
    main()
