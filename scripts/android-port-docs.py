#!/usr/bin/env python3
"""Actions-only read-only source enumeration and documentation checks.

No Android build, no application execution, no repository writes, no secrets.
Generated rows are deliberately unreviewed. They are NOT parity evidence.
"""
from __future__ import annotations
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import time
from urllib.error import HTTPError
from urllib.parse import unquote
from urllib.request import Request, urlopen
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / 'docs/android-port'
OUT = DOC / 'generated'
SHA = re.compile(r'^[0-9a-f]{40}$')


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def save(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def get(endpoint: str) -> dict:
    require(endpoint.startswith('/repos/'), 'Only public repository GET endpoints are supported')
    headers = {'Accept': 'application/vnd.github+json', 'User-Agent': 'Fabushi-Android-Docs', 'X-GitHub-Api-Version': '2022-11-28'}
    token = os.environ.get('GH_TOKEN')
    if token:
        headers['Authorization'] = 'Bearer ' + token
    for attempt in range(3):
        try:
            with urlopen(Request('https://api.github.com' + endpoint, headers=headers), timeout=45) as response:
                return json.load(response)
        except HTTPError as error:
            if error.code not in (429, 500, 502, 503, 504) or attempt == 2:
                raise RuntimeError('GitHub GET failed with HTTP ' + str(error.code)) from None
            time.sleep(2 ** attempt)
    raise RuntimeError('GitHub GET failed')


def complete_tree(repo: str, tree_sha: str) -> tuple[list[dict], bool]:
    data = get(f'/repos/{repo}/git/trees/{tree_sha}?recursive=1')
    require(data.get('sha') == tree_sha, 'Tree identity mismatch')
    save(OUT / 'desktop-recursive-api.json', data)
    if not data.get('truncated'):
        entries = data['tree']
        fallback = False
    else:
        # A truncated recursive response is never accepted as the inventory.
        entries, pending, cache = [], [('', tree_sha)], {}
        while pending:
            prefix, sha = pending.pop()
            if sha not in cache:
                cache[sha] = get(f'/repos/{repo}/git/trees/{sha}')
            part = cache[sha]
            require(not part.get('truncated'), 'A non-recursive tree was truncated')
            require(part.get('sha') == sha, 'Subtree identity mismatch')
            for entry in part['tree']:
                item = dict(entry)
                item['path'] = prefix + entry['path']
                entries.append(item)
                if entry['type'] == 'tree':
                    pending.append((item['path'] + '/', entry['sha']))
        fallback = True
    paths = [item['path'] for item in entries]
    require(len(paths) == len(set(paths)), 'Duplicate paths in source inventory')
    for item in entries:
        path = PurePosixPath(item['path'])
        require(not path.is_absolute() and '..' not in path.parts, 'Unsafe tree path')
        require(SHA.fullmatch(item['sha']) is not None, 'Invalid source object SHA')
    return sorted(entries, key=lambda item: item['path']), fallback


def inventory() -> None:
    baseline = json.loads((DOC / 'authority/baseline.json').read_text(encoding='utf-8'))
    repo, sha, tree_sha = (baseline[name] for name in ('desktop_repository', 'desktop_commit', 'desktop_root_tree'))
    require(repo == 'bhrumom/fabushi-desktop', 'Unexpected source repository')
    require(SHA.fullmatch(sha) is not None and SHA.fullmatch(tree_sha) is not None, 'Invalid source lock')
    before = get(f'/repos/{repo}/branches/main')['commit']['sha']
    commit = get(f'/repos/{repo}/git/commits/{sha}')
    require(commit.get('sha') == sha and commit['tree']['sha'] == tree_sha, 'Commit-to-tree lock mismatch')
    root = get(f'/repos/{repo}/git/trees/{tree_sha}')
    require(not root.get('truncated'), 'Root tree truncated')
    save(OUT / 'desktop-root-tree.json', root)
    entries, fallback = complete_tree(repo, tree_sha)
    save(OUT / 'desktop-complete-tree.json', {'sha': tree_sha, 'truncated': False, 'tree': entries})
    files = [item for item in entries if item['type'] != 'tree']
    require(files, 'Empty source inventory')
    by_path = {item['path']: item for item in files}
    rows = []
    for item in files:
        require(item['type'] in ('blob', 'commit'), 'Unexpected Git object type')
        require(item['mode'] in ('100644', '100755', '120000', '160000'), 'Unexpected Git mode')
        rows.append({'source_repository': repo, 'source_commit': sha, 'source_path': item['path'], 'source_blob_sha': item['sha'], 'source_type': item['type'], 'source_mode': item['mode'], 'responsibilities': [], 'android_targets': [], 'disposition': None, 'implementation_status': 'unreviewed', 'evidence': []})
    text = ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in rows)
    (OUT / 'initial-parity-ledger.jsonl').write_text(text, encoding='utf-8')
    anchor_records = []
    for path in baseline['desktop_verified_path_anchors']:
        require(path in by_path and by_path[path]['type'] == 'blob', 'Missing source anchor: ' + path)
        blob_sha = by_path[path]['sha']
        blob = get(f'/repos/{repo}/git/blobs/{blob_sha}')
        require(blob.get('encoding') == 'base64', 'Unsupported blob encoding')
        data = base64.b64decode(blob['content'])
        actual = hashlib.sha1(('blob ' + str(len(data)) + '\0').encode('ascii') + data).hexdigest()
        require(actual == blob_sha, 'Source anchor blob integrity mismatch: ' + path)
        anchor_records.append({'path': path, 'blob_sha': blob_sha, 'size': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'content_retrieved': True, 'semantic_review': 'not-claimed'})
    save(OUT / 'anchor-integrity.json', anchor_records)
    after = get(f'/repos/{repo}/branches/main')['commit']['sha']
    checkout = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    top_counts = {}
    for item in files:
        top = item['path'].split('/')[0]
        top_counts[top] = top_counts.get(top, 0) + 1
    manifest = {'schema_version': 1, 'desktop_commit': sha, 'desktop_root_tree': tree_sha, 'desktop_main_before': before, 'desktop_main_after': after, 'authority_current': before == sha == after, 'android_tested_checkout_sha': checkout, 'workflow_run_id': os.environ.get('GITHUB_RUN_ID'), 'run_attempt': os.environ.get('GITHUB_RUN_ATTEMPT'), 'tree_entries': len(entries), 'tracked_non_tree_entries': len(files), 'root_counts': top_counts, 'gitlinks': [item for item in files if item['type'] == 'commit'], 'symlinks': [item for item in files if item['mode'] == '120000'], 'truncation_fallback_used': fallback, 'initial_status': 'unreviewed', 'ledger_sha256': hashlib.sha256(text.encode('utf-8')).hexdigest(), 'closure_review': 'submodule-LFS-dynamic-import-and-semantic-review-pending', 'product_acceptance': 'not-claimed'}
    save(OUT / 'source-manifest.json', manifest)
    print(json.dumps(manifest, ensure_ascii=False, indent=2))
    require(before == sha == after, 'Desktop main moved: inventory saved, rebaseline required')


