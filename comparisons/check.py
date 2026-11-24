#!/usr/bin/env python3
"""Check each fixture's Rust build, runtime assertions and recorded binding results."""
import argparse
import functools
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile

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
    labels = [span['label'] for span in message.get('spans', []) if span.get('label')]
    return '\n'.join([message['message']] + [message_text(c) for c in message.get('children', [])] + labels)


def executable(stdout, name):
    for line in stdout.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (event.get('reason') == 'compiler-artifact' and event['target']['name'] == name
                and event.get('executable')):
            return event['executable']
    raise ValueError(f'Cargo returned no executable for {name}')


@functools.lru_cache(maxsize=None)
def diplomat_checker(offline):
    name = 'check-diplomat-interface'
    command = ['cargo', 'build', '--locked', '--features', 'diplomat-audit',
               '--bin', name, '--message-format=json']
    if offline:
        command.append('--offline')
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        sys.exit(f'Cannot build the Diplomat interface checker:\n{result.stderr}\n{result.stdout}')
    return executable(result.stdout, name)


def check_c_header(name, symbol, offline):
    command = ['cargo', 'test', '--locked', '--quiet', '--example', name, '--features', 'safer_ffi']
    if offline:
        command.append('--offline')
    with tempfile.TemporaryDirectory() as directory:
        header = Path(directory) / 'interface.h'
        result = subprocess.run(command + ['--', '--exact', 'comparison_c_header'], cwd=ROOT,
                                env={**os.environ, 'CO3_COMPARISON_HEADER': str(header)},
                                text=True, capture_output=True)
        if result.returncode:
            return result
        source = f'#include "interface.h"\nint main(void) {{ (void)&{symbol}; return 0; }}\n'
        compiler = shlex.split(os.environ.get('CC', 'cc'))
        return subprocess.run(compiler + ['-std=c11', '-fsyntax-only', '-I', directory, '-x', 'c', '-'],
                              cwd=ROOT, input=source, text=True, capture_output=True)


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
    matches = (None if expected['status'] == 'pending' else
               result.returncode == 0 if expected['status'] == 'pass' else
               result.returncode != 0 and any(expected['diagnostic'] in message_text(e) for e in errors))
    checks = {}
    if result.returncode == 0 and expected['status'] == 'pass':
        # Execute the binary Cargo actually built, including any assertions in main.
        run = subprocess.run([executable(result.stdout, name)], cwd=ROOT, capture_output=True, text=True)
        checks['run'] = 'pass' if run.returncode == 0 else 'fail'
        matches = matches and run.returncode == 0
        with log.open('a') as out:
            out.write('\nRun:\n' + run.stdout + run.stderr)
        if framework == 'diplomat':
            hir = subprocess.run([diplomat_checker(offline), case['path']], cwd=ROOT,
                                 capture_output=True, text=True)
            diagnostic = expected.get('hir_diagnostic')
            checks['hir'] = 'pass' if hir.returncode == 0 else 'reject'
            matches = matches and (hir.returncode == 0 if diagnostic is None else
                                   hir.returncode != 0 and diagnostic in hir.stderr)
            with log.open('a') as out:
                out.write('\nDiplomat interface:\n' + hir.stdout + hir.stderr)
        if framework == 'safer_ffi':
            header = check_c_header(name, expected['c_symbol'], offline)
            checks['c_header'] = 'pass' if header.returncode == 0 else 'fail'
            matches = matches and header.returncode == 0
            with log.open('a') as out:
                out.write('\nC header:\n' + header.stdout + header.stderr)
    return {'matches': matches, 'status': 'pass' if result.returncode == 0 else 'fail',
            'checks': checks, 'errors': [message_text(e) for e in errors], 'log': str(log.relative_to(ROOT))}


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
    unreviewed = 0
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
            unreviewed += result['matches'] is None
            failures += result['matches'] is False
            label = ('UNREVIEWED' if result['matches'] is None else
                     'OK' if result['matches'] else 'UNEXPECTED')
            stages = ''.join(f', {stage} {status}' for stage, status in result['checks'].items())
            print(f'{label} {framework:12} {name}: {result["status"]}{stages}', flush=True)
    (ROOT / 'results').mkdir(exist_ok=True)
    (ROOT / 'results' / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f'{failures} unexpected results, {unreviewed} unreviewed. Logs and report: {ROOT / "results"}')
    return int(bool(failures or unreviewed))


if __name__ == '__main__':
    sys.exit(main())
