"""
Verify that params.rs matches RDKit's Params.cpp exactly.

Parses both files independently and compares every value.
Run: python verify_params.py
"""

import re
import sys
from pathlib import Path

PARAMS_RS = Path(__file__).parent.parent.parent.parent.parent / "src" / "crystolecule" / "simulation" / "uff" / "params.rs"
PARAMS_CPP = Path(__file__).parent / "Params.cpp"  # Download RDKit's Params.cpp here

def parse_cpp(text: str) -> list[dict]:
    """Parse the defaultParamData string from Params.cpp."""
    # Extract the string content between the first and last quote of defaultParamData
    # The data is a series of C++ string literals concatenated together
    # Find the start of defaultParamData
    start = text.find('defaultParamData =')
    if start == -1:
        raise ValueError("Could not find defaultParamData in Params.cpp")

    # Extract all string literals after defaultParamData
    chunk = text[start:]
    # Get all quoted strings
    strings = re.findall(r'"([^"]*)"', chunk)
    # Join them all together and process C escape sequences
    raw = "".join(strings)
    raw = raw.replace('\\n', '\n').replace('\\t', '\t')

    entries = []
    for line in raw.split('\n'):
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        parts = line.split('\t')
        if len(parts) != 12:
            continue
        entries.append({
            'label': parts[0],
            'r1': float(parts[1]),
            'theta0': float(parts[2]),
            'x1': float(parts[3]),
            'd1': float(parts[4]),
            'zeta': float(parts[5]),
            'z1': float(parts[6]),
            'v1': float(parts[7]),
            'u1': float(parts[8]),
            'gmp_xi': float(parts[9]),
            'gmp_hardness': float(parts[10]),
            'gmp_radius': float(parts[11]),
        })
    return entries


def parse_rs(text: str) -> list[dict]:
    """Parse UffAtomParams entries from params.rs."""
    entries = []
    # Match each UffAtomParams { ... } block
    pattern = re.compile(
        r'UffAtomParams\s*\{\s*'
        r'label:\s*"([^"]+)"\s*,\s*'
        r'r1:\s*([0-9.eE+-]+)\s*,\s*'
        r'theta0:\s*([0-9.eE+-]+)\s*,\s*'
        r'x1:\s*([0-9.eE+-]+)\s*,\s*'
        r'd1:\s*([0-9.eE+-]+)\s*,\s*'
        r'zeta:\s*([0-9.eE+-]+)\s*,\s*'
        r'z1:\s*([0-9.eE+-]+)\s*,\s*'
        r'v1:\s*([0-9.eE+-]+)\s*,\s*'
        r'u1:\s*([0-9.eE+-]+)\s*,\s*'
        r'gmp_xi:\s*([0-9.eE+-]+)\s*,\s*'
        r'gmp_hardness:\s*([0-9.eE+-]+)\s*,\s*'
        r'gmp_radius:\s*([0-9.eE+-]+)\s*'
        r'\}'
    )
    for m in pattern.finditer(text):
        entries.append({
            'label': m.group(1),
            'r1': float(m.group(2)),
            'theta0': float(m.group(3)),
            'x1': float(m.group(4)),
            'd1': float(m.group(5)),
            'zeta': float(m.group(6)),
            'z1': float(m.group(7)),
            'v1': float(m.group(8)),
            'u1': float(m.group(9)),
            'gmp_xi': float(m.group(10)),
            'gmp_hardness': float(m.group(11)),
            'gmp_radius': float(m.group(12)),
        })
    return entries


def main():
    # Download Params.cpp if not present
    if not PARAMS_CPP.exists():
        print(f"Downloading Params.cpp from RDKit GitHub...")
        import urllib.request
        url = "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/Params.cpp"
        urllib.request.urlretrieve(url, PARAMS_CPP)
        print(f"Saved to {PARAMS_CPP}")

    cpp_text = PARAMS_CPP.read_text()
    rs_text = PARAMS_RS.read_text()

    cpp_entries = parse_cpp(cpp_text)
    rs_entries = parse_rs(rs_text)

    print(f"Params.cpp: {len(cpp_entries)} entries")
    print(f"params.rs:  {len(rs_entries)} entries")

    if len(cpp_entries) != len(rs_entries):
        print(f"\nERROR: Entry count mismatch!")
        # Show which labels are missing
        cpp_labels = {e['label'] for e in cpp_entries}
        rs_labels = {e['label'] for e in rs_entries}
        missing_in_rs = cpp_labels - rs_labels
        extra_in_rs = rs_labels - cpp_labels
        if missing_in_rs:
            print(f"  Missing in params.rs: {sorted(missing_in_rs)}")
        if extra_in_rs:
            print(f"  Extra in params.rs: {sorted(extra_in_rs)}")

    fields = ['r1', 'theta0', 'x1', 'd1', 'zeta', 'z1', 'v1', 'u1', 'gmp_xi', 'gmp_hardness', 'gmp_radius']

    errors = 0
    checked = 0
    for i, (cpp, rs) in enumerate(zip(cpp_entries, rs_entries)):
        if cpp['label'] != rs['label']:
            print(f"\nERROR at index {i}: label mismatch: C++ '{cpp['label']}' vs Rust '{rs['label']}'")
            errors += 1
            continue

        for field in fields:
            cpp_val = cpp[field]
            rs_val = rs[field]
            if cpp_val != rs_val:
                print(f"  ERROR [{cpp['label']}].{field}: C++ = {cpp_val}, Rust = {rs_val}")
                errors += 1
            checked += 1

    print(f"\nChecked {checked} values across {min(len(cpp_entries), len(rs_entries))} entries")
    if errors == 0:
        print("ALL VALUES MATCH - params.rs is correct!")
    else:
        print(f"FOUND {errors} ERRORS")

    return 1 if errors else 0


if __name__ == '__main__':
    sys.exit(main())
