#!/usr/bin/env python3
"""Actions-only source authority enumeration and documentation checks.

The workflow may persist generated authority evidence back to the active migration branch.
No Android build, no application execution, and no secret material are handled here.
Generated rows remain unreviewed and are never parity evidence.
"""
from __future__ import annotations
import argparse
import base64
import hashlib
import io
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
SHA = re.compile(r'^[0-9a-f]{40}
def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def save(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def current_android_provenance() -> dict:
    """Return the exact Android checkout and live PR identity used for evidence generation."""
    head = os.environ.get('ANDROID_PR_HEAD_SHA') or os.environ.get('GITHUB_SHA')
    require(SHA.fullmatch(head or '') is not None, 'Invalid Android PR head SHA')
    checkout = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(checkout == head, f'Android checkout/head mismatch: checkout={checkout} head={head}')
    run_id = os.environ.get('GITHUB_RUN_ID')
    run_attempt = os.environ.get('GITHUB_RUN_ATTEMPT')
    repository = os.environ.get('GITHUB_REPOSITORY', 'fabu-android/fabushi-android')
    pr_number = int(os.environ.get('ANDROID_PR_NUMBER') or '3')
    pr = get(f'/repos/{repository}/pulls/{pr_number}')
    require(pr.get('number') == pr_number, 'Android PR lookup mismatch')
    require(pr.get('head', {}).get('sha') == head,
            f'Android PR/head mismatch: pr={pr.get("head", {}).get("sha")} checkout={head}')
    require(pr.get('head', {}).get('repo', {}).get('full_name') == repository,
            'Android PR head repository mismatch')
    return {
        'android_repository': repository,
        'android_pr': pr_number,
        'android_pr_head_sha': head,
        'android_pr_head_ref': pr.get('head', {}).get('ref') or os.environ.get('ANDROID_PR_HEAD_REF') or os.environ.get('GITHUB_REF_NAME'),
        'android_pr_base_sha': pr.get('base', {}).get('sha'),
        'android_pr_base_ref': pr.get('base', {}).get('ref') or os.environ.get('ANDROID_PR_BASE_REF') or 'main',
        'android_checkout_sha': checkout,
        'workflow_event': os.environ.get('GITHUB_EVENT_NAME'),
        'workflow_run_id': int(run_id) if run_id else None,
        'run_attempt': int(run_attempt) if run_attempt else None,
        'workflow_sha': os.environ.get('GITHUB_WORKFLOW_SHA') or None,
        'workflow_ref': os.environ.get('GITHUB_WORKFLOW_REF') or None,
        'run_url': (
            f"{os.environ.get('GITHUB_SERVER_URL', 'https://github.com')}/"
            f"{repository}/actions/runs/{run_id}"
            if run_id else None
        ),
        'generated_at_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    }

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


def _default_source_row(repo: str, source_commit: str, item: dict) -> dict:
    require(item['type'] in ('blob', 'commit'), 'Unexpected Git object type')
    require(item['mode'] in ('100644', '100755', '120000', '160000'), 'Unexpected Git mode')
    return {
        'source_repository': repo,
        'source_commit': source_commit,
        'source_path': item['path'],
        'source_blob_sha': item['sha'],
        'source_type': item['type'],
        'source_mode': item['mode'],
        'responsibilities': [],
        'android_targets': [],
        'disposition': None,
        'implementation_status': 'unreviewed',
        'evidence': [],
    }



def _is_semantic_source_anchor(responsibility_id: str, source: dict) -> bool:
    path = source.get('path')
    if not isinstance(path, str) or not path:
        return False
    has_identity = (
        source.get('source_mode') in ALLOWED_SOURCE_MODES
        and source.get('source_type') in ALLOWED_SOURCE_TYPES
    )
    return has_identity or responsibility_id not in LEGACY_FALLBACK_RESPONSIBILITY_IDS


def load_predecessor_responsibility_mapping(
    responsibility_path: Path,
) -> tuple[dict, dict[str, dict], str, dict]:
    try:
        if responsibility_path.is_file() and responsibility_path.stat().st_size > 0:
            candidate = json.loads(responsibility_path.read_text(encoding='utf-8'))
            if isinstance(candidate, dict) and candidate.get('responsibilities'):
                return candidate, {}, 'current-responsibility-ledger', {
                    'artifact_id': None,
                    'artifact_digest': None,
                    'source_manifest_run_id': None,
                    'source_manifest_android_head': None,
                }
    except (OSError, ValueError, TypeError, AttributeError, json.JSONDecodeError):
        pass

    artifact_id = os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT', DEFAULT_RESPONSIBILITY_ARTIFACT)
    expected_digest = os.environ.get(
        'ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST',
        DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST,
    )
    token = os.environ.get('GH_TOKEN')
    require(token, 'GH_TOKEN is required to recover predecessor responsibility mapping')
    req = Request(
        f'https://api.github.com/repos/fabu-android/fabushi-android/actions/artifacts/{artifact_id}/zip',
        headers={
            'Accept': 'application/vnd.github+json',
            'Authorization': 'Bearer ' + token,
            'User-Agent': 'Fabushi-Android-Docs',
            'X-GitHub-Api-Version': '2022-11-28',
        },
    )
    try:
        with urlopen(req, timeout=60) as response:
            payload = response.read()
    except Exception as error:
        raise RuntimeError('failed to recover predecessor responsibility artifact ' + artifact_id) from error
    actual_digest = 'sha256:' + hashlib.sha256(payload).hexdigest()
    require(actual_digest == expected_digest,
            f'predecessor responsibility artifact digest mismatch: expected={expected_digest} actual={actual_digest}')
    try:
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            responsibility_names = [n for n in archive.namelist()
                                    if n.endswith('docs/android-port/authority/responsibility-ledger.json')]
            manifest_names = [n for n in archive.namelist()
                              if n.endswith('docs/android-port/authority/source-inventory-manifest.json')]
            ledger_names = [n for n in archive.namelist()
                            if n.endswith('docs/android-port/authority/source-inventory-ledger.jsonl')]
            require(len(responsibility_names) == 1, 'predecessor artifact does not contain exactly one responsibility ledger')
            require(len(manifest_names) == 1, 'predecessor artifact does not contain exactly one source inventory manifest')
            require(len(ledger_names) == 1, 'predecessor artifact does not contain exactly one source inventory ledger')
            candidate = json.loads(archive.read(responsibility_names[0]).decode('utf-8'))
            source_manifest = json.loads(archive.read(manifest_names[0]).decode('utf-8'))
            source_rows = {}
            for line in archive.read(ledger_names[0]).decode('utf-8').splitlines():
                if line.strip():
                    row = json.loads(line)
                    source_rows[row['source_path']] = row
    except Exception as error:
        raise RuntimeError('predecessor responsibility artifact is invalid') from error
    require(isinstance(candidate, dict) and candidate.get('responsibilities'),
            'predecessor responsibility mapping is empty')
    require(len(candidate['responsibilities']) == EXPECTED_RESPONSIBILITY_COUNT,
            f'expected {EXPECTED_RESPONSIBILITY_COUNT} responsibility definitions, got {len(candidate["responsibilities"])}')
    require(source_manifest.get('tracked_non_tree_entries') == 8258,
            'predecessor source manifest does not cover the full 8258-row Desktop inventory')
    require(len(source_rows) == 8258,
            f'predecessor source ledger must contain 8258 unique rows, got {len(source_rows)}')
    return candidate, source_rows, 'pinned-predecessor-artifact', {
        'artifact_id': int(artifact_id),
        'artifact_digest': actual_digest,
        'source_manifest_run_id': source_manifest.get('workflow_run_id'),
        'source_manifest_android_head': source_manifest.get('android_pr_head_sha'),
        'source_manifest_desktop_commit': source_manifest.get('desktop_commit'),
        'source_manifest_desktop_root_tree': source_manifest.get('desktop_root_tree'),
    }


def merge_responsibility_mapping(
    responsibility_path: Path,
    ledger_rows: list[dict],
    new_by_path: dict[str, dict],
    current: str,
    tree_sha: str,
) -> dict:
    predecessor, predecessor_source_rows, mapping_origin, mapping_provenance = load_predecessor_responsibility_mapping(responsibility_path)
    android = current_android_provenance()
    predecessor_authority = predecessor.get('authority') or {}
    require(predecessor_authority.get('desktop_commit') == current,
            'responsibility mapping authority Desktop commit does not match current Desktop main')
    require(predecessor_authority.get('desktop_root_tree') == tree_sha,
            'responsibility mapping authority Desktop tree does not match current Desktop main')

    responsibilities = json.loads(json.dumps(predecessor.get('responsibilities', [])))
    require(len(responsibilities) == EXPECTED_RESPONSIBILITY_COUNT,
            f'expected {EXPECTED_RESPONSIBILITY_COUNT} responsibility definitions, got {len(responsibilities)}')

    rid_by_path: dict[str, set[str]] = {}
    source_symbols_by_path: dict[str, set[str]] = {}
    source_owners_by_path: dict[str, list[dict]] = {}
    semantic_anchor_paths: set[str] = set()
    semantic_anchor_entries = 0

    for item in responsibilities:
        rid = item.get('responsibility_id')
        require(rid, 'responsibility definition without responsibility_id')
        cleaned_sources = []
        for source in item.get('desktop_sources', []):
            path = source.get('path')
            if path not in new_by_path or not _is_semantic_source_anchor(rid, source):
                continue
            refreshed = dict(source)
            refreshed['blob_sha'] = new_by_path[path]['sha']
            refreshed['source_mode'] = new_by_path[path]['mode']
            refreshed['source_type'] = new_by_path[path]['type']
            cleaned_sources.append(refreshed)
            rid_by_path.setdefault(path, set()).add(rid)
            semantic_anchor_paths.add(path)
            semantic_anchor_entries += 1
            source_symbols_by_path.setdefault(path, set()).update(
                s for s in (source.get('symbols') or [])
                if isinstance(s, str) and s.strip()
            )
            source_owners_by_path.setdefault(path, []).append({
                'responsibility_id': rid,
                'source_owner': item.get('source_owner'),
                'android_owner': item.get('android_owner'),
            })
        item['desktop_sources'] = cleaned_sources

    ids = {item.get('responsibility_id') for item in responsibilities}
    targets_by_rid = {
        item['responsibility_id']: sorted({
            str(target) for target in (item.get('android_target') or [])
            if isinstance(target, str) and target.strip()
        })
        for item in responsibilities
    }

    unmapped_paths = []
    missing_dispositions = []
    missing_targets = []
    for row in ledger_rows:
        path = row['source_path']
        links = rid_by_path.get(path, set())
        if not links:
            unmapped_paths.append(path)
            row.update({
                'responsibilities': [],
                'source_symbols': [],
                'responsibility_owners': [],
                'android_targets': [],
                'android_target_provenance': 'unmapped; semantic mapping required',
                'disposition': None,
                'implementation_status': 'unreviewed',
                'mapping_status': 'unmapped',
                'evidence': [{
                    'kind': 'source-closure-gap',
                    'mapping_origin': mapping_origin,
                    'mapping_artifact_id': mapping_provenance.get('artifact_id'),
                    'mapping_artifact_digest': mapping_provenance.get('artifact_digest'),
                    'status': 'missing-semantic-mapping',
                    'source_path': path,
                    'source_blob_sha': row['source_blob_sha'],
                    'desktop_commit': current,
                    'desktop_root_tree': tree_sha,
                    'android_head': android['android_pr_head_sha'],
                    'android_pr_base_sha': android['android_pr_base_sha'],
                    'workflow_run_id': android['workflow_run_id'],
                    'run_attempt': android['run_attempt'],
                    'verification_status': 'not-verified',
                }],
            })
            continue
        require(all(rid in ids for rid in links), 'missing closure responsibility definition')
        row['responsibilities'] = sorted(links)
        row['source_symbols'] = sorted(source_symbols_by_path.get(path, set()))
        row['responsibility_owners'] = source_owners_by_path.get(path, [])
        row['android_targets'] = sorted({
            target for rid in links for target in targets_by_rid.get(rid, [])
        })
        row['android_target_provenance'] = 'responsibility-ledger-explicit-anchor; provenance-only; not semantic verification'
        predecessor_row = predecessor_source_rows.get(path)
        inherited_disposition = predecessor_row.get('disposition') if predecessor_row else None
        if row.get('disposition') not in ALLOWED_SOURCE_DISPOSITIONS:
            if inherited_disposition in ALLOWED_SOURCE_DISPOSITIONS:
                row['disposition'] = inherited_disposition
            else:
                missing_dispositions.append(path)
        if not row['android_targets']:
            missing_targets.append(path)
        row['implementation_status'] = 'unreviewed'
        row['mapping_status'] = 'explicit'
        row['evidence'] = [{
            'kind': 'responsibility-mapping-provenance',
            'mapping_origin': mapping_origin,
            'mapping_artifact_id': mapping_provenance.get('artifact_id'),
            'mapping_artifact_digest': mapping_provenance.get('artifact_digest'),
            'source_manifest_run_id': mapping_provenance.get('source_manifest_run_id'),
            'source_manifest_android_head': mapping_provenance.get('source_manifest_android_head'),
            'status': 'provenance-only',
            'source_path': path,
            'source_blob_sha': row['source_blob_sha'],
            'desktop_commit': current,
            'desktop_root_tree': tree_sha,
            'android_head': android['android_pr_head_sha'],
            'android_pr_base_sha': android['android_pr_base_sha'],
            'workflow_event': android['workflow_event'],
            'workflow_run_id': android['workflow_run_id'],
            'run_attempt': android['run_attempt'],
            'verification_status': 'not-verified',
        }]

    save(OUT / 'unmapped-source-rows.json', {
        'status': 'incomplete' if unmapped_paths else 'passed',
        'count': len(unmapped_paths),
        'paths': unmapped_paths,
        'mapping_policy': 'only explicit responsibility desktop_sources anchors; known legacy top-level fallback additions are discarded; no directory or filename fallback is used',
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'android_provenance': android,
        'mapping_provenance': mapping_provenance,
    })
    save(OUT / 'responsibility-mapping-summary.json', {
        'status': 'incomplete' if (unmapped_paths or missing_dispositions or missing_targets) else 'passed',
        'responsibility_count': len(responsibilities),
        'semantic_source_anchor_entries': semantic_anchor_entries,
        'semantic_source_anchor_unique_paths': len(semantic_anchor_paths),
        'mapped_source_rows': len(ledger_rows) - len(unmapped_paths),
        'unmapped_source_rows': len(unmapped_paths),
        'missing_disposition_rows': missing_dispositions,
        'missing_target_rows': missing_targets,
        'mapping_origin': mapping_origin,
        'mapping_provenance': mapping_provenance,
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'android_provenance': android,
    })

    for item in responsibilities:
        item['source_commit'] = current
        item['android_head_at_update'] = android['android_pr_head_sha']
        item['implementation_status'] = 'unreviewed'
        item['verification_status'] = 'not-verified'
        note = (
            f'Current-main rebaseline to {current}: explicit source responsibility mappings are '
            'provenance-only; implementation and exact-head verification remain open. '
            f'Unmapped source rows: {len(unmapped_paths)}.'
        )
        notes = item.get('notes')
        if isinstance(notes, str):
            if note not in notes:
                item['notes'] = notes + ' ' + note
        elif isinstance(notes, list):
            if note not in notes:
                notes.append(note)
        else:
            item['notes'] = note

    return {
        'schema_version': 2,
        'authority': {
            'desktop_repository': 'bhrumom/fabushi-desktop',
            'desktop_commit': current,
            'desktop_root_tree': tree_sha,
            'android_repository': 'fabu-android/fabushi-android',
            'android_pr': 3,
            'status': 'incomplete' if (unmapped_paths or missing_dispositions or missing_targets) else 'mapped-provenance-only',
            'responsibility_count': len(responsibilities),
            'semantic_source_anchor_entries': semantic_anchor_entries,
            'semantic_source_anchor_unique_paths': len(semantic_anchor_paths),
            'mapped_source_rows': len(ledger_rows) - len(unmapped_paths),
            'unmapped_source_rows': len(unmapped_paths),
            'missing_disposition_rows': len(missing_dispositions),
            'missing_target_rows': len(missing_targets),
            'responsibility_mapping_origin': mapping_origin,
            'responsibility_mapping_artifact_id': mapping_provenance.get('artifact_id'),
            'responsibility_mapping_artifact_digest': mapping_provenance.get('artifact_digest'),
            'responsibility_mapping_source_android_head': mapping_provenance.get('source_manifest_android_head'),
            'android_provenance': android,
            'note': 'No generated row is marked verified. Known legacy directory fallback is explicitly excluded.',
        },
        'responsibilities': responsibilities,
        'traceability': predecessor.get('traceability', {}),
        'metadata': predecessor.get('metadata', {}),
    }

def rebaseline() -> None:
    authority_dir = DOC / 'authority'
    baseline_path = authority_dir / 'baseline.json'
    manifest_path = authority_dir / 'source-inventory-manifest.json'
    ledger_path = authority_dir / 'source-inventory-ledger.jsonl'
    responsibility_path = authority_dir / 'responsibility-ledger.json'

    baseline = json.loads(baseline_path.read_text(encoding='utf-8'))
    repo = baseline['desktop_repository']
    require(repo == 'bhrumom/fabushi-desktop', 'Unexpected source repository')
    current = get(f'/repos/{repo}/branches/main')['commit']['sha']
    require(SHA.fullmatch(current) is not None, 'Invalid current Desktop main SHA')
    commit = get(f'/repos/{repo}/git/commits/{current}')
    require(commit.get('sha') == current, 'Desktop main commit lookup mismatch')
    tree_sha = commit['tree']['sha']
    require(SHA.fullmatch(tree_sha) is not None, 'Invalid Desktop root tree SHA')
    entries, fallback = complete_tree(repo, tree_sha)
    files = [item for item in entries if item['type'] != 'tree']
    require(files, 'Empty source inventory during rebaseline')

    old_rows = []
    if ledger_path.is_file():
        for line in ledger_path.read_text(encoding='utf-8').splitlines():
            if line.strip():
                old_rows.append(json.loads(line))
    old_by_path = {row.get('source_path'): row for row in old_rows if row.get('source_path')}
    new_by_path = {item['path']: item for item in files}

    preserved = 0
    reset = 0
    new_rows = []
    changed_paths = set()
    for item in files:
        old = old_by_path.get(item['path'])
        exact_same_object = (
            old is not None
            and old.get('source_blob_sha') == item['sha']
            and old.get('source_type') == item['type']
            and old.get('source_mode') == item['mode']
        )
        if exact_same_object:
            row = dict(old)
            row['source_repository'] = repo
            row['source_commit'] = current
            preserved += 1
        else:
            row = _default_source_row(repo, current, item)
            changed_paths.add(item['path'])
            reset += 1
        new_rows.append(row)

    deleted_paths = sorted(set(old_by_path) - set(new_by_path))
    changed_paths.update(deleted_paths)
    ledger_text = ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in new_rows)
    ledger_path.write_text(ledger_text, encoding='utf-8')

    top_counts = {}
    for item in files:
        top = item['path'].split('/')[0]
        top_counts[top] = top_counts.get(top, 0) + 1
    android = current_android_provenance()
    manifest = {
        'schema_version': 2,
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'desktop_main_before': current,
        'desktop_main_after': current,
        'authority_current': True,
        'android_tested_checkout_sha': android['android_checkout_sha'],
        'android_repository': android['android_repository'],
        'android_pr': android['android_pr'],
        'android_pr_head_sha': android['android_pr_head_sha'],
        'android_pr_head_ref': android['android_pr_head_ref'],
        'android_pr_base_sha': android['android_pr_base_sha'],
        'android_pr_base_ref': android['android_pr_base_ref'],
        'workflow_event': android['workflow_event'],
        'workflow_run_id': android['workflow_run_id'],
        'run_attempt': android['run_attempt'],
        'workflow_sha': android['workflow_sha'],
        'workflow_ref': android['workflow_ref'],
        'run_url': android['run_url'],
        'generated_at_utc': android['generated_at_utc'],
        'tree_entries': len(entries),
        'tracked_non_tree_entries': len(files),
        'root_counts': top_counts,
        'gitlinks': [item for item in files if item['type'] == 'commit'],
        'symlinks': [item for item in files if item['mode'] == '120000'],
        'truncation_fallback_used': fallback,
        'initial_status': 'unreviewed',
        'ledger_sha256': hashlib.sha256(ledger_text.encode('utf-8')).hexdigest(),
        'closure_review': f'incremental-rebaseline-preserved-{preserved}-reset-{reset}-deleted-{len(deleted_paths)}',
        'product_acceptance': 'not-claimed',
    }
    save(manifest_path, manifest)

    baseline['observed_date'] = time.strftime('%Y-%m-%d', time.gmtime())
    baseline['desktop_commit'] = current
    baseline['desktop_root_tree'] = tree_sha
    save(baseline_path, baseline)

    touched_responsibilities = 0
    deleted_anchor_count = 0
    if responsibility_path.is_file():
        responsibility_doc = merge_responsibility_mapping(responsibility_path, new_rows, new_by_path, current, tree_sha)
        authority = responsibility_doc.setdefault('authority', {})
        authority['desktop_commit'] = current
        authority['desktop_root_tree'] = tree_sha
        for responsibility in responsibility_doc.get('responsibilities', []):
            touched = False
            refreshed_sources = []
            for source in responsibility.get('desktop_sources', []):
                path = source.get('path')
                item = new_by_path.get(path)
                if item is None:
                    if path in changed_paths:
                        touched = True
                        deleted_anchor_count += 1
                    continue
                refreshed = dict(source)
                if refreshed.get('blob_sha') != item['sha']:
                    touched = True
                    refreshed['blob_sha'] = item['sha']
                refreshed_sources.append(refreshed)
            if touched:
                responsibility['desktop_sources'] = refreshed_sources
                responsibility['implementation_status'] = 'mapped'
                responsibility['verification_status'] = 'not-verified'
                note = (
                    f"Desktop authority rebaseline to {current}: one or more source anchors changed; "
                    "prior implementation/verification status was conservatively invalidated pending semantic re-audit."
                )
                notes = responsibility.get('notes')
                if isinstance(notes, str):
                    if note not in notes:
                        responsibility['notes'] = notes + ' ' + note
                elif isinstance(notes, list):
                    if note not in notes:
                        notes.append(note)
                else:
                    responsibility['notes'] = note
                touched_responsibilities += 1
        reverse_links = {}
        for responsibility in responsibility_doc.get('responsibilities', []):
            rid = responsibility.get('responsibility_id')
            if not rid:
                continue
            for source in responsibility.get('desktop_sources', []):
                path = source.get('path')
                if path in new_by_path:
                    reverse_links.setdefault(path, set()).add(rid)
        for row in new_rows:
            linked = reverse_links.get(row['source_path'])
            if linked:
                row['responsibilities'] = sorted(set(row.get('responsibilities') or []) | linked)
        ledger_text = ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in new_rows)
        ledger_path.write_text(ledger_text, encoding='utf-8')
        manifest['ledger_sha256'] = hashlib.sha256(ledger_text.encode('utf-8')).hexdigest()
        mapping_authority = responsibility_doc.get('authority', {})
        for key in (
            'status',
            'responsibility_count',
            'semantic_source_anchor_entries',
            'semantic_source_anchor_unique_paths',
            'mapped_source_rows',
            'unmapped_source_rows',
            'missing_disposition_rows',
            'missing_target_rows',
            'responsibility_mapping_artifact_id',
            'responsibility_mapping_artifact_digest',
        ):
            manifest['responsibility_mapping_' + key] = mapping_authority.get(key)
        save(manifest_path, manifest)
        save(responsibility_path, responsibility_doc)

    summary = {
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'tracked_non_tree_entries': len(files),
        'preserved_rows': preserved,
        'reset_rows': reset,
        'deleted_rows': len(deleted_paths),
        'touched_responsibilities': touched_responsibilities,
        'deleted_responsibility_anchors': deleted_anchor_count,
        'responsibility_mapping_status': manifest.get('responsibility_mapping_status'),
        'mapped_source_rows': manifest.get('responsibility_mapping_mapped_source_rows'),
        'unmapped_source_rows': manifest.get('responsibility_mapping_unmapped_source_rows'),
        'android_provenance': current_android_provenance(),
    }
    save(OUT / 'rebaseline-summary.json', summary)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


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
        rows.append(_default_source_row(repo, sha, item))
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
    android = current_android_provenance()
    authority_ledger = DOC / 'authority/source-inventory-ledger.jsonl'
    authority_ledger_text = authority_ledger.read_text(encoding='utf-8') if authority_ledger.is_file() else ''
    manifest = {'schema_version': 2, 'desktop_commit': sha, 'desktop_root_tree': tree_sha, 'desktop_main_before': before, 'desktop_main_after': after, 'authority_current': before == sha == after, 'android_tested_checkout_sha': android['android_checkout_sha'], 'android_repository': android['android_repository'], 'android_pr': android['android_pr'], 'android_pr_head_sha': android['android_pr_head_sha'], 'android_pr_head_ref': android['android_pr_head_ref'], 'android_pr_base_sha': android['android_pr_base_sha'], 'android_pr_base_ref': android['android_pr_base_ref'], 'workflow_event': android['workflow_event'], 'workflow_run_id': android['workflow_run_id'], 'run_attempt': android['run_attempt'], 'workflow_sha': android['workflow_sha'], 'workflow_ref': android['workflow_ref'], 'run_url': android['run_url'], 'generated_at_utc': android['generated_at_utc'], 'tree_entries': len(entries), 'tracked_non_tree_entries': len(files), 'root_counts': top_counts, 'gitlinks': [item for item in files if item['type'] == 'commit'], 'symlinks': [item for item in files if item['mode'] == '120000'], 'truncation_fallback_used': fallback, 'initial_status': 'unreviewed', 'ledger_sha256': hashlib.sha256(authority_ledger_text.encode('utf-8')).hexdigest() if authority_ledger_text else None, 'authority_ledger_rows': len([line for line in authority_ledger_text.splitlines() if line.strip()]), 'closure_review': 'submodule-LFS-dynamic-import-and-semantic-review-pending', 'product_acceptance': 'not-claimed'}
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
    report = OUT / 'docs-validation.json'
    save(report, {'status': 'failed' if errors else 'passed', 'markdown_files': len(docs), 'errors': errors, 'scope': 'local links, JSON syntax and required handbook presence; NOT product parity'})
    require(report.is_file() and report.stat().st_size > 0, 'Documentation validation report was not persisted')
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
    parser.add_argument('command', choices=('rebaseline', 'inventory', 'validate', 'bundle'))
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    try:
        {'rebaseline': rebaseline, 'inventory': inventory, 'validate': validate, 'bundle': bundle}[args.command]()
        return 0
    except (ValueError, RuntimeError, OSError) as error:
        print(type(error).__name__ + ': ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
)

ALLOWED_SOURCE_MODES = frozenset({'100644', '100755', '120000', '160000'})
ALLOWED_SOURCE_TYPES = frozenset({'blob', 'commit'})
ALLOWED_SOURCE_DISPOSITIONS = frozenset({
    'direct-port',
    'android-adapted',
    'not-applicable-with-replacement',
    'mixed',
})
LEGACY_FALLBACK_RESPONSIBILITY_IDS = frozenset({
    'RUST-ANDROID-RUNTIME-GATE-001',
    'AUTHORITY-CANONICAL-MIGRATION-SOURCE-001',
    'UI-PRODUCTION-COMPOSITION-ANDROID-001',
    'CI-DESKTOP-SOURCE-AUTHORITY-REPLACEMENT-001',
    'GOVERNANCE-SPEC-FIRST-ANDROID-001',
    'GOVERNANCE-REPOSITORY-BOUNDARY-001',
})
EXPECTED_RESPONSIBILITY_COUNT = 306
DEFAULT_RESPONSIBILITY_ARTIFACT = '12053467058'
DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST = 'sha256:26fe89b6a05f76ce96ab5ddab5a6107251586ad8143f42fff0d4794a4da15eb5';


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def save(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def current_android_provenance() -> dict:
    """Return the exact Android checkout/run identity used to generate evidence."""
    head = os.environ.get('ANDROID_PR_HEAD_SHA') or os.environ.get('GITHUB_SHA')
    require(SHA.fullmatch(head or '') is not None, 'Invalid Android PR head SHA')
    checkout = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(checkout == head, f'Android checkout/head mismatch: checkout={checkout} head={head}')
    run_id = os.environ.get('GITHUB_RUN_ID')
    run_attempt = os.environ.get('GITHUB_RUN_ATTEMPT')
    return {
        'android_repository': os.environ.get('GITHUB_REPOSITORY', 'fabu-android/fabushi-android'),
        'android_pr': int(os.environ.get('ANDROID_PR_NUMBER') or '3'),
        'android_pr_head_sha': head,
        'android_pr_head_ref': os.environ.get('ANDROID_PR_HEAD_REF') or os.environ.get('GITHUB_REF_NAME'),
        'android_pr_base_sha': os.environ.get('ANDROID_PR_BASE_SHA') or None,
        'android_pr_base_ref': os.environ.get('ANDROID_PR_BASE_REF') or None,
        'android_checkout_sha': checkout,
        'workflow_event': os.environ.get('GITHUB_EVENT_NAME'),
        'workflow_run_id': int(run_id) if run_id else None,
        'run_attempt': int(run_attempt) if run_attempt else None,
        'workflow_sha': os.environ.get('GITHUB_WORKFLOW_SHA') or None,
        'workflow_ref': os.environ.get('GITHUB_WORKFLOW_REF') or None,
        'run_url': (
            f"{os.environ.get('GITHUB_SERVER_URL', 'https://github.com')}/"
            f"{os.environ.get('GITHUB_REPOSITORY', 'fabu-android/fabushi-android')}/actions/runs/{run_id}"
            if run_id else None
        ),
        'generated_at_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    }


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


def _default_source_row(repo: str, source_commit: str, item: dict) -> dict:
    require(item['type'] in ('blob', 'commit'), 'Unexpected Git object type')
    require(item['mode'] in ('100644', '100755', '120000', '160000'), 'Unexpected Git mode')
    return {
        'source_repository': repo,
        'source_commit': source_commit,
        'source_path': item['path'],
        'source_blob_sha': item['sha'],
        'source_type': item['type'],
        'source_mode': item['mode'],
        'responsibilities': [],
        'android_targets': [],
        'disposition': None,
        'implementation_status': 'unreviewed',
        'evidence': [],
    }



def load_predecessor_responsibility_mapping(responsibility_path: Path) -> tuple[dict, str]:
    try:
        if responsibility_path.is_file() and responsibility_path.stat().st_size > 0:
            candidate = json.loads(responsibility_path.read_text(encoding='utf-8'))
            if isinstance(candidate, dict) and candidate.get('responsibilities'):
                return candidate, 'current-responsibility-ledger'
    except (OSError, ValueError, TypeError, AttributeError, json.JSONDecodeError):
        # Corrupt/empty semantic state is never accepted as valid input; recover only from the pinned predecessor artifact.
        pass
    artifact_id = os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT', '12031753556')
    token = os.environ.get('GH_TOKEN')
    require(token, 'GH_TOKEN is required to recover predecessor responsibility mapping')
    req = Request(
        f'https://api.github.com/repos/fabu-android/fabushi-android/actions/artifacts/{artifact_id}/zip',
        headers={'Accept': 'application/vnd.github+json', 'Authorization': 'Bearer ' + token, 'User-Agent': 'Fabushi-Android-Docs', 'X-GitHub-Api-Version': '2022-11-28'},
    )
    try:
        with urlopen(req, timeout=60) as response:
            payload = response.read()
    except Exception as error:
        raise RuntimeError('failed to recover predecessor responsibility artifact ' + artifact_id) from error
    try:
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            names = [name for name in archive.namelist() if name.endswith('docs/android-port/authority/responsibility-ledger.json')]
            require(len(names) == 1, 'predecessor artifact does not contain exactly one responsibility ledger')
            candidate = json.loads(archive.read(names[0]).decode('utf-8'))
    except Exception as error:
        raise RuntimeError('predecessor responsibility artifact is invalid') from error
    require(isinstance(candidate, dict) and candidate.get('responsibilities'), 'predecessor responsibility mapping is empty')
    return candidate, 'pinned-predecessor-artifact'

def merge_responsibility_mapping(responsibility_path: Path, ledger_rows: list[dict], new_by_path: dict[str, dict], current: str, tree_sha: str) -> dict:
    predecessor, mapping_origin = load_predecessor_responsibility_mapping(responsibility_path)
    android = current_android_provenance()
    responsibilities = json.loads(json.dumps(predecessor.get('responsibilities', [])))
    require(responsibilities, 'predecessor responsibility mapping has no definitions')
    rid_by_path: dict[str, set[str]] = {}
    for item in responsibilities:
        rid = item.get('responsibility_id')
        require(rid, 'responsibility definition without responsibility_id')
        for source in item.get('desktop_sources', []):
            path = source.get('path')
            if path in new_by_path:
                rid_by_path.setdefault(path, set()).add(rid)
    ids = {item.get('responsibility_id') for item in responsibilities}
    targets_by_rid = {
        item['responsibility_id']: sorted({
            str(target)
            for target in (item.get('android_target') or [])
            if isinstance(target, str) and target.strip()
        })
        for item in responsibilities
    }
    unmapped_paths = []
    for row in ledger_rows:
        path = row['source_path']
        links = rid_by_path.get(path, set())
        if not links:
            unmapped_paths.append(path)
            row['responsibilities'] = []
            row['android_targets'] = []
            row['android_target_provenance'] = 'unmapped; semantic mapping required'
            row['disposition'] = None
            row['implementation_status'] = 'unreviewed'
            row['evidence'] = [{
                'kind': 'source-closure-gap',
                'mapping_origin': mapping_origin,
                'status': 'missing-semantic-mapping',
                'source_path': path,
                'desktop_commit': current,
                'desktop_root_tree': tree_sha,
                'android_head': android['android_pr_head_sha'],
                'workflow_run_id': android['workflow_run_id'],
                'run_attempt': android['run_attempt'],
                'verification_status': 'not-verified',
            }]
            continue
        require(all(rid in ids for rid in links), 'missing closure responsibility definition')
        row['responsibilities'] = sorted(links)
        row['android_targets'] = sorted({
            target
            for rid in links
            for target in targets_by_rid.get(rid, [])
        })
        row['android_target_provenance'] = 'responsibility-ledger-explicit-anchor; provenance-only; not semantic verification'
        require(row.get('disposition') in {'direct-port', 'android-adapted', 'not-applicable-with-replacement'},
                'invalid or missing source disposition for explicitly mapped row: ' + path)
        row['implementation_status'] = 'unreviewed'
        row['evidence'] = [{
            'kind': 'responsibility-mapping-provenance',
            'mapping_origin': mapping_origin,
            'status': 'provenance-only',
            'source_path': path,
            'desktop_commit': current,
            'desktop_root_tree': tree_sha,
            'android_head': android['android_pr_head_sha'],
            'workflow_event': android['workflow_event'],
            'workflow_run_id': android['workflow_run_id'],
            'run_attempt': android['run_attempt'],
            'verification_status': 'not-verified',
        }]
    save(OUT / 'unmapped-source-rows.json', {
        'status': 'failed' if unmapped_paths else 'passed',
        'count': len(unmapped_paths),
        'paths': unmapped_paths,
        'mapping_policy': 'explicit responsibility desktop_sources only; directory fallback and predecessor heuristic are forbidden as semantic mapping',
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'android_provenance': android,
    })
    require(not unmapped_paths, f'{len(unmapped_paths)} source rows lack explicit responsibility mapping')
    for item in responsibilities:
        rid = item['responsibility_id']
        existing = {s.get('path') for s in item.get('desktop_sources', [])}
        for row in ledger_rows:
            if rid in row['responsibilities'] and row['source_path'] not in existing:
                item.setdefault('desktop_sources', []).append({'path':row['source_path'],'blob_sha':new_by_path[row['source_path']]['sha'],'symbols':[]})
        for source in item.get('desktop_sources', []):
            if source.get('path') in new_by_path:
                source['blob_sha'] = new_by_path[source['path']]['sha']
        item['source_commit'] = current
        item['android_head_at_update'] = android['android_pr_head_sha']
        item['implementation_status'] = 'unreviewed'
        item['verification_status'] = 'not-verified'
        item['notes'] = (item.get('notes') or '') + ' Current-main rebaseline: predecessor mapping is provenance only; fresh semantic and exact-head verification is required.'
    return {'schema_version':2,'authority':{'desktop_repository':'bhrumom/fabushi-desktop','desktop_commit':current,'desktop_root_tree':tree_sha,'android_repository':'fabu-android/fabushi-android','android_pr':3,'status':'incomplete','predecessor_mapping_artifact':int(os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT','12031753556')),'responsibility_mapping_origin':mapping_origin,'android_provenance':android,'note':'Current-main responsibility mapping is provenance only; implementation and verification are reset and require fresh semantic/exact-head evidence.'},'responsibilities':responsibilities,'traceability':predecessor.get('traceability',{}),'metadata':predecessor.get('metadata',{})}

def rebaseline() -> None:
    authority_dir = DOC / 'authority'
    baseline_path = authority_dir / 'baseline.json'
    manifest_path = authority_dir / 'source-inventory-manifest.json'
    ledger_path = authority_dir / 'source-inventory-ledger.jsonl'
    responsibility_path = authority_dir / 'responsibility-ledger.json'

    baseline = json.loads(baseline_path.read_text(encoding='utf-8'))
    repo = baseline['desktop_repository']
    require(repo == 'bhrumom/fabushi-desktop', 'Unexpected source repository')
    current = get(f'/repos/{repo}/branches/main')['commit']['sha']
    require(SHA.fullmatch(current) is not None, 'Invalid current Desktop main SHA')
    commit = get(f'/repos/{repo}/git/commits/{current}')
    require(commit.get('sha') == current, 'Desktop main commit lookup mismatch')
    tree_sha = commit['tree']['sha']
    require(SHA.fullmatch(tree_sha) is not None, 'Invalid Desktop root tree SHA')
    entries, fallback = complete_tree(repo, tree_sha)
    files = [item for item in entries if item['type'] != 'tree']
    require(files, 'Empty source inventory during rebaseline')

    old_rows = []
    if ledger_path.is_file():
        for line in ledger_path.read_text(encoding='utf-8').splitlines():
            if line.strip():
                old_rows.append(json.loads(line))
    old_by_path = {row.get('source_path'): row for row in old_rows if row.get('source_path')}
    new_by_path = {item['path']: item for item in files}

    preserved = 0
    reset = 0
    new_rows = []
    changed_paths = set()
    for item in files:
        old = old_by_path.get(item['path'])
        exact_same_object = (
            old is not None
            and old.get('source_blob_sha') == item['sha']
            and old.get('source_type') == item['type']
            and old.get('source_mode') == item['mode']
        )
        if exact_same_object:
            row = dict(old)
            row['source_repository'] = repo
            row['source_commit'] = current
            preserved += 1
        else:
            row = _default_source_row(repo, current, item)
            changed_paths.add(item['path'])
            reset += 1
        new_rows.append(row)

    deleted_paths = sorted(set(old_by_path) - set(new_by_path))
    changed_paths.update(deleted_paths)
    ledger_text = ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in new_rows)
    ledger_path.write_text(ledger_text, encoding='utf-8')

    top_counts = {}
    for item in files:
        top = item['path'].split('/')[0]
        top_counts[top] = top_counts.get(top, 0) + 1
    android = current_android_provenance()
    manifest = {
        'schema_version': 2,
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'desktop_main_before': current,
        'desktop_main_after': current,
        'authority_current': True,
        'android_tested_checkout_sha': android['android_checkout_sha'],
        'android_repository': android['android_repository'],
        'android_pr': android['android_pr'],
        'android_pr_head_sha': android['android_pr_head_sha'],
        'android_pr_head_ref': android['android_pr_head_ref'],
        'android_pr_base_sha': android['android_pr_base_sha'],
        'android_pr_base_ref': android['android_pr_base_ref'],
        'workflow_event': android['workflow_event'],
        'workflow_run_id': android['workflow_run_id'],
        'run_attempt': android['run_attempt'],
        'workflow_sha': android['workflow_sha'],
        'workflow_ref': android['workflow_ref'],
        'run_url': android['run_url'],
        'generated_at_utc': android['generated_at_utc'],
        'tree_entries': len(entries),
        'tracked_non_tree_entries': len(files),
        'root_counts': top_counts,
        'gitlinks': [item for item in files if item['type'] == 'commit'],
        'symlinks': [item for item in files if item['mode'] == '120000'],
        'truncation_fallback_used': fallback,
        'initial_status': 'unreviewed',
        'ledger_sha256': hashlib.sha256(ledger_text.encode('utf-8')).hexdigest(),
        'closure_review': f'incremental-rebaseline-preserved-{preserved}-reset-{reset}-deleted-{len(deleted_paths)}',
        'product_acceptance': 'not-claimed',
    }
    save(manifest_path, manifest)

    baseline['observed_date'] = time.strftime('%Y-%m-%d', time.gmtime())
    baseline['desktop_commit'] = current
    baseline['desktop_root_tree'] = tree_sha
    save(baseline_path, baseline)

    touched_responsibilities = 0
    deleted_anchor_count = 0
    if responsibility_path.is_file():
        responsibility_doc = merge_responsibility_mapping(responsibility_path, new_rows, new_by_path, current, tree_sha)
        authority = responsibility_doc.setdefault('authority', {})
        authority['desktop_commit'] = current
        authority['desktop_root_tree'] = tree_sha
        for responsibility in responsibility_doc.get('responsibilities', []):
            touched = False
            refreshed_sources = []
            for source in responsibility.get('desktop_sources', []):
                path = source.get('path')
                item = new_by_path.get(path)
                if item is None:
                    if path in changed_paths:
                        touched = True
                        deleted_anchor_count += 1
                    continue
                refreshed = dict(source)
                if refreshed.get('blob_sha') != item['sha']:
                    touched = True
                    refreshed['blob_sha'] = item['sha']
                refreshed_sources.append(refreshed)
            if touched:
                responsibility['desktop_sources'] = refreshed_sources
                responsibility['implementation_status'] = 'mapped'
                responsibility['verification_status'] = 'not-verified'
                note = (
                    f"Desktop authority rebaseline to {current}: one or more source anchors changed; "
                    "prior implementation/verification status was conservatively invalidated pending semantic re-audit."
                )
                notes = responsibility.get('notes')
                if isinstance(notes, str):
                    if note not in notes:
                        responsibility['notes'] = notes + ' ' + note
                elif isinstance(notes, list):
                    if note not in notes:
                        notes.append(note)
                else:
                    responsibility['notes'] = note
                touched_responsibilities += 1
        reverse_links = {}
        for responsibility in responsibility_doc.get('responsibilities', []):
            rid = responsibility.get('responsibility_id')
            if not rid:
                continue
            for source in responsibility.get('desktop_sources', []):
                path = source.get('path')
                if path in new_by_path:
                    reverse_links.setdefault(path, set()).add(rid)
        for row in new_rows:
            linked = reverse_links.get(row['source_path'])
            if linked:
                row['responsibilities'] = sorted(set(row.get('responsibilities') or []) | linked)
        ledger_text = ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in new_rows)
        ledger_path.write_text(ledger_text, encoding='utf-8')
        manifest['ledger_sha256'] = hashlib.sha256(ledger_text.encode('utf-8')).hexdigest()
        save(manifest_path, manifest)
        save(responsibility_path, responsibility_doc)

    summary = {
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'tracked_non_tree_entries': len(files),
        'preserved_rows': preserved,
        'reset_rows': reset,
        'deleted_rows': len(deleted_paths),
        'touched_responsibilities': touched_responsibilities,
        'deleted_responsibility_anchors': deleted_anchor_count,
    }
    save(OUT / 'rebaseline-summary.json', summary)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


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
        rows.append(_default_source_row(repo, sha, item))
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
    android = current_android_provenance()
    authority_ledger = DOC / 'authority/source-inventory-ledger.jsonl'
    authority_ledger_text = authority_ledger.read_text(encoding='utf-8') if authority_ledger.is_file() else ''
    manifest = {'schema_version': 2, 'desktop_commit': sha, 'desktop_root_tree': tree_sha, 'desktop_main_before': before, 'desktop_main_after': after, 'authority_current': before == sha == after, 'android_tested_checkout_sha': android['android_checkout_sha'], 'android_repository': android['android_repository'], 'android_pr': android['android_pr'], 'android_pr_head_sha': android['android_pr_head_sha'], 'android_pr_head_ref': android['android_pr_head_ref'], 'android_pr_base_sha': android['android_pr_base_sha'], 'android_pr_base_ref': android['android_pr_base_ref'], 'workflow_event': android['workflow_event'], 'workflow_run_id': android['workflow_run_id'], 'run_attempt': android['run_attempt'], 'workflow_sha': android['workflow_sha'], 'workflow_ref': android['workflow_ref'], 'run_url': android['run_url'], 'generated_at_utc': android['generated_at_utc'], 'tree_entries': len(entries), 'tracked_non_tree_entries': len(files), 'root_counts': top_counts, 'gitlinks': [item for item in files if item['type'] == 'commit'], 'symlinks': [item for item in files if item['mode'] == '120000'], 'truncation_fallback_used': fallback, 'initial_status': 'unreviewed', 'ledger_sha256': hashlib.sha256(authority_ledger_text.encode('utf-8')).hexdigest() if authority_ledger_text else None, 'authority_ledger_rows': len([line for line in authority_ledger_text.splitlines() if line.strip()]), 'closure_review': 'submodule-LFS-dynamic-import-and-semantic-review-pending', 'product_acceptance': 'not-claimed'}
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
    report = OUT / 'docs-validation.json'
    save(report, {'status': 'failed' if errors else 'passed', 'markdown_files': len(docs), 'errors': errors, 'scope': 'local links, JSON syntax and required handbook presence; NOT product parity'})
    require(report.is_file() and report.stat().st_size > 0, 'Documentation validation report was not persisted')
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
    parser.add_argument('command', choices=('rebaseline', 'inventory', 'validate', 'bundle'))
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    try:
        {'rebaseline': rebaseline, 'inventory': inventory, 'validate': validate, 'bundle': bundle}[args.command]()
        return 0
    except (ValueError, RuntimeError, OSError) as error:
        print(type(error).__name__ + ': ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def save(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def current_android_provenance() -> dict:
    """Return the exact Android checkout and live PR identity used for evidence generation."""
    head = os.environ.get('ANDROID_PR_HEAD_SHA') or os.environ.get('GITHUB_SHA')
    require(SHA.fullmatch(head or '') is not None, 'Invalid Android PR head SHA')
    checkout = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(checkout == head, f'Android checkout/head mismatch: checkout={checkout} head={head}')
    run_id = os.environ.get('GITHUB_RUN_ID')
    run_attempt = os.environ.get('GITHUB_RUN_ATTEMPT')
    repository = os.environ.get('GITHUB_REPOSITORY', 'fabu-android/fabushi-android')
    pr_number = int(os.environ.get('ANDROID_PR_NUMBER') or '3')
    pr = get(f'/repos/{repository}/pulls/{pr_number}')
    require(pr.get('number') == pr_number, 'Android PR lookup mismatch')
    require(pr.get('head', {}).get('sha') == head,
            f'Android PR/head mismatch: pr={pr.get("head", {}).get("sha")} checkout={head}')
    require(pr.get('head', {}).get('repo', {}).get('full_name') == repository,
            'Android PR head repository mismatch')
    return {
        'android_repository': repository,
        'android_pr': pr_number,
        'android_pr_head_sha': head,
        'android_pr_head_ref': pr.get('head', {}).get('ref') or os.environ.get('ANDROID_PR_HEAD_REF') or os.environ.get('GITHUB_REF_NAME'),
        'android_pr_base_sha': pr.get('base', {}).get('sha'),
        'android_pr_base_ref': pr.get('base', {}).get('ref') or os.environ.get('ANDROID_PR_BASE_REF') or 'main',
        'android_checkout_sha': checkout,
        'workflow_event': os.environ.get('GITHUB_EVENT_NAME'),
        'workflow_run_id': int(run_id) if run_id else None,
        'run_attempt': int(run_attempt) if run_attempt else None,
        'workflow_sha': os.environ.get('GITHUB_WORKFLOW_SHA') or None,
        'workflow_ref': os.environ.get('GITHUB_WORKFLOW_REF') or None,
        'run_url': (
            f"{os.environ.get('GITHUB_SERVER_URL', 'https://github.com')}/"
            f"{repository}/actions/runs/{run_id}"
            if run_id else None
        ),
        'generated_at_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    }

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


def _default_source_row(repo: str, source_commit: str, item: dict) -> dict:
    require(item['type'] in ('blob', 'commit'), 'Unexpected Git object type')
    require(item['mode'] in ('100644', '100755', '120000', '160000'), 'Unexpected Git mode')
    return {
        'source_repository': repo,
        'source_commit': source_commit,
        'source_path': item['path'],
        'source_blob_sha': item['sha'],
        'source_type': item['type'],
        'source_mode': item['mode'],
        'responsibilities': [],
        'android_targets': [],
        'disposition': None,
        'implementation_status': 'unreviewed',
        'evidence': [],
    }



def _is_semantic_source_anchor(responsibility_id: str, source: dict) -> bool:
    path = source.get('path')
    if not isinstance(path, str) or not path:
        return False
    has_identity = (
        source.get('source_mode') in ALLOWED_SOURCE_MODES
        and source.get('source_type') in ALLOWED_SOURCE_TYPES
    )
    return has_identity or responsibility_id not in LEGACY_FALLBACK_RESPONSIBILITY_IDS


def load_predecessor_responsibility_mapping(
    responsibility_path: Path,
) -> tuple[dict, dict[str, dict], str, dict]:
    try:
        if responsibility_path.is_file() and responsibility_path.stat().st_size > 0:
            candidate = json.loads(responsibility_path.read_text(encoding='utf-8'))
            if isinstance(candidate, dict) and candidate.get('responsibilities'):
                return candidate, {}, 'current-responsibility-ledger', {
                    'artifact_id': None,
                    'artifact_digest': None,
                    'source_manifest_run_id': None,
                    'source_manifest_android_head': None,
                }
    except (OSError, ValueError, TypeError, AttributeError, json.JSONDecodeError):
        pass

    artifact_id = os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT', DEFAULT_RESPONSIBILITY_ARTIFACT)
    expected_digest = os.environ.get(
        'ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST',
        DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST,
    )
    token = os.environ.get('GH_TOKEN')
    require(token, 'GH_TOKEN is required to recover predecessor responsibility mapping')
    req = Request(
        f'https://api.github.com/repos/fabu-android/fabushi-android/actions/artifacts/{artifact_id}/zip',
        headers={
            'Accept': 'application/vnd.github+json',
            'Authorization': 'Bearer ' + token,
            'User-Agent': 'Fabushi-Android-Docs',
            'X-GitHub-Api-Version': '2022-11-28',
        },
    )
    try:
        with urlopen(req, timeout=60) as response:
            payload = response.read()
    except Exception as error:
        raise RuntimeError('failed to recover predecessor responsibility artifact ' + artifact_id) from error
    actual_digest = 'sha256:' + hashlib.sha256(payload).hexdigest()
    require(actual_digest == expected_digest,
            f'predecessor responsibility artifact digest mismatch: expected={expected_digest} actual={actual_digest}')
    try:
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            responsibility_names = [n for n in archive.namelist()
                                    if n.endswith('docs/android-port/authority/responsibility-ledger.json')]
            manifest_names = [n for n in archive.namelist()
                              if n.endswith('docs/android-port/authority/source-inventory-manifest.json')]
            ledger_names = [n for n in archive.namelist()
                            if n.endswith('docs/android-port/authority/source-inventory-ledger.jsonl')]
            require(len(responsibility_names) == 1, 'predecessor artifact does not contain exactly one responsibility ledger')
            require(len(manifest_names) == 1, 'predecessor artifact does not contain exactly one source inventory manifest')
            require(len(ledger_names) == 1, 'predecessor artifact does not contain exactly one source inventory ledger')
            candidate = json.loads(archive.read(responsibility_names[0]).decode('utf-8'))
            source_manifest = json.loads(archive.read(manifest_names[0]).decode('utf-8'))
            source_rows = {}
            for line in archive.read(ledger_names[0]).decode('utf-8').splitlines():
                if line.strip():
                    row = json.loads(line)
                    source_rows[row['source_path']] = row
    except Exception as error:
        raise RuntimeError('predecessor responsibility artifact is invalid') from error
    require(isinstance(candidate, dict) and candidate.get('responsibilities'),
            'predecessor responsibility mapping is empty')
    require(len(candidate['responsibilities']) == EXPECTED_RESPONSIBILITY_COUNT,
            f'expected {EXPECTED_RESPONSIBILITY_COUNT} responsibility definitions, got {len(candidate["responsibilities"])}')
    require(source_manifest.get('tracked_non_tree_entries') == 8258,
            'predecessor source manifest does not cover the full 8258-row Desktop inventory')
    require(len(source_rows) == 8258,
            f'predecessor source ledger must contain 8258 unique rows, got {len(source_rows)}')
    return candidate, source_rows, 'pinned-predecessor-artifact', {
        'artifact_id': int(artifact_id),
        'artifact_digest': actual_digest,
        'source_manifest_run_id': source_manifest.get('workflow_run_id'),
        'source_manifest_android_head': source_manifest.get('android_pr_head_sha'),
        'source_manifest_desktop_commit': source_manifest.get('desktop_commit'),
        'source_manifest_desktop_root_tree': source_manifest.get('desktop_root_tree'),
    }


def merge_responsibility_mapping(
    responsibility_path: Path,
    ledger_rows: list[dict],
    new_by_path: dict[str, dict],
    current: str,
    tree_sha: str,
) -> dict:
    predecessor, predecessor_source_rows, mapping_origin, mapping_provenance = load_predecessor_responsibility_mapping(responsibility_path)
    android = current_android_provenance()
    predecessor_authority = predecessor.get('authority') or {}
    require(predecessor_authority.get('desktop_commit') == current,
            'responsibility mapping authority Desktop commit does not match current Desktop main')
    require(predecessor_authority.get('desktop_root_tree') == tree_sha,
            'responsibility mapping authority Desktop tree does not match current Desktop main')

    responsibilities = json.loads(json.dumps(predecessor.get('responsibilities', [])))
    require(len(responsibilities) == EXPECTED_RESPONSIBILITY_COUNT,
            f'expected {EXPECTED_RESPONSIBILITY_COUNT} responsibility definitions, got {len(responsibilities)}')

    rid_by_path: dict[str, set[str]] = {}
    source_symbols_by_path: dict[str, set[str]] = {}
    source_owners_by_path: dict[str, list[dict]] = {}
    semantic_anchor_paths: set[str] = set()
    semantic_anchor_entries = 0

    for item in responsibilities:
        rid = item.get('responsibility_id')
        require(rid, 'responsibility definition without responsibility_id')
        cleaned_sources = []
        for source in item.get('desktop_sources', []):
            path = source.get('path')
            if path not in new_by_path or not _is_semantic_source_anchor(rid, source):
                continue
            refreshed = dict(source)
            refreshed['blob_sha'] = new_by_path[path]['sha']
            refreshed['source_mode'] = new_by_path[path]['mode']
            refreshed['source_type'] = new_by_path[path]['type']
            cleaned_sources.append(refreshed)
            rid_by_path.setdefault(path, set()).add(rid)
            semantic_anchor_paths.add(path)
            semantic_anchor_entries += 1
            source_symbols_by_path.setdefault(path, set()).update(
                s for s in (source.get('symbols') or [])
                if isinstance(s, str) and s.strip()
            )
            source_owners_by_path.setdefault(path, []).append({
                'responsibility_id': rid,
                'source_owner': item.get('source_owner'),
                'android_owner': item.get('android_owner'),
            })
        item['desktop_sources'] = cleaned_sources

    ids = {item.get('responsibility_id') for item in responsibilities}
    targets_by_rid = {
        item['responsibility_id']: sorted({
            str(target) for target in (item.get('android_target') or [])
            if isinstance(target, str) and target.strip()
        })
        for item in responsibilities
    }

    unmapped_paths = []
    missing_dispositions = []
    missing_targets = []
    for row in ledger_rows:
        path = row['source_path']
        links = rid_by_path.get(path, set())
        if not links:
            unmapped_paths.append(path)
            row.update({
                'responsibilities': [],
                'source_symbols': [],
                'responsibility_owners': [],
                'android_targets': [],
                'android_target_provenance': 'unmapped; semantic mapping required',
                'disposition': None,
                'implementation_status': 'unreviewed',
                'mapping_status': 'unmapped',
                'evidence': [{
                    'kind': 'source-closure-gap',
                    'mapping_origin': mapping_origin,
                    'mapping_artifact_id': mapping_provenance.get('artifact_id'),
                    'mapping_artifact_digest': mapping_provenance.get('artifact_digest'),
                    'status': 'missing-semantic-mapping',
                    'source_path': path,
                    'source_blob_sha': row['source_blob_sha'],
                    'desktop_commit': current,
                    'desktop_root_tree': tree_sha,
                    'android_head': android['android_pr_head_sha'],
                    'android_pr_base_sha': android['android_pr_base_sha'],
                    'workflow_run_id': android['workflow_run_id'],
                    'run_attempt': android['run_attempt'],
                    'verification_status': 'not-verified',
                }],
            })
            continue
        require(all(rid in ids for rid in links), 'missing closure responsibility definition')
        row['responsibilities'] = sorted(links)
        row['source_symbols'] = sorted(source_symbols_by_path.get(path, set()))
        row['responsibility_owners'] = source_owners_by_path.get(path, [])
        row['android_targets'] = sorted({
            target for rid in links for target in targets_by_rid.get(rid, [])
        })
        row['android_target_provenance'] = 'responsibility-ledger-explicit-anchor; provenance-only; not semantic verification'
        predecessor_row = predecessor_source_rows.get(path)
        inherited_disposition = predecessor_row.get('disposition') if predecessor_row else None
        if row.get('disposition') not in ALLOWED_SOURCE_DISPOSITIONS:
            if inherited_disposition in ALLOWED_SOURCE_DISPOSITIONS:
                row['disposition'] = inherited_disposition
            else:
                missing_dispositions.append(path)
        if not row['android_targets']:
            missing_targets.append(path)
        row['implementation_status'] = 'unreviewed'
        row['mapping_status'] = 'explicit'
        row['evidence'] = [{
            'kind': 'responsibility-mapping-provenance',
            'mapping_origin': mapping_origin,
            'mapping_artifact_id': mapping_provenance.get('artifact_id'),
            'mapping_artifact_digest': mapping_provenance.get('artifact_digest'),
            'source_manifest_run_id': mapping_provenance.get('source_manifest_run_id'),
            'source_manifest_android_head': mapping_provenance.get('source_manifest_android_head'),
            'status': 'provenance-only',
            'source_path': path,
            'source_blob_sha': row['source_blob_sha'],
            'desktop_commit': current,
            'desktop_root_tree': tree_sha,
            'android_head': android['android_pr_head_sha'],
            'android_pr_base_sha': android['android_pr_base_sha'],
            'workflow_event': android['workflow_event'],
            'workflow_run_id': android['workflow_run_id'],
            'run_attempt': android['run_attempt'],
            'verification_status': 'not-verified',
        }]

    save(OUT / 'unmapped-source-rows.json', {
        'status': 'incomplete' if unmapped_paths else 'passed',
        'count': len(unmapped_paths),
        'paths': unmapped_paths,
        'mapping_policy': 'only explicit responsibility desktop_sources anchors; known legacy top-level fallback additions are discarded; no directory or filename fallback is used',
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'android_provenance': android,
        'mapping_provenance': mapping_provenance,
    })
    save(OUT / 'responsibility-mapping-summary.json', {
        'status': 'incomplete' if (unmapped_paths or missing_dispositions or missing_targets) else 'passed',
        'responsibility_count': len(responsibilities),
        'semantic_source_anchor_entries': semantic_anchor_entries,
        'semantic_source_anchor_unique_paths': len(semantic_anchor_paths),
        'mapped_source_rows': len(ledger_rows) - len(unmapped_paths),
        'unmapped_source_rows': len(unmapped_paths),
        'missing_disposition_rows': missing_dispositions,
        'missing_target_rows': missing_targets,
        'mapping_origin': mapping_origin,
        'mapping_provenance': mapping_provenance,
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'android_provenance': android,
    })

    for item in responsibilities:
        item['source_commit'] = current
        item['android_head_at_update'] = android['android_pr_head_sha']
        item['implementation_status'] = 'unreviewed'
        item['verification_status'] = 'not-verified'
        note = (
            f'Current-main rebaseline to {current}: explicit source responsibility mappings are '
            'provenance-only; implementation and exact-head verification remain open. '
            f'Unmapped source rows: {len(unmapped_paths)}.'
        )
        notes = item.get('notes')
        if isinstance(notes, str):
            if note not in notes:
                item['notes'] = notes + ' ' + note
        elif isinstance(notes, list):
            if note not in notes:
                notes.append(note)
        else:
            item['notes'] = note

    return {
        'schema_version': 2,
        'authority': {
            'desktop_repository': 'bhrumom/fabushi-desktop',
            'desktop_commit': current,
            'desktop_root_tree': tree_sha,
            'android_repository': 'fabu-android/fabushi-android',
            'android_pr': 3,
            'status': 'incomplete' if (unmapped_paths or missing_dispositions or missing_targets) else 'mapped-provenance-only',
            'responsibility_count': len(responsibilities),
            'semantic_source_anchor_entries': semantic_anchor_entries,
            'semantic_source_anchor_unique_paths': len(semantic_anchor_paths),
            'mapped_source_rows': len(ledger_rows) - len(unmapped_paths),
            'unmapped_source_rows': len(unmapped_paths),
            'missing_disposition_rows': len(missing_dispositions),
            'missing_target_rows': len(missing_targets),
            'responsibility_mapping_origin': mapping_origin,
            'responsibility_mapping_artifact_id': mapping_provenance.get('artifact_id'),
            'responsibility_mapping_artifact_digest': mapping_provenance.get('artifact_digest'),
            'responsibility_mapping_source_android_head': mapping_provenance.get('source_manifest_android_head'),
            'android_provenance': android,
            'note': 'No generated row is marked verified. Known legacy directory fallback is explicitly excluded.',
        },
        'responsibilities': responsibilities,
        'traceability': predecessor.get('traceability', {}),
        'metadata': predecessor.get('metadata', {}),
    }

def rebaseline() -> None:
    authority_dir = DOC / 'authority'
    baseline_path = authority_dir / 'baseline.json'
    manifest_path = authority_dir / 'source-inventory-manifest.json'
    ledger_path = authority_dir / 'source-inventory-ledger.jsonl'
    responsibility_path = authority_dir / 'responsibility-ledger.json'

    baseline = json.loads(baseline_path.read_text(encoding='utf-8'))
    repo = baseline['desktop_repository']
    require(repo == 'bhrumom/fabushi-desktop', 'Unexpected source repository')
    current = get(f'/repos/{repo}/branches/main')['commit']['sha']
    require(SHA.fullmatch(current) is not None, 'Invalid current Desktop main SHA')
    commit = get(f'/repos/{repo}/git/commits/{current}')
    require(commit.get('sha') == current, 'Desktop main commit lookup mismatch')
    tree_sha = commit['tree']['sha']
    require(SHA.fullmatch(tree_sha) is not None, 'Invalid Desktop root tree SHA')
    entries, fallback = complete_tree(repo, tree_sha)
    files = [item for item in entries if item['type'] != 'tree']
    require(files, 'Empty source inventory during rebaseline')

    old_rows = []
    if ledger_path.is_file():
        for line in ledger_path.read_text(encoding='utf-8').splitlines():
            if line.strip():
                old_rows.append(json.loads(line))
    old_by_path = {row.get('source_path'): row for row in old_rows if row.get('source_path')}
    new_by_path = {item['path']: item for item in files}

    preserved = 0
    reset = 0
    new_rows = []
    changed_paths = set()
    for item in files:
        old = old_by_path.get(item['path'])
        exact_same_object = (
            old is not None
            and old.get('source_blob_sha') == item['sha']
            and old.get('source_type') == item['type']
            and old.get('source_mode') == item['mode']
        )
        if exact_same_object:
            row = dict(old)
            row['source_repository'] = repo
            row['source_commit'] = current
            preserved += 1
        else:
            row = _default_source_row(repo, current, item)
            changed_paths.add(item['path'])
            reset += 1
        new_rows.append(row)

    deleted_paths = sorted(set(old_by_path) - set(new_by_path))
    changed_paths.update(deleted_paths)
    ledger_text = ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in new_rows)
    ledger_path.write_text(ledger_text, encoding='utf-8')

    top_counts = {}
    for item in files:
        top = item['path'].split('/')[0]
        top_counts[top] = top_counts.get(top, 0) + 1
    android = current_android_provenance()
    manifest = {
        'schema_version': 2,
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'desktop_main_before': current,
        'desktop_main_after': current,
        'authority_current': True,
        'android_tested_checkout_sha': android['android_checkout_sha'],
        'android_repository': android['android_repository'],
        'android_pr': android['android_pr'],
        'android_pr_head_sha': android['android_pr_head_sha'],
        'android_pr_head_ref': android['android_pr_head_ref'],
        'android_pr_base_sha': android['android_pr_base_sha'],
        'android_pr_base_ref': android['android_pr_base_ref'],
        'workflow_event': android['workflow_event'],
        'workflow_run_id': android['workflow_run_id'],
        'run_attempt': android['run_attempt'],
        'workflow_sha': android['workflow_sha'],
        'workflow_ref': android['workflow_ref'],
        'run_url': android['run_url'],
        'generated_at_utc': android['generated_at_utc'],
        'tree_entries': len(entries),
        'tracked_non_tree_entries': len(files),
        'root_counts': top_counts,
        'gitlinks': [item for item in files if item['type'] == 'commit'],
        'symlinks': [item for item in files if item['mode'] == '120000'],
        'truncation_fallback_used': fallback,
        'initial_status': 'unreviewed',
        'ledger_sha256': hashlib.sha256(ledger_text.encode('utf-8')).hexdigest(),
        'closure_review': f'incremental-rebaseline-preserved-{preserved}-reset-{reset}-deleted-{len(deleted_paths)}',
        'product_acceptance': 'not-claimed',
    }
    save(manifest_path, manifest)

    baseline['observed_date'] = time.strftime('%Y-%m-%d', time.gmtime())
    baseline['desktop_commit'] = current
    baseline['desktop_root_tree'] = tree_sha
    save(baseline_path, baseline)

    touched_responsibilities = 0
    deleted_anchor_count = 0
    if responsibility_path.is_file():
        responsibility_doc = merge_responsibility_mapping(responsibility_path, new_rows, new_by_path, current, tree_sha)
        authority = responsibility_doc.setdefault('authority', {})
        authority['desktop_commit'] = current
        authority['desktop_root_tree'] = tree_sha
        for responsibility in responsibility_doc.get('responsibilities', []):
            touched = False
            refreshed_sources = []
            for source in responsibility.get('desktop_sources', []):
                path = source.get('path')
                item = new_by_path.get(path)
                if item is None:
                    if path in changed_paths:
                        touched = True
                        deleted_anchor_count += 1
                    continue
                refreshed = dict(source)
                if refreshed.get('blob_sha') != item['sha']:
                    touched = True
                    refreshed['blob_sha'] = item['sha']
                refreshed_sources.append(refreshed)
            if touched:
                responsibility['desktop_sources'] = refreshed_sources
                responsibility['implementation_status'] = 'mapped'
                responsibility['verification_status'] = 'not-verified'
                note = (
                    f"Desktop authority rebaseline to {current}: one or more source anchors changed; "
                    "prior implementation/verification status was conservatively invalidated pending semantic re-audit."
                )
                notes = responsibility.get('notes')
                if isinstance(notes, str):
                    if note not in notes:
                        responsibility['notes'] = notes + ' ' + note
                elif isinstance(notes, list):
                    if note not in notes:
                        notes.append(note)
                else:
                    responsibility['notes'] = note
                touched_responsibilities += 1
        reverse_links = {}
        for responsibility in responsibility_doc.get('responsibilities', []):
            rid = responsibility.get('responsibility_id')
            if not rid:
                continue
            for source in responsibility.get('desktop_sources', []):
                path = source.get('path')
                if path in new_by_path:
                    reverse_links.setdefault(path, set()).add(rid)
        for row in new_rows:
            linked = reverse_links.get(row['source_path'])
            if linked:
                row['responsibilities'] = sorted(set(row.get('responsibilities') or []) | linked)
        ledger_text = ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in new_rows)
        ledger_path.write_text(ledger_text, encoding='utf-8')
        manifest['ledger_sha256'] = hashlib.sha256(ledger_text.encode('utf-8')).hexdigest()
        mapping_authority = responsibility_doc.get('authority', {})
        for key in (
            'status',
            'responsibility_count',
            'semantic_source_anchor_entries',
            'semantic_source_anchor_unique_paths',
            'mapped_source_rows',
            'unmapped_source_rows',
            'missing_disposition_rows',
            'missing_target_rows',
            'responsibility_mapping_artifact_id',
            'responsibility_mapping_artifact_digest',
        ):
            manifest['responsibility_mapping_' + key] = mapping_authority.get(key)
        save(manifest_path, manifest)
        save(responsibility_path, responsibility_doc)

    summary = {
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'tracked_non_tree_entries': len(files),
        'preserved_rows': preserved,
        'reset_rows': reset,
        'deleted_rows': len(deleted_paths),
        'touched_responsibilities': touched_responsibilities,
        'deleted_responsibility_anchors': deleted_anchor_count,
        'responsibility_mapping_status': manifest.get('responsibility_mapping_status'),
        'mapped_source_rows': manifest.get('responsibility_mapping_mapped_source_rows'),
        'unmapped_source_rows': manifest.get('responsibility_mapping_unmapped_source_rows'),
        'android_provenance': current_android_provenance(),
    }
    save(OUT / 'rebaseline-summary.json', summary)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


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
        rows.append(_default_source_row(repo, sha, item))
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
    android = current_android_provenance()
    authority_ledger = DOC / 'authority/source-inventory-ledger.jsonl'
    authority_ledger_text = authority_ledger.read_text(encoding='utf-8') if authority_ledger.is_file() else ''
    manifest = {'schema_version': 2, 'desktop_commit': sha, 'desktop_root_tree': tree_sha, 'desktop_main_before': before, 'desktop_main_after': after, 'authority_current': before == sha == after, 'android_tested_checkout_sha': android['android_checkout_sha'], 'android_repository': android['android_repository'], 'android_pr': android['android_pr'], 'android_pr_head_sha': android['android_pr_head_sha'], 'android_pr_head_ref': android['android_pr_head_ref'], 'android_pr_base_sha': android['android_pr_base_sha'], 'android_pr_base_ref': android['android_pr_base_ref'], 'workflow_event': android['workflow_event'], 'workflow_run_id': android['workflow_run_id'], 'run_attempt': android['run_attempt'], 'workflow_sha': android['workflow_sha'], 'workflow_ref': android['workflow_ref'], 'run_url': android['run_url'], 'generated_at_utc': android['generated_at_utc'], 'tree_entries': len(entries), 'tracked_non_tree_entries': len(files), 'root_counts': top_counts, 'gitlinks': [item for item in files if item['type'] == 'commit'], 'symlinks': [item for item in files if item['mode'] == '120000'], 'truncation_fallback_used': fallback, 'initial_status': 'unreviewed', 'ledger_sha256': hashlib.sha256(authority_ledger_text.encode('utf-8')).hexdigest() if authority_ledger_text else None, 'authority_ledger_rows': len([line for line in authority_ledger_text.splitlines() if line.strip()]), 'closure_review': 'submodule-LFS-dynamic-import-and-semantic-review-pending', 'product_acceptance': 'not-claimed'}
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
    report = OUT / 'docs-validation.json'
    save(report, {'status': 'failed' if errors else 'passed', 'markdown_files': len(docs), 'errors': errors, 'scope': 'local links, JSON syntax and required handbook presence; NOT product parity'})
    require(report.is_file() and report.stat().st_size > 0, 'Documentation validation report was not persisted')
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
    parser.add_argument('command', choices=('rebaseline', 'inventory', 'validate', 'bundle'))
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    try:
        {'rebaseline': rebaseline, 'inventory': inventory, 'validate': validate, 'bundle': bundle}[args.command]()
        return 0
    except (ValueError, RuntimeError, OSError) as error:
        print(type(error).__name__ + ': ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
)

ALLOWED_SOURCE_MODES = frozenset({'100644', '100755', '120000', '160000'})
ALLOWED_SOURCE_TYPES = frozenset({'blob', 'commit'})
ALLOWED_SOURCE_DISPOSITIONS = frozenset({
    'direct-port',
    'android-adapted',
    'not-applicable-with-replacement',
    'mixed',
})
LEGACY_FALLBACK_RESPONSIBILITY_IDS = frozenset({
    'RUST-ANDROID-RUNTIME-GATE-001',
    'AUTHORITY-CANONICAL-MIGRATION-SOURCE-001',
    'UI-PRODUCTION-COMPOSITION-ANDROID-001',
    'CI-DESKTOP-SOURCE-AUTHORITY-REPLACEMENT-001',
    'GOVERNANCE-SPEC-FIRST-ANDROID-001',
    'GOVERNANCE-REPOSITORY-BOUNDARY-001',
})
EXPECTED_RESPONSIBILITY_COUNT = 306
DEFAULT_RESPONSIBILITY_ARTIFACT = '12053467058'
DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST = 'sha256:26fe89b6a05f76ce96ab5ddab5a6107251586ad8143f42fff0d4794a4da15eb5';


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def save(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def current_android_provenance() -> dict:
    """Return the exact Android checkout/run identity used to generate evidence."""
    head = os.environ.get('ANDROID_PR_HEAD_SHA') or os.environ.get('GITHUB_SHA')
    require(SHA.fullmatch(head or '') is not None, 'Invalid Android PR head SHA')
    checkout = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(checkout == head, f'Android checkout/head mismatch: checkout={checkout} head={head}')
    run_id = os.environ.get('GITHUB_RUN_ID')
    run_attempt = os.environ.get('GITHUB_RUN_ATTEMPT')
    return {
        'android_repository': os.environ.get('GITHUB_REPOSITORY', 'fabu-android/fabushi-android'),
        'android_pr': int(os.environ.get('ANDROID_PR_NUMBER') or '3'),
        'android_pr_head_sha': head,
        'android_pr_head_ref': os.environ.get('ANDROID_PR_HEAD_REF') or os.environ.get('GITHUB_REF_NAME'),
        'android_pr_base_sha': os.environ.get('ANDROID_PR_BASE_SHA') or None,
        'android_pr_base_ref': os.environ.get('ANDROID_PR_BASE_REF') or None,
        'android_checkout_sha': checkout,
        'workflow_event': os.environ.get('GITHUB_EVENT_NAME'),
        'workflow_run_id': int(run_id) if run_id else None,
        'run_attempt': int(run_attempt) if run_attempt else None,
        'workflow_sha': os.environ.get('GITHUB_WORKFLOW_SHA') or None,
        'workflow_ref': os.environ.get('GITHUB_WORKFLOW_REF') or None,
        'run_url': (
            f"{os.environ.get('GITHUB_SERVER_URL', 'https://github.com')}/"
            f"{os.environ.get('GITHUB_REPOSITORY', 'fabu-android/fabushi-android')}/actions/runs/{run_id}"
            if run_id else None
        ),
        'generated_at_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    }


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


def _default_source_row(repo: str, source_commit: str, item: dict) -> dict:
    require(item['type'] in ('blob', 'commit'), 'Unexpected Git object type')
    require(item['mode'] in ('100644', '100755', '120000', '160000'), 'Unexpected Git mode')
    return {
        'source_repository': repo,
        'source_commit': source_commit,
        'source_path': item['path'],
        'source_blob_sha': item['sha'],
        'source_type': item['type'],
        'source_mode': item['mode'],
        'responsibilities': [],
        'android_targets': [],
        'disposition': None,
        'implementation_status': 'unreviewed',
        'evidence': [],
    }



def load_predecessor_responsibility_mapping(responsibility_path: Path) -> tuple[dict, str]:
    try:
        if responsibility_path.is_file() and responsibility_path.stat().st_size > 0:
            candidate = json.loads(responsibility_path.read_text(encoding='utf-8'))
            if isinstance(candidate, dict) and candidate.get('responsibilities'):
                return candidate, 'current-responsibility-ledger'
    except (OSError, ValueError, TypeError, AttributeError, json.JSONDecodeError):
        # Corrupt/empty semantic state is never accepted as valid input; recover only from the pinned predecessor artifact.
        pass
    artifact_id = os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT', '12031753556')
    token = os.environ.get('GH_TOKEN')
    require(token, 'GH_TOKEN is required to recover predecessor responsibility mapping')
    req = Request(
        f'https://api.github.com/repos/fabu-android/fabushi-android/actions/artifacts/{artifact_id}/zip',
        headers={'Accept': 'application/vnd.github+json', 'Authorization': 'Bearer ' + token, 'User-Agent': 'Fabushi-Android-Docs', 'X-GitHub-Api-Version': '2022-11-28'},
    )
    try:
        with urlopen(req, timeout=60) as response:
            payload = response.read()
    except Exception as error:
        raise RuntimeError('failed to recover predecessor responsibility artifact ' + artifact_id) from error
    try:
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            names = [name for name in archive.namelist() if name.endswith('docs/android-port/authority/responsibility-ledger.json')]
            require(len(names) == 1, 'predecessor artifact does not contain exactly one responsibility ledger')
            candidate = json.loads(archive.read(names[0]).decode('utf-8'))
    except Exception as error:
        raise RuntimeError('predecessor responsibility artifact is invalid') from error
    require(isinstance(candidate, dict) and candidate.get('responsibilities'), 'predecessor responsibility mapping is empty')
    return candidate, 'pinned-predecessor-artifact'

def merge_responsibility_mapping(responsibility_path: Path, ledger_rows: list[dict], new_by_path: dict[str, dict], current: str, tree_sha: str) -> dict:
    predecessor, mapping_origin = load_predecessor_responsibility_mapping(responsibility_path)
    android = current_android_provenance()
    responsibilities = json.loads(json.dumps(predecessor.get('responsibilities', [])))
    require(responsibilities, 'predecessor responsibility mapping has no definitions')
    rid_by_path: dict[str, set[str]] = {}
    for item in responsibilities:
        rid = item.get('responsibility_id')
        require(rid, 'responsibility definition without responsibility_id')
        for source in item.get('desktop_sources', []):
            path = source.get('path')
            if path in new_by_path:
                rid_by_path.setdefault(path, set()).add(rid)
    ids = {item.get('responsibility_id') for item in responsibilities}
    targets_by_rid = {
        item['responsibility_id']: sorted({
            str(target)
            for target in (item.get('android_target') or [])
            if isinstance(target, str) and target.strip()
        })
        for item in responsibilities
    }
    unmapped_paths = []
    for row in ledger_rows:
        path = row['source_path']
        links = rid_by_path.get(path, set())
        if not links:
            unmapped_paths.append(path)
            row['responsibilities'] = []
            row['android_targets'] = []
            row['android_target_provenance'] = 'unmapped; semantic mapping required'
            row['disposition'] = None
            row['implementation_status'] = 'unreviewed'
            row['evidence'] = [{
                'kind': 'source-closure-gap',
                'mapping_origin': mapping_origin,
                'status': 'missing-semantic-mapping',
                'source_path': path,
                'desktop_commit': current,
                'desktop_root_tree': tree_sha,
                'android_head': android['android_pr_head_sha'],
                'workflow_run_id': android['workflow_run_id'],
                'run_attempt': android['run_attempt'],
                'verification_status': 'not-verified',
            }]
            continue
        require(all(rid in ids for rid in links), 'missing closure responsibility definition')
        row['responsibilities'] = sorted(links)
        row['android_targets'] = sorted({
            target
            for rid in links
            for target in targets_by_rid.get(rid, [])
        })
        row['android_target_provenance'] = 'responsibility-ledger-explicit-anchor; provenance-only; not semantic verification'
        require(row.get('disposition') in {'direct-port', 'android-adapted', 'not-applicable-with-replacement'},
                'invalid or missing source disposition for explicitly mapped row: ' + path)
        row['implementation_status'] = 'unreviewed'
        row['evidence'] = [{
            'kind': 'responsibility-mapping-provenance',
            'mapping_origin': mapping_origin,
            'status': 'provenance-only',
            'source_path': path,
            'desktop_commit': current,
            'desktop_root_tree': tree_sha,
            'android_head': android['android_pr_head_sha'],
            'workflow_event': android['workflow_event'],
            'workflow_run_id': android['workflow_run_id'],
            'run_attempt': android['run_attempt'],
            'verification_status': 'not-verified',
        }]
    save(OUT / 'unmapped-source-rows.json', {
        'status': 'failed' if unmapped_paths else 'passed',
        'count': len(unmapped_paths),
        'paths': unmapped_paths,
        'mapping_policy': 'explicit responsibility desktop_sources only; directory fallback and predecessor heuristic are forbidden as semantic mapping',
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'android_provenance': android,
    })
    require(not unmapped_paths, f'{len(unmapped_paths)} source rows lack explicit responsibility mapping')
    for item in responsibilities:
        rid = item['responsibility_id']
        existing = {s.get('path') for s in item.get('desktop_sources', [])}
        for row in ledger_rows:
            if rid in row['responsibilities'] and row['source_path'] not in existing:
                item.setdefault('desktop_sources', []).append({'path':row['source_path'],'blob_sha':new_by_path[row['source_path']]['sha'],'symbols':[]})
        for source in item.get('desktop_sources', []):
            if source.get('path') in new_by_path:
                source['blob_sha'] = new_by_path[source['path']]['sha']
        item['source_commit'] = current
        item['android_head_at_update'] = android['android_pr_head_sha']
        item['implementation_status'] = 'unreviewed'
        item['verification_status'] = 'not-verified'
        item['notes'] = (item.get('notes') or '') + ' Current-main rebaseline: predecessor mapping is provenance only; fresh semantic and exact-head verification is required.'
    return {'schema_version':2,'authority':{'desktop_repository':'bhrumom/fabushi-desktop','desktop_commit':current,'desktop_root_tree':tree_sha,'android_repository':'fabu-android/fabushi-android','android_pr':3,'status':'incomplete','predecessor_mapping_artifact':int(os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT','12031753556')),'responsibility_mapping_origin':mapping_origin,'android_provenance':android,'note':'Current-main responsibility mapping is provenance only; implementation and verification are reset and require fresh semantic/exact-head evidence.'},'responsibilities':responsibilities,'traceability':predecessor.get('traceability',{}),'metadata':predecessor.get('metadata',{})}

def rebaseline() -> None:
    authority_dir = DOC / 'authority'
    baseline_path = authority_dir / 'baseline.json'
    manifest_path = authority_dir / 'source-inventory-manifest.json'
    ledger_path = authority_dir / 'source-inventory-ledger.jsonl'
    responsibility_path = authority_dir / 'responsibility-ledger.json'

    baseline = json.loads(baseline_path.read_text(encoding='utf-8'))
    repo = baseline['desktop_repository']
    require(repo == 'bhrumom/fabushi-desktop', 'Unexpected source repository')
    current = get(f'/repos/{repo}/branches/main')['commit']['sha']
    require(SHA.fullmatch(current) is not None, 'Invalid current Desktop main SHA')
    commit = get(f'/repos/{repo}/git/commits/{current}')
    require(commit.get('sha') == current, 'Desktop main commit lookup mismatch')
    tree_sha = commit['tree']['sha']
    require(SHA.fullmatch(tree_sha) is not None, 'Invalid Desktop root tree SHA')
    entries, fallback = complete_tree(repo, tree_sha)
    files = [item for item in entries if item['type'] != 'tree']
    require(files, 'Empty source inventory during rebaseline')

    old_rows = []
    if ledger_path.is_file():
        for line in ledger_path.read_text(encoding='utf-8').splitlines():
            if line.strip():
                old_rows.append(json.loads(line))
    old_by_path = {row.get('source_path'): row for row in old_rows if row.get('source_path')}
    new_by_path = {item['path']: item for item in files}

    preserved = 0
    reset = 0
    new_rows = []
    changed_paths = set()
    for item in files:
        old = old_by_path.get(item['path'])
        exact_same_object = (
            old is not None
            and old.get('source_blob_sha') == item['sha']
            and old.get('source_type') == item['type']
            and old.get('source_mode') == item['mode']
        )
        if exact_same_object:
            row = dict(old)
            row['source_repository'] = repo
            row['source_commit'] = current
            preserved += 1
        else:
            row = _default_source_row(repo, current, item)
            changed_paths.add(item['path'])
            reset += 1
        new_rows.append(row)

    deleted_paths = sorted(set(old_by_path) - set(new_by_path))
    changed_paths.update(deleted_paths)
    ledger_text = ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in new_rows)
    ledger_path.write_text(ledger_text, encoding='utf-8')

    top_counts = {}
    for item in files:
        top = item['path'].split('/')[0]
        top_counts[top] = top_counts.get(top, 0) + 1
    android = current_android_provenance()
    manifest = {
        'schema_version': 2,
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'desktop_main_before': current,
        'desktop_main_after': current,
        'authority_current': True,
        'android_tested_checkout_sha': android['android_checkout_sha'],
        'android_repository': android['android_repository'],
        'android_pr': android['android_pr'],
        'android_pr_head_sha': android['android_pr_head_sha'],
        'android_pr_head_ref': android['android_pr_head_ref'],
        'android_pr_base_sha': android['android_pr_base_sha'],
        'android_pr_base_ref': android['android_pr_base_ref'],
        'workflow_event': android['workflow_event'],
        'workflow_run_id': android['workflow_run_id'],
        'run_attempt': android['run_attempt'],
        'workflow_sha': android['workflow_sha'],
        'workflow_ref': android['workflow_ref'],
        'run_url': android['run_url'],
        'generated_at_utc': android['generated_at_utc'],
        'tree_entries': len(entries),
        'tracked_non_tree_entries': len(files),
        'root_counts': top_counts,
        'gitlinks': [item for item in files if item['type'] == 'commit'],
        'symlinks': [item for item in files if item['mode'] == '120000'],
        'truncation_fallback_used': fallback,
        'initial_status': 'unreviewed',
        'ledger_sha256': hashlib.sha256(ledger_text.encode('utf-8')).hexdigest(),
        'closure_review': f'incremental-rebaseline-preserved-{preserved}-reset-{reset}-deleted-{len(deleted_paths)}',
        'product_acceptance': 'not-claimed',
    }
    save(manifest_path, manifest)

    baseline['observed_date'] = time.strftime('%Y-%m-%d', time.gmtime())
    baseline['desktop_commit'] = current
    baseline['desktop_root_tree'] = tree_sha
    save(baseline_path, baseline)

    touched_responsibilities = 0
    deleted_anchor_count = 0
    if responsibility_path.is_file():
        responsibility_doc = merge_responsibility_mapping(responsibility_path, new_rows, new_by_path, current, tree_sha)
        authority = responsibility_doc.setdefault('authority', {})
        authority['desktop_commit'] = current
        authority['desktop_root_tree'] = tree_sha
        for responsibility in responsibility_doc.get('responsibilities', []):
            touched = False
            refreshed_sources = []
            for source in responsibility.get('desktop_sources', []):
                path = source.get('path')
                item = new_by_path.get(path)
                if item is None:
                    if path in changed_paths:
                        touched = True
                        deleted_anchor_count += 1
                    continue
                refreshed = dict(source)
                if refreshed.get('blob_sha') != item['sha']:
                    touched = True
                    refreshed['blob_sha'] = item['sha']
                refreshed_sources.append(refreshed)
            if touched:
                responsibility['desktop_sources'] = refreshed_sources
                responsibility['implementation_status'] = 'mapped'
                responsibility['verification_status'] = 'not-verified'
                note = (
                    f"Desktop authority rebaseline to {current}: one or more source anchors changed; "
                    "prior implementation/verification status was conservatively invalidated pending semantic re-audit."
                )
                notes = responsibility.get('notes')
                if isinstance(notes, str):
                    if note not in notes:
                        responsibility['notes'] = notes + ' ' + note
                elif isinstance(notes, list):
                    if note not in notes:
                        notes.append(note)
                else:
                    responsibility['notes'] = note
                touched_responsibilities += 1
        reverse_links = {}
        for responsibility in responsibility_doc.get('responsibilities', []):
            rid = responsibility.get('responsibility_id')
            if not rid:
                continue
            for source in responsibility.get('desktop_sources', []):
                path = source.get('path')
                if path in new_by_path:
                    reverse_links.setdefault(path, set()).add(rid)
        for row in new_rows:
            linked = reverse_links.get(row['source_path'])
            if linked:
                row['responsibilities'] = sorted(set(row.get('responsibilities') or []) | linked)
        ledger_text = ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in new_rows)
        ledger_path.write_text(ledger_text, encoding='utf-8')
        manifest['ledger_sha256'] = hashlib.sha256(ledger_text.encode('utf-8')).hexdigest()
        save(manifest_path, manifest)
        save(responsibility_path, responsibility_doc)

    summary = {
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'tracked_non_tree_entries': len(files),
        'preserved_rows': preserved,
        'reset_rows': reset,
        'deleted_rows': len(deleted_paths),
        'touched_responsibilities': touched_responsibilities,
        'deleted_responsibility_anchors': deleted_anchor_count,
    }
    save(OUT / 'rebaseline-summary.json', summary)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


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
        rows.append(_default_source_row(repo, sha, item))
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
    android = current_android_provenance()
    authority_ledger = DOC / 'authority/source-inventory-ledger.jsonl'
    authority_ledger_text = authority_ledger.read_text(encoding='utf-8') if authority_ledger.is_file() else ''
    manifest = {'schema_version': 2, 'desktop_commit': sha, 'desktop_root_tree': tree_sha, 'desktop_main_before': before, 'desktop_main_after': after, 'authority_current': before == sha == after, 'android_tested_checkout_sha': android['android_checkout_sha'], 'android_repository': android['android_repository'], 'android_pr': android['android_pr'], 'android_pr_head_sha': android['android_pr_head_sha'], 'android_pr_head_ref': android['android_pr_head_ref'], 'android_pr_base_sha': android['android_pr_base_sha'], 'android_pr_base_ref': android['android_pr_base_ref'], 'workflow_event': android['workflow_event'], 'workflow_run_id': android['workflow_run_id'], 'run_attempt': android['run_attempt'], 'workflow_sha': android['workflow_sha'], 'workflow_ref': android['workflow_ref'], 'run_url': android['run_url'], 'generated_at_utc': android['generated_at_utc'], 'tree_entries': len(entries), 'tracked_non_tree_entries': len(files), 'root_counts': top_counts, 'gitlinks': [item for item in files if item['type'] == 'commit'], 'symlinks': [item for item in files if item['mode'] == '120000'], 'truncation_fallback_used': fallback, 'initial_status': 'unreviewed', 'ledger_sha256': hashlib.sha256(authority_ledger_text.encode('utf-8')).hexdigest() if authority_ledger_text else None, 'authority_ledger_rows': len([line for line in authority_ledger_text.splitlines() if line.strip()]), 'closure_review': 'submodule-LFS-dynamic-import-and-semantic-review-pending', 'product_acceptance': 'not-claimed'}
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
    report = OUT / 'docs-validation.json'
    save(report, {'status': 'failed' if errors else 'passed', 'markdown_files': len(docs), 'errors': errors, 'scope': 'local links, JSON syntax and required handbook presence; NOT product parity'})
    require(report.is_file() and report.stat().st_size > 0, 'Documentation validation report was not persisted')
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
    parser.add_argument('command', choices=('rebaseline', 'inventory', 'validate', 'bundle'))
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    try:
        {'rebaseline': rebaseline, 'inventory': inventory, 'validate': validate, 'bundle': bundle}[args.command]()
        return 0
    except (ValueError, RuntimeError, OSError) as error:
        print(type(error).__name__ + ': ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
