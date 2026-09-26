#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Create a reviewed emergency-APN candidate from authenticated stock inputs.

Never overwrites either input or an existing output. Does not install anything.
All attributes of stock emergency rows are preserved; no APN is guessed.
"""
import argparse
import copy
from pathlib import Path
import xml.etree.ElementTree as ET

MAX_INPUT = 8 * 1024 * 1024

def read(path):
    with path.open('rb') as stream:
        data = stream.read(MAX_INPUT + 1)
    if len(data) > MAX_INPUT or b'<!DOCTYPE' in data or b'<!ENTITY' in data:
        raise ValueError('unsupported APN XML')
    root = ET.fromstring(data)
    if root.tag != 'apns' or any(child.tag != 'apn' for child in root):
        raise ValueError('not an APN table')
    return root

def merge(base, stock):
    result = copy.deepcopy(base)
    existing = {tuple(sorted(row.attrib.items())) for row in result}
    added = 0
    for row in stock:
        if 'emergency' not in {v.strip() for v in row.get('type', '').split(',')}:
            continue
        if not row.get('mcc', '').isdigit() or len(row.get('mcc', '')) != 3:
            raise ValueError('invalid emergency MCC')
        if not row.get('mnc', '').isdigit() or len(row.get('mnc', '')) not in (2, 3):
            raise ValueError('invalid emergency MNC')
        identity = tuple(sorted(row.attrib.items()))
        if identity not in existing:
            result.append(copy.deepcopy(row)); existing.add(identity); added += 1
    return result, added

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', type=Path, required=True)
    parser.add_argument('--stock', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result, added = merge(read(args.base), read(args.stock))
    ET.indent(result)
    with args.output.open('xb') as out:
        ET.ElementTree(result).write(out, encoding='utf-8', xml_declaration=True)
    print(f'Created candidate with {added} additional emergency APN rows; not installed')

if __name__ == '__main__':
    main()
