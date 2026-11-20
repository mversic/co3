#!/usr/bin/env python3
"""Verify the reviewed Rust-build comparison matrix (Python 3.9+)."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent


def diagnostics(stdout, stderr, name, path):
    messages = []
    for line in stdout.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (event.get('reason') == 'compiler-message'
                and event.get('target', {}).get('name') == name
                and 'example' in event.get('target', {}).get('kind', [])):
            messages.append(event['message'])
    # Some proc macros print without a newline, leaving rustc's diagnostic JSON
    # concatenated to their stderr instead of Cargo's stdout event stream.
    decoder = json.JSONDecoder()
    marker = '{"$message_type":"diagnostic"'
    offset = 0
    while (offset := stderr.find(marker, offset)) >= 0:
        try:
            message, size = decoder.raw_decode(stderr[offset:])
        except json.JSONDecodeError:
            offset += len(marker)
            continue
        offset += size
        if any(span.get('file_name', '').endswith(path) for span in message.get('spans', [])):
            messages.append(message)
    return [m for m in messages if m.get('level') == 'error']


def message_text(message):
    return '\n'.join([message['message']] + [message_text(c) for c in message.get('children', [])])


def build(framework, name, case, offline):
    command = ['cargo', 'build', '--manifest-path', str(ROOT / 'Cargo.toml'),
               '--locked', '--no-default-features', '--features', framework,
               '--example', name, '--message-format=json']
    if offline:
        command.append('--offline')
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    errors = diagnostics(result.stdout, result.stderr, name, case['path'])
    directory = ROOT / 'results' / framework
    directory.mkdir(parents=True, exist_ok=True)
    log = directory / (name + '.log')
    log.write_text(result.stderr + '\n' + result.stdout)
    expected = case['expected'][framework]
    matches = (result.returncode == 0 if expected['status'] == 'pass' else
               result.returncode != 0 and any(expected['diagnostic'] in message_text(e) for e in errors))
    return {'matches': matches, 'status': 'pass' if result.returncode == 0 else 'fail',
            'errors': [message_text(e) for e in errors], 'log': str(log.relative_to(ROOT))}


def main():
    spec = json.loads((ROOT / 'expectations.json').read_text())
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--framework', action='append', choices=spec['frameworks'])
    parser.add_argument('--offline', action='store_true')
    parser.add_argument('cases', nargs='*')
    args = parser.parse_args()
    unknown = set(args.cases) - spec['cases'].keys()
    if unknown:
        parser.error(f'Unknown cases: {sorted(unknown)}')
    selected = args.cases or spec['cases']
    failures = 0
    report = {}
    for framework in args.framework or spec['frameworks']:
        report[framework] = {}
        for name in selected:
            case = spec['cases'][name]
            expected = case['expected'][framework]
            if expected['status'] == 'skip':
                report[framework][name] = {'status': 'skip', 'reason': expected['reason']}
                print(f'SKIP {framework:12} {name}: {expected["reason"]}', flush=True)
                continue
            result = build(framework, name, case, args.offline)
            report[framework][name] = result
            failures += not result['matches']
            label = 'OK' if result['matches'] else 'UNEXPECTED'
            print(f'{label} {framework:12} {name}: {result["status"]}', flush=True)
    (ROOT / 'results').mkdir(exist_ok=True)
    (ROOT / 'results' / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f'{failures} unexpected results. Logs and report: {ROOT / "results"}')
    return int(bool(failures))


if __name__ == '__main__':
    sys.exit(main())