def validate() -> None:
    errors = []
    docs = sorted(path for path in DOC.rglob('*.md') if 'generated' not in path.relative_to(DOC).parts)
    for path in docs:
        text = path.read_text(encoding='utf-8')
        if not text.startswith('# ') or len(text.strip()) < 100:
            errors.append('Empty or untitled document: ' + str(path.relative_to(ROOT)))
        for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)', text):
            if re.match(r'^[a-zA-Z][a-zA-Z0-9+.-]*:', target) or target.startswith('#'):
                continue
            target = unquote(target.split('#')[0])
            resolved = (path.parent / target).resolve()
            if not resolved.is_relative_to(ROOT) or not resolved.exists():
                errors.append(str(path.relative_to(ROOT)) + ' -> missing local target: ' + target)
    for path in DOC.rglob('*.json'):
        if 'generated' not in path.relative_to(DOC).parts:
            try:
                json.loads(path.read_text(encoding='utf-8'))
            except (ValueError, OSError) as error:
                errors.append(str(path.relative_to(ROOT)) + ': ' + str(error))
    expected = ['01-account', '02-messaging', '03-agent', '04-tools', '05-mcp', '06-miniapp', '07-media', '08-remote', '09-commerce', '10-navigation', '11-automation', '12-settings']
    for name in expected:
        if not (DOC / 'features' / (name + '.md')).is_file():
            errors.append('Required feature handbook missing: ' + name)
    save(OUT / 'docs-validation.json', {'status': 'failed' if errors else 'passed', 'markdown_files': len(docs), 'errors': errors, 'scope': 'local links, JSON syntax and required handbook presence; NOT product parity'})
    if errors:
        for error in errors:
            print('ERROR:', error)
        raise ValueError('Documentation completeness gate failed')
    print('Documentation structure passed:', len(docs), 'Markdown files')


def bundle() -> None:
    paths = [path for path in DOC.rglob('*') if path.is_file()]
    paths += [ROOT / 'ANDROID_PORT.md', ROOT / 'docs/specs/desktop-main-android-full-parity.md']
    paths = sorted(set(path for path in paths if path.is_file()))
    with zipfile.ZipFile(ROOT / 'android-port-documentation.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
        for path in paths:
            archive.write(path, path.relative_to(ROOT))
    digest = hashlib.sha256((ROOT / 'android-port-documentation.zip').read_bytes()).hexdigest()
    (ROOT / 'android-port-documentation.zip.sha256').write_text(digest + '  android-port-documentation.zip\n', encoding='ascii')
    print('Documentation bundle:', len(paths), 'files; SHA-256:', digest)


def main() -> int:
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        print('Run this validation only in GitHub Actions.', file=sys.stderr)
        return 2
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=('inventory', 'validate', 'bundle'))
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    try:
        {'inventory': inventory, 'validate': validate, 'bundle': bundle}[args.command]()
        return 0
    except (ValueError, RuntimeError, OSError) as error:
        print(type(error).__name__ + ': ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
