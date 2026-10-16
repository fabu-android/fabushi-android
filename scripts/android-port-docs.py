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
SHA = re.compile(r'^[0-9a-f]{40}')

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
EXPECTED_TREE_ENTRY_COUNT = 9412
EXPECTED_TRACKED_NON_TREE_COUNT = 8258
DEFAULT_RESPONSIBILITY_ARTIFACT = '12053467058'
DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST = 'sha256:26fe89b6a05f76ce96ab5ddab5a6107251586ad8143f42fff0d4794a4da15eb5'

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



def serialize_source_inventory_ledger(rows: list[dict]) -> str:
    """Serialize the complete inventory in one deterministic, manifest-digestable order."""
    paths = [row.get('source_path') for row in rows]
    require(all(isinstance(path, str) and path for path in paths),
            'source inventory row without source_path')
    require(len(paths) == len(set(paths)), 'duplicate path in source inventory serialization')
    require(paths == sorted(paths), 'source inventory rows must use canonical sorted path order')
    return ''.join(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n' for row in rows)


def _source_mapping_quality(responsibility_id: str, source: dict) -> str:
    explicit = source.get('mapping_quality')
    if explicit in {'semantic', 'coverage-only'}:
        return explicit
    has_object_metadata = (
        source.get('source_mode') in ALLOWED_SOURCE_MODES
        and source.get('source_type') in ALLOWED_SOURCE_TYPES
    )
    if responsibility_id in LEGACY_FALLBACK_RESPONSIBILITY_IDS and not has_object_metadata:
        return 'coverage-only'
    return 'semantic'


def _is_semantic_source_anchor(responsibility_id: str, source: dict) -> bool:
    path = source.get('path')
    if not isinstance(path, str) or not path:
        return False
    return _source_mapping_quality(responsibility_id, source) == 'semantic'


def _merge_responsibility_sources(current: dict, predecessor: dict) -> dict:
    """Merge current durable mappings with pinned predecessor coverage without promotion."""
    current_by_id = {
        item.get('responsibility_id'): item
        for item in current.get('responsibilities', [])
        if item.get('responsibility_id')
    }
    predecessor_by_id = {
        item.get('responsibility_id'): item
        for item in predecessor.get('responsibilities', [])
        if item.get('responsibility_id')
    }
    require(set(current_by_id) == set(predecessor_by_id),
            'current and predecessor responsibility authorities have different responsibility IDs')
    require(len(current_by_id) == EXPECTED_RESPONSIBILITY_COUNT,
            f'expected {EXPECTED_RESPONSIBILITY_COUNT} responsibility definitions, got {len(current_by_id)}')

    merged = json.loads(json.dumps(current))
    for item in merged.get('responsibilities', []):
        rid = item['responsibility_id']
        current_sources = {
            source.get('path'): source
            for source in item.get('desktop_sources', [])
            if isinstance(source, dict) and isinstance(source.get('path'), str)
        }
        merged_sources = list(current_sources.values())
        for source in predecessor_by_id[rid].get('desktop_sources', []):
            path = source.get('path')
            if not isinstance(path, str) or not path or path in current_sources:
                continue
            restored = dict(source)
            restored['mapping_quality'] = _source_mapping_quality(rid, restored)
            merged_sources.append(restored)
        for source in merged_sources:
            source['mapping_quality'] = _source_mapping_quality(rid, source)
        item['desktop_sources'] = sorted(merged_sources, key=lambda source: source.get('path', ''))
    return merged


def _load_pinned_responsibility_mapping() -> tuple[dict, dict[str, dict], dict]:
    artifact_id = os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT', DEFAULT_RESPONSIBILITY_ARTIFACT)
    expected_digest = os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST', DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST)
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
    except HTTPError as error:
        raise RuntimeError(
            'failed to recover predecessor responsibility artifact '
            + artifact_id + ' (HTTP ' + str(error.code) + ')'
        ) from None
    except Exception as error:
        raise RuntimeError(
            'failed to recover predecessor responsibility artifact '
            + artifact_id + ' (' + type(error).__name__ + ')'
        ) from None
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
    return candidate, source_rows, {
        'artifact_id': int(artifact_id),
        'artifact_digest': actual_digest,
        'source_manifest_run_id': source_manifest.get('workflow_run_id'),
        'source_manifest_android_head': source_manifest.get('android_pr_head_sha'),
        'source_manifest_desktop_commit': source_manifest.get('desktop_commit'),
        'source_manifest_desktop_root_tree': source_manifest.get('desktop_root_tree'),
    }


def _parse_persisted_source_inventory(
    ledger_text: str,
    manifest: dict,
    expected_paths: dict[str, dict] | None = None,
) -> dict[str, dict]:
    """Validate the persisted JSONL bytes, order, full denominator and source provenance."""
    actual_digest = hashlib.sha256(ledger_text.encode('utf-8')).hexdigest()
    require(actual_digest == manifest.get('ledger_sha256'),
            'local source inventory ledger digest does not match its manifest')
    desktop_commit = manifest.get('desktop_commit')
    require(SHA.fullmatch(str(desktop_commit or '')) is not None,
            'local source inventory manifest has an invalid Desktop commit')

    source_rows: dict[str, dict] = {}
    ordered_paths: list[str] = []
    for line_number, line in enumerate(ledger_text.splitlines(), 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f'local source inventory has invalid JSONL at line {line_number}') from error
        require(isinstance(row, dict),
                f'local source inventory row {line_number} is not an object')
        path = row.get('source_path')
        require(isinstance(path, str) and bool(path),
                'local source inventory contains a row without source_path')
        require(path not in source_rows,
                'local source inventory contains duplicate path: ' + path)
        source_path = PurePosixPath(path)
        require(not source_path.is_absolute() and '..' not in source_path.parts,
                'local source inventory contains an unsafe path: ' + path)
        require(row.get('source_commit') == desktop_commit,
                'local source inventory row has stale Desktop source commit: ' + path)
        require(row.get('source_type') in ALLOWED_SOURCE_TYPES,
                'local source inventory row has invalid source type: ' + path)
        require(row.get('source_mode') in ALLOWED_SOURCE_MODES,
                'local source inventory row has invalid source mode: ' + path)
        require(SHA.fullmatch(str(row.get('source_blob_sha') or '')) is not None,
                'local source inventory row has invalid source blob SHA: ' + path)
        ordered_paths.append(path)
        source_rows[path] = row

    require(ordered_paths == sorted(ordered_paths),
            'local source inventory rows are not in canonical sorted path order')
    require(manifest.get('tracked_non_tree_entries') == EXPECTED_TRACKED_NON_TREE_COUNT,
            'local source authority does not preserve the 8258-entry tracked non-tree inventory')
    require(len(source_rows) == EXPECTED_TRACKED_NON_TREE_COUNT,
            f'local source inventory must contain 8258 unique rows, got {len(source_rows)}')
    if expected_paths is not None:
        require(set(source_rows) == set(expected_paths),
                'persisted source inventory paths do not match the current Desktop inventory')
        for path, item in expected_paths.items():
            row = source_rows[path]
            require(row.get('source_blob_sha') == item.get('sha'),
                    'persisted source inventory blob does not match current Desktop inventory: ' + path)
            require(row.get('source_type') == item.get('type'),
                    'persisted source inventory type does not match current Desktop inventory: ' + path)
            require(row.get('source_mode') == item.get('mode'),
                    'persisted source inventory mode does not match current Desktop inventory: ' + path)
    return source_rows


def _load_local_persisted_mapping(
    responsibility_path: Path,
    current_candidate: dict,
) -> tuple[dict, dict[str, dict], dict]:
    """Recover coverage from the committed snapshot only after verifying its authority and digest.

    This is mapping continuity, not verification evidence. Strict parity checks still decide
    whether any source row or responsibility is complete.
    """
    authority_dir = responsibility_path.parent
    manifest_path = authority_dir / 'source-inventory-manifest.json'
    ledger_path = authority_dir / 'source-inventory-ledger.jsonl'
    require(manifest_path.is_file() and ledger_path.is_file(),
            'validated local source authority snapshot is unavailable')
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    authority = current_candidate.get('authority') or {}
    require(manifest.get('authority_current') is True,
            'local source authority snapshot is not current')
    require(manifest.get('desktop_commit') == authority.get('desktop_commit'),
            'local source authority Desktop commit does not match responsibility ledger')
    require(manifest.get('desktop_root_tree') == authority.get('desktop_root_tree'),
            'local source authority Desktop tree does not match responsibility ledger')
    require(manifest.get('tree_entries') == 9412,
            'local source authority does not preserve the 9412-entry recursive tree')
    require(manifest.get('tracked_non_tree_entries') == 8258,
            'local source authority does not preserve the 8258-entry tracked non-tree inventory')

    ledger_text = ledger_path.read_text(encoding='utf-8')
    source_rows = _parse_persisted_source_inventory(ledger_text, manifest)
    return current_candidate, source_rows, {
        'artifact_id': None,
        'artifact_digest': None,
        'source_manifest_run_id': manifest.get('workflow_run_id'),
        'source_manifest_android_head': manifest.get('android_pr_head_sha'),
        'source_manifest_desktop_commit': manifest.get('desktop_commit'),
        'source_manifest_desktop_root_tree': manifest.get('desktop_root_tree'),
        'mapping_source': 'validated-local-persisted-snapshot',
    }


def load_predecessor_responsibility_mapping(
    responsibility_path: Path,
) -> tuple[dict, dict[str, dict], str, dict]:
    current_candidate = None
    try:
        if responsibility_path.is_file() and responsibility_path.stat().st_size > 0:
            candidate = json.loads(responsibility_path.read_text(encoding='utf-8'))
            if isinstance(candidate, dict) and candidate.get('responsibilities'):
                current_candidate = candidate
    except (OSError, ValueError, TypeError, AttributeError, json.JSONDecodeError):
        current_candidate = None

    if current_candidate is not None:
        require(len(current_candidate['responsibilities']) == EXPECTED_RESPONSIBILITY_COUNT,
                f'expected {EXPECTED_RESPONSIBILITY_COUNT} responsibility definitions, got {len(current_candidate["responsibilities"])}')
        try:
            predecessor, source_rows, provenance = _load_pinned_responsibility_mapping()
        except RuntimeError as error:
            if not str(error).startswith('failed to recover predecessor responsibility artifact '):
                raise
            predecessor, source_rows, provenance = _load_local_persisted_mapping(
                responsibility_path, current_candidate
            )
            print(
                'WARNING: pinned predecessor artifact is unavailable; using the digest-verified '
                'local source snapshot for mapping continuity only',
                file=sys.stderr,
            )
            merged = _merge_responsibility_sources(current_candidate, predecessor)
            return merged, source_rows, 'current-responsibility-ledger+validated-local-coverage', provenance
        merged = _merge_responsibility_sources(current_candidate, predecessor)
        return merged, source_rows, 'current-responsibility-ledger+pin-predecessor-coverage', provenance

    predecessor, source_rows, provenance = _load_pinned_responsibility_mapping()
    return predecessor, source_rows, 'pinned-predecessor-artifact', provenance

def merge_responsibility_mapping(
    responsibility_path: Path,
    ledger_rows: list[dict],
    new_by_path: dict[str, dict],
    current: str,
    tree_sha: str,
    expected_responsibility_count: int | None = None,
) -> dict:
    predecessor, predecessor_source_rows, mapping_origin, mapping_provenance = load_predecessor_responsibility_mapping(responsibility_path)
    android = current_android_provenance()
    predecessor_authority = predecessor.get('authority') or {}
    require(predecessor_authority.get('desktop_commit') == current,
            'responsibility mapping authority Desktop commit does not match current Desktop main')
    require(predecessor_authority.get('desktop_root_tree') == tree_sha,
            'responsibility mapping authority Desktop tree does not match current Desktop main')

    responsibilities = json.loads(json.dumps(predecessor.get('responsibilities', [])))
    if expected_responsibility_count is not None:
        require(len(responsibilities) == expected_responsibility_count,
                f'expected {expected_responsibility_count} responsibility definitions, got {len(responsibilities)}')


    rid_by_path: dict[str, set[str]] = {}
    coverage_rid_by_path: dict[str, set[str]] = {}
    source_symbols_by_path: dict[str, set[str]] = {}
    source_owners_by_path: dict[str, list[dict]] = {}
    coverage_owners_by_path: dict[str, list[dict]] = {}
    semantic_anchor_paths: set[str] = set()
    coverage_anchor_paths: set[str] = set()
    semantic_anchor_entries = 0
    coverage_anchor_entries = 0

    for item in responsibilities:
        rid = item.get('responsibility_id')
        require(rid, 'responsibility definition without responsibility_id')
        cleaned = []
        for source in item.get('desktop_sources', []):
            path = source.get('path')
            if path not in new_by_path:
                continue
            quality = _source_mapping_quality(rid, source)
            refreshed = dict(source)
            refreshed['mapping_quality'] = quality
            refreshed['blob_sha'] = new_by_path[path]['sha']
            refreshed['source_mode'] = new_by_path[path]['mode']
            refreshed['source_type'] = new_by_path[path]['type']
            cleaned.append(refreshed)
            if quality == 'semantic':
                rid_by_path.setdefault(path, set()).add(rid)
                semantic_anchor_paths.add(path)
                semantic_anchor_entries += 1
                source_symbols_by_path.setdefault(path, set()).update(
                    symbol for symbol in (source.get('symbols') or [])
                    if isinstance(symbol, str) and symbol.strip()
                )
                source_owners_by_path.setdefault(path, []).append({
                    'responsibility_id': rid,
                    'source_owner': item.get('source_owner'),
                    'android_owner': item.get('android_owner'),
                })
            else:
                coverage_rid_by_path.setdefault(path, set()).add(rid)
                coverage_anchor_paths.add(path)
                coverage_anchor_entries += 1
                coverage_owners_by_path.setdefault(path, []).append({
                    'responsibility_id': rid,
                    'source_owner': item.get('source_owner'),
                    'android_owner': item.get('android_owner'),
                })
        item['desktop_sources'] = cleaned

    ids = {item.get('responsibility_id') for item in responsibilities}
    targets_by_rid = {
        item['responsibility_id']: sorted({
            str(target) for target in (item.get('android_target') or [])
            if isinstance(target, str) and target.strip()
        })
        for item in responsibilities
    }

    unmapped_paths = []
    coverage_only_paths = []
    missing_dispositions = []
    missing_targets = []
    for row in ledger_rows:
        path = row['source_path']
        links = rid_by_path.get(path, set())
        coverage_links = coverage_rid_by_path.get(path, set())
        if not links:
            unmapped_paths.append(path)
            if coverage_links:
                coverage_only_paths.append(path)
            row.update({
                'responsibilities': [],
                'source_symbols': [],
                'responsibility_owners': [],
                'android_targets': [],
                'android_target_provenance': 'unmapped; semantic mapping required',
                'coverage_responsibilities': sorted(coverage_links),
                'coverage_responsibility_owners': coverage_owners_by_path.get(path, []),
                'coverage_source_symbols': sorted({
                    symbol
                    for rid in coverage_links
                    for item in responsibilities
                    if item.get('responsibility_id') == rid
                    for source in item.get('desktop_sources', [])
                    if source.get('path') == path
                    for symbol in (source.get('symbols') or [])
                    if isinstance(symbol, str) and symbol.strip()
                }),
                'disposition': None,
                'implementation_status': 'unreviewed',
                'mapping_status': 'coverage-only' if coverage_links else 'unmapped',
                'evidence': [{
                    'kind': 'source-closure-gap',
                    'mapping_origin': mapping_origin,
                    'mapping_artifact_id': mapping_provenance.get('artifact_id'),
                    'mapping_artifact_digest': mapping_provenance.get('artifact_digest'),
                    'status': 'coverage-only-mapping' if coverage_links else 'missing-semantic-mapping',
                    'coverage_responsibilities': sorted(coverage_links),
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
        row['coverage_responsibilities'] = sorted(coverage_links)
        row['coverage_responsibility_owners'] = coverage_owners_by_path.get(path, [])
        row['android_targets'] = sorted({
            target for rid in links for target in targets_by_rid.get(rid, [])
        })
        row['android_target_provenance'] = 'responsibility-ledger-explicit-anchor; provenance-only; not semantic verification'
        predecessor_row = predecessor_source_rows.get(path)
        inherited = predecessor_row.get('disposition') if predecessor_row else None
        if row.get('disposition') not in ALLOWED_SOURCE_DISPOSITIONS:
            if inherited in ALLOWED_SOURCE_DISPOSITIONS:
                row['disposition'] = inherited
            else:
                missing_dispositions.append(path)
        if not row['android_targets']:
            missing_targets.append(path)
        row['implementation_status'] = 'unreviewed'
        row['mapping_status'] = 'semantic'
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
        'mapping_policy': 'semantic responsibility anchors only; legacy fallback anchors are retained as coverage-only metadata and never promoted; no directory or filename fallback is used',
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'android_provenance': android,
        'coverage_only_count': len(coverage_only_paths),
        'coverage_only_paths': coverage_only_paths,
        'mapping_provenance': mapping_provenance,
    })
    save(OUT / 'responsibility-mapping-summary.json', {
        'status': 'incomplete' if (unmapped_paths or missing_dispositions or missing_targets) else 'passed',
        'responsibility_count': len(responsibilities),
        'semantic_source_anchor_entries': semantic_anchor_entries,
        'semantic_source_anchor_unique_paths': len(semantic_anchor_paths),
        'coverage_only_source_anchor_entries': coverage_anchor_entries,
        'coverage_only_source_anchor_unique_paths': len(coverage_anchor_paths),
        'mapped_source_rows': len(ledger_rows) - len(unmapped_paths),
        'unmapped_source_rows': len(unmapped_paths),
        'coverage_only_source_rows': len(coverage_only_paths),
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
            f'Unmapped semantic source rows: {len(unmapped_paths)}; coverage-only rows retained: {len(coverage_only_paths)}.'
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
            'coverage_only_source_anchor_entries': coverage_anchor_entries,
            'coverage_only_source_anchor_unique_paths': len(coverage_anchor_paths),
            'mapped_source_rows': len(ledger_rows) - len(unmapped_paths),
            'unmapped_source_rows': len(unmapped_paths),
            'coverage_only_source_rows': len(coverage_only_paths),
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
    ledger_text = serialize_source_inventory_ledger(new_rows)
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
        'authority_ledger_rows': len(new_rows),
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
        responsibility_doc = merge_responsibility_mapping(
            responsibility_path,
            new_rows,
            new_by_path,
            current,
            tree_sha,
            expected_responsibility_count=EXPECTED_RESPONSIBILITY_COUNT,
        )
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
        ledger_text = serialize_source_inventory_ledger(new_rows)
        ledger_path.write_text(ledger_text, encoding='utf-8')
        manifest['ledger_sha256'] = hashlib.sha256(ledger_text.encode('utf-8')).hexdigest()
        mapping_authority = responsibility_doc.get('authority', {})
        manifest['responsibility_mapping_status'] = mapping_authority.get('status')
        manifest['responsibility_count'] = mapping_authority.get('responsibility_count')
        manifest['semantic_source_anchor_entries'] = mapping_authority.get('semantic_source_anchor_entries')
        manifest['semantic_source_anchor_unique_paths'] = mapping_authority.get('semantic_source_anchor_unique_paths')
        manifest['mapped_source_rows'] = mapping_authority.get('mapped_source_rows')
        manifest['unmapped_source_rows'] = mapping_authority.get('unmapped_source_rows')
        manifest['missing_disposition_rows'] = mapping_authority.get('missing_disposition_rows')
        manifest['missing_target_rows'] = mapping_authority.get('missing_target_rows')
        manifest['responsibility_mapping_artifact_id'] = mapping_authority.get('responsibility_mapping_artifact_id')
        manifest['responsibility_mapping_artifact_digest'] = mapping_authority.get('responsibility_mapping_artifact_digest')
        save(responsibility_path, responsibility_doc)
        save(manifest_path, manifest)

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
    authority_manifest_path = DOC / 'authority/source-inventory-manifest.json'
    require(authority_manifest_path.is_file(), 'Persistent source inventory manifest is missing')
    authority_manifest = json.loads(authority_manifest_path.read_text(encoding='utf-8'))
    require(authority_manifest.get('authority_current') is True,
            'Persistent source inventory manifest is not current')
    require(authority_manifest.get('desktop_commit') == sha,
            'Persistent source inventory manifest Desktop commit mismatch')
    require(authority_manifest.get('desktop_root_tree') == tree_sha,
            'Persistent source inventory manifest Desktop tree mismatch')
    require(authority_manifest.get('tree_entries') == EXPECTED_TREE_ENTRY_COUNT == len(entries),
            'Persistent source inventory manifest must preserve all 9412 recursive tree entries')
    require(authority_manifest.get('tracked_non_tree_entries') == EXPECTED_TRACKED_NON_TREE_COUNT == len(files),
            'Persistent source inventory manifest must preserve all 8258 tracked non-tree entries')
    require(authority_manifest.get('authority_ledger_rows') == EXPECTED_TRACKED_NON_TREE_COUNT,
            'Persistent source inventory manifest row count mismatch')
    _parse_persisted_source_inventory(authority_ledger_text, authority_manifest, expected_paths=by_path)
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