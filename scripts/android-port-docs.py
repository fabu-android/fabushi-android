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
import urllib.request

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
})
INVALID_ANDROID_TARGET_LABELS = frozenset({
    'source/android-host-jni typed host dispatch path',
    'frontend messagingviewmodel json projection consumer',
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
EXPECTED_TREE_ENTRY_COUNT = 9415
EXPECTED_TRACKED_NON_TREE_COUNT = 8260
PREDECESSOR_TREE_ENTRY_COUNT = 9412
PREDECESSOR_TRACKED_NON_TREE_COUNT = 8258
EXPECTED_PACKAGES_SOURCE_COUNT = 852
OWNER_PLACEHOLDER_VALUES = frozenset({
    '', 'unknown', 'tbd', 'todo', 'n/a', 'na', 'none', 'unassigned',
    'not assigned', 'not reviewed', 'placeholder', 'owner unknown',
})
DEFAULT_RESPONSIBILITY_ARTIFACT = '12072277149'
DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST = 'sha256:d15bc9e019dfa01307bf92294f78851d01a4cde2d792ab1377591dde52091f2d'
ANDROID_REPOSITORY = 'fabu-android/fabushi-android'
ANDROID_PARITY_FULL_CI_WORKFLOW_ID = 364063299
EXPECTED_DESKTOP_COMMIT = 'dc81eac2df50f6c08ae4daf98988076bfdc956d9'
EXPECTED_DESKTOP_TREE = '04473e188c5088b66a7f984e5e89e6d47e75eafb'
PREDECESSOR_DESKTOP_COMMIT = '12a4e29819592e3a7e32f7435cb0cf2e3cc53848'
PREDECESSOR_DESKTOP_TREE = '72fc5f777012f6e7bf5f5f471b6f121fd3612bb2'

AGENT_KV_RESPONSIBILITY_ID = 'AGENT-RUN-LIFECYCLE-001'
AGENT_KV_PRODUCTION_STEP = 'Build exact-HEAD native Mahayana Coordinator Host'
AGENT_KV_TEST_STEP = 'Compile, unit-test, lint, and assemble non-release acceptance APKs'
AGENT_KV_SOURCE_SPECS = {
    'source/packages/agent-kv/agent-store.ts': {
        'symbols': ['AgentMetadata', 'AgentMetadataSerde', 'AgentStore2', 'getDefaultAgentMetadata', 'generateBlobEncryptionKeyHex', 'isValidBlobEncryptionKeyHex'],
        'source_owner': 'Desktop AgentStore2 and AgentMetadataSerde in source/packages/agent-kv/agent-store.ts',
        'android_owner': 'Rust Host ProductionAgentStore plus agent-kv AgentMetadataSerde',
        'android_targets': ['source/host/src/extensions/session/production_agent_store.rs', 'source/packages/agent-kv/src/agent_store.rs', 'source/packages/agent-kv/src/lib.rs'],
        'production_target': 'source/host/src/extensions/session/production_agent_store.rs',
        'test_target': 'source/packages/agent-kv/src/agent_store.rs',
        'focused_tests': ['source/packages/agent-kv/src/agent_store.rs::tests::metadata_normalizes_legacy_values', 'source/packages/agent-kv/src/lib.rs::tests::typed_blob_and_metadata_roundtrip'],
        'disposition': 'android-adapted',
    },
    'source/packages/agent-kv/blob-store.ts': {
        'symbols': ['BlobStore', 'isBlobDurable', 'toUint8Array', 'InMemoryBlobStore'],
        'source_owner': 'Desktop content-addressed BlobStore in source/packages/agent-kv/blob-store.ts',
        'android_owner': 'Rust agent-kv BlobStore with the existing Rust Host ProductionAgentStore adapter',
        'android_targets': ['source/packages/agent-kv/src/blob_store.rs', 'source/host/src/extensions/session/production_agent_store.rs'],
        'production_target': 'source/packages/agent-kv/src/blob_store.rs',
        'test_target': 'source/packages/agent-kv/src/blob_store.rs',
        'focused_tests': ['source/packages/agent-kv/src/blob_store.rs::tests::memory_missing_and_digest_validation', 'source/packages/agent-kv/src/blob_store.rs::tests::durable_file_store_detects_corruption'],
        'disposition': 'android-adapted',
    },
    'source/packages/agent-kv/blob-not-found-error.ts': {
        'symbols': ['BlobNotFoundError', 'findBlobNotFoundError'],
        'source_owner': 'Desktop BlobNotFoundError and cause-chain lookup in source/packages/agent-kv/blob-not-found-error.ts',
        'android_owner': 'Rust BlobNotFoundError and BlobStoreError in the existing agent-kv crate',
        'android_targets': ['source/packages/agent-kv/src/blob_not_found_error.rs', 'source/packages/agent-kv/src/blob_store.rs'],
        'production_target': 'source/packages/agent-kv/src/blob_not_found_error.rs',
        'test_target': 'source/packages/agent-kv/src/blob_not_found_error.rs',
        'focused_tests': ['source/packages/agent-kv/src/blob_not_found_error.rs::tests::formats_missing_blob_context', 'source/packages/agent-kv/src/blob_store.rs::tests::memory_missing_and_digest_validation'],
        'disposition': 'android-adapted',
    },
    'source/packages/agent-kv/reference.ts': {
        'symbols': ['Writeable', 'EagerReference', 'LazyReference'],
        'source_owner': 'Desktop EagerReference and LazyReference in source/packages/agent-kv/reference.ts',
        'android_owner': 'Rust BlobRef, EagerReference and LazyReference in the existing agent-kv crate',
        'android_targets': ['source/packages/agent-kv/src/reference.rs', 'source/packages/agent-kv/src/typed_blob_store.rs', 'source/host/src/extensions/session/production_agent_store.rs'],
        'production_target': 'source/packages/agent-kv/src/reference.rs',
        'test_target': 'source/packages/agent-kv/src/reference.rs',
        'focused_tests': ['source/packages/agent-kv/src/reference.rs::tests::sha256_is_strict_and_verified', 'source/packages/agent-kv/src/reference.rs::tests::eager_lazy_roundtrip'],
        'disposition': 'android-adapted',
    },
    'source/packages/agent-kv/serde.ts': {
        'symbols': ['BlobType', 'Serde', 'Utf8Serde', 'ProtoSerde', 'toHex', 'fromHex'],
        'source_owner': 'Desktop Utf8Serde, ProtoSerde and hex serialization in source/packages/agent-kv/serde.ts',
        'android_owner': 'Rust Utf8Codec, JsonCodec and AgentMetadataSerde in the existing agent-kv crate',
        'android_targets': ['source/packages/agent-kv/src/serde.rs', 'source/packages/agent-kv/src/agent_store.rs'],
        'production_target': 'source/packages/agent-kv/src/serde.rs',
        'test_target': 'source/packages/agent-kv/src/serde.rs',
        'focused_tests': ['source/packages/agent-kv/src/serde.rs::tests::utf8_codec_round_trip_and_invalid_bytes_fail_closed', 'source/packages/agent-kv/src/serde.rs::tests::json_codec_round_trip_and_malformed_payload_fail_closed', 'source/packages/agent-kv/src/agent_store.rs::tests::metadata_normalizes_legacy_values'],
        'disposition': 'android-adapted',
    },
    'source/packages/agent-kv/subagent-states.ts': {
        'symbols': ['resolveSubagentPersistedStates'],
        'source_owner': 'Desktop persisted subagent-state resolution in source/packages/agent-kv/subagent-states.ts',
        'android_owner': 'Rust resolve_subagent_state_references integrated by the existing ProductionAgentStore',
        'android_targets': ['source/packages/agent-kv/src/subagent_states.rs', 'source/host/src/extensions/session/production_agent_store.rs'],
        'production_target': 'source/host/src/extensions/session/production_agent_store.rs',
        'test_target': 'source/packages/agent-kv/src/subagent_states.rs',
        'focused_tests': ['source/packages/agent-kv/src/subagent_states.rs::tests::inline_fallback_and_missing_diagnostic', 'source/packages/agent-kv/src/subagent_states.rs::tests::fetches_concurrently_with_bound'],
        'disposition': 'android-adapted',
    },
    'source/packages/agent-kv/typed-blob-store.ts': {
        'symbols': ['BLOB_METADATA_CALLBACK_SYMBOL', 'BlobMetadata', 'BlobMetadataCallback', 'getBlobMetadataCallback', 'unwrapBlobStore', 'TypedBlobStore'],
        'source_owner': 'Desktop TypedBlobStore, metadata callbacks and cloud mirror boundary in source/packages/agent-kv/typed-blob-store.ts',
        'android_owner': 'Rust TypedBlobStore and BlobMirror in the existing agent-kv crate',
        'android_targets': ['source/packages/agent-kv/src/typed_blob_store.rs', 'source/packages/agent-kv/src/blob_store.rs', 'source/packages/agent-kv/src/lib.rs'],
        'production_target': 'source/packages/agent-kv/src/typed_blob_store.rs',
        'test_target': 'source/packages/agent-kv/src/typed_blob_store.rs',
        'focused_tests': ['source/packages/agent-kv/src/typed_blob_store.rs::tests::metadata_and_mirror_write_through', 'source/packages/agent-kv/src/lib.rs::tests::typed_blob_and_metadata_roundtrip'],
        'disposition': 'android-adapted',
    },
}
AGENT_KV_SOURCE_SPEC_BY_PATH = AGENT_KV_SOURCE_SPECS


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
    workflow_sha = os.environ.get('ANDROID_WORKFLOW_SHA') or os.environ.get('GITHUB_WORKFLOW_SHA') or None
    require(workflow_sha == checkout,
            f'Android workflow SHA/checkout mismatch: workflow={workflow_sha} checkout={checkout}')
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
        'workflow_sha': workflow_sha,
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

def _current_android_tracked_files() -> set[str]:
    """Return exact file paths tracked by the current Android checkout."""
    raw = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT)
    paths = {
        entry.decode('utf-8', 'surrogateescape')
        for entry in raw.split(b'\0')
        if entry
    }
    require(bool(paths), 'cannot validate Android targets against an empty tracked file tree')
    return paths


def _filter_android_targets(
    targets: object,
    tracked_paths: set[str] | None = None,
) -> tuple[list[str], list[dict]]:
    """Keep only concrete tracked-file targets and retain every rejected target as a diagnostic."""
    if targets is None:
        return [], []
    if not isinstance(targets, list):
        return [], [{'target': str(targets), 'reason': 'android-targets-not-a-list'}]
    accepted = set()
    rejected = []
    for target in targets:
        if not isinstance(target, str):
            rejected.append({'target': repr(target), 'reason': 'target-not-a-string'})
            continue
        normalized = re.sub(r'\s+', ' ', target).strip()
        if not normalized:
            rejected.append({'target': target, 'reason': 'empty-target'})
            continue
        if normalized.casefold() in INVALID_ANDROID_TARGET_LABELS:
            rejected.append({'target': normalized, 'reason': 'legacy-consumer-label-not-a-path'})
            continue
        if tracked_paths is not None and normalized not in tracked_paths:
            rejected.append({'target': normalized, 'reason': 'not-in-current-android-tracked-file-tree'})
            continue
        accepted.add(normalized)
    return sorted(accepted), sorted(rejected, key=lambda item: (item['target'], item['reason']))


def _real_android_targets(
    targets: object,
    tracked_paths: set[str] | None = None,
) -> list[str]:
    """Return concrete targets only; callers that need auditability use _filter_android_targets."""
    accepted, _ = _filter_android_targets(targets, tracked_paths)
    return accepted


def _has_real_owner(value: object) -> bool:
    if not isinstance(value, str):
        return False
    normalized = re.sub(r'\s+', ' ', value).strip()
    folded = normalized.casefold()
    return bool(normalized) and folded not in OWNER_PLACEHOLDER_VALUES and not (
        folded.startswith('unknown ') or folded.startswith('tbd ') or folded.startswith('todo ')
    )



def _latest_exact_head_android_ci_evidence(android_head: str) -> dict | None:
    """Wait for a real exact-head Android Parity Full CI attempt and named successful steps."""
    if not os.environ.get('GH_TOKEN'):
        print('WARNING: GH_TOKEN is unavailable; agent-kv CI evidence remains incomplete', file=sys.stderr)
        return None
    try:
        wait_seconds = max(0, int(os.environ.get('ANDROID_CI_EVIDENCE_WAIT_SECONDS', '3600')))
    except ValueError:
        wait_seconds = 3600
    deadline = time.monotonic() + wait_seconds
    while True:
        try:
            # Runs can be delayed in the API and a prior terminal run can be returned
            # before the current synchronize run appears. Walk pages and keep polling
            # until the deadline instead of treating that first stale snapshot as final.
            runs_by_id = {}
            for page in range(1, 21):
                response = get(
                    f'/repos/{ANDROID_REPOSITORY}/actions/runs?head_sha={android_head}&per_page=100&page={page}'
                )
                batch = response.get('workflow_runs', [])
                for run in batch:
                    if (
                        run.get('name') == 'Android Parity Full CI'
                        and run.get('workflow_id') == ANDROID_PARITY_FULL_CI_WORKFLOW_ID
                        and run.get('path') == '.github/workflows/android-parity-full-ci.yml'
                        and run.get('event') == 'pull_request'
                        and run.get('head_sha') == android_head
                        and (run.get('repository') or {}).get('full_name') == ANDROID_REPOSITORY
                    ):
                        runs_by_id[run.get('id')] = run
                if len(batch) < 100:
                    break
            runs = list(runs_by_id.values())
            terminal = [
                run for run in runs
                if run.get('status') == 'completed'
                and run.get('conclusion') in {'success', 'failure'}
            ]
            for run in sorted(
                terminal,
                key=lambda value: (value.get('run_number') or 0, value.get('run_attempt') or 0),
                reverse=True,
            ):
                jobs_by_id = {}
                for page in range(1, 21):
                    jobs_response = get(
                        f'/repos/{ANDROID_REPOSITORY}/actions/runs/{run["id"]}/jobs?filter=latest&per_page=100&page={page}'
                    )
                    batch = jobs_response.get('jobs', [])
                    for value in batch:
                        jobs_by_id[value.get('id')] = value
                    if len(batch) < 100:
                        break
                job = next((
                    value for value in jobs_by_id.values()
                    if value.get('name') == 'android-build-unit-lint'
                    and value.get('status') == 'completed'
                    and value.get('conclusion') == 'success'
                    and value.get('run_attempt', run.get('run_attempt')) == run.get('run_attempt')
                ), None)
                if not job:
                    continue
                steps = {
                    value.get('name'): value
                    for value in job.get('steps', [])
                    if isinstance(value, dict) and isinstance(value.get('name'), str)
                }
                production_step = steps.get(AGENT_KV_PRODUCTION_STEP)
                test_step = steps.get(AGENT_KV_TEST_STEP)
                if not all(
                    step and step.get('status') == 'completed' and step.get('conclusion') == 'success'
                    for step in (production_step, test_step)
                ):
                    continue
                return {
                    'workflow_name': run['name'],
                    'workflow_id': run['workflow_id'],
                    'workflow_path': run['path'],
                    'workflow_event': run['event'],
                    'workflow_run_id': run['id'],
                    'run_attempt': run.get('run_attempt'),
                    'run_conclusion': run.get('conclusion'),
                    'workflow_job_name': job['name'],
                    'workflow_job_id': job['id'],
                    'production_step_name': production_step['name'],
                    'production_step_number': production_step.get('number'),
                    'test_step_name': test_step['name'],
                    'test_step_number': test_step.get('number'),
                    'android_head': android_head,
                }
        except (RuntimeError, KeyError, TypeError, ValueError) as error:
            print(
                'WARNING: unable to read exact-head Android Parity Full CI evidence: '
                + type(error).__name__,
                file=sys.stderr,
            )
            return None
        except OSError:
            pass
        if time.monotonic() >= deadline:
            print(
                'WARNING: timed out waiting for terminal exact-head Android Parity Full CI '
                'with successful required job steps; agent-kv evidence remains incomplete',
                file=sys.stderr,
            )
            return None
        time.sleep(15)

def _package_action_job_passed(
    entry: dict,
    *,
    android_head: str,
    tracked_android_paths: set[str],
    action_job_cache: dict[tuple[int, int, str, str], bool],
) -> bool:
    """Count only a successful job and named successful step on the exact Android checkout."""
    run_id = entry.get('workflow_run_id')
    job_id = entry.get('workflow_job_id')
    job_name = entry.get('workflow_job_name')
    step_name = entry.get('workflow_step_name')
    evidence_ref = entry.get('evidence_ref')
    if (
        not isinstance(run_id, int) or isinstance(run_id, bool) or run_id <= 0
        or not isinstance(job_id, int) or isinstance(job_id, bool) or job_id <= 0
        or not isinstance(job_name, str) or not job_name.strip()
        or not isinstance(step_name, str) or not step_name.strip()
        or not isinstance(evidence_ref, str) or not evidence_ref.strip()
    ):
        return False
    evidence_path = evidence_ref.split('#', 1)[0]
    if evidence_path not in tracked_android_paths:
        return False
    key = (run_id, job_id, android_head, step_name)
    if key not in action_job_cache:
        try:
            run = get(f'/repos/{ANDROID_REPOSITORY}/actions/runs/{run_id}')
            if (
                run.get('name') != 'Android Parity Full CI'
                or run.get('workflow_id') != ANDROID_PARITY_FULL_CI_WORKFLOW_ID
                or run.get('path') != '.github/workflows/android-parity-full-ci.yml'
                or run.get('event') != 'pull_request'
                or (run.get('repository') or {}).get('full_name') != ANDROID_REPOSITORY
                or run.get('head_sha') != android_head
                or run.get('status') != 'completed'
                or run.get('conclusion') != 'success'
                or run.get('run_attempt') != entry.get('run_attempt')
            ):
                action_job_cache[key] = False
            else:
                jobs = get(f'/repos/{ANDROID_REPOSITORY}/actions/runs/{run_id}/jobs?filter=latest&per_page=100&page=1')
                job = next((
                    value for value in jobs.get('jobs', [])
                    if value.get('id') == job_id
                ), None)
                if (
                    not job or job.get('name') != job_name
                    or job.get('status') != 'completed'
                    or job.get('conclusion') != 'success'
                    or job.get('run_attempt', run.get('run_attempt')) != run.get('run_attempt')
                ):
                    action_job_cache[key] = False
                else:
                    step = next((
                        value for value in job.get('steps', [])
                        if value.get('name') == step_name
                    ), None)
                    action_job_cache[key] = bool(
                        step and step.get('status') == 'completed'
                        and step.get('conclusion') == 'success'
                    )
        except (RuntimeError, KeyError, TypeError, ValueError, OSError):
            action_job_cache[key] = False
    return action_job_cache[key]


def _has_package_evidence(
    entries: object,
    *,
    source_path: str,
    source_commit: str,
    android_head: str,
    required_kinds: frozenset[str],
    tracked_android_paths: set[str],
    action_job_cache: dict[tuple[int, int, str, str], bool],
) -> bool:
    """Require source-scoped evidence plus a successful exact-head Android CI job."""
    if not isinstance(entries, list):
        return False
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        paths = entry.get('source_paths')
        if not isinstance(paths, list):
            paths = [entry.get('source_path')]
        if (
            source_path in paths and entry.get('kind') in required_kinds
            and entry.get('source_commit') == source_commit
            and entry.get('desktop_commit') == source_commit
            and entry.get('android_pr_head_sha') == android_head
            and entry.get('android_tested_checkout_sha') == android_head
            and entry.get('workflow_sha') == android_head
            and entry.get('result') == 'passed'
            and _package_action_job_passed(
                entry, android_head=android_head,
                tracked_android_paths=tracked_android_paths,
                action_job_cache=action_job_cache,
            )
        ):
            return True
    return False


def _package_semantic_coverage(
    ledger_rows: list[dict],
    responsibilities: list[dict],
    tracked_android_paths: set[str],
) -> dict:
    """Report strict semantic/owner/target/disposition and same-head evidence for all package rows."""
    package_rows = [
        row for row in ledger_rows
        if str(row.get('source_path') or '').startswith('source/packages/')
    ]
    responsibility_by_id = {
        item.get('responsibility_id'): item
        for item in responsibilities
        if item.get('responsibility_id')
    }
    gaps = []
    semantic_rows = 0
    source_owner_rows = 0
    android_owner_rows = 0
    target_rows = 0
    disposition_rows = 0
    action_job_cache: dict[tuple[int, int, str, str], bool] = {}

    for row in package_rows:
        path = row.get('source_path') or '<missing-source-path>'
        linked = []
        linked_anchors = []
        for rid in row.get('responsibilities') or []:
            item = responsibility_by_id.get(rid)
            if item is None:
                continue
            matching_sources = [
                source for source in item.get('desktop_sources') or []
                if source.get('path') == path
                and _source_mapping_quality(rid, source) == 'semantic'
            ]
            if matching_sources:
                linked.append(item)
                linked_anchors.extend((item, source) for source in matching_sources)

        reasons = []
        if row.get('mapping_status') != 'semantic' or not linked:
            reasons.append('missing-semantic-responsibility')
        else:
            semantic_rows += 1

        has_source_owner = any(
            _has_real_owner(source.get('source_owner') or item.get('source_owner'))
            for item, source in linked_anchors
        )
        has_android_owner = any(
            _has_real_owner(source.get('android_owner') or item.get('android_owner'))
            for item, source in linked_anchors
        )
        if has_source_owner:
            source_owner_rows += 1
        else:
            reasons.append('missing-real-source-owner')
        if has_android_owner:
            android_owner_rows += 1
        else:
            reasons.append('missing-real-android-owner')

        expected_head = row.get('android_pr_head_sha')
        expected_source_commit = row.get('source_commit')
        has_production_evidence = any(
            _has_package_evidence(
                item.get('production_evidence'),
                source_path=path,
                source_commit=expected_source_commit,
                android_head=expected_head,
                required_kinds=frozenset({'production-wiring'}),
                tracked_android_paths=tracked_android_paths,
                action_job_cache=action_job_cache,
            )
            for item in linked
        )
        if not has_production_evidence:
            reasons.append('missing-exact-head-production-evidence')

        has_test_evidence = any(
            _has_package_evidence(
                item.get('test_evidence'),
                source_path=path,
                source_commit=expected_source_commit,
                android_head=expected_head,
                required_kinds=frozenset({
                    'focused-unit-test', 'contract-test', 'integration-test',
                    'packaged-test', 'device-test',
                }),
                tracked_android_paths=tracked_android_paths,
                action_job_cache=action_job_cache,
            )
            for item in linked
        )
        if not has_test_evidence:
            reasons.append('missing-exact-head-test-evidence')

        row_targets = row.get('android_targets') or []
        valid_row_targets = [
            target for target in row_targets
            if isinstance(target, str) and target in tracked_android_paths
        ]
        has_responsibility_target = any(
            _real_android_targets(
                source.get('android_targets')
                if isinstance(source.get('android_targets'), list)
                else item.get('android_target'),
                tracked_android_paths,
            )
            for item, source in linked_anchors
        )
        if not row_targets or len(valid_row_targets) != len(row_targets) or not has_responsibility_target:
            reasons.append('missing-valid-tracked-android-target')
        else:
            target_rows += 1

        if path in AGENT_KV_SOURCE_SPEC_BY_PATH:
            has_focused_tests = any(
                isinstance(source.get('focused_tests'), list)
                and bool(source.get('focused_tests'))
                and all(isinstance(test, str) and test.strip() for test in source.get('focused_tests', []))
                for _, source in linked_anchors
            )
            if not has_focused_tests:
                reasons.append('missing-focused-android-tests')

        if row.get('disposition') not in ALLOWED_SOURCE_DISPOSITIONS:
            reasons.append('missing-or-invalid-disposition')
        else:
            disposition_rows += 1

        row['package_semantic_coverage_status'] = 'complete' if not reasons else 'incomplete'
        row['package_semantic_coverage_reasons'] = reasons
        if reasons:
            gaps.append({
                'source_path': path,
                'responsibilities': sorted(
                    item.get('responsibility_id') for item in linked
                    if item.get('responsibility_id')
                ),
                'reasons': reasons,
            })

    count_delta = abs(EXPECTED_PACKAGES_SOURCE_COUNT - len(package_rows))
    strict_incomplete_rows = len(gaps) + count_delta
    return {
        'status': 'passed' if strict_incomplete_rows == 0 else 'incomplete',
        'expected_source_rows': EXPECTED_PACKAGES_SOURCE_COUNT,
        'observed_source_rows': len(package_rows),
        'semantic_responsibility_rows': semantic_rows,
        'real_source_owner_rows': source_owner_rows,
        'real_android_owner_rows': android_owner_rows,
        'valid_android_target_rows': target_rows,
        'valid_disposition_rows': disposition_rows,
        'complete_rows': len(package_rows) - len(gaps),
        'strict_incomplete_rows': strict_incomplete_rows,
        'gaps': gaps,
        'policy': (
            'Every source/packages row requires an explicit semantic source anchor, real source_owner '
            'and android_owner, a tracked Android file target, and one legal disposition. Coverage-only '
            'anchors and provenance-only evidence do not satisfy semantic mapping or verification; '
            'agent-kv sources additionally require source-specific tracked targets, focused tests, and '
            'successful same-exact-head job/step evidence.'
        ),
    }


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



def _ensure_agent_kv_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Attach seven current-main Agent KV sources to the existing canonical Agent lifecycle owner."""
    # Generic fixtures without agent-kv sources must not require a domain responsibility.
    # A real Desktop inventory containing any agent-kv source must map all seven source files.
    if not any(path.startswith('source/packages/agent-kv/') for path in new_by_path):
        return
    missing = sorted(set(AGENT_KV_SOURCE_SPECS) - set(new_by_path))
    require(not missing, 'current Desktop agent-kv inventory is incomplete: ' + ', '.join(missing))
    matches = [
        item for item in responsibilities
        if item.get('responsibility_id') == AGENT_KV_RESPONSIBILITY_ID
    ]
    require(len(matches) == 1,
            'agent-kv semantic mapping requires exactly one ' + AGENT_KV_RESPONSIBILITY_ID)
    item = matches[0]
    item['source_owner'] = (
        'Desktop canonical Agent lifecycle plus AgentStore2/AgentMetadataSerde and the '
        'source/packages/agent-kv storage/reference/serde/subagent owners'
    )
    item['android_owner'] = (
        'Existing Rust Host ProductionAgentStore plus source/packages/agent-kv/src/*; '
        'no parallel Agent store or session owner'
    )
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    for path, spec in AGENT_KV_SOURCE_SPECS.items():
        source_object = new_by_path.get(path)
        require(source_object is not None,
                'agent-kv Desktop source is absent from current authority tree: ' + path)
        require(source_object.get('type') == 'blob'
                and source_object.get('mode') in ALLOWED_SOURCE_MODES,
                'agent-kv Desktop source has invalid blob/mode/type: ' + path)
        source = existing_sources.get(path, {})
        source.update({
            'path': path,
            'blob_sha': source_object['sha'],
            'source_mode': source_object['mode'],
            'source_type': source_object['type'],
            'source_commit': desktop_commit,
            'mapping_quality': 'semantic',
            'symbols': list(spec['symbols']),
            'focused_tests': list(spec['focused_tests']),
            'android_targets': list(spec['android_targets']),
            'source_owner': spec['source_owner'],
            'android_owner': spec['android_owner'],
        })
        existing_sources[path] = source
    item['desktop_sources'] = sorted(
        existing_sources.values(), key=lambda source: source.get('path', '')
    )
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for spec in AGENT_KV_SOURCE_SPECS.values():
        for target in spec['android_targets']:
            if target not in raw_targets:
                raw_targets.append(target)
    item['android_target'] = raw_targets
    item['android_disposition'] = 'android-adapted'
    note = (
        'Current-main Agent KV semantic mapping covers AgentStore2/metadata serde, content-addressed '
        'BlobStore and not-found errors, eager/lazy references, codecs, persisted subagent-state resolution, '
        'and typed blob metadata/mirror semantics. Source-specific Android targets and focused tests are '
        'explicit; exact-head production/test evidence is added only from successful Android Parity Full CI '
        'job steps. Mapping and CI evidence do not promote implementation_status or verification_status.'
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


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """Expose artifact redirects so bearer credentials never cross hosts."""
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def _download_predecessor_bytes(artifact_id: str, token: str) -> bytes:
    endpoint = f'https://api.github.com/repos/{ANDROID_REPOSITORY}/actions/artifacts/{artifact_id}/zip'
    request = Request(endpoint, headers={'Accept':'application/vnd.github+json','Authorization':'Bearer '+token,'User-Agent':'Fabushi-Android-Docs','X-GitHub-Api-Version':'2022-11-28'})
    try:
        try:
            with urllib.request.build_opener(_NoRedirect()).open(request, timeout=60) as response:
                payload = response.read()
        except HTTPError as error:
            if error.code not in (301,302,303,307,308): raise
            location = error.headers.get('Location')
            require(bool(location), 'GitHub artifact redirect omitted Location')
            with urlopen(Request(location,headers={'User-Agent':'Fabushi-Android-Docs','Accept':'application/octet-stream'}),timeout=60) as response:
                payload=response.read()
    except HTTPError as error:
        raise RuntimeError('failed to recover predecessor responsibility artifact '+artifact_id+' (HTTP '+str(error.code)+')') from None
    except Exception as error:
        raise RuntimeError('failed to recover predecessor responsibility artifact '+artifact_id+' ('+type(error).__name__+': '+str(error)+')') from None
    require(payload.startswith(b'PK\x03\x04'),'predecessor artifact download is not a ZIP local-file header')
    return payload


def _load_pinned_responsibility_mapping() -> tuple[dict, dict[str, dict], dict]:
    artifact_id=os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT',DEFAULT_RESPONSIBILITY_ARTIFACT)
    expected_digest=os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST',DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST)
    token=os.environ.get('GH_TOKEN')
    require(token,'GH_TOKEN is required to recover predecessor responsibility mapping')
    payload=_download_predecessor_bytes(artifact_id,token)
    actual_digest='sha256:'+hashlib.sha256(payload).hexdigest()
    require(actual_digest==expected_digest,f'predecessor responsibility artifact digest mismatch: expected={expected_digest} actual={actual_digest}')
    try:
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            bad=archive.testzip()
            require(bad is None,'predecessor artifact ZIP integrity failure: '+str(bad))
            names=archive.namelist()
            rn=[n for n in names if n.endswith('docs/android-port/authority/responsibility-ledger.json')]
            mn=[n for n in names if n.endswith('docs/android-port/authority/source-inventory-manifest.json')]
            ln=[n for n in names if n.endswith('docs/android-port/authority/source-inventory-ledger.jsonl')]
            require(len(rn)==len(mn)==len(ln)==1,'predecessor artifact is missing or duplicates an authority input')
            candidate=json.loads(archive.read(rn[0]).decode('utf-8'))
            manifest=json.loads(archive.read(mn[0]).decode('utf-8'))
            source_rows={}
            for line in archive.read(ln[0]).decode('utf-8').splitlines():
                if line.strip():
                    row=json.loads(line); source_rows[row['source_path']]=row
    except Exception as error:
        raise RuntimeError('predecessor responsibility artifact is invalid') from error
    require(isinstance(candidate,dict) and candidate.get('responsibilities'),'predecessor responsibility mapping is empty')
    require(len(candidate['responsibilities'])==EXPECTED_RESPONSIBILITY_COUNT,f'expected {EXPECTED_RESPONSIBILITY_COUNT} responsibility definitions, got {len(candidate["responsibilities"])}')
    require(manifest.get('tracked_non_tree_entries')==PREDECESSOR_TRACKED_NON_TREE_COUNT,'predecessor manifest does not cover historical 8258-row inventory')
    require(manifest.get('tree_entries')==PREDECESSOR_TREE_ENTRY_COUNT,'predecessor manifest does not preserve historical 9412-entry tree')
    require(len(source_rows)==PREDECESSOR_TRACKED_NON_TREE_COUNT,f'predecessor source ledger must contain 8258 unique rows, got {len(source_rows)}')
    return candidate,source_rows,{'artifact_id':int(artifact_id),'artifact_digest':actual_digest,'source_manifest_run_id':manifest.get('workflow_run_id'),'source_manifest_android_head':manifest.get('android_pr_head_sha'),'source_manifest_desktop_commit':manifest.get('desktop_commit'),'source_manifest_desktop_root_tree':manifest.get('desktop_root_tree')}


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
    if manifest.get('source_commit') is not None:
        require(manifest.get('source_commit') == desktop_commit,
                'local source inventory manifest source_commit mismatch')
    require(manifest.get('tracked_non_tree_entries') == EXPECTED_TRACKED_NON_TREE_COUNT,
            'local source authority does not preserve the 8260-entry current tracked non-tree inventory')
    require(len(source_rows) == EXPECTED_TRACKED_NON_TREE_COUNT,
            f'local source inventory must contain 8260 unique rows, got {len(source_rows)}')
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


def _validate_rebaseline_boundary(
    baseline: dict,
    prior_manifest: dict,
    prior_ledger_text: str,
    current: str,
    tree_sha: str,
    tree_entry_count: int,
    tracked_non_tree_count: int,
) -> list[dict]:
    """Accept only the pinned historical inventory or an already-current inventory."""
    require(baseline.get('desktop_repository') == 'bhrumom/fabushi-desktop',
            'rebaseline baseline repository mismatch')
    require(current == EXPECTED_DESKTOP_COMMIT,
            'Desktop main commit differs from the pinned current authority')
    require(tree_sha == EXPECTED_DESKTOP_TREE,
            'Desktop main tree differs from the pinned current authority')
    require(baseline.get('desktop_commit') == current,
            'baseline Desktop commit does not match current main')
    require(baseline.get('desktop_root_tree') == tree_sha,
            'baseline Desktop tree does not match current main')
    require(tree_entry_count == EXPECTED_TREE_ENTRY_COUNT,
            f'current Desktop tree must contain {EXPECTED_TREE_ENTRY_COUNT} recursive entries, got {tree_entry_count}')
    require(tracked_non_tree_count == EXPECTED_TRACKED_NON_TREE_COUNT,
            f'current Desktop tree must contain {EXPECTED_TRACKED_NON_TREE_COUNT} non-tree entries, got {tracked_non_tree_count}')
    require(prior_manifest.get('authority_current') is True,
            'persisted source manifest is not a validated authority snapshot')
    prior_commit = prior_manifest.get('desktop_commit')
    prior_tree = prior_manifest.get('desktop_root_tree')
    if (prior_commit, prior_tree) == (PREDECESSOR_DESKTOP_COMMIT, PREDECESSOR_DESKTOP_TREE):
        prior_rows_expected = PREDECESSOR_TRACKED_NON_TREE_COUNT
        prior_tree_expected = PREDECESSOR_TREE_ENTRY_COUNT
    elif (prior_commit, prior_tree) == (current, tree_sha):
        prior_rows_expected = EXPECTED_TRACKED_NON_TREE_COUNT
        prior_tree_expected = EXPECTED_TREE_ENTRY_COUNT
    else:
        raise ValueError('persisted source manifest is neither the pinned historical 8258/9412 predecessor nor the current 8260/9415 authority')
    require(prior_manifest.get('source_commit') == prior_commit,
            'persisted source manifest source_commit does not match its Desktop commit')
    require(prior_manifest.get('tree_entries') == prior_tree_expected,
            f'persisted source manifest tree count does not match its authority boundary ({prior_tree_expected})')
    require(prior_manifest.get('tracked_non_tree_entries') == prior_rows_expected,
            f'persisted source manifest non-tree count does not match its authority boundary ({prior_rows_expected})')
    require(prior_manifest.get('authority_ledger_rows') == prior_rows_expected,
            f'persisted source manifest ledger count does not match its authority boundary ({prior_rows_expected})')
    actual_digest = hashlib.sha256(prior_ledger_text.encode('utf-8')).hexdigest()
    require(actual_digest == prior_manifest.get('ledger_sha256'),
            'persisted source ledger digest does not match its source manifest')
    rows, paths = [], []
    for line_number, line in enumerate(prior_ledger_text.splitlines(), 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f'persisted source ledger has invalid JSON at line {line_number}') from error
        require(isinstance(row, dict), f'persisted source ledger row {line_number} is not an object')
        path = row.get('source_path')
        require(isinstance(path, str) and path, 'persisted source ledger row has no source_path')
        source_path = PurePosixPath(path)
        require(not source_path.is_absolute() and '..' not in source_path.parts,
                'persisted source ledger contains unsafe path: ' + path)
        require(row.get('source_commit') == prior_commit,
                'persisted source ledger row has stale source commit: ' + path)
        require(row.get('source_type') in ALLOWED_SOURCE_TYPES
                and row.get('source_mode') in ALLOWED_SOURCE_MODES
                and SHA.fullmatch(str(row.get('source_blob_sha') or '')) is not None,
                'persisted source ledger row has invalid blob/mode/type: ' + path)
        paths.append(path)
        rows.append(row)
    require(paths == sorted(paths), 'persisted source ledger is not in canonical sorted path order')
    require(len(paths) == len(set(paths)) == prior_rows_expected,
            f'persisted source ledger expected {prior_rows_expected} unique rows, got {len(paths)}')
    return rows


def validate_exact_head_provenance(
    manifest: dict,
    ledger_rows: list[dict],
    responsibility_doc: dict,
    expected_android_checkout_sha: str,
) -> None:
    """Validate persisted provenance without upgrading mapping or implementation status."""
    require(SHA.fullmatch(expected_android_checkout_sha or '') is not None,
            'invalid expected Android checkout SHA')
    for field in ('android_pr_head_sha', 'android_tested_checkout_sha', 'workflow_sha'):
        require(manifest.get(field) == expected_android_checkout_sha,
                f'exact-head manifest mismatch for {field}: expected {expected_android_checkout_sha}')
    desktop_commit = manifest.get('desktop_commit')
    desktop_tree = manifest.get('desktop_root_tree')
    run_id = manifest.get('workflow_run_id')
    require(SHA.fullmatch(str(desktop_commit or '')) is not None,
            'exact-head manifest has invalid Desktop commit')
    require(SHA.fullmatch(str(desktop_tree or '')) is not None,
            'exact-head manifest has invalid Desktop tree')
    require(manifest.get('source_commit') == desktop_commit,
            'exact-head manifest source_commit mismatch')
    for field in ('android_pr_head_sha', 'android_tested_checkout_sha', 'workflow_sha'):
        require(responsibility_doc.get(field) == expected_android_checkout_sha,
                f'exact-head responsibility ledger mismatch for {field}')
    require(responsibility_doc.get('source_commit') == desktop_commit,
            'exact-head responsibility ledger source_commit mismatch')
    require(isinstance(run_id, int) and run_id > 0,
            'exact-head manifest has invalid workflow run id')

    authority = responsibility_doc.get('authority') or {}
    require(authority.get('desktop_commit') == desktop_commit,
            'exact-head responsibility ledger Desktop commit mismatch')
    require(authority.get('desktop_root_tree') == desktop_tree,
            'exact-head responsibility ledger Desktop tree mismatch')
    responsibilities = responsibility_doc.get('responsibilities')
    require(isinstance(responsibilities, list) and len(responsibilities) == EXPECTED_RESPONSIBILITY_COUNT,
            f'exact-head responsibility ledger must retain {EXPECTED_RESPONSIBILITY_COUNT} definitions')
    responsibility_ids = [item.get('responsibility_id') for item in responsibilities]
    require(all(isinstance(rid, str) and rid for rid in responsibility_ids)
            and len(set(responsibility_ids)) == EXPECTED_RESPONSIBILITY_COUNT,
            'exact-head responsibility ledger must retain 306 unique responsibility definitions')

    rows_by_path = {}
    for row in ledger_rows:
        path = row.get('source_path')
        require(isinstance(path, str) and path and path not in rows_by_path,
                'exact-head source ledger contains a missing or duplicate path')
        require(row.get('source_commit') == desktop_commit,
                'exact-head source ledger has stale Desktop source commit: ' + path)
        for field in ('android_pr_head_sha', 'android_tested_checkout_sha', 'workflow_sha'):
            require(row.get(field) == expected_android_checkout_sha,
                    f'exact-head source ledger row mismatch for {field}: ' + path)
        require(row.get('source_type') in ALLOWED_SOURCE_TYPES
                and row.get('source_mode') in ALLOWED_SOURCE_MODES
                and SHA.fullmatch(str(row.get('source_blob_sha') or '')) is not None,
                'exact-head source ledger row has invalid blob/mode/type: ' + path)
        require(row.get('implementation_status') == 'unreviewed'
                and row.get('verification_status') == 'not-verified',
                'exact-head source ledger status was promoted without verification: ' + path)
        evidence = row.get('evidence')
        require(isinstance(evidence, list) and evidence,
                'exact-head source ledger row lacks provenance evidence: ' + path)
        for item in evidence:
            require(isinstance(item, dict), 'exact-head source ledger has malformed evidence: ' + path)
            require(item.get('source_path') == path
                    and item.get('source_blob_sha') == row.get('source_blob_sha'),
                    'exact-head source evidence does not match current source object: ' + path)
            require(item.get('desktop_commit') == desktop_commit
                    and item.get('desktop_root_tree') == desktop_tree,
                    'exact-head source evidence has stale Desktop authority: ' + path)
            require(item.get('source_commit') == desktop_commit,
                    'exact-head source evidence has stale Desktop source commit: ' + path)
            require(item.get('android_head') == expected_android_checkout_sha,
                    'exact-head source evidence has stale Android head: ' + path)
            require(item.get('android_pr_head_sha') == expected_android_checkout_sha
                    and item.get('android_tested_checkout_sha') == expected_android_checkout_sha
                    and item.get('workflow_sha') == expected_android_checkout_sha,
                    'exact-head source evidence has inconsistent Android provenance: ' + path)
            require(item.get('workflow_run_id') == run_id,
                    'exact-head source evidence has stale workflow run: ' + path)
        rows_by_path[path] = row

    for responsibility in responsibilities:
        rid = responsibility.get('responsibility_id') or '<missing-id>'
        require(responsibility.get('source_commit') == desktop_commit,
                'exact-head responsibility has stale Desktop source commit: ' + rid)
        require(responsibility.get('android_head_at_update') == expected_android_checkout_sha,
                'exact-head responsibility has stale Android head: ' + rid)
        require(responsibility.get('implementation_status') == 'unreviewed'
                and responsibility.get('verification_status') == 'not-verified',
                'exact-head responsibility status was promoted without verification: ' + rid)
        for source in responsibility.get('desktop_sources') or []:
            path = source.get('path')
            row = rows_by_path.get(path)
            require(row is not None,
                    'exact-head responsibility anchor is absent from source ledger: ' + str(path))
            require(source.get('source_commit') == desktop_commit
                    and source.get('blob_sha') == row.get('source_blob_sha')
                    and source.get('source_mode') == row.get('source_mode')
                    and source.get('source_type') == row.get('source_type'),
                    'exact-head responsibility anchor differs from current Desktop object: ' + str(path))


def load_predecessor_responsibility_mapping(responsibility_path: Path) -> tuple[dict, dict[str, dict], str, dict]:
    """Fail closed unless the pinned, digest-verified predecessor artifact is available."""
    predecessor,source_rows,provenance=_load_pinned_responsibility_mapping()
    candidate=None
    try:
        if responsibility_path.is_file() and responsibility_path.stat().st_size:
            value=json.loads(responsibility_path.read_text(encoding='utf-8'))
            if isinstance(value,dict) and value.get('responsibilities'): candidate=value
    except (OSError,ValueError,TypeError,AttributeError,json.JSONDecodeError): candidate=None
    if candidate is None: return predecessor,source_rows,'pinned-predecessor-artifact',provenance
    require(len(candidate['responsibilities'])==EXPECTED_RESPONSIBILITY_COUNT,f'expected {EXPECTED_RESPONSIBILITY_COUNT} responsibility definitions, got {len(candidate["responsibilities"])}')
    return _merge_responsibility_sources(candidate,predecessor),source_rows,'current-responsibility-ledger+pin-predecessor-coverage',provenance


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
    require(predecessor_authority.get('desktop_commit') == mapping_provenance.get('source_manifest_desktop_commit'), 'pinned mapping and predecessor manifest Desktop commit disagree')
    require(predecessor_authority.get('desktop_root_tree') == mapping_provenance.get('source_manifest_desktop_root_tree'), 'pinned mapping and predecessor manifest tree disagree')
    require(mapping_provenance.get('artifact_id') is not None and mapping_provenance.get('artifact_digest'), 'mapping must originate from a verified predecessor artifact')

    responsibilities = json.loads(json.dumps(predecessor.get('responsibilities', [])))
    if any(path.startswith('source/packages/agent-kv/') for path in new_by_path):
        _ensure_agent_kv_semantic_mapping(responsibilities, new_by_path, current)
    if expected_responsibility_count is not None:
        require(len(responsibilities) == expected_responsibility_count,
                f'expected {expected_responsibility_count} responsibility definitions, got {len(responsibilities)}')


    rid_by_path: dict[str, set[str]] = {}
    coverage_rid_by_path: dict[str, set[str]] = {}
    source_symbols_by_path: dict[str, set[str]] = {}
    source_owners_by_path: dict[str, list[dict]] = {}
    coverage_owners_by_path: dict[str, list[dict]] = {}
    source_targets_raw_by_path: dict[str, list] = {}
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
            refreshed['source_commit'] = current
            refreshed['mapping_quality'] = quality
            refreshed['blob_sha'] = new_by_path[path]['sha']
            refreshed['source_mode'] = new_by_path[path]['mode']
            refreshed['source_type'] = new_by_path[path]['type']
            cleaned.append(refreshed)
            if isinstance(source.get('android_targets'), list):
                source_targets_raw_by_path[path] = list(source['android_targets'])
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
                    'source_owner': source.get('source_owner') or item.get('source_owner'),
                    'android_owner': source.get('android_owner') or item.get('android_owner'),
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

    tracked_android_paths = _current_android_tracked_files()
    has_agent_kv_inventory = any(path in new_by_path for path in AGENT_KV_SOURCE_SPECS)
    rejected_android_targets = []
    source_targets_by_path: dict[str, list[str]] = {}
    ids = {item.get('responsibility_id') for item in responsibilities}
    require(len(ids) == len(responsibilities),
            'responsibility mapping contains duplicate responsibility IDs')
    for item in responsibilities:
        rid = item['responsibility_id']
        raw_targets = item.get('android_target')
        filtered_targets, rejected = _filter_android_targets(raw_targets, tracked_android_paths)
        if raw_targets is not None and raw_targets != filtered_targets:
            item['android_target'] = filtered_targets
        if rejected:
            item['android_target_rejections'] = rejected
            rejected_android_targets.extend({
                'responsibility_id': rid,
                'target': entry['target'],
                'reason': entry['reason'],
            } for entry in rejected)
            note = (
                'Rejected Android target labels or paths absent from the current tracked Android file tree; '
                'see generated/rejected-android-targets.json. Rejections are diagnostic and are not parity evidence.'
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
        else:
            item.pop('android_target_rejections', None)
    targets_by_rid = {
        item['responsibility_id']: _real_android_targets(
            item.get('android_target'), tracked_android_paths
        )
        for item in responsibilities
    }
    if has_agent_kv_inventory:
        for path, spec in AGENT_KV_SOURCE_SPECS.items():
            missing_targets = [target for target in spec['android_targets'] if target not in tracked_android_paths]
            require(not missing_targets,
                    'agent-kv Android target is not tracked at exact HEAD for ' + path + ': ' + ', '.join(missing_targets))
        for path, raw_targets in source_targets_raw_by_path.items():
            accepted_targets, rejected_targets = _filter_android_targets(raw_targets, tracked_android_paths)
            source_targets_by_path[path] = accepted_targets
            for rejected in rejected_targets:
                rejected_android_targets.append({
                    'responsibility_id': AGENT_KV_RESPONSIBILITY_ID,
                    'source_path': path,
                    'target': rejected['target'],
                    'reason': rejected['reason'],
                })


        ci_evidence = _latest_exact_head_android_ci_evidence(android['android_pr_head_sha'])
        agent_kv_responsibility = next(
            item for item in responsibilities
            if item.get('responsibility_id') == AGENT_KV_RESPONSIBILITY_ID
        )
        agent_kv_paths = set(AGENT_KV_SOURCE_SPECS)
        for evidence_key in ('production_evidence', 'test_evidence'):
            existing_evidence = agent_kv_responsibility.get(evidence_key)
            if not isinstance(existing_evidence, list):
                existing_evidence = []
            agent_kv_responsibility[evidence_key] = [
                evidence for evidence in existing_evidence
                if not any(path in agent_kv_paths for path in (
                    evidence.get('source_paths') if isinstance(evidence.get('source_paths'), list)
                    else [evidence.get('source_path')]
                ))
            ]
        if ci_evidence is not None:
            for path, spec in AGENT_KV_SOURCE_SPECS.items():
                production_target = spec['production_target']
                test_target = spec['test_target']
                require(production_target in tracked_android_paths and test_target in tracked_android_paths,
                        'agent-kv evidence target is not tracked at exact HEAD: ' + path)
                common = {
                    'source_paths': [path],
                    'source_commit': current,
                    'desktop_commit': current,
                    'desktop_root_tree': tree_sha,
                    'android_pr_head_sha': android['android_pr_head_sha'],
                    'android_tested_checkout_sha': android['android_checkout_sha'],
                    'workflow_sha': android['workflow_sha'],
                    'workflow_name': ci_evidence['workflow_name'],
                    'workflow_run_id': ci_evidence['workflow_run_id'],
                    'workflow_job_name': ci_evidence['workflow_job_name'],
                    'workflow_job_id': ci_evidence['workflow_job_id'],
                    'run_attempt': ci_evidence['run_attempt'],
                    'run_conclusion': ci_evidence['run_conclusion'],
                    'result': 'passed',
                }
                agent_kv_responsibility['production_evidence'].append({
                    **common,
                    'kind': 'production-wiring',
                    'workflow_step_name': ci_evidence['production_step_name'],
                    'workflow_step_number': ci_evidence['production_step_number'],
                    'evidence_ref': production_target,
                })
                agent_kv_responsibility['test_evidence'].append({
                    **common,
                    'kind': 'focused-unit-test',
                    'workflow_step_name': ci_evidence['test_step_name'],
                    'workflow_step_number': ci_evidence['test_step_number'],
                    'evidence_ref': test_target,
                    'focused_tests': list(spec['focused_tests']),
                })
        else:
            print(
                'WARNING: no successful same-exact-head Android build/unit-test job and step evidence; '
                'agent-kv coverage remains incomplete',
                file=sys.stderr,
            )

    ids = {item.get('responsibility_id') for item in responsibilities}

    unmapped_paths = []
    coverage_only_paths = []
    missing_dispositions = []
    missing_targets = []
    for row in ledger_rows:
        path = row['source_path']
        source_object = new_by_path.get(path)
        require(source_object is not None,
                'source inventory row is absent from current Desktop tree: ' + path)
        row['source_repository'] = 'bhrumom/fabushi-desktop'
        row['source_commit'] = current
        row['source_blob_sha'] = source_object['sha']
        row['source_mode'] = source_object['mode']
        row['source_type'] = source_object['type']
        row['android_pr_head_sha'] = android['android_pr_head_sha']
        row['android_tested_checkout_sha'] = android['android_checkout_sha']
        row['workflow_sha'] = android['workflow_sha']
        row['verification_status'] = 'not-verified'
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
                    'source_commit': current,
                    'android_head': android['android_pr_head_sha'],
                    'android_pr_head_sha': android['android_pr_head_sha'],
                    'android_tested_checkout_sha': android['android_checkout_sha'],
                    'workflow_sha': android['workflow_sha'],
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
        row['android_targets'] = source_targets_by_path[path] if path in source_targets_by_path else sorted({
            target for rid in links for target in targets_by_rid.get(rid, [])
        })
        row['android_target_provenance'] = 'responsibility-ledger-explicit-anchor; provenance-only; not semantic verification'
        agent_kv_spec = AGENT_KV_SOURCE_SPEC_BY_PATH.get(path)
        if agent_kv_spec is not None:
            row['disposition'] = agent_kv_spec['disposition']
        else:
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
            'source_commit': current,
            'android_head': android['android_pr_head_sha'],
            'android_pr_head_sha': android['android_pr_head_sha'],
            'android_tested_checkout_sha': android['android_checkout_sha'],
            'workflow_sha': android['workflow_sha'],
            'android_pr_base_sha': android['android_pr_base_sha'],
            'workflow_event': android['workflow_event'],
            'workflow_run_id': android['workflow_run_id'],
            'run_attempt': android['run_attempt'],
            'verification_status': 'not-verified',
        }]


    package_coverage = _package_semantic_coverage(
        ledger_rows, responsibilities, tracked_android_paths
    )
    package_coverage_summary = {
        key: value for key, value in package_coverage.items() if key != 'gaps'
    }
    save(OUT / 'packages-semantic-coverage.json', package_coverage)
    save(OUT / 'rejected-android-targets.json', {
        'status': 'incomplete' if rejected_android_targets else 'passed',
        'count': len(rejected_android_targets),
        'rejected_targets': sorted(
            rejected_android_targets,
            key=lambda item: (item['responsibility_id'], item['target'], item['reason']),
        ),
        'policy': 'Rejected targets are retained for diagnosis and never count as Android parity evidence.',
    })
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
        'status': 'incomplete' if (
            unmapped_paths or missing_dispositions or missing_targets
            or package_coverage_summary['strict_incomplete_rows']
        ) else 'passed',
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
        'packages_semantic_coverage': package_coverage_summary,
        'rejected_android_target_count': len(rejected_android_targets),
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
        'source_commit': current,
        'android_pr_head_sha': android['android_pr_head_sha'],
        'android_tested_checkout_sha': android['android_checkout_sha'],
        'workflow_sha': android['workflow_sha'],
        'authority': {
            'desktop_repository': 'bhrumom/fabushi-desktop',
            'desktop_commit': current,
            'desktop_root_tree': tree_sha,
            'android_repository': 'fabu-android/fabushi-android',
            'android_pr': 3,
            'status': 'incomplete' if (
                unmapped_paths or missing_dispositions or missing_targets
                or package_coverage_summary['strict_incomplete_rows']
            ) else 'mapped-provenance-only',
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
            'packages_semantic_coverage': package_coverage_summary,
            'rejected_android_target_count': len(rejected_android_targets),
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
    require(current == EXPECTED_DESKTOP_COMMIT and tree_sha == EXPECTED_DESKTOP_TREE,
            'Desktop authority changed from the pinned dc81eac2/04473e18 current-main lock')
    require(len(entries) == EXPECTED_TREE_ENTRY_COUNT,
            f'current Desktop recursive tree count mismatch: expected {EXPECTED_TREE_ENTRY_COUNT}, got {len(entries)}')
    require(len(files) == EXPECTED_TRACKED_NON_TREE_COUNT,
            f'current Desktop non-tree count mismatch: expected {EXPECTED_TRACKED_NON_TREE_COUNT}, got {len(files)}')
    require(manifest_path.is_file() and ledger_path.is_file(),
            'persisted source manifest and ledger are required for a guarded rebaseline')
    prior_manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    prior_ledger_text = ledger_path.read_text(encoding='utf-8')
    old_rows = _validate_rebaseline_boundary(
        baseline, prior_manifest, prior_ledger_text, current, tree_sha, len(entries), len(files)
    )
    old_by_path = {row['source_path']: row for row in old_rows}
    require(responsibility_path.is_file(), 'Persistent responsibility ledger is missing')
    prior_responsibility_doc = json.loads(responsibility_path.read_text(encoding='utf-8'))
    prior_authority = prior_responsibility_doc.get('authority') or {}
    require(prior_responsibility_doc.get('source_commit') == prior_manifest.get('desktop_commit'),
            'persisted responsibility ledger source_commit differs from source manifest')
    require(prior_authority.get('desktop_commit') == prior_manifest.get('desktop_commit')
            and prior_authority.get('desktop_root_tree') == prior_manifest.get('desktop_root_tree'),
            'persisted responsibility ledger authority differs from source manifest')
    prior_responsibilities = prior_responsibility_doc.get('responsibilities')
    require(isinstance(prior_responsibilities, list)
            and len(prior_responsibilities) == EXPECTED_RESPONSIBILITY_COUNT,
            f'persisted responsibility ledger must retain {EXPECTED_RESPONSIBILITY_COUNT} definitions')
    prior_ids = [item.get('responsibility_id') for item in prior_responsibilities]
    require(all(isinstance(item, str) and item for item in prior_ids)
            and len(set(prior_ids)) == EXPECTED_RESPONSIBILITY_COUNT,
            'persisted responsibility ledger must retain 306 unique responsibility IDs')
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
    top_counts = {}
    for item in files:
        top = item['path'].split('/')[0]
        top_counts[top] = top_counts.get(top, 0) + 1
    android = current_android_provenance()
    manifest = {
        'schema_version': 2,
        'desktop_commit': current,
        'source_commit': current,
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
    baseline['observed_date'] = time.strftime('%Y-%m-%d', time.gmtime())
    baseline['desktop_commit'] = current
    baseline['desktop_root_tree'] = tree_sha
    require(responsibility_path.is_file(), 'Persistent responsibility ledger is missing')

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
                expected_source_fields = {
                    'source_commit': current, 'blob_sha': item['sha'],
                    'source_mode': item['mode'], 'source_type': item['type'],
                }
                if any(refreshed.get(field) != value for field, value in expected_source_fields.items()):
                    touched = True
                refreshed.update(expected_source_fields)
                refreshed_sources.append(refreshed)
            if touched:
                responsibility['desktop_sources'] = refreshed_sources
                responsibility['implementation_status'] = 'unreviewed'
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
                if path in new_by_path and _source_mapping_quality(rid, source) == 'semantic':
                    reverse_links.setdefault(path, set()).add(rid)
        for row in new_rows:
            linked = reverse_links.get(row['source_path'])
            if linked:
                row['responsibilities'] = sorted(set(row.get('responsibilities') or []) | linked)
        ledger_text = serialize_source_inventory_ledger(new_rows)
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
        manifest['packages_semantic_coverage'] = mapping_authority.get('packages_semantic_coverage')
        manifest['rejected_android_target_count'] = mapping_authority.get('rejected_android_target_count')
        responsibility_doc['source_commit'] = current
        responsibility_doc['android_pr_head_sha'] = android['android_pr_head_sha']
        responsibility_doc['android_tested_checkout_sha'] = android['android_checkout_sha']
        responsibility_doc['workflow_sha'] = android['workflow_sha']
        manifest['authority_ledger_rows'] = len(new_rows)
        manifest['tree_entries'] = len(entries)
        manifest['tracked_non_tree_entries'] = len(files)
        persisted_rows = _parse_persisted_source_inventory(
            ledger_text, manifest, expected_paths=new_by_path
        )
        validate_exact_head_provenance(
            manifest, list(persisted_rows.values()), responsibility_doc,
            android['android_checkout_sha'],
        )
        latest_desktop_main = get(f'/repos/{repo}/branches/main')['commit']['sha']
        require(latest_desktop_main == current,
                'Desktop main moved during rebaseline; authority files were not persisted')
        ledger_path.write_text(ledger_text, encoding='utf-8')
        save(responsibility_path, responsibility_doc)
        save(manifest_path, manifest)
        save(baseline_path, baseline)

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
    require(sha == EXPECTED_DESKTOP_COMMIT and tree_sha == EXPECTED_DESKTOP_TREE,
            'baseline does not match the pinned current Desktop main authority')
    before = get(f'/repos/{repo}/branches/main')['commit']['sha']
    require(before == sha, 'Desktop main moved beyond the pinned baseline; rebaseline required')
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
            'Persistent source inventory manifest must preserve all 9415 recursive tree entries')
    require(authority_manifest.get('tracked_non_tree_entries') == EXPECTED_TRACKED_NON_TREE_COUNT == len(files),
            'Persistent source inventory manifest must preserve all 8260 tracked non-tree entries')
    require(authority_manifest.get('authority_ledger_rows') == EXPECTED_TRACKED_NON_TREE_COUNT,
            'Persistent source inventory manifest row count mismatch')
    authority_rows = _parse_persisted_source_inventory(
        authority_ledger_text, authority_manifest, expected_paths=by_path
    )
    package_coverage_path = OUT / 'packages-semantic-coverage.json'
    require(package_coverage_path.is_file(),
            'source/packages semantic coverage diagnostics are missing')
    package_coverage = json.loads(package_coverage_path.read_text(encoding='utf-8'))
    require(package_coverage.get('observed_source_rows') == EXPECTED_PACKAGES_SOURCE_COUNT,
            'source/packages semantic coverage must retain all 852 source rows')
    responsibility_path = DOC / 'authority/responsibility-ledger.json'
    require(responsibility_path.is_file(), 'Persistent responsibility ledger is missing')
    responsibility_doc = json.loads(responsibility_path.read_text(encoding='utf-8'))
    validate_exact_head_provenance(
        authority_manifest, list(authority_rows.values()), responsibility_doc,
        android['android_checkout_sha'],
    )
    manifest = {'schema_version': 2, 'desktop_commit': sha, 'source_commit': sha, 'desktop_root_tree': tree_sha, 'desktop_main_before': before, 'desktop_main_after': after, 'authority_current': before == sha == after, 'android_tested_checkout_sha': android['android_checkout_sha'], 'android_repository': android['android_repository'], 'android_pr': android['android_pr'], 'android_pr_head_sha': android['android_pr_head_sha'], 'android_pr_head_ref': android['android_pr_head_ref'], 'android_pr_base_sha': android['android_pr_base_sha'], 'android_pr_base_ref': android['android_pr_base_ref'], 'workflow_event': android['workflow_event'], 'workflow_run_id': android['workflow_run_id'], 'run_attempt': android['run_attempt'], 'workflow_sha': android['workflow_sha'], 'workflow_ref': android['workflow_ref'], 'run_url': android['run_url'], 'generated_at_utc': android['generated_at_utc'], 'tree_entries': len(entries), 'tracked_non_tree_entries': len(files), 'root_counts': top_counts, 'gitlinks': [item for item in files if item['type'] == 'commit'], 'symlinks': [item for item in files if item['mode'] == '120000'], 'truncation_fallback_used': fallback, 'initial_status': 'unreviewed', 'ledger_sha256': hashlib.sha256(authority_ledger_text.encode('utf-8')).hexdigest() if authority_ledger_text else None, 'authority_ledger_rows': len([line for line in authority_ledger_text.splitlines() if line.strip()]), 'packages_semantic_coverage': {key: value for key, value in package_coverage.items() if key != 'gaps'}, 'closure_review': 'submodule-LFS-dynamic-import-and-semantic-review-pending', 'product_acceptance': 'not-claimed'}
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


def audit_receipt() -> None:
    """Create an independent receipt in RUNNER_TEMP; never persist it into the repository."""
    runner_temp = os.environ.get('RUNNER_TEMP')
    require(bool(runner_temp), 'RUNNER_TEMP is required')
    token = os.environ.get('GH_TOKEN')
    require(bool(token), 'GH_TOKEN is required to download the predecessor artifact')
    artifact_id = os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT', DEFAULT_RESPONSIBILITY_ARTIFACT)
    expected_digest = os.environ.get('ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST', DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST)
    artifact_bytes = _download_predecessor_bytes(artifact_id, token)
    require(artifact_bytes.startswith(b'PK\x03\x04'), 'predecessor download is not a ZIP artifact')
    actual_digest = 'sha256:' + hashlib.sha256(artifact_bytes).hexdigest()
    require(actual_digest == expected_digest, 'predecessor artifact SHA-256 mismatch: expected '
            + expected_digest + ', got ' + actual_digest)
    try:
        with zipfile.ZipFile(io.BytesIO(artifact_bytes)) as archive:
            bad_member = archive.testzip()
            require(bad_member is None, 'predecessor artifact ZIP integrity failure: ' + str(bad_member))
            artifact_entries = archive.namelist()
            require(bool(artifact_entries), 'predecessor artifact is empty')
    except zipfile.BadZipFile as error:
        raise RuntimeError('predecessor artifact ZIP invalid: ' + str(error)) from None

    inputs = {
        'baseline': DOC / 'authority/baseline.json',
        'source_inventory_manifest': DOC / 'authority/source-inventory-manifest.json',
        'source_inventory_ledger': DOC / 'authority/source-inventory-ledger.jsonl',
        'responsibility_ledger': DOC / 'authority/responsibility-ledger.json',
        'package_semantic_coverage': OUT / 'packages-semantic-coverage.json',
    }
    digests, docs = {}, {}
    for name, path in inputs.items():
        require(path.is_file(), 'missing receipt input: ' + str(path.relative_to(ROOT)))
        raw = path.read_bytes()
        digests[name] = hashlib.sha256(raw).hexdigest()
        if path.suffix == '.json':
            docs[name] = json.loads(raw.decode('utf-8'))
    baseline, manifest = docs['baseline'], docs['source_inventory_manifest']
    responsibility = docs['responsibility_ledger']
    coverage = docs['package_semantic_coverage']
    require(baseline.get('desktop_repository') == 'bhrumom/fabushi-desktop'
            and baseline.get('desktop_commit') == EXPECTED_DESKTOP_COMMIT
            and baseline.get('desktop_root_tree') == EXPECTED_DESKTOP_TREE,
            'receipt baseline does not match the pinned current Desktop authority')
    require(manifest.get('authority_current') is True
            and manifest.get('desktop_commit') == baseline.get('desktop_commit')
            and manifest.get('source_commit') == manifest.get('desktop_commit')
            and manifest.get('desktop_root_tree') == baseline.get('desktop_root_tree'),
            'receipt source manifest and baseline authority disagree')
    require(manifest.get('tree_entries') == EXPECTED_TREE_ENTRY_COUNT
            and manifest.get('tracked_non_tree_entries') == EXPECTED_TRACKED_NON_TREE_COUNT
            and manifest.get('authority_ledger_rows') == EXPECTED_TRACKED_NON_TREE_COUNT,
            'receipt manifest does not preserve the current 8260/9415 authority boundary')
    tree_path = OUT / 'desktop-complete-tree.json'
    require(tree_path.is_file(), 'current Desktop complete-tree evidence is missing')
    tree_doc = json.loads(tree_path.read_text(encoding='utf-8'))
    require(tree_doc.get('sha') == manifest.get('desktop_root_tree'),
            'receipt complete-tree evidence does not match the current Desktop root tree')
    tree_entries = tree_doc.get('tree')
    require(isinstance(tree_entries, list) and len(tree_entries) == EXPECTED_TREE_ENTRY_COUNT,
            'receipt complete-tree evidence has the wrong recursive entry count')
    tree_paths = [item.get('path') for item in tree_entries]
    require(all(isinstance(path, str) and path for path in tree_paths)
            and len(tree_paths) == len(set(tree_paths)),
            'receipt complete-tree evidence contains missing or duplicate paths')
    expected_paths = {item['path']: item for item in tree_entries if item.get('type') != 'tree'}
    require(len(expected_paths) == EXPECTED_TRACKED_NON_TREE_COUNT,
            'receipt complete-tree evidence has the wrong non-tree entry count')
    source_ledger_text = inputs['source_inventory_ledger'].read_text(encoding='utf-8')
    source_rows_by_path = _parse_persisted_source_inventory(
        source_ledger_text, manifest, expected_paths=expected_paths
    )
    source_rows = list(source_rows_by_path.values())
    responsibility_rows = responsibility.get('responsibilities', [])
    source_paths = [row.get('source_path') for row in source_rows]
    responsibility_ids = [row.get('responsibility_id') for row in responsibility_rows]
    require(len(source_paths) == len(set(source_paths)) == EXPECTED_TRACKED_NON_TREE_COUNT,
            'receipt requires 8260 unique current source_path values')
    require(len(responsibility_ids) == len(set(responsibility_ids)) == EXPECTED_RESPONSIBILITY_COUNT,
            'receipt requires 306 unique responsibility IDs')
    provenance = current_android_provenance()
    validate_exact_head_provenance(
        manifest, source_rows, responsibility, provenance['android_checkout_sha']
    )
    package_rows = [row for row in source_rows if row['source_path'].startswith('source/packages/')]
    require(len(package_rows) == EXPECTED_PACKAGES_SOURCE_COUNT,
            'receipt package inventory does not preserve all 852 source/packages rows')
    require(coverage.get('expected_source_rows') == EXPECTED_PACKAGES_SOURCE_COUNT
            and coverage.get('observed_source_rows') == EXPECTED_PACKAGES_SOURCE_COUNT,
            'receipt package coverage has an incorrect expected/observed denominator')
    require(coverage.get('complete_rows', 0) + coverage.get('strict_incomplete_rows', 0)
            == EXPECTED_PACKAGES_SOURCE_COUNT,
            'receipt package coverage does not account for all source/packages rows')
    require(coverage.get('strict_incomplete_rows') == sum(
        row.get('package_semantic_coverage_status') == 'incomplete' for row in package_rows
    ), 'receipt strict-incomplete count disagrees with source ledger rows')
    require(coverage.get('complete_rows') == sum(
        row.get('package_semantic_coverage_status') == 'complete' for row in package_rows
    ), 'receipt complete count disagrees with source ledger rows')
    require(coverage.get('status') == (
        'passed' if coverage.get('strict_incomplete_rows') == 0 else 'incomplete'
    ), 'receipt package coverage status disagrees with strict closure counts')
    rejected_path = OUT / 'rejected-android-targets.json'
    require(rejected_path.is_file(), 'rejected target diagnostics missing')
    rejected = json.loads(rejected_path.read_text(encoding='utf-8'))
    tracked = _current_android_tracked_files()
    target_rows = []
    for item in responsibility_rows:
        raw_targets = item.get('android_target', item.get('android_targets', []))
        accepted, invalid = _filter_android_targets(raw_targets, tracked)
        target_rows.append({
            'responsibility_id': item.get('responsibility_id'),
            'source_owner': item.get('source_owner'),
            'android_owner': item.get('android_owner'),
            'tracked_targets': accepted,
            'rejected_targets': invalid,
            'disposition': item.get('android_disposition', item.get('disposition')),
            'implementation_status': item.get('implementation_status', 'unreviewed'),
            'verification_status': item.get('verification_status', 'not-verified'),
        })
    receipt_doc = {
        'schema': 'MAHAYANA_ANDROID_AUDIT_RECEIPT_V1',
        'status': 'audit-only-not-product-acceptance',
        'predecessor_artifact': {'artifact_id': int(artifact_id), 'downloaded_bytes_sha256': actual_digest,
                                 'expected_sha256': expected_digest, 'zip_integrity_verified': True,
                                 'entry_count': len(artifact_entries)},
        'authority_input_sha256': digests,
        'provenance': provenance,
        'desktop_authority': {'repository': baseline.get('desktop_repository'),
                              'commit': manifest.get('desktop_commit'),
                              'tree': manifest.get('desktop_root_tree'),
                              'tree_entries': manifest.get('tree_entries'),
                              'tracked_non_tree_entries': manifest.get('tracked_non_tree_entries')},
        'android_three_sha': {'pr_head_sha': provenance['android_pr_head_sha'],
                              'checkout_sha': provenance['android_checkout_sha'],
                              'workflow_sha': provenance['workflow_sha']},
        'inventory': {'unique_source_path_count': len(set(source_paths)),
                      'unique_responsibility_id_count': len(set(responsibility_ids)),
                      'source_status_counts': {s: sum(r.get('implementation_status') == s for r in source_rows)
                                               for s in ('unreviewed','mapped','implemented','verified','not-applicable')},
                      'all_source_rows_unreviewed': all(r.get('implementation_status') == 'unreviewed' for r in source_rows),
                      'responsibility_unreviewed_count': sum(r.get('implementation_status') == 'unreviewed' for r in responsibility_rows),
                      'responsibility_not_verified_count': sum(r.get('verification_status') == 'not-verified' for r in responsibility_rows)},
        'package_semantic_coverage': {k:v for k,v in coverage.items() if k != 'gaps'},
        'rejected_android_targets': rejected,
        'responsibility_target_disposition_audit': target_rows,
    }
    output = Path(runner_temp) / 'android-port-audit-receipt.json'
    output.write_text(json.dumps(receipt_doc, ensure_ascii=False, sort_keys=True, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'receipt_path': str(output), 'predecessor_artifact_sha256': actual_digest,
                      'authority_inputs': digests, 'unique_source_paths': len(set(source_paths)),
                      'unique_responsibility_ids': len(set(responsibility_ids)),
                      'strict_incomplete_rows': coverage.get('strict_incomplete_rows'),
                      'status': receipt_doc['status']}, sort_keys=True))


def main() -> int:
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        print('Run this validation only in GitHub Actions.', file=sys.stderr)
        return 2
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=('rebaseline', 'inventory', 'validate', 'bundle', 'receipt'))
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    try:
        {'rebaseline': rebaseline, 'inventory': inventory, 'validate': validate, 'bundle': bundle, 'receipt': audit_receipt}[args.command]()
        return 0
    except (ValueError, RuntimeError, OSError) as error:
        print(type(error).__name__ + ': ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())