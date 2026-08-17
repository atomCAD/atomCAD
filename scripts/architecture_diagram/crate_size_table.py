"""
Print the per-crate size table as Markdown.

Companion to `count_loc.py` (it reuses the same counting rule: non-empty,
non-comment lines). Where `count_loc.py` feeds the architecture diagram with
one number per crate, this adds the test tree beside each crate's `src/`, so
the table answers "how big is this crate, and how well is it covered".

Usage:
    python crate_size_table.py                 # pipe table (the doc / GFM form)
    python crate_size_table.py --format text   # fixed-width, inside a code fence
    python crate_size_table.py --format both

The markdown form goes into `doc/architecture_overview.md` under "Crate sizes"
and renders as a real table in Mattermost. The text form is for pasting where
tables are not rendered, or where a monospace block is simply safer.
"""

import subprocess
import sys

from count_loc import PROJECT_ROOT, count_loc_in_file

# The table contains em dashes; the Windows console defaults to cp1252.
sys.stdout.reconfigure(encoding='utf-8')

RUST = PROJECT_ROOT / 'rust'
CRATES = RUST / 'crates'
FRB_BINDINGS = PROJECT_ROOT / 'lib' / 'src' / 'rust'

# (label, source dirs, test dirs). Order is bottom-of-the-DAG last, matching
# how the architecture diagram reads left to right.
ROWS = [
    ('atomcad-structure-designer',
     [CRATES / 'atomcad-structure-designer' / 'src'],
     [CRATES / 'atomcad-structure-designer' / 'tests']),
    ('atomcad-crystolecule',
     [CRATES / 'atomcad-crystolecule' / 'src'],
     [CRATES / 'atomcad-crystolecule' / 'tests']),
    ('atomcad-renderer',
     [CRATES / 'atomcad-renderer' / 'src'],
     [CRATES / 'atomcad-renderer' / 'tests']),
    ('atomcad-display',
     [CRATES / 'atomcad-display' / 'src'],
     [CRATES / 'atomcad-display' / 'tests']),
    ('atomcad-geo-tree',
     [CRATES / 'atomcad-geo-tree' / 'src'],
     [CRATES / 'atomcad-geo-tree' / 'tests']),
    ('atomcad-util',
     [CRATES / 'atomcad-util' / 'src'],
     [CRATES / 'atomcad-util' / 'tests']),
    ('atomcad-test-support',
     [CRATES / 'atomcad-test-support' / 'src'],
     []),
    ('rust_lib_flutter_cad (api)',
     [RUST / 'src' / 'api'],
     [RUST / 'tests']),
]


def scan(paths, exts=('.rs',), skip_generated=False):
    """(code lines, file count) over the given directories or single files."""
    lines = files = 0
    for path in paths:
        if not path.exists():
            continue
        found = ([path] if path.is_file()
                 else [f for ext in exts for f in path.rglob(f'*{ext}')])
        for f in found:
            if skip_generated and FRB_BINDINGS in f.parents:
                continue
            lines += count_loc_in_file(f)
            files += 1
    return lines, files


def count_tests(paths):
    """Number of `#[test]` functions (0 if the crate has no test tree)."""
    total = 0
    for path in paths:
        if not path.exists():
            continue
        for f in path.rglob('*.rs'):
            total += f.read_text(encoding='utf-8', errors='replace').count('#[test]')
    return total


def cell(n):
    return f'{n:,}' if n else '—'


HEADERS = ('Crate', 'Source', 'Files', 'Test code', 'Tests')


def collect():
    """[(label, source, files, test code, tests)] plus the totals row."""
    rows = []
    src_total = test_total = tests_total = files_total = 0

    for label, src_paths, test_paths in ROWS:
        src, files = scan(src_paths)
        test, _ = scan(test_paths)
        n = count_tests(test_paths)
        src_total += src
        test_total += test
        tests_total += n
        files_total += files
        rows.append((label, cell(src), str(files), cell(test), cell(n)))

    dart_src, dart_files = scan([PROJECT_ROOT / 'lib'], ('.dart',),
                                skip_generated=True)
    dart_test, _ = scan([PROJECT_ROOT / 'test',
                         PROJECT_ROOT / 'integration_test'], ('.dart',))
    rows.append(('Flutter UI (lib/, Dart)', cell(dart_src), str(dart_files),
                 cell(dart_test), '—'))

    total = ('Total (hand-written)', f'{src_total + dart_src:,}',
             str(files_total + dart_files), f'{test_total + dart_test:,}',
             f'{tests_total:,}')
    return rows, total


def footnotes():
    frb_rs, _ = scan([RUST / 'src' / 'frb_generated.rs'])
    frb_dart, frb_dart_files = scan([FRB_BINDINGS], ('.dart',))
    lines = ['Non-empty, non-comment lines; each crate\'s src/ beside its own '
             'tests/.',
             f'Generated code excluded: rust/src/frb_generated.rs {frb_rs:,} '
             f'lines, lib/src/rust/ {frb_dart:,} lines in {frb_dart_files} '
             'files.']
    try:
        sha = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'],
                             cwd=PROJECT_ROOT, capture_output=True, text=True,
                             check=True).stdout.strip()
        lines.append(f'Measured at commit {sha}.')
    except Exception:
        pass
    return lines


def as_markdown(rows, total):
    """Pipe table. Renders as a real table in Mattermost, Slack canvases,
    GitHub, and anything else GFM-flavoured."""
    out = [f'| {" | ".join(HEADERS)} |', '|---|---:|---:|---:|---:|']
    for label, *cells in rows:
        tick = label if label.startswith('Flutter') else f'`{label}`'
        out.append(f'| {tick} | {" | ".join(cells)} |')
    out.append(f'| **{total[0]}** | ' +
               ' | '.join(f'**{c}**' for c in total[1:]) + ' |')
    return '\n'.join(out)


def as_text(rows, total):
    """Fixed-width table inside a fenced code block — for chat clients that
    do not render tables, or where a monospace block is simply safer."""
    body = list(rows) + [total]
    widths = [max(len(HEADERS[i]), max(len(r[i]) for r in body))
              for i in range(len(HEADERS))]

    def line(cells, pad=' '):
        first = cells[0].ljust(widths[0], pad)
        rest = [cells[i].rjust(widths[i], pad) for i in range(1, len(cells))]
        return f'{first}  {"  ".join(rest)}'.rstrip()

    rule = line(['' for _ in HEADERS], '-').replace(' ', '-')
    return '\n'.join(['```', line(HEADERS), rule,
                      *[line(r) for r in rows], rule, line(total), '```'])


def main():
    import argparse
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--format', choices=('markdown', 'text', 'both'),
                    default='markdown',
                    help='markdown pipe table (default), fixed-width code '
                         'block, or both')
    args = ap.parse_args()

    rows, total = collect()
    notes = footnotes()

    if args.format in ('markdown', 'both'):
        print(as_markdown(rows, total))
        print()
        print('\n'.join(notes))
    if args.format == 'both':
        print()
    if args.format in ('text', 'both'):
        print(as_text(rows, total))
        print('\n'.join(notes))


if __name__ == '__main__':
    main()
