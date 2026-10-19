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
from urllib.parse import unquote, urlparse
from urllib.request import Request, urlopen
import zipfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / 'docs/android-port'
OUT = DOC / 'generated'
SHA = re.compile(r'^[0-9a-f]{40}$')

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
EXPECTED_TREE_ENTRY_COUNT = 9453
EXPECTED_TRACKED_NON_TREE_COUNT = 8298
PRIOR_TREE_ENTRY_COUNT = 9443
PRIOR_TRACKED_NON_TREE_COUNT = 8288
PERSISTED_INTERMEDIATE_TREE_ENTRY_COUNT = 9431
PERSISTED_INTERMEDIATE_TRACKED_NON_TREE_COUNT = 8276
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
ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME = 'Grok Android Architecture'
ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID = 364026685
ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH = '.github/workflows/grok-android-architecture.yml'
ANDROID_AGENT_KV_EVIDENCE_JOB_NAME = 'agent-kv-exact-head-evidence'
PREDECESSOR_SEMANTIC_SOURCE_ANCHOR_COUNT = 6066
EXPECTED_DESKTOP_COMMIT = '2b394d7551f3dbbe1320bba7f771a4694134e382'
EXPECTED_DESKTOP_TREE = '96c80d118016cbe962018fbadcd83b41b04c5158'
PRIOR_DESKTOP_COMMIT = '0ecf132681845231b597ece11bc5f979a9a8f0cc'
PRIOR_DESKTOP_TREE = 'af3eb1b18ea3b6c80aa8359623e07f473f251c5b'
PRIOR_BASELINE_PARENT_DESKTOP_COMMIT = '72ca525b7c2f5d1d605bf4a3261485609bd2c05f'
PRIOR_BASELINE_PARENT_DESKTOP_TREE = 'd19997afb9e1c4f3473777ac9ef2b96d6ef6a6da'
PERSISTED_INTERMEDIATE_DESKTOP_COMMIT = '12887d2a44ba450b13ca8390066a39418c52e0c9'
PERSISTED_INTERMEDIATE_DESKTOP_TREE = '4bc59314edd76a51bd8f0e2d0654935d9e8ffb41'
PREDECESSOR_DESKTOP_COMMIT = '12a4e29819592e3a7e32f7435cb0cf2e3cc53848'
PREDECESSOR_DESKTOP_TREE = '72fc5f777012f6e7bf5f5f471b6f121fd3612bb2'

AGENT_KV_RESPONSIBILITY_ID = 'AGENT-RUN-LIFECYCLE-001'
AGENT_KV_PRODUCTION_STEP = 'Build exact-HEAD agent-kv production owners'
AGENT_KV_TEST_STEP = 'Test exact-HEAD agent-kv production owners'
OFFICIAL_DEVICE_PRODUCTION_STEP = 'Verify exact-HEAD official-device registration owner'
OFFICIAL_DEVICE_TEST_STEP = 'Test exact-HEAD official-device registration owner'
OFFICIAL_DEVICE_EVIDENCE_SOURCE_PATH = 'source/product/fabushi/fabushi-official-device-registry.ts'
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


CLOUD_AGENT_CONSTANT_SOURCE_PATH = 'source/packages/constants/cloud-agent.ts'
CLOUD_AGENT_CONSTANT_RESPONSIBILITY_ID = 'AGENT-RUNTIME-PROMPT-ASSEMBLY-ANDROID-001'
CLOUD_AGENT_CONSTANT_PRODUCTION_STEP = 'Verify exact-HEAD cloud-agent artifact prompt owner'
CLOUD_AGENT_CONSTANT_TEST_STEP = 'Test exact-HEAD cloud-agent artifact prompt owner'
CLOUD_AGENT_CONSTANT_SOURCE_SPEC = {
    'symbols': ['CLOUD_AGENT_ARTIFACTS_DIR'],
    'source_owner': (
        'Desktop source/packages/constants/cloud-agent.ts owns the canonical walkthrough artifact '
        'fallback directory consumed by source/packages/agent/prompts/shared.ts, then the shipping '
        'testing/parent.ts -> testing/index.ts -> composer2-cloud-testing-sections.ts prompt chain'
    ),
    'android_owner': (
        'Existing Rust Host android_system_prompt root-turn prompt owner consumes the shared '
        'fabushi-constants cloud-agent artifact-directory contract; AndroidJsonHost remains the '
        'shipping caller and no second prompt or artifact-directory owner is introduced'
    ),
    'android_targets': [
        'source/packages/constants/src/cloud_agent.rs',
        'source/host/src/runner/android_system_prompt.rs',
        'source/host/src/android_json_runtime.rs',
    ],
    'production_target': 'source/host/src/android_json_runtime.rs',
    'test_target': 'source/host/src/runner/android_system_prompt.rs',
    'focused_tests': [
        'source/host/src/runner/android_system_prompt.rs::tests::walkthrough_artifact_directory_uses_canonical_cloud_agent_fallback_and_normalizes_override',
        'source/host/src/runner/android_system_prompt.rs::tests::walkthrough_artifact_prompt_is_deduped_and_does_not_grant_capability',
    ],
    'disposition': 'android-adapted',
}



AGENT_CORE_INTERACTION_RESPONSIBILITY_ID = 'AGENT-RUN-LIFECYCLE-001'
AGENT_CORE_INTERACTION_PRODUCTION_STEP = 'Verify exact-HEAD agent-core interaction production owner'
AGENT_CORE_INTERACTION_TEST_STEP = 'Test exact-HEAD agent-core interaction production owner'
AGENT_CORE_INTERACTION_SOURCE_SPECS = {
    'source/packages/constants/ask-question.ts': {
        'symbols': [
            'ASK_QUESTION_AUTO_ANSWER_MARKER',
            'ASK_QUESTION_AUTO_ANSWER_REASON_PREFIX',
            'ASK_QUESTION_AUTO_ANSWER_REASON_BODY',
            'AskQuestionAutoAnswerIdentity',
            'createAskQuestionAutoAnswerIdentity',
            'formatAskQuestionAutoAnswerReason',
            'parseAskQuestionAutoAnswerIdentity',
            'isAskQuestionAutoAnswerReason',
            'ASK_QUESTION_AUTO_ANSWER_REASON',
        ],
        'source_owner': (
            'Desktop source/packages/constants/ask-question.ts owns canonical AskQuestion auto-answer '
            'identity/reason detection; source/packages/agent/tools/core/ask-question/index.ts consumes it in '
            'shouldReceiptAskQuestionResult, source/packages/agent/actions/ask-question-completion.ts applies that '
            'decision before markAskQuestionCompleted, replay-horizon.ts owns live async-original identity recovery, '
            'and async-ask-question-completion-action-handler.ts resumes the Agent turn with the settled result'
        ),
        'android_owner': (
            'Rust fabushi-constants ask_question identity contract is consumed by the shipping Host '
            'SharedAgentInteractionOwner::settle_response/should_receipt_ask_question_result owner; that same owner '
            'holds pending/completed receipt deduplication and account/process fencing, while TranscriptStore supplies '
            'the bounded live replay horizon used by turn_ended pruning without introducing a second receipt owner'
        ),
        'android_targets': [
            'source/packages/constants/src/ask_question.rs',
            'source/host/src/runner/agent_v1_interaction.rs',
            'source/host/src/extensions/transcript/transcript_store.rs',
        ],
        'production_target': 'source/host/src/runner/agent_v1_interaction.rs',
        'test_target': 'source/host/src/runner/agent_v1_interaction.rs',
        'focused_tests': [
            'source/packages/constants/src/ask_question.rs::tests::structured_identity_round_trips_and_legacy_prefix_remains_recognized',
            'source/host/src/runner/agent_v1_interaction.rs::tests::structured_and_legacy_auto_answer_rejections_do_not_consume_the_completion_receipt_identity',
            'source/host/src/runner/agent_v1_interaction.rs::tests::ordinary_async_success_error_and_user_rejection_consume_completion_receipts',
            'source/host/src/runner/agent_v1_interaction.rs::tests::duplicate_sync_settlement_is_rejected_without_replacing_the_first_result',
            'source/host/src/runner/agent_v1_interaction.rs::tests::completed_receipts_follow_the_live_window_while_pending_receipts_are_preserved',
            'source/host/src/runner/agent_v1_interaction.rs::tests::completed_receipt_deduplication_is_account_and_process_fenced',
            'source/host/src/runner/agent_v1_interaction.rs::tests::stale_account_and_process_epoch_responses_fail_closed',
            'source/host/src/extensions/transcript/transcript_store.rs::tests::async_ask_question_replay_window_is_account_agent_scoped_and_bounded',
            'source/host/src/extensions/transcript/transcript_store.rs::tests::live_async_ask_question_ids_follow_the_current_transcript_window_and_fallback',
        ],
        'disposition': 'android-adapted',
    },
    'source/packages/agent-core/interaction-listener.ts': {
        'symbols': ['InteractionListener', 'flushPostTurnEndedWork', 'NoopInteractionListener', 'InteractionListenerStreamClosedError'],
        'source_owner': (
            'Desktop Agent interaction listener owns update/query delivery and bounded post-turn work flushing '
            'used by the shipping Agent runtime'
        ),
        'android_owner': (
            'Rust Host AgentV1 interaction listener and AndroidJsonHost/JNI dispatch path; Coordinator owns '
            'account/process fencing and no presentation layer owns query settlement'
        ),
        'android_targets': [
            'source/host/src/runner/agent_v1_interaction.rs',
            'source/host/src/android_json_runtime.rs',
            'source/android-host-jni/src/lib.rs',
        ],
        'production_target': 'source/host/src/runner/agent_v1_interaction.rs',
        'test_target': 'source/host/src/runner/agent_v1_interaction.rs',
        'focused_tests': [
            'generated_compatible_interaction_query_response_wire_roundtrip',
            'run_async_completion_is_enqueued_and_privacy_projection_is_redacted',
            'native_dispatch_routes_interaction_post_turn_flush_through_coordinator_host',
        ],
        'disposition': 'android-adapted',
    },
    'source/packages/agent-core/interaction-queries.ts': {
        'symbols': ['InteractionQueryListener', 'DeferredInteractionResponseError', 'Responses', 'queryWebSearch', 'queryWebFetch', 'queryMcpAuth'],
        'source_owner': (
            'Desktop Agent interaction query/response wire constructors own typed approval/question response '
            'settlement over generated Agent v1 protocol messages'
        ),
        'android_owner': (
            'Rust Host AgentV1InteractionQuery/Response wire owner with AndroidJsonHost settlement and '
            'account/process epoch fencing'
        ),
        'android_targets': [
            'source/host/src/runner/agent_v1_interaction.rs',
            'source/host/src/android_json_runtime.rs',
        ],
        'production_target': 'source/host/src/runner/agent_v1_interaction.rs',
        'test_target': 'source/host/src/runner/agent_v1_interaction.rs',
        'focused_tests': [
            'generated_compatible_interaction_query_response_wire_roundtrip',
            'android_json_host_settles_generated_ask_question_response_with_exact_fence',
            'android_json_host_rejects_stale_interaction_response_after_account_or_process_change',
        ],
        'disposition': 'android-adapted',
    },
    'source/packages/agent-core/interaction-updates.ts': {
        'symbols': ['Updates', 'TurnUsage', 'textDelta', 'toolCallStarted', 'toolCallCompleted', 'turnEnded'],
        'source_owner': (
            'Desktop Agent interaction update constructors own typed streaming/tool/turn-ended update wire '
            'shape and usage settlement before transcript projection'
        ),
        'android_owner': (
            'Rust Host AgentV1 interaction update/projection owner consumed by AndroidJsonHost; JNI transports '
            'the Host projection without creating a second transcript owner'
        ),
        'android_targets': [
            'source/host/src/runner/agent_v1_interaction.rs',
            'source/host/src/android_json_runtime.rs',
            'source/android-host-jni/src/lib.rs',
        ],
        'production_target': 'source/host/src/runner/agent_v1_interaction.rs',
        'test_target': 'source/host/src/runner/agent_v1_interaction.rs',
        'focused_tests': [
            'run_async_completion_is_enqueued_and_privacy_projection_is_redacted',
            'stale_account_and_process_epoch_responses_fail_closed',
            'native_dispatch_routes_interaction_post_turn_flush_through_coordinator_host',
        ],
        'disposition': 'android-adapted',
    },
}


BACKGROUND_COMPLETION_RESPONSIBILITY_ID = 'AGENT-GENERATED-SUBAGENT-001'
BACKGROUND_COMPLETION_PRODUCTION_STEP = 'Verify exact-HEAD background completion production owner'
BACKGROUND_COMPLETION_TEST_STEP = 'Test exact-HEAD background completion production owner'
BACKGROUND_COMPLETION_SOURCE_SPECS = {
    'source/packages/agent/actions/background-task-completion.ts': {
        'symbols': [
            'BackgroundTaskCompletion',
            'BackgroundCompletionsPrompt',
            'formatBackgroundCompletions',
            'neutralizeBackgroundTaskDetailCloseTags',
        ],
        'source_owner': (
            'Desktop background-task completion formatter owns canonical system_notification '
            'prompt formatting, completion metadata and notification acknowledgement policy'
        ),
        'android_owner': (
            'Rust Host completion_revivals formatter consumed by AndroidJsonHost durable '
            'background completion revival turns'
        ),
        'android_targets': [
            'source/packages/constants/src/system_notification.rs',
            'source/host/src/extensions/transcript/completion_revivals.rs',
            'source/host/src/android_json_runtime.rs',
            'source/android-host-jni/src/lib.rs',
            'source/host/tests/android_background_completion_shipping_contract.rs',
        ],
        'production_target': 'source/host/src/extensions/transcript/completion_revivals.rs',
        'test_target': 'source/host/tests/android_background_completion_shipping_contract.rs',
        'focused_tests': [
            'source/host/tests/android_background_completion_shipping_contract.rs::background_completion_formatter_uses_canonical_system_notification_wrapper',
            'source/android-host-jni/src/lib.rs::tests::background_completion_hidden_turn_owner_is_linked_through_jni_android_json_host',
        ],
        'disposition': 'android-adapted',
    },
    'source/packages/agent/actions/background-task-completion-action-handler.ts': {
        'symbols': [
            'BackgroundTaskCompletionActionHandler',
            'prepareFollowupTurn',
            'handle',
        ],
        'source_owner': (
            'Desktop BackgroundTaskCompletionActionHandler owns the hidden simulated-user '
            'completion follow-up turn and dispatch into the Agent run loop'
        ),
        'android_owner': (
            'Rust AndroidJsonHost durable background completion revival queue plus canonical '
            'ProductionTurnDispatch/turn journal; JNI only transports the Host runtime'
        ),
        'android_targets': [
            'source/host/src/extensions/transcript/completion_revivals.rs',
            'source/host/src/android_json_runtime.rs',
            'source/host/src/runner/durable_turn_journal.rs',
            'source/android-host-jni/src/lib.rs',
            'source/host/tests/android_background_completion_shipping_contract.rs',
        ],
        'production_target': 'source/host/src/android_json_runtime.rs',
        'test_target': 'source/host/tests/android_background_completion_shipping_contract.rs',
        'focused_tests': [
            'source/host/tests/android_background_completion_shipping_contract.rs::shipping_runtime_routes_completion_prompts_through_jni_host_hidden_turn_queue',
            'source/android-host-jni/src/lib.rs::tests::background_completion_hidden_turn_owner_is_linked_through_jni_android_json_host',
        ],
        'disposition': 'android-adapted',
    },
    'source/packages/constants/system-notification.ts': {
        'symbols': [
            'SYSTEM_NOTIFICATION_TAG',
            'SYSTEM_NOTIFICATION_OPEN_TAG',
            'SYSTEM_NOTIFICATION_CLOSE_TAG',
        ],
        'source_owner': (
            'Desktop source/packages/constants/system-notification.ts@06b02de059b028a3f8dd5e4f78fc1a27b4b92349 '
            'declares the canonical SYSTEM_NOTIFICATION_TAG and derives SYSTEM_NOTIFICATION_OPEN_TAG/CLOSE_TAG '
            'as source declarations. Current Desktop production directly consumes only SYSTEM_NOTIFICATION_TAG; '
            'the OPEN/CLOSE constants have no direct production caller, while the prompt renderer materializes '
            'the matching section tags from the title. The verified direct shipping consumer '
            'source/packages/agent/actions/background-task-completion.ts@f7c2bd46d2b06a48c774f70905efa915779fdb8b '
            'imports SYSTEM_NOTIFICATION_TAG and renders BackgroundCompletionsPrompt with that section title; '
            'source/packages/agent/actions/background-task-completion-action-handler.ts@'
            'b7e8a047584480e79ec86644648e355bdf22a9b3 calls formatBackgroundCompletions, emits the synthetic '
            'BACKGROUND_TASK_COMPLETION user-message update, creates the follow-up Agent turn, and appends the '
            'same prompt into the root prompt executor. source/packages/agent/index.ts@'
            'f069375000904ce19414bafb070b896c64ee3b3c registers that handler as backgroundTaskCompletionAction '
            'on AnysphereAgent and runStream dispatches the incoming action case through the registered handler'
        ),
        'android_owner': (
            'Rust fabushi-constants system_notification contract consumed by shipping '
            'Host completion_revivals formatting. Android explicitly stores the open/close literals to render '
            'the same shipping wire shape; this is an Android implementation detail, not evidence that the '
            'Desktop OPEN/CLOSE constants themselves have production callers'
        ),
        'android_targets': [
            'source/packages/constants/src/system_notification.rs',
            'source/host/src/extensions/transcript/completion_revivals.rs',
            'source/host/src/android_json_runtime.rs',
            'source/android-host-jni/src/lib.rs',
            'source/host/tests/android_background_completion_shipping_contract.rs',
        ],
        'production_target': 'source/host/src/extensions/transcript/completion_revivals.rs',
        'test_target': 'source/host/tests/android_background_completion_shipping_contract.rs',
        'focused_tests': [
            'source/packages/constants/src/system_notification.rs::tests::wrapper_is_recognizable_without_matching_partial_tags',
            'source/host/tests/android_background_completion_shipping_contract.rs::background_completion_formatter_uses_canonical_system_notification_wrapper',
            'source/android-host-jni/src/lib.rs::tests::background_completion_hidden_turn_owner_is_linked_through_jni_android_json_host',
        ],
        'disposition': 'android-adapted',
    },
}

COMPOSER_CONSTANT_SOURCE_PATH = 'source/packages/constants/composer.ts'
COMPOSER_CONSTANT_RESPONSIBILITY_ID = 'COMPOSER-DRAFT-ANDROID-001'
COMPOSER_CONSTANT_PRODUCTION_STEP = 'Verify exact-HEAD constants composer production owner'
COMPOSER_CONSTANT_TEST_STEP = 'Test exact-HEAD constants composer production owner'
COMPOSER_CONSTANT_SOURCE_SPEC = {
    'symbols': ['MAX_TEXT_SIZE'],
    'source_owner': 'Desktop source/packages/constants/composer.ts owns the canonical 8 MiB Composer text-size contract',
    'android_owner': 'Rust fabushi-constants composer contract enforced by Host DurableComposerDraftOwner',
    'android_targets': [
        'source/packages/constants/src/composer.rs',
        'source/host/src/composer_draft.rs',
    ],
    'production_target': 'source/host/src/composer_draft.rs',
    'test_target': 'source/host/src/composer_draft.rs',
    'focused_tests': [
        'source/packages/constants/src/composer.rs::tests::text_limit_is_inclusive',
        'source/host/src/composer_draft.rs::tests::prompt_size_limit_matches_desktop_composer_contract',
    ],
    'disposition': 'android-adapted',
}



AUTO_SPILLOVER_UI_SOURCE_PATH = 'source/packages/constants/auto-spillover-ui.ts'
AUTO_SPILLOVER_UI_RESPONSIBILITY_ID = 'ACCOUNT-USAGE-CONTROLS-ANDROID-001'
AUTO_SPILLOVER_UI_PRODUCTION_STEP = 'Verify exact-HEAD auto-spillover usage UI production owner'
AUTO_SPILLOVER_UI_TEST_STEP = 'Test exact-HEAD auto-spillover usage UI replacement'
AUTO_SPILLOVER_UI_SOURCE_SPEC = {
    'symbols': [
        'AUTO_SPILLOVER_UI_DEFAULTS',
        'autoTitle',
        'autoDescription',
        'apiTitle',
        'apiDescription',
        'autoBeyondLimitDescription',
        'autoUsageBarLabel',
        'apiUsageBarLabel',
    ],
    'source_owner': (
        'Desktop source/packages/constants/auto-spillover-ui.ts@16af77656b1b1a01b0dba7541737c0ef233fe760 '
        'declares seven recovered spillover copy defaults only and has no runtime import at current authority. '
        'The mechanically recovered source/shared/node/experiments/experiment-config.gen.ts@'
        'b4ac08fbb9ec2bf6cccc77c1b1245e08d295ff5e independently declares the same local defaults and assigns '
        'them to EXPERIMENTS.auto_spillover_ui_config.fallbackValues. source/shared/node/experiments/'
        'cursor-experiments.ts@b9f644e1795bccc5c27aacc5b0e90b5953409de9 imports the EXPERIMENTS registry, resolves fallback/live '
        'experiment values, and includes every experiment in its snapshot; electron-main adapters/experiments.ts@'
        '4cefd20a838019c34ac1e954ef5d6d86d223e013 ships that SandExperimentService and emits snapshots. '
        'An exact-authority full-tree git-grep must prove the AUTO_SPILLOVER_UI_DEFAULTS occurrences are limited '
        'to the standalone constants declaration plus the generated registry declaration/fallback reference, and '
        'that auto_spillover_ui_config has no named consumer outside the generated registry. Current shipping '
        'Usage & Billing state/UI is independently owned by source/electron-main/account/cursor-profile.ts@'
        '69645a4dee5b40a700fef5199c044892f46eaf3a fetchSandUsageSummary/buildSandUsageSummary, then '
        'frontend/src/recovered/features/settings/overlay/desktop.ts@ff8e61b13ea9475c502da6ed58a0906fd44538d6 '
        'bridge.cursorAccount.getUsageSummary, desktop-surface.tsx@2ccb6c4f94c094495c24907be32cee6447796d6d '
        'UsageSettingsPanel wiring and panels.tsx@994d0e009430e6dfb0b1edb39f717b673be40a8f '
        'usageMetersFromSummary; those shipping views use Trial/Weekly/On-demand copy rather than the seven defaults'
    ),
    'android_owner': (
        'Rust fabushi-constants auto_spillover_ui preserves the seven source literals as a declaration-only '
        'parity asset; it is not a shipping usage state owner and is intentionally not wired through JNI or '
        'Compose. Existing shipping ownership remains CursorDashboardMcpAuthBackend.resolve_sand_usage_summary '
        '-> AndroidJsonHost feature.account.usageSummary -> AndroidCoordinatorRuntime.accountUsageSummary -> '
        'AndroidAccountUsageSummary -> FabushiSettingsSurface, with account/generation fencing and reconciliation'
    ),
    'android_targets': [
        'source/packages/constants/src/auto_spillover_ui.rs',
        'source/host/src/mcp_auth/dashboard_mcp_auth_backend.rs',
        'source/host/src/android_json_runtime.rs',
        'source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/runtime/AndroidCoordinatorPort.kt',
        'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt',
        'frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/FabushiSettingsSurface.kt',
        'mobile/android/app/src/test/java/com/ombhrum/fabushi/AccountUsageReconciliationTest.kt',
    ],
    'production_target': 'source/host/src/mcp_auth/dashboard_mcp_auth_backend.rs',
    'test_target': 'source/host/src/mcp_auth/dashboard_mcp_auth_backend.rs',
    'focused_tests': [
        'source/packages/constants/src/auto_spillover_ui.rs::tests::labels_are_nonempty_and_distinct',
        'source/host/src/mcp_auth/dashboard_mcp_auth_backend.rs::tests::usage_paths_and_decoder_match_desktop_dashboard_contract',
        'source/host/src/mcp_auth/dashboard_mcp_auth_backend.rs::tests::usage_decoder_preserves_enterprise_and_safe_upgrade_cta_contract',
        'source/host/src/mcp_auth/dashboard_mcp_auth_backend.rs::tests::usage_decoder_rejects_unsafe_upgrade_cta_variants',
        'mobile/android/app/src/test/java/com/ombhrum/fabushi/AccountUsageReconciliationTest.kt::AccountUsageReconciliationTest.currentAccountGenerationAcceptsUsageProjection',
        'mobile/android/app/src/test/java/com/ombhrum/fabushi/AccountUsageReconciliationTest.kt::AccountUsageReconciliationTest.staleGenerationOrAccountSwitchRejectsUsageProjection',
        'mobile/android/app/src/test/java/com/ombhrum/fabushi/AccountUsageReconciliationTest.kt::AccountUsageReconciliationTest.upgradeCtaPolicyAllowsOnlySettledAccountExactDashboardAction',
    ],
    'platform_delta': (
        'Desktop retains a generic Statsig experiment-registry fallback for the seven labels, but current authority '
        'has no feature-specific auto_spillover_ui_config consumer beyond that registry and the shipping Usage & '
        'Billing surface uses its independent usage-summary projection. Android preserves the literal declaration '
        'mirror without adding a second experiment/UI owner; the existing native usage/settings projection is the '
        'Android-adapted replacement until Desktop gains a named feature consumer'
    ),
    'evidence_semantics': (
        'Same-head evidence proves the exact generated-registry boundary, the absence of a named '
        'auto_spillover_ui_config consumer outside that registry, the Android declaration mirror, and the existing '
        'native usage replacement. It must not be cited as evidence that the seven labels are currently user-visible '
        'in Desktop Settings or Android, or that fabushi-constants owns canonical usage/quota state'
    ),
    'disposition': 'android-adapted',
}


STRUCTURED_LOG_CONSTANT_SOURCE_PATH = 'source/packages/constants/structured-log.ts'
STRUCTURED_LOG_CONSTANT_RESPONSIBILITY_ID = 'OBSERVABILITY-STRUCTURED-LOG-001'
STRUCTURED_LOG_CONSTANT_PRODUCTION_STEP = 'Verify exact-HEAD constants structured log production owner'
STRUCTURED_LOG_CONSTANT_TEST_STEP = 'Test exact-HEAD constants structured log replay and Android hardening'
STRUCTURED_LOG_CONSTANT_SOURCE_SPEC = {
    'symbols': [
        'STRUCTURED_LOG_REPLAY_MAX_AGE_MS',
        'STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS',
    ],
    'source_owner': (
        'Desktop source/packages/constants/structured-log.ts@d848ec0619f1ca0f8e46e50ff5da47d6178f95fc '
        'declares 17-hour replay and 2-hour future-timestamp constants only. Current Desktop shipping '
        '17-hour replay enforcement is independently owned by '
        'source/shared/observability/structured-log-transport.ts@742bffb8e5e9653b70798d63875cd5c4f775e935 '
        'and source/host/src/extensions/telemetry/structured_log_telemetry.rs@'
        '5c6cb25a4ec4f28a3f09127cb5bd358e0f4f325f; '
        'STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS has no current Desktop production caller'
    ),
    'android_owner': (
        'Rust fabushi-constants structured-log policy is enforced by the Host '
        'AndroidStructuredLogSpill durable telemetry owner and shipped by AndroidJsonHost through '
        'CursorDashboardMcpAuthBackend. The 17-hour replay bound is Desktop-shipping-equivalent; '
        'the 2-hour future-timestamp rejection is Android-adapted hardening, not Desktop shipping parity'
    ),
    'android_targets': [
        'source/packages/constants/src/structured_log.rs',
        'source/host/src/android_structured_log_spill.rs',
        'source/host/src/android_json_runtime.rs',
        'source/host/src/mcp_auth/dashboard_mcp_auth_backend.rs',
    ],
    'production_target': 'source/host/src/android_json_runtime.rs',
    'test_target': 'source/host/src/android_structured_log_spill.rs',
    'focused_tests': [
        'source/packages/constants/src/structured_log.rs::tests::replay_window_rejects_old_and_far_future_entries',
        'source/host/src/android_structured_log_spill.rs::tests::replay_age_matches_desktop_structured_log_contract',
        'source/host/src/android_structured_log_spill.rs::tests::spill_rejects_far_future_records_under_desktop_skew_contract',
    ],
    'platform_delta': (
        'Desktop shipping enforces the 17-hour replay horizon but has no production consumer of the '
        'declared 2-hour future-skew constant; Android deliberately enforces both bounds, with future-skew '
        'rejection classified as Android-adapted hardening'
    ),
    'evidence_semantics': (
        'Same-head CI evidence proves Android 17-hour replay wiring plus Android-adapted 2-hour '
        'future-skew hardening; it must not be cited as evidence of Desktop 2-hour shipping enforcement'
    ),
    'disposition': 'android-adapted',
}


MCP_TOOL_TIMEOUT_SOURCE_PATH = 'source/packages/mcp-core/config/mcp-tool-call-timeout.ts'
MCP_TOOL_TIMEOUT_RESPONSIBILITY_ID = 'MCP-META-TOOLS-ANDROID-001'
MCP_TOOL_TIMEOUT_PRODUCTION_STEP = 'Verify exact-HEAD MCP tool timeout production owner'
MCP_TOOL_TIMEOUT_TEST_STEP = 'Test exact-HEAD MCP tool timeout production owner'
MCP_TOOL_TIMEOUT_SOURCE_SPEC = {
    'symbols': [
        'DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS',
        'LEGACY_MCP_TOOL_CALL_TIMEOUT_MS',
    ],
    'source_owner': (
        'Desktop source/packages/mcp-core/config/mcp-tool-call-timeout.ts owns the canonical '
        'one-hour default and legacy MCP tool-call timeout contract'
    ),
    'android_owner': (
        'Rust fabushi-android-mcp-core timeout contract consumed by the shipping Host '
        'MCP meta-tool execution owner'
    ),
    'android_targets': [
        'source/packages/mcp-core/src/config/mcp_tool_call_timeout.rs',
        'source/host/src/runner/mcp_meta_routed_tools.rs',
    ],
    'production_target': 'source/host/src/runner/mcp_meta_routed_tools.rs',
    'test_target': 'source/host/src/runner/mcp_meta_routed_tools.rs',
    'focused_tests': [
        'source/packages/mcp-core/src/config/mcp_tool_call_timeout.rs::tests::desktop_default_and_legacy_timeouts_are_one_hour',
        'source/host/src/runner/mcp_meta_routed_tools.rs::tests::long_timeout_tier_uses_desktop_mcp_default_contract',
    ],
    'disposition': 'android-adapted',
}


AUTOMATION_SCHEDULE_SOURCE_PATH = 'source/host/src/automations/automation_schedule.rs'
AUTOMATION_SCHEDULE_DESKTOP_BLOB = '156e0483699813d1c98cdec187ed4b3b477a7489'
AUTOMATION_SCHEDULE_RESPONSIBILITY_ID = 'AUTOMATION-SCHEDULE-ANDROID-001'
AUTOMATION_SCHEDULE_PRODUCTION_STEP = 'Verify exact-HEAD automation schedule production owner'
AUTOMATION_SCHEDULE_TEST_STEP = 'Test exact-HEAD automation schedule production owner'
AUTOMATION_SCHEDULE_SOURCE_SPEC = {
    'symbols': [
        'normalize_schedule',
        'parse_every_interval_ms',
        'compile_cron_matcher',
        'compute_next_run_at',
        'summarize_schedule_next_7_days',
    ],
    'source_owner': (
        'Desktop Rust Host automation schedule runtime owns schedule normalization, @every/cron aliases, '
        'explicit/fallback time zones and deterministic next-run derivation'
    ),
    'android_owner': (
        'Byte-identical Rust Host automation schedule direct port consumed by AutomationRuntime and '
        'FileAutomationStore; presentation owns no canonical next-run truth'
    ),
    'android_targets': [
        'source/host/src/automation_schedule.rs',
        'source/host/src/automations/automation_schedule.rs',
        'source/host/src/automation_runtime.rs',
        'source/host/src/automations/automation_store.rs',
        'source/host/tests/automation_schedule_contract.rs',
    ],
    'production_target': 'source/host/src/automation_runtime.rs',
    'test_targets': {
        'source/host/tests/automation_schedule_contract.rs': [
            'source/host/tests/automation_schedule_contract.rs::frozen_next_run_contract_supports_every_alias_timezone_and_explicit_cron_timezone',
        ],
        'source/host/src/automation_runtime.rs': [
            'source/host/src/automation_runtime.rs::automation_runtime::tests::schedule_is_normalized_and_next_run_is_host_owned',
        ],
    },
    'disposition': 'direct-port',
}


CLIENT_SIDE_TOOL_V2_RESPONSIBILITY_ID = 'TOOLS-CLIENT-SIDE-TOOL-V2-RELAY-001'
CLIENT_SIDE_TOOL_V2_PRODUCTION_STEP = 'Verify exact-HEAD client-side tool v2 relay production owner'
CLIENT_SIDE_TOOL_V2_TEST_STEP = 'Test exact-HEAD client-side tool v2 relay production owner'
CLIENT_SIDE_TOOL_V2_SOURCE_SPECS = {
    'source/node-agent-coordinator/src/client_side_tool_v2_relay.rs': {
        'blob_sha': 'a01c69f3eb7aec0e07a8bcc003d251ad9d3d7c33',
        'symbols': ['ClientSideToolV2Relay', 'CLIENT_SIDE_TOOL_V2_FAMILY', 'CLIENT_SIDE_TOOL_V2_WIRE_VERSION', 'CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT'],
        'source_owner': 'Desktop Rust Coordinator relay owns client-side-tool-v2 wire admission, ordering, epoch fencing, replay materialization and reset semantics',
        'android_owner': 'Android Rust MahayanaCoordinator owns the same relay state and durable replay/account-retirement fencing',
        'android_targets': ['source/mahayana-agent-coordinator/src/client_side_tool_v2_relay.rs', 'source/mahayana-agent-coordinator/src/main.rs'],
        'disposition': 'direct-port',
    },
    'source/node-agent-coordinator/src/main.rs': {
        'blob_sha': 'c822c07dff3027a22a658d1ece6aefe919691365',
        'symbols': ['CoordinatorState.tool_relay', 'post_tool_event', 'replay_tool_events'],
        'source_owner': 'Desktop Coordinator production composition owns relay construction, Host channel consumption, renderer projection/replay and lifecycle clearing',
        'android_owner': 'Android MahayanaCoordinator plus android-host-jni own production ingress/replay and account retirement; Kotlin/UI does not decode or own canonical relay state',
        'android_targets': ['source/mahayana-agent-coordinator/src/main.rs', 'source/android-host-jni/src/lib.rs'],
        'disposition': 'android-adapted',
    },
    'source/shared/rpc/client-side-tool-v2-transport.ts': {
        'blob_sha': 'e0a298c7dd5ca454ae2b61e8337a9d74bf18b8ba',
        'symbols': ['CLIENT_SIDE_TOOL_V2_FAMILY', 'CLIENT_SIDE_TOOL_V2_WIRE_VERSION', 'CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT', 'parseClientSideToolV2TransportEvent', 'materializeClientSideToolV2RendererEvent'],
        'source_owner': 'Desktop shared RPC contract owns versioned wire identity, encoded payload validation and renderer materialization',
        'android_owner': 'Android Rust client_side_tool_v2_relay enforces the versioned transport contract before JNI projection',
        'android_targets': ['source/mahayana-agent-coordinator/src/client_side_tool_v2_relay.rs', 'source/android-host-jni/src/lib.rs'],
        'disposition': 'android-adapted',
    },
}
CLIENT_SIDE_TOOL_V2_TEST_TARGETS = {
    'source/mahayana-agent-coordinator/src/client_side_tool_v2_relay.rs': [
        'client_side_tool_v2_relay::tests::malformed_wire_version_account_base64_type_and_identity_fail_closed',
        'client_side_tool_v2_relay::tests::duplicate_out_of_order_inconsistent_result_and_stale_epoch_are_rejected',
        'client_side_tool_v2_relay::tests::reset_account_switch_and_serialized_reopen_replay_are_fenced',
    ],
    'source/mahayana-agent-coordinator/src/main.rs': [
        'main::tests::client_side_tool_v2_replay_survives_process_reopen_and_account_retirement_is_durable',
    ],
    'source/android-host-jni/src/lib.rs': [
        'tests::client_side_tool_v2_wire_ingress_is_reachable_only_through_rust_coordinator',
    ],
}


APP_AGENT_SURFACE_E2E_SOURCE_PATH = 'desktop/e2e/app-agent-surface.spec.ts'
APP_AGENT_SURFACE_E2E_RESPONSIBILITY_ID = 'APP-AGENT-SURFACE-ANDROID-001'
APP_AGENT_SURFACE_E2E_SOURCE_SPEC = {
    'symbols': [
        'packaged Fabushi publishes a generation-safe semantic App MCP over the private loopback bridge',
        'snapshot/find/action/assert generation fencing',
        'sensitive-value redaction and web fallback registry',
    ],
    'source_owner': (
        'Desktop packaged App Agent Surface E2E owner proving the shipping private loopback App MCP '
        'and semantic six-tool contract rather than a renderer-only fixture'
    ),
    'android_owner': (
        'Native FabushiAppAgentSurface semantic state/action owner plus account-scoped '
        'FabushiRemoteDeviceGateway transport; Coordinator/Host remain canonical business-state owners'
    ),
    'android_targets': [
        'source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/semantic/FabushiAppAgentSurface.kt',
        'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/remote/FabushiRemoteDeviceGateway.kt',
        'mobile/android/app/src/test/java/com/ombhrum/fabushi/FabushiAppAgentSurfaceTest.kt',
    ],
    'disposition': 'not-applicable-with-replacement',
}
APP_AGENT_SURFACE_CROSS_SOURCE_SPECS = {
    'source/product/fabushi/fabushi-official-device-registry.ts': {
        'symbols': ['FabushiOfficialDeviceRegistryService', 'FABUSHI_APP_AGENT_TOOLS', 'handleAgentToolRequest'],
        'source_owner': (
            'Desktop official-device registry publishes and dispatches the six App Agent Surface tools '
            'through the account-fenced production WSS registration'
        ),
        'android_owner': (
            'FabushiRemoteDeviceGateway publishes the same six FabushiAppAgentSurface.ToolNames and '
            'dispatches into the native FabushiAppAgentSurface under session/account fencing'
        ),
        'android_targets': [
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/remote/FabushiRemoteDeviceGateway.kt',
            'source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/semantic/FabushiAppAgentSurface.kt',
        ],
    },
    'source/product/fabushi/fabushi-official-device-registry.contract.test.ts': {
        'symbols': ['official registry App Agent tool/account/revoke/non-retry contract tests'],
        'source_owner': 'Desktop official-device registry contract tests for the production App Agent transport boundary',
        'android_owner': 'FabushiAppAgentSurfaceTest plus RemoteCommandJournalTest focused production-boundary tests',
        'android_targets': [
            'mobile/android/app/src/test/java/com/ombhrum/fabushi/FabushiAppAgentSurfaceTest.kt',
            'mobile/android/app/src/test/java/com/ombhrum/fabushi/RemoteCommandJournalTest.kt',
        ],
    },
}


OFFICIAL_DEVICE_SOURCE_SPECS = {
    'source/electron-main/account/auth-service-contract.ts': {
        'responsibility_id': 'ACCOUNT-SESSION-001',
        'symbols': ['FabushiDeviceAgentSession', 'AuthServicePort.deviceAgentSession'],
        'source_owner': 'Desktop AuthServicePort device-agent session contract',
        'android_owner': 'Rust AndroidAccountService session owner exposed through typed Coordinator authDeviceAgentSession',
        'android_targets': [
            'source/host/src/account_service.rs',
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt',
        ],
        'disposition': 'android-adapted',
    },
    'source/electron-main/main-production-services.ts': {
        'responsibility_id': 'ACCOUNT-SESSION-001',
        'symbols': ['ProductionAccountAuthService.deviceAgentSession'],
        'source_owner': 'Desktop production account service contract',
        'android_owner': 'Rust AndroidAccountService plus typed AndroidCoordinatorRuntime account boundary',
        'android_targets': [
            'source/host/src/account_service.rs',
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt',
        ],
        'disposition': 'android-adapted',
    },
    'source/product/fabushi/fabushi-account-service.ts': {
        'responsibility_id': 'ACCOUNT-SESSION-001',
        'symbols': ['FabushiAuthService.deviceAgentSession'],
        'source_owner': 'Desktop FabushiAuthService deviceAgentSession token/device/session issuance',
        'android_owner': 'Rust AndroidAccountService canonical Fabushi session owner',
        'android_targets': [
            'source/host/src/account_service.rs',
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt',
        ],
        'disposition': 'android-adapted',
    },
    'source/product/fabushi/fabushi-account-service.contract.test.ts': {
        'responsibility_id': 'ACCOUNT-SESSION-001',
        'symbols': ['deviceAgentSession contract tests'],
        'source_owner': 'Desktop Fabushi account device-session contract tests',
        'android_owner': 'Rust AndroidAccountService and Android account/session contract tests',
        'android_targets': [
            'source/host/src/account_service.rs',
            'mobile/android/app/src/test/java/com/ombhrum/fabushi/RemoteBindingFencePolicyTest.kt',
        ],
        'disposition': 'android-adapted',
    },
    'source/product/fabushi/fabushi-account-adapters.ts': {
        'responsibility_id': 'REMOTE-COMPUTER-001',
        'symbols': ['createFabushiProductionAccountOAuthBinding', 'createFabushiOfficialDeviceRegistryService'],
        'source_owner': 'Desktop production account adapter binding official device registry lifecycle to account changes',
        'android_owner': 'FabushiRemoteDeviceGateway lifecycle driven by typed Coordinator account session projection',
        'android_targets': [
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/remote/FabushiRemoteDeviceGateway.kt',
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt',
        ],
        'disposition': 'android-adapted',
    },
    'source/product/fabushi/fabushi-official-device-registry.ts': {
        'responsibility_id': 'REMOTE-COMPUTER-001',
        'symbols': ['FabushiOfficialDeviceRegistryService', 'resolveFabushiDeviceGatewayUrl', 'FABUSHI_APP_AGENT_STATUS_TOOL'],
        'source_owner': 'Desktop official-device WSS registration owner with account/revoke fencing and read-only status admission',
        'android_owner': 'FabushiRemoteDeviceGateway plus FabushiAppAgentSurface; official gateway is status-only while full WSS/account fencing remains independently reviewable',
        'android_targets': [
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/remote/FabushiRemoteDeviceGateway.kt',
            'source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/semantic/FabushiAppAgentSurface.kt',
        ],
        'production_target': 'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/remote/FabushiRemoteDeviceGateway.kt',
        'test_targets': {
            'mobile/android/app/src/test/java/com/ombhrum/fabushi/FabushiAppAgentSurfaceTest.kt': [
                'FabushiAppAgentSurfaceTest.officialDeviceGatewayMatchesDesktopStatusOnlyRegistrationBoundary',
                'FabushiAppAgentSurfaceTest.officialDeviceGatewayFencesExpiryIdentityAndReadOnlyPreflight',
            ],
            'mobile/android/app/src/test/java/com/ombhrum/fabushi/RemoteCommandJournalTest.kt': [
                'RemoteCommandJournalTest.duplicateTerminalRequestReplaysButMismatchedSessionFailsClosed',
                'RemoteCommandJournalTest.processRestartTurnsPendingSideEffectIntoOutcomeUnknown',
                'RemoteCommandJournalTest.disconnectMarksAllPendingCommandsOutcomeUnknown',
            ],
        },
        'disposition': 'android-adapted',
    },
    'source/product/fabushi/fabushi-official-device-registry.contract.test.ts': {
        'responsibility_id': 'REMOTE-COMPUTER-001',
        'symbols': ['official registry account-switch/revoke/token-boundary/non-retry contracts'],
        'source_owner': 'Desktop official-device registration contract tests',
        'android_owner': 'Android remote-device gateway, command journal and App Agent surface tests',
        'android_targets': [
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/remote/FabushiRemoteDeviceGateway.kt',
            'mobile/android/app/src/test/java/com/ombhrum/fabushi/RemoteCommandJournalTest.kt',
            'mobile/android/app/src/test/java/com/ombhrum/fabushi/FabushiAppAgentSurfaceTest.kt',
        ],
        'disposition': 'android-adapted',
    },
    'source/product/fabushi/ws-shim.d.ts': {
        'responsibility_id': 'REMOTE-COMPUTER-001',
        'symbols': ['WebSocket type shim'],
        'source_owner': 'Desktop Node ws compile-time shim for the official-device registry transport',
        'android_owner': 'Android OkHttp WebSocket transport in FabushiRemoteDeviceGateway',
        'android_targets': [
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/remote/FabushiRemoteDeviceGateway.kt',
        ],
        'disposition': 'not-applicable-with-replacement',
    },
    '.github/workflows/fdn-official-device-registration.yml': {
        'responsibility_id': 'REMOTE-COMPUTER-001',
        'symbols': ['FDN official device registration exact-head acceptance'],
        'source_owner': 'Desktop exact-head official-device registration CI gate',
        'android_owner': 'Android exact-head parity CI plus remote-device gateway focused coverage',
        'android_targets': [
            '.github/workflows/android-parity-full-ci.yml',
            'source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/remote/FabushiRemoteDeviceGateway.kt',
        ],
        'disposition': 'not-applicable-with-replacement',
    },
}




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



def _latest_exact_head_android_ci_evidence(
    android_head: str,
    *,
    production_step_name: str = AGENT_KV_PRODUCTION_STEP,
    test_step_name: str = AGENT_KV_TEST_STEP,
    evidence_label: str = 'Agent KV',
) -> dict | None:
    """Wait for a dedicated exact-head workflow/job and the requested evidence steps to pass."""
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
            runs_by_id = {}
            for page in range(1, 21):
                response = get(
                    f'/repos/{ANDROID_REPOSITORY}/actions/runs?head_sha={android_head}&per_page=100&page={page}'
                )
                batch = response.get('workflow_runs', [])
                for run in batch:
                    if (
                        run.get('name') == ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME
                        and run.get('workflow_id') == ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID
                        and run.get('path') == ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH
                        and run.get('event') in ('pull_request', 'push')
                        and run.get('head_sha') == android_head
                        and (run.get('repository') or {}).get('full_name') == ANDROID_REPOSITORY
                    ):
                        runs_by_id[run.get('id')] = run
                if len(batch) < 100:
                    break
            terminal = [
                run for run in runs_by_id.values()
                if run.get('status') == 'completed'
                and run.get('conclusion') == 'success'
                and isinstance(run.get('run_attempt'), int)
                and not isinstance(run.get('run_attempt'), bool)
                and run.get('run_attempt') > 0
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
                    if value.get('name') == ANDROID_AGENT_KV_EVIDENCE_JOB_NAME
                    and value.get('status') == 'completed'
                    and value.get('conclusion') == 'success'
                    and value.get('run_attempt') == run.get('run_attempt')
                ), None)
                if not job:
                    continue
                steps = {
                    value.get('name'): value
                    for value in job.get('steps', [])
                    if isinstance(value, dict) and isinstance(value.get('name'), str)
                }
                production_step = steps.get(production_step_name)
                test_step = steps.get(test_step_name)
                if not all(
                    step
                    and step.get('status') == 'completed'
                    and step.get('conclusion') == 'success'
                    and isinstance(step.get('number'), int)
                    and not isinstance(step.get('number'), bool)
                    and step.get('number') > 0
                    for step in (production_step, test_step)
                ):
                    continue
                return {
                    'workflow_name': run['name'],
                    'workflow_id': run['workflow_id'],
                    'workflow_path': run['path'],
                    'workflow_event': run['event'],
                    'workflow_repository': (run.get('repository') or {}).get('full_name'),
                    'workflow_run_id': run['id'],
                    'run_attempt': run['run_attempt'],
                    'run_conclusion': run['conclusion'],
                    'workflow_job_name': job['name'],
                    'workflow_job_id': job['id'],
                    'workflow_job_run_attempt': job['run_attempt'],
                    'production_step_name': production_step['name'],
                    'production_step_number': production_step['number'],
                    'test_step_name': test_step['name'],
                    'test_step_number': test_step['number'],
                    'android_head': android_head,
                }
        except (RuntimeError, KeyError, TypeError, ValueError) as error:
            print(
                'WARNING: unable to read exact-head Agent KV CI evidence: '
                + type(error).__name__,
                file=sys.stderr,
            )
            return None
        except OSError:
            pass
        if time.monotonic() >= deadline:
            print(
                f'WARNING: timed out waiting for successful exact-head {evidence_label} evidence; '
                f'{evidence_label} remains incomplete',
                file=sys.stderr,
            )
            return None
        time.sleep(15)


def _package_action_job_passed(
    entry: dict,
    *,
    android_head: str,
    tracked_android_paths: set[str],
    action_job_cache: dict[tuple[object, ...], bool],
) -> bool:
    """Count only a successful dedicated run/job/step on the exact Android checkout."""
    run_id = entry.get('workflow_run_id')
    job_id = entry.get('workflow_job_id')
    job_name = entry.get('workflow_job_name')
    step_name = entry.get('workflow_step_name')
    step_number = entry.get('workflow_step_number')
    run_attempt = entry.get('run_attempt')
    job_attempt = entry.get('workflow_job_run_attempt')
    evidence_ref = entry.get('evidence_ref')
    if (
        not isinstance(run_id, int) or isinstance(run_id, bool) or run_id <= 0
        or not isinstance(job_id, int) or isinstance(job_id, bool) or job_id <= 0
        or not isinstance(run_attempt, int) or isinstance(run_attempt, bool) or run_attempt <= 0
        or not isinstance(job_attempt, int) or isinstance(job_attempt, bool) or job_attempt <= 0
        or not isinstance(step_number, int) or isinstance(step_number, bool) or step_number <= 0
        or job_attempt != run_attempt
        or job_name != ANDROID_AGENT_KV_EVIDENCE_JOB_NAME
        or not isinstance(step_name, str) or not step_name.strip()
        or entry.get('workflow_name') != ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME
        or entry.get('workflow_id') != ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID
        or entry.get('workflow_path') != ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH
        or entry.get('workflow_event') not in ('pull_request', 'push')
        or entry.get('workflow_repository') != ANDROID_REPOSITORY
        or entry.get('run_conclusion') != 'success'
        or not isinstance(evidence_ref, str) or evidence_ref.count('#') != 1
    ):
        return False
    evidence_path, evidence_anchor = evidence_ref.split('#', 1)
    evidence_posix = PurePosixPath(evidence_path)
    if (
        not evidence_path or not evidence_anchor.strip()
        or evidence_posix.is_absolute() or '..' in evidence_posix.parts
        or evidence_path not in tracked_android_paths
    ):
        return False
    key = (
        run_id, run_attempt, job_id, job_attempt, job_name,
        android_head, step_name, step_number, evidence_ref,
    )
    if key not in action_job_cache:
        try:
            run = get(f'/repos/{ANDROID_REPOSITORY}/actions/runs/{run_id}')
            if (
                run.get('name') != ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME
                or run.get('workflow_id') != ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID
                or run.get('path') != ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH
                or run.get('event') not in ('pull_request', 'push')
                or (run.get('repository') or {}).get('full_name') != ANDROID_REPOSITORY
                or run.get('head_sha') != android_head
                or run.get('status') != 'completed'
                or run.get('conclusion') != 'success'
                or run.get('run_attempt') != run_attempt
            ):
                action_job_cache[key] = False
            else:
                jobs = get(
                    f'/repos/{ANDROID_REPOSITORY}/actions/runs/{run_id}/jobs?filter=latest&per_page=100&page=1'
                )
                job = next((
                    value for value in jobs.get('jobs', [])
                    if value.get('id') == job_id
                ), None)
                if (
                    not job
                    or job.get('name') != job_name
                    or job.get('status') != 'completed'
                    or job.get('conclusion') != 'success'
                    or job.get('run_attempt') != run_attempt
                    or job.get('run_attempt') != job_attempt
                ):
                    action_job_cache[key] = False
                else:
                    step = next((
                        value for value in job.get('steps', [])
                        if value.get('name') == step_name and value.get('number') == step_number
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
    action_job_cache: dict[tuple[object, ...], bool],
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
    action_job_cache: dict[tuple[object, ...], bool] = {}

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

        if path in AGENT_KV_SOURCE_SPEC_BY_PATH or path in BACKGROUND_COMPLETION_SOURCE_SPECS or path in AGENT_CORE_INTERACTION_SOURCE_SPECS:
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
        'explicit; exact-head production/test evidence is added only from the successful dedicated Grok Android Architecture '
        'agent-kv evidence job steps. Mapping and CI evidence do not promote implementation_status or verification_status.'
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


def _ensure_agent_core_interaction_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Map the shipping Agent v1 listener/query/update package boundary to the existing Rust Host owner."""
    if not any(path in new_by_path for path in AGENT_CORE_INTERACTION_SOURCE_SPECS):
        return
    missing = sorted(set(AGENT_CORE_INTERACTION_SOURCE_SPECS) - set(new_by_path))
    require(not missing, 'current Desktop agent-core interaction source set is incomplete: ' + ', '.join(missing))
    matches = [
        item for item in responsibilities
        if item.get('responsibility_id') == AGENT_CORE_INTERACTION_RESPONSIBILITY_ID
    ]
    require(len(matches) == 1, 'agent-core interaction mapping requires exactly one ' + AGENT_CORE_INTERACTION_RESPONSIBILITY_ID)
    item = matches[0]
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    for path, spec in AGENT_CORE_INTERACTION_SOURCE_SPECS.items():
        source_object = new_by_path[path]
        require(
            source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
            'agent-core interaction Desktop source has invalid blob/mode/type: ' + path,
        )
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
    item['desktop_sources'] = sorted(existing_sources.values(), key=lambda source: source.get('path', ''))
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for spec in AGENT_CORE_INTERACTION_SOURCE_SPECS.values():
        for target in spec['android_targets']:
            if target not in raw_targets:
                raw_targets.append(target)
    item['android_target'] = raw_targets
    item['android_disposition'] = 'android-adapted'
    note = (
        'Current-main Agent v1 interaction listener/query/update package sources are mapped to the existing '
        'Rust Host/JNI shipping owner with source-specific focused tests. Mapping and same-head CI evidence '
        'remain provenance and do not promote responsibility implementation/verification status.'
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


def _ensure_background_completion_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Map only Desktop package sources proven to feed shipping Android completion revival."""
    if not any(path in new_by_path for path in BACKGROUND_COMPLETION_SOURCE_SPECS):
        return
    missing = sorted(set(BACKGROUND_COMPLETION_SOURCE_SPECS) - set(new_by_path))
    require(
        not missing,
        'current Desktop background-completion inventory is incomplete: ' + ', '.join(missing),
    )
    matches = [
        item for item in responsibilities
        if item.get('responsibility_id') == BACKGROUND_COMPLETION_RESPONSIBILITY_ID
    ]
    require(
        len(matches) == 1,
        'background-completion semantic mapping requires exactly one '
        + BACKGROUND_COMPLETION_RESPONSIBILITY_ID,
    )
    item = matches[0]
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    for path, spec in BACKGROUND_COMPLETION_SOURCE_SPECS.items():
        source_object = new_by_path.get(path)
        require(
            source_object is not None,
            'background-completion Desktop source is absent from current authority tree: ' + path,
        )
        require(
            source_object.get('type') == 'blob'
            and source_object.get('mode') in ALLOWED_SOURCE_MODES,
            'background-completion Desktop source has invalid blob/mode/type: ' + path,
        )
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
    for spec in BACKGROUND_COMPLETION_SOURCE_SPECS.values():
        for target in spec['android_targets']:
            if target not in raw_targets:
                raw_targets.append(target)
    item['android_target'] = raw_targets
    note = (
        'Current-main semantic closure is intentionally narrow: only background-task completion '
        'formatting, its hidden follow-up action handler, and the canonical system_notification tag '
        'are mapped to the shipping AndroidJsonHost completion-revival path. goal-continuation, '
        'synthetic-user-message classification, agent/index registration and agent/state remain '
        'unmapped until their additional shipping responsibilities have equivalent Android owners. '
        'Exact-head Actions evidence is required and never promotes implementation/verification status.'
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


def _http_200_payload(response: object, label: str) -> bytes:
    """Read a bounded HTTP response only when its observable status is exactly 200."""
    status = getattr(response, 'status', None)
    if status is None:
        getcode = getattr(response, 'getcode', None)
        status = getcode() if callable(getcode) else None
    require(status == 200, f'{label} returned unexpected HTTP status: {status}')
    payload = response.read()
    require(isinstance(payload, (bytes, bytearray)) and len(payload) > 0,
            f'{label} returned an empty response body')
    headers = getattr(response, 'headers', None)
    if headers is not None:
        raw_length = headers.get('Content-Length')
        if raw_length not in (None, ''):
            try:
                expected_length = int(raw_length)
            except (TypeError, ValueError):
                raise ValueError(f'{label} returned an invalid Content-Length') from None
            require(expected_length == len(payload),
                    f'{label} Content-Length mismatch: expected={expected_length} actual={len(payload)}')
    return bytes(payload)


def _download_predecessor_bytes(artifact_id: str, token: str) -> bytes:
    endpoint = f'https://api.github.com/repos/{ANDROID_REPOSITORY}/actions/artifacts/{artifact_id}/zip'
    request = Request(endpoint, headers={
        'Accept': 'application/vnd.github+json',
        'Authorization': 'Bearer ' + token,
        'User-Agent': 'Fabushi-Android-Docs',
        'X-GitHub-Api-Version': '2022-11-28',
    })
    try:
        try:
            with urllib.request.build_opener(_NoRedirect()).open(request, timeout=60) as response:
                payload = _http_200_payload(response, 'GitHub predecessor artifact endpoint')
        except HTTPError as error:
            if error.code not in (301, 302, 303, 307, 308):
                raise
            location = error.headers.get('Location')
            require(bool(location), 'GitHub artifact redirect omitted Location')
            parsed = urlparse(location)
            require(parsed.scheme == 'https' and bool(parsed.netloc)
                    and parsed.username is None and parsed.password is None and not parsed.fragment,
                    'GitHub artifact redirect is not a safe absolute HTTPS URL')
            redirected = Request(location, headers={
                'User-Agent': 'Fabushi-Android-Docs',
                'Accept': 'application/octet-stream',
            })
            with urlopen(redirected, timeout=60) as response:
                payload = _http_200_payload(response, 'GitHub predecessor artifact signed download')
    except HTTPError as error:
        raise RuntimeError(
            'failed to recover predecessor responsibility artifact '
            + artifact_id + ' (HTTP ' + str(error.code) + ')'
        ) from None
    except (ValueError, RuntimeError):
        raise
    except Exception as error:
        raise RuntimeError(
            'failed to recover predecessor responsibility artifact '
            + artifact_id + ' (' + type(error).__name__ + ': ' + str(error) + ')'
        ) from None
    require(payload.startswith(b'PK\x03\x04'),
            'predecessor artifact download is not a ZIP local-file header')
    return payload


def _load_pinned_responsibility_mapping() -> tuple[dict, dict[str, dict], dict]:
    artifact_id = os.environ.get(
        'ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT', DEFAULT_RESPONSIBILITY_ARTIFACT
    )
    expected_digest = os.environ.get(
        'ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST',
        DEFAULT_RESPONSIBILITY_ARTIFACT_DIGEST,
    )
    require(re.fullmatch(r'sha256:[0-9a-f]{64}', expected_digest or '') is not None,
            'predecessor responsibility artifact digest is not a canonical sha256 value')
    token = os.environ.get('GH_TOKEN')
    require(token, 'GH_TOKEN is required to recover predecessor responsibility mapping')
    payload = _download_predecessor_bytes(artifact_id, token)
    actual_digest = 'sha256:' + hashlib.sha256(payload).hexdigest()
    require(
        actual_digest == expected_digest,
        f'predecessor responsibility artifact digest mismatch: expected={expected_digest} actual={actual_digest}',
    )
    try:
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            bad = archive.testzip()
            require(bad is None, 'predecessor artifact ZIP integrity failure: ' + str(bad))
            infos = archive.infolist()
            require(bool(infos), 'predecessor artifact ZIP is empty')
            names = [info.filename for info in infos]
            require(len(names) == len(set(names)), 'predecessor artifact ZIP contains duplicate member names')
            for info in infos:
                raw_name = info.filename.rstrip('/')
                if not raw_name:
                    continue
                member = PurePosixPath(raw_name)
                require(
                    not member.is_absolute() and '..' not in member.parts and '\x00' not in raw_name
                    and not raw_name.startswith(('/', '\\')) and '\\' not in raw_name
                    and re.match(r'^[A-Za-z]:', raw_name) is None,
                    'predecessor artifact ZIP contains an unsafe member path: ' + info.filename,
                )
                unix_type = (info.external_attr >> 16) & 0o170000
                require(unix_type != 0o120000,
                        'predecessor artifact ZIP contains a symbolic-link member: ' + info.filename)
                require(not (info.flag_bits & 0x1),
                        'predecessor artifact ZIP contains an encrypted member: ' + info.filename)
                require(info.file_size <= 24 * 1024 * 1024,
                        'predecessor artifact ZIP member exceeds size limit: ' + info.filename)

            def one_member(suffixes: tuple[str, ...], label: str) -> str:
                matches = [
                    name for name in names
                    if any(name.endswith(suffix) for suffix in suffixes)
                ]
                require(len(matches) == 1,
                        f'predecessor artifact must contain exactly one {label}; got {len(matches)}')
                return matches[0]

            baseline_name = one_member(
                ('docs/android-port/authority/baseline.json',), 'baseline authority input'
            )
            responsibility_name = one_member(
                ('docs/android-port/authority/responsibility-ledger.json',),
                'responsibility authority input',
            )
            manifest_name = one_member(
                ('docs/android-port/authority/source-inventory-manifest.json',),
                'source inventory manifest',
            )
            ledger_name = one_member(
                ('docs/android-port/authority/source-inventory-ledger.jsonl',),
                'source inventory ledger',
            )
            tree_names = [
                name for name in names
                if name.endswith('docs/android-port/generated/desktop-recursive-api.json')
                or name.endswith('docs/android-port/generated/desktop-complete-tree.json')
            ]
            require(
                1 <= len(tree_names) <= 2,
                'predecessor artifact must contain one or both historical recursive tree evidence files; '
                + 'got ' + str(len(tree_names)),
            )
            baseline = json.loads(archive.read(baseline_name).decode('utf-8'))
            candidate = json.loads(archive.read(responsibility_name).decode('utf-8'))
            manifest = json.loads(archive.read(manifest_name).decode('utf-8'))
            ledger_text = archive.read(ledger_name).decode('utf-8')
            tree_docs = [
                json.loads(archive.read(name).decode('utf-8'))
                for name in sorted(tree_names)
            ]
            tree_doc = tree_docs[0]
    except (zipfile.BadZipFile, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError(
            'predecessor responsibility artifact is invalid: ' + type(error).__name__
        ) from None

    predecessor_pair = (PREDECESSOR_DESKTOP_COMMIT, PREDECESSOR_DESKTOP_TREE)
    current_pair = (EXPECTED_DESKTOP_COMMIT, EXPECTED_DESKTOP_TREE)
    require(isinstance(baseline, dict), 'predecessor baseline is not an object')
    require(baseline.get('desktop_repository') == 'bhrumom/fabushi-desktop',
            'predecessor baseline repository mismatch')
    baseline_pair = (baseline.get('desktop_commit'), baseline.get('desktop_root_tree'))
    require(baseline_pair in {predecessor_pair, current_pair},
            'predecessor artifact baseline has an unknown Desktop authority')
    if baseline_pair == current_pair:
        lineage = str(baseline.get('rebaseline_note') or '')
        require(
            PREDECESSOR_DESKTOP_COMMIT in lineage
            and PREDECESSOR_DESKTOP_TREE in lineage
            and str(PREDECESSOR_TRACKED_NON_TREE_COUNT) in lineage
            and str(PREDECESSOR_TREE_ENTRY_COUNT) in lineage,
            'current baseline in predecessor artifact does not preserve the historical authority lineage',
        )
        if baseline.get('desktop_main_before') is not None:
            require(baseline.get('desktop_main_before') == PREDECESSOR_DESKTOP_COMMIT,
                    'predecessor artifact baseline desktop_main_before mismatch')
        if baseline.get('desktop_main_after') is not None:
            require(baseline.get('desktop_main_after') == EXPECTED_DESKTOP_COMMIT,
                    'predecessor artifact baseline desktop_main_after mismatch')

    require(isinstance(manifest, dict), 'predecessor source manifest is not an object')
    require(
        manifest.get('authority_current') is True
        and manifest.get('desktop_commit') == PREDECESSOR_DESKTOP_COMMIT
        and manifest.get('source_commit') == PREDECESSOR_DESKTOP_COMMIT
        and manifest.get('desktop_root_tree') == PREDECESSOR_DESKTOP_TREE,
        'predecessor manifest does not match the pinned historical Desktop authority',
    )
    require(
        manifest.get('desktop_main_before') == PREDECESSOR_DESKTOP_COMMIT
        and manifest.get('desktop_main_after') == PREDECESSOR_DESKTOP_COMMIT,
        'predecessor manifest Desktop main lineage is not the historical authority',
    )
    require(
        manifest.get('tracked_non_tree_entries') == PREDECESSOR_TRACKED_NON_TREE_COUNT
        and manifest.get('authority_ledger_rows') == PREDECESSOR_TRACKED_NON_TREE_COUNT,
        'predecessor manifest does not cover the historical 8258-row inventory',
    )
    require(
        manifest.get('tree_entries') == PREDECESSOR_TREE_ENTRY_COUNT,
        'predecessor manifest does not preserve the historical 9412-entry tree',
    )
    require(
        hashlib.sha256(ledger_text.encode('utf-8')).hexdigest() == manifest.get('ledger_sha256'),
        'predecessor source ledger digest does not match its manifest',
    )

    require(
        isinstance(tree_doc, dict)
        and tree_doc.get('sha') == PREDECESSOR_DESKTOP_TREE
        and tree_doc.get('truncated') is False,
        'predecessor recursive tree evidence is not the complete pinned historical tree',
    )
    tree_entries = tree_doc.get('tree')
    require(
        isinstance(tree_entries, list) and len(tree_entries) == PREDECESSOR_TREE_ENTRY_COUNT,
        f'predecessor recursive tree must contain {PREDECESSOR_TREE_ENTRY_COUNT} entries',
    )
    canonical_tree_objects = sorted(
        (item.get('path'), item.get('sha'), item.get('mode'), item.get('type'))
        for item in tree_entries if isinstance(item, dict)
    )
    for duplicate_tree_doc in tree_docs[1:]:
        require(
            duplicate_tree_doc.get('sha') == PREDECESSOR_DESKTOP_TREE
            and duplicate_tree_doc.get('truncated') is False,
            'duplicate predecessor recursive tree evidence has wrong root identity or is truncated',
        )
        duplicate_entries = duplicate_tree_doc.get('tree')
        require(
            isinstance(duplicate_entries, list)
            and len(duplicate_entries) == PREDECESSOR_TREE_ENTRY_COUNT,
            'duplicate predecessor recursive tree evidence has the wrong entry count',
        )
        duplicate_objects = sorted(
            (item.get('path'), item.get('sha'), item.get('mode'), item.get('type'))
            for item in duplicate_entries if isinstance(item, dict)
        )
        require(
            duplicate_objects == canonical_tree_objects,
            'predecessor recursive tree evidence files disagree on path/object/mode/type',
        )
    tree_by_path: dict[str, dict] = {}
    historical_non_tree: dict[str, dict] = {}
    for item in tree_entries:
        require(isinstance(item, dict), 'predecessor recursive tree contains a non-object entry')
        path = item.get('path')
        require(isinstance(path, str) and path, 'predecessor recursive tree entry is missing a path')
        posix = PurePosixPath(path)
        require(not posix.is_absolute() and '..' not in posix.parts,
                'predecessor recursive tree contains an unsafe path: ' + path)
        require(path not in tree_by_path, 'predecessor recursive tree contains duplicate path: ' + path)
        require(SHA.fullmatch(str(item.get('sha') or '')) is not None,
                'predecessor recursive tree contains an invalid object SHA: ' + path)
        item_type = item.get('type')
        item_mode = item.get('mode')
        require(
            (item_type == 'tree' and item_mode == '040000')
            or (item_type in ALLOWED_SOURCE_TYPES and item_mode in ALLOWED_SOURCE_MODES),
            'predecessor recursive tree contains an invalid mode/type: ' + path,
        )
        tree_by_path[path] = item
        if item_type != 'tree':
            historical_non_tree[path] = item
    require(
        len(historical_non_tree) == PREDECESSOR_TRACKED_NON_TREE_COUNT,
        f'predecessor recursive tree must contain {PREDECESSOR_TRACKED_NON_TREE_COUNT} non-tree entries',
    )

    source_rows: dict[str, dict] = {}
    ordered_paths: list[str] = []
    for line_number, line in enumerate(ledger_text.splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        require(isinstance(row, dict),
                f'predecessor source ledger row {line_number} is not an object')
        path = row.get('source_path')
        require(isinstance(path, str) and path,
                f'predecessor source ledger row {line_number} has no source_path')
        posix = PurePosixPath(path)
        require(not posix.is_absolute() and '..' not in posix.parts,
                'predecessor source ledger contains an unsafe path: ' + path)
        require(path not in source_rows,
                'predecessor source ledger contains duplicate path: ' + path)
        require(
            row.get('source_commit') == PREDECESSOR_DESKTOP_COMMIT
            and row.get('source_type') in ALLOWED_SOURCE_TYPES
            and row.get('source_mode') in ALLOWED_SOURCE_MODES
            and SHA.fullmatch(str(row.get('source_blob_sha') or '')) is not None,
            'predecessor source ledger has invalid provenance/object metadata: ' + path,
        )
        ordered_paths.append(path)
        source_rows[path] = row
    require(ordered_paths == sorted(ordered_paths),
            'predecessor source ledger is not in canonical sorted path order')
    require(
        len(source_rows) == PREDECESSOR_TRACKED_NON_TREE_COUNT,
        f'predecessor source ledger must contain {PREDECESSOR_TRACKED_NON_TREE_COUNT} unique rows',
    )
    require(set(source_rows) == set(historical_non_tree),
            'predecessor source ledger paths do not equal the historical non-tree paths')
    for path, tree_item in historical_non_tree.items():
        row = source_rows[path]
        require(
            row.get('source_blob_sha') == tree_item.get('sha')
            and row.get('source_mode') == tree_item.get('mode')
            and row.get('source_type') == tree_item.get('type'),
            'predecessor source ledger object does not match historical tree: ' + path,
        )

    require(isinstance(candidate, dict) and candidate.get('responsibilities'),
            'predecessor responsibility mapping is empty')
    candidate_authority = candidate.get('authority') or {}
    require(
        candidate_authority.get('desktop_commit') == PREDECESSOR_DESKTOP_COMMIT
        and candidate_authority.get('desktop_root_tree') == PREDECESSOR_DESKTOP_TREE,
        'predecessor responsibility authority disagrees with historical source authority',
    )
    if candidate.get('source_commit') is not None:
        require(candidate.get('source_commit') == PREDECESSOR_DESKTOP_COMMIT,
                'predecessor responsibility ledger source_commit mismatch')
    responsibilities = candidate['responsibilities']
    require(
        len(responsibilities) == EXPECTED_RESPONSIBILITY_COUNT,
        f'expected {EXPECTED_RESPONSIBILITY_COUNT} responsibility definitions, got {len(responsibilities)}',
    )
    responsibility_ids = [item.get('responsibility_id') for item in responsibilities]
    require(
        all(isinstance(rid, str) and rid for rid in responsibility_ids)
        and len(set(responsibility_ids)) == EXPECTED_RESPONSIBILITY_COUNT,
        f'predecessor responsibility ledger must contain {EXPECTED_RESPONSIBILITY_COUNT} unique IDs',
    )
    semantic_anchor_count = 0
    semantic_anchor_paths: set[str] = set()
    reverse_links: dict[str, set[str]] = {}
    for item in responsibilities:
        rid = item['responsibility_id']
        sources = item.get('desktop_sources') or []
        require(isinstance(sources, list),
                'predecessor responsibility desktop_sources is not a list: ' + rid)
        local_paths: set[str] = set()
        for source in sources:
            require(isinstance(source, dict),
                    'predecessor responsibility source anchor is not an object: ' + rid)
            require(_is_semantic_source_anchor(rid, source),
                    'predecessor responsibility contains a non-semantic source anchor: ' + rid)
            path = source.get('path')
            require(isinstance(path, str) and path in source_rows,
                    'predecessor semantic source anchor is absent from source ledger: ' + str(path))
            require(path not in local_paths,
                    'predecessor responsibility contains duplicate source anchor: ' + rid + ' ' + path)
            local_paths.add(path)
            semantic_anchor_count += 1
            semantic_anchor_paths.add(path)
            reverse_links.setdefault(path, set()).add(rid)
            row = source_rows[path]
            require(source.get('source_commit') == PREDECESSOR_DESKTOP_COMMIT,
                    'predecessor semantic source anchor has stale source_commit: ' + path)
            require(
                source.get('blob_sha') == row.get('source_blob_sha')
                and source.get('source_mode') == row.get('source_mode')
                and source.get('source_type') == row.get('source_type'),
                'predecessor semantic source anchor object differs from source ledger: ' + path,
            )
    require(
        semantic_anchor_count == PREDECESSOR_SEMANTIC_SOURCE_ANCHOR_COUNT,
        f'predecessor responsibility ledger must preserve {PREDECESSOR_SEMANTIC_SOURCE_ANCHOR_COUNT} semantic source anchors; got {semantic_anchor_count}',
    )
    if manifest.get('semantic_source_anchor_entries') is not None:
        require(manifest.get('semantic_source_anchor_entries') == semantic_anchor_count,
                'predecessor manifest semantic source-anchor count disagrees with responsibility ledger')
    if manifest.get('semantic_source_anchor_unique_paths') is not None:
        require(manifest.get('semantic_source_anchor_unique_paths') == len(semantic_anchor_paths),
                'predecessor manifest semantic source-anchor unique-path count disagrees with responsibility ledger')
    for path, row in source_rows.items():
        linked = row.get('responsibilities') or []
        require(isinstance(linked, list) and len(linked) == len(set(linked))
                and set(linked) == reverse_links.get(path, set()),
                'predecessor source ledger responsibility links disagree with source anchors: ' + path)

    return candidate, source_rows, {
        'artifact_id': int(artifact_id),
        'artifact_digest': actual_digest,
        'zip_member_count': len(names),
        'source_manifest_run_id': manifest.get('workflow_run_id'),
        'source_manifest_android_head': manifest.get('android_pr_head_sha'),
        'source_manifest_desktop_commit': manifest.get('desktop_commit'),
        'source_manifest_desktop_root_tree': manifest.get('desktop_root_tree'),
        'validated_tree_entries': len(tree_entries),
        'validated_source_rows': len(source_rows),
        'validated_responsibility_ids': len(set(responsibility_ids)),
        'validated_semantic_source_anchors': semantic_anchor_count,
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
    if manifest.get('source_commit') is not None:
        require(manifest.get('source_commit') == desktop_commit,
                'local source inventory manifest source_commit mismatch')
    require(manifest.get('tracked_non_tree_entries') == EXPECTED_TRACKED_NON_TREE_COUNT,
            f'local source authority does not preserve the {EXPECTED_TRACKED_NON_TREE_COUNT}-entry current tracked non-tree inventory')
    require(len(source_rows) == EXPECTED_TRACKED_NON_TREE_COUNT,
            f'local source inventory must contain {EXPECTED_TRACKED_NON_TREE_COUNT} unique rows, got {len(source_rows)}')
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
    predecessor_source_rows: dict[str, dict] | None = None,
) -> list[dict]:
    """Accept only the pinned historical inventory or an already-current inventory."""
    require(baseline.get('desktop_repository') == 'bhrumom/fabushi-desktop',
            'rebaseline baseline repository mismatch')
    require(current == EXPECTED_DESKTOP_COMMIT,
            'Desktop main commit differs from the pinned current authority')
    require(tree_sha == EXPECTED_DESKTOP_TREE,
            'Desktop main tree differs from the pinned current authority')
    baseline_pair = (
        baseline.get('desktop_commit'),
        baseline.get('desktop_root_tree'),
    )
    prior_pair = (PRIOR_DESKTOP_COMMIT, PRIOR_DESKTOP_TREE)
    current_pair = (current, tree_sha)
    require(
        baseline_pair in {prior_pair, current_pair},
        'baseline Desktop authority is neither the exact prior accepted authority nor current main',
    )
    if baseline_pair == prior_pair:
        require(
            baseline.get('desktop_main_after') == PRIOR_DESKTOP_COMMIT
            and baseline.get('desktop_main_before') in {
                PREDECESSOR_DESKTOP_COMMIT,
                PERSISTED_INTERMEDIATE_DESKTOP_COMMIT,
                PRIOR_BASELINE_PARENT_DESKTOP_COMMIT,
                PRIOR_DESKTOP_COMMIT,
            },
            'prior baseline does not preserve an accepted Desktop main lineage',
        )
    else:
        require(
            baseline.get('desktop_main_before') in {
                PREDECESSOR_DESKTOP_COMMIT,
                PERSISTED_INTERMEDIATE_DESKTOP_COMMIT,
                PRIOR_DESKTOP_COMMIT,
                current,
            }
            and baseline.get('desktop_main_after') == current,
            'current baseline does not preserve an accepted Desktop main lineage',
        )
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
        require(prior_manifest.get('desktop_main_before') == PREDECESSOR_DESKTOP_COMMIT
                and prior_manifest.get('desktop_main_after') == PREDECESSOR_DESKTOP_COMMIT,
                'historical source manifest Desktop main lineage mismatch')
    elif (prior_commit, prior_tree) == (PERSISTED_INTERMEDIATE_DESKTOP_COMMIT, PERSISTED_INTERMEDIATE_DESKTOP_TREE):
        prior_rows_expected = PERSISTED_INTERMEDIATE_TRACKED_NON_TREE_COUNT
        prior_tree_expected = PERSISTED_INTERMEDIATE_TREE_ENTRY_COUNT
        require(prior_manifest.get('desktop_main_before') == PERSISTED_INTERMEDIATE_DESKTOP_COMMIT
                and prior_manifest.get('desktop_main_after') == PERSISTED_INTERMEDIATE_DESKTOP_COMMIT,
                'persisted intermediate source manifest Desktop main lineage mismatch')
    elif (prior_commit, prior_tree) == (PRIOR_DESKTOP_COMMIT, PRIOR_DESKTOP_TREE):
        prior_rows_expected = PRIOR_TRACKED_NON_TREE_COUNT
        prior_tree_expected = PRIOR_TREE_ENTRY_COUNT
        require(prior_manifest.get('desktop_main_after') == PRIOR_DESKTOP_COMMIT
                and prior_manifest.get('desktop_main_before') in {
                    PREDECESSOR_DESKTOP_COMMIT, PRIOR_DESKTOP_COMMIT
                },
                'prior current source manifest Desktop main lineage mismatch')
    elif (prior_commit, prior_tree) == (current, tree_sha):
        prior_rows_expected = EXPECTED_TRACKED_NON_TREE_COUNT
        prior_tree_expected = EXPECTED_TREE_ENTRY_COUNT
        require(prior_manifest.get('desktop_main_after') == current
                and prior_manifest.get('desktop_main_before') in {
                    PREDECESSOR_DESKTOP_COMMIT, PRIOR_DESKTOP_COMMIT, current
                },
                'current source manifest Desktop main lineage mismatch')
    else:
        raise ValueError(
            'persisted source manifest is neither the pinned historical predecessor, '
            'the exact prior accepted authority, nor the current authority'
        )
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
    if prior_commit == PREDECESSOR_DESKTOP_COMMIT:
        require(predecessor_source_rows is not None,
                'historical persisted source ledger requires digest-pinned predecessor source rows')
        require(set(paths) == set(predecessor_source_rows),
                'historical persisted source ledger paths differ from digest-pinned predecessor artifact')
        for row in rows:
            pinned = predecessor_source_rows[row['source_path']]
            require(
                row.get('source_blob_sha') == pinned.get('source_blob_sha')
                and row.get('source_mode') == pinned.get('source_mode')
                and row.get('source_type') == pinned.get('source_type'),
                'historical persisted source ledger object differs from digest-pinned predecessor artifact: '
                + row['source_path'],
            )
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

    reverse_links: dict[str, set[str]] = {}
    for responsibility in responsibilities:
        rid = responsibility.get('responsibility_id') or '<missing-id>'
        require(responsibility.get('source_commit') == desktop_commit,
                'exact-head responsibility has stale Desktop source commit: ' + rid)
        require(responsibility.get('android_head_at_update') == expected_android_checkout_sha,
                'exact-head responsibility has stale Android head: ' + rid)
        require(responsibility.get('implementation_status') == 'unreviewed'
                and responsibility.get('verification_status') == 'not-verified',
                'exact-head responsibility status was promoted without verification: ' + rid)
        sources = responsibility.get('desktop_sources') or []
        require(isinstance(sources, list),
                'exact-head responsibility source anchors must be a list: ' + rid)
        for source in sources:
            require(isinstance(source, dict),
                    'exact-head responsibility source anchor must be an object: ' + rid)
            path = source.get('path')
            row = rows_by_path.get(path)
            require(row is not None,
                    'exact-head responsibility anchor is absent from source ledger: ' + str(path))
            require(source.get('source_commit') == desktop_commit
                    and source.get('blob_sha') == row.get('source_blob_sha')
                    and source.get('source_mode') == row.get('source_mode')
                    and source.get('source_type') == row.get('source_type'),
                    'exact-head responsibility anchor differs from current Desktop object: ' + str(path))
            if _source_mapping_quality(rid, source) == 'semantic':
                reverse_links.setdefault(path, set()).add(rid)

    for path, row in rows_by_path.items():
        linked = row.get('responsibilities') or []
        require(isinstance(linked, list)
                and all(isinstance(rid, str) and rid for rid in linked)
                and len(set(linked)) == len(linked),
                'exact-head source ledger has malformed responsibility links: ' + path)
        require(set(linked) == reverse_links.get(path, set()),
                'exact-head source ledger responsibility links disagree with responsibility source anchors: ' + path)


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


def _ensure_cloud_agent_constant_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Attach the Desktop walkthrough artifact-directory contract to the existing Host prompt owner."""
    source_object = new_by_path.get(CLOUD_AGENT_CONSTANT_SOURCE_PATH)
    if source_object is None:
        return
    require(
        source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
        'Desktop cloud-agent constant has invalid blob/mode/type',
    )
    matches = [
        item for item in responsibilities
        if item.get('responsibility_id') == CLOUD_AGENT_CONSTANT_RESPONSIBILITY_ID
    ]
    require(
        len(matches) == 1,
        'cloud-agent semantic mapping requires exactly one '
        + CLOUD_AGENT_CONSTANT_RESPONSIBILITY_ID,
    )
    item = matches[0]
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    source = existing_sources.get(CLOUD_AGENT_CONSTANT_SOURCE_PATH, {})
    source.update({
        'path': CLOUD_AGENT_CONSTANT_SOURCE_PATH,
        'blob_sha': source_object['sha'],
        'source_mode': source_object['mode'],
        'source_type': source_object['type'],
        'source_commit': desktop_commit,
        'mapping_quality': 'semantic',
        'symbols': list(CLOUD_AGENT_CONSTANT_SOURCE_SPEC['symbols']),
        'focused_tests': list(CLOUD_AGENT_CONSTANT_SOURCE_SPEC['focused_tests']),
        'android_targets': list(CLOUD_AGENT_CONSTANT_SOURCE_SPEC['android_targets']),
        'source_owner': CLOUD_AGENT_CONSTANT_SOURCE_SPEC['source_owner'],
        'android_owner': CLOUD_AGENT_CONSTANT_SOURCE_SPEC['android_owner'],
    })
    existing_sources[CLOUD_AGENT_CONSTANT_SOURCE_PATH] = source
    item['desktop_sources'] = sorted(
        existing_sources.values(), key=lambda value: value.get('path', '')
    )
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for target in CLOUD_AGENT_CONSTANT_SOURCE_SPEC['android_targets']:
        if target not in raw_targets:
            raw_targets.append(target)
    item['android_target'] = raw_targets
    note = (
        'Desktop walkthrough artifact-directory fallback is semantically mapped to the existing '
        'Host root-turn prompt owner. Android consumes the shared constant in android_system_prompt '
        'and keeps AndroidJsonHost as the shipping caller; no second prompt owner is added.'
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




SAND_BOX_ARCHIVE_SOURCE_PATH = 'source/packages/constants/sand-box-archive.ts'
SAND_BOX_ARCHIVE_RESPONSIBILITY_ID = 'REMOTE-BOX-STORE-SYNC-ANDROID-001'
SAND_BOX_ARCHIVE_SOURCE_SPEC = {
    'symbols': ['SAND_BOX_PERSIST_ARCHIVE_EXCLUDES', 'SAND_BOX_WORKSPACE_IGNORE_FILE', 'SAND_BOX_WORKSPACE_DEFAULT_IGNORE_PATTERNS'],
    'source_owner': (
        'Desktop constants mirror declares archive exclusions and workspace-ignore names; shipping ownership '
        'is Host box_store_sync::production/workspace_ignore, where chrome-profile owns the four cache '
        'exclusions and workspace snapshot owns .sandignore/default ignore semantics'
    ),
    'android_owner': (
        'External RemoteBoxStoreBackend plus box_store_workspace_ignore are the canonical Remote Runner '
        'owners; source/packages/constants/src/sand_box_archive.rs is retained only as duplicate mirror/non-owner'
    ),
    'android_targets': [
        'source/box-exec-daemon/src/box_store_backend.rs',
        'source/box-exec-daemon/src/box_store_workspace_ignore.rs',
        'source/packages/constants/src/sand_box_archive.rs',
    ],
    'focused_tests': [
        'source/box-exec-daemon/src/box_store_backend.rs::tests::snapshot_and_hydrate_keep_profile_state_without_cache_and_do_not_filter_chrome_session',
        'source/box-exec-daemon/src/box_store_backend.rs::tests::frozen_destination_routes_desktop_chrome_profile_to_dedicated_root',
        'source/box-exec-daemon/src/box_store_backend.rs::tests::snapshot_applies_desktop_workspace_ignore_and_negation_semantics',
    ],
    'disposition': 'android-adapted',
}


def _ensure_sand_box_archive_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    source_object = new_by_path.get(SAND_BOX_ARCHIVE_SOURCE_PATH)
    if source_object is None:
        return
    matches = [item for item in responsibilities if item.get('responsibility_id') == SAND_BOX_ARCHIVE_RESPONSIBILITY_ID]
    require(len(matches) == 1, 'sand-box-archive semantic mapping requires exactly one ' + SAND_BOX_ARCHIVE_RESPONSIBILITY_ID)
    item = matches[0]
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    source = existing_sources.get(SAND_BOX_ARCHIVE_SOURCE_PATH, {})
    source.update({
        'path': SAND_BOX_ARCHIVE_SOURCE_PATH,
        'blob_sha': source_object['sha'],
        'source_mode': source_object['mode'],
        'source_type': source_object['type'],
        'source_commit': desktop_commit,
        'mapping_quality': 'semantic',
        'symbols': list(SAND_BOX_ARCHIVE_SOURCE_SPEC['symbols']),
        'focused_tests': list(SAND_BOX_ARCHIVE_SOURCE_SPEC['focused_tests']),
        'android_targets': list(SAND_BOX_ARCHIVE_SOURCE_SPEC['android_targets']),
        'source_owner': SAND_BOX_ARCHIVE_SOURCE_SPEC['source_owner'],
        'android_owner': SAND_BOX_ARCHIVE_SOURCE_SPEC['android_owner'],
    })
    existing_sources[SAND_BOX_ARCHIVE_SOURCE_PATH] = source
    item['desktop_sources'] = sorted(existing_sources.values(), key=lambda value: value.get('path', ''))
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for target in SAND_BOX_ARCHIVE_SOURCE_SPEC['android_targets']:
        if target not in raw_targets:
            raw_targets.append(target)
    item['android_target'] = raw_targets


CLOUD_AGENT_CONSTANT_INCOMPLETE_SOURCE_PATH = CLOUD_AGENT_CONSTANT_SOURCE_PATH


def _keep_cloud_agent_constant_incomplete(
    responsibilities: list[dict],
) -> None:
    """Fail closed until the complete Desktop cloud-agent constant contract has one Android owner."""
    for item in responsibilities:
        desktop_sources = item.get('desktop_sources')
        if isinstance(desktop_sources, list):
            item['desktop_sources'] = [
                source for source in desktop_sources
                if not (
                    isinstance(source, dict)
                    and source.get('path') == CLOUD_AGENT_CONSTANT_INCOMPLETE_SOURCE_PATH
                )
            ]
        for evidence_key in ('production_evidence', 'test_evidence'):
            evidence_rows = item.get(evidence_key)
            if not isinstance(evidence_rows, list):
                continue
            item[evidence_key] = [
                evidence for evidence in evidence_rows
                if CLOUD_AGENT_CONSTANT_INCOMPLETE_SOURCE_PATH not in (
                    evidence.get('source_paths')
                    if isinstance(evidence, dict) and isinstance(evidence.get('source_paths'), list)
                    else [evidence.get('source_path') if isinstance(evidence, dict) else None]
                )
            ]


AGENT_STORE_IDS_INCOMPLETE_SOURCE_PATH = 'source/packages/constants/agent-store-ids.ts'


def _keep_agent_store_ids_incomplete(
    responsibilities: list[dict],
) -> None:
    """Fail closed until the full mounted-store/token-routing contract has one Android owner."""
    for item in responsibilities:
        desktop_sources = item.get('desktop_sources')
        if isinstance(desktop_sources, list):
            item['desktop_sources'] = [
                source for source in desktop_sources
                if not (
                    isinstance(source, dict)
                    and source.get('path') == AGENT_STORE_IDS_INCOMPLETE_SOURCE_PATH
                )
            ]
        for evidence_key in ('production_evidence', 'test_evidence'):
            evidence_rows = item.get(evidence_key)
            if not isinstance(evidence_rows, list):
                continue
            item[evidence_key] = [
                evidence for evidence in evidence_rows
                if AGENT_STORE_IDS_INCOMPLETE_SOURCE_PATH not in (
                    evidence.get('source_paths')
                    if isinstance(evidence, dict) and isinstance(evidence.get('source_paths'), list)
                    else [evidence.get('source_path') if isinstance(evidence, dict) else None]
                )
            ]


MCP_CONSTANT_INCOMPLETE_SOURCE_PATH = 'source/packages/constants/mcp.ts'


def _keep_mcp_constant_incomplete(
    responsibilities: list[dict],
) -> None:
    """Fail closed until provider policy, redirect, vault and Android auth routing share one production owner."""
    for item in responsibilities:
        desktop_sources = item.get('desktop_sources')
        if isinstance(desktop_sources, list):
            item['desktop_sources'] = [
                source for source in desktop_sources
                if not (
                    isinstance(source, dict)
                    and source.get('path') == MCP_CONSTANT_INCOMPLETE_SOURCE_PATH
                )
            ]
        for evidence_key in ('production_evidence', 'test_evidence'):
            evidence_rows = item.get(evidence_key)
            if not isinstance(evidence_rows, list):
                continue
            item[evidence_key] = [
                evidence for evidence in evidence_rows
                if MCP_CONSTANT_INCOMPLETE_SOURCE_PATH not in (
                    evidence.get('source_paths')
                    if isinstance(evidence, dict) and isinstance(evidence.get('source_paths'), list)
                    else [evidence.get('source_path') if isinstance(evidence, dict) else None]
                )
            ]


def _ensure_constants_composer_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Attach the Desktop Composer text-size contract to the canonical Android draft owner."""
    source_object = new_by_path.get(COMPOSER_CONSTANT_SOURCE_PATH)
    if source_object is None:
        return
    require(
        source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
        'Desktop Composer constant has invalid blob/mode/type',
    )
    matches = [
        item for item in responsibilities
        if item.get('responsibility_id') == COMPOSER_CONSTANT_RESPONSIBILITY_ID
    ]
    require(
        len(matches) == 1,
        'Composer constant semantic mapping requires exactly one '
        + COMPOSER_CONSTANT_RESPONSIBILITY_ID,
    )
    item = matches[0]
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    source = existing_sources.get(COMPOSER_CONSTANT_SOURCE_PATH, {})
    source.update({
        'path': COMPOSER_CONSTANT_SOURCE_PATH,
        'blob_sha': source_object['sha'],
        'source_mode': source_object['mode'],
        'source_type': source_object['type'],
        'source_commit': desktop_commit,
        'mapping_quality': 'semantic',
        'symbols': list(COMPOSER_CONSTANT_SOURCE_SPEC['symbols']),
        'focused_tests': list(COMPOSER_CONSTANT_SOURCE_SPEC['focused_tests']),
        'android_targets': list(COMPOSER_CONSTANT_SOURCE_SPEC['android_targets']),
        'source_owner': COMPOSER_CONSTANT_SOURCE_SPEC['source_owner'],
        'android_owner': COMPOSER_CONSTANT_SOURCE_SPEC['android_owner'],
    })
    existing_sources[COMPOSER_CONSTANT_SOURCE_PATH] = source
    item['desktop_sources'] = sorted(
        existing_sources.values(), key=lambda value: value.get('path', '')
    )
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for target in COMPOSER_CONSTANT_SOURCE_SPEC['android_targets']:
        if target not in raw_targets:
            raw_targets.append(target)
    item['android_target'] = raw_targets
    note = (
        'Desktop MAX_TEXT_SIZE is semantically mapped to the Rust constants crate and enforced by '
        'Host DurableComposerDraftOwner. Same-head production/test evidence is required before this '
        'source row can satisfy package semantic coverage.'
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



def _ensure_auto_spillover_ui_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Map declaration-only Desktop copy to the existing Android native usage replacement."""
    source_object = new_by_path.get(AUTO_SPILLOVER_UI_SOURCE_PATH)
    if source_object is None:
        return
    require(
        source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
        'Desktop auto-spillover-ui constant has invalid blob/mode/type',
    )
    matches = [
        item for item in responsibilities
        if item.get('responsibility_id') == AUTO_SPILLOVER_UI_RESPONSIBILITY_ID
    ]
    require(
        len(matches) == 1,
        'auto-spillover-ui semantic mapping requires exactly one '
        + AUTO_SPILLOVER_UI_RESPONSIBILITY_ID,
    )
    item = matches[0]
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    source = existing_sources.get(AUTO_SPILLOVER_UI_SOURCE_PATH, {})
    source.update({
        'path': AUTO_SPILLOVER_UI_SOURCE_PATH,
        'blob_sha': source_object['sha'],
        'source_mode': source_object['mode'],
        'source_type': source_object['type'],
        'source_commit': desktop_commit,
        'mapping_quality': 'semantic',
        'symbols': list(AUTO_SPILLOVER_UI_SOURCE_SPEC['symbols']),
        'focused_tests': list(AUTO_SPILLOVER_UI_SOURCE_SPEC['focused_tests']),
        'android_targets': list(AUTO_SPILLOVER_UI_SOURCE_SPEC['android_targets']),
        'source_owner': AUTO_SPILLOVER_UI_SOURCE_SPEC['source_owner'],
        'android_owner': AUTO_SPILLOVER_UI_SOURCE_SPEC['android_owner'],
        'platform_delta': AUTO_SPILLOVER_UI_SOURCE_SPEC['platform_delta'],
        'evidence_semantics': AUTO_SPILLOVER_UI_SOURCE_SPEC['evidence_semantics'],
    })
    existing_sources[AUTO_SPILLOVER_UI_SOURCE_PATH] = source
    item['desktop_sources'] = sorted(
        existing_sources.values(), key=lambda value: value.get('path', '')
    )
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for target in AUTO_SPILLOVER_UI_SOURCE_SPEC['android_targets']:
        if target not in raw_targets:
            raw_targets.append(target)
    item['android_target'] = raw_targets
    item['android_disposition'] = AUTO_SPILLOVER_UI_SOURCE_SPEC['disposition']
    note = (
        'Desktop auto-spillover-ui.ts is a standalone recovered declaration; the generated experiment registry '
        'independently mirrors the seven literals as auto_spillover_ui_config fallback values and SandExperimentService '
        'generically projects that registry, but exact-tree evidence shows no named config consumer outside the generated '
        'registry. Current shipping Usage & Billing state still flows from cursor-profile through the Settings usage '
        'projection. Android mirrors the seven literals in fabushi-constants but deliberately leaves them out of JNI/'
        'Compose; the existing Dashboard backend, typed Coordinator usage projection and native Settings surface are '
        'the Android-adapted replacement. Same-head production/test evidence is required and does not promote '
        'implementation/verification status.'
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


def _ensure_constants_structured_log_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Attach Desktop structured-log replay/skew policy to the durable Android Host spool."""
    source_object = new_by_path.get(STRUCTURED_LOG_CONSTANT_SOURCE_PATH)
    if source_object is None:
        return
    require(
        source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
        'Desktop structured-log constant has invalid blob/mode/type',
    )
    matches = [
        item for item in responsibilities
        if item.get('responsibility_id') == STRUCTURED_LOG_CONSTANT_RESPONSIBILITY_ID
    ]
    require(
        len(matches) == 1,
        'structured-log constant semantic mapping requires exactly one '
        + STRUCTURED_LOG_CONSTANT_RESPONSIBILITY_ID,
    )
    item = matches[0]
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    source = existing_sources.get(STRUCTURED_LOG_CONSTANT_SOURCE_PATH, {})
    source.update({
        'path': STRUCTURED_LOG_CONSTANT_SOURCE_PATH,
        'blob_sha': source_object['sha'],
        'source_mode': source_object['mode'],
        'source_type': source_object['type'],
        'source_commit': desktop_commit,
        'mapping_quality': 'semantic',
        'symbols': list(STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['symbols']),
        'focused_tests': list(STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['focused_tests']),
        'android_targets': list(STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['android_targets']),
        'source_owner': STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['source_owner'],
        'android_owner': STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['android_owner'],
        'platform_delta': STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['platform_delta'],
        'evidence_semantics': STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['evidence_semantics'],
    })
    existing_sources[STRUCTURED_LOG_CONSTANT_SOURCE_PATH] = source
    item['desktop_sources'] = sorted(
        existing_sources.values(), key=lambda value: value.get('path', '')
    )
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for target in STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['android_targets']:
        if target not in raw_targets:
            raw_targets.append(target)
    item['android_target'] = raw_targets
    item['android_disposition'] = STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['disposition']
    stale_note = (
        'Desktop structured-log replay age/future-skew constants are semantically mapped to '
        'fabushi-constants and enforced by AndroidStructuredLogSpill before recovered telemetry '
        'can re-enter the shipper. Same-head production/test evidence is required for package closure.'
    )
    note = (
        'Desktop structured-log.ts is declaration-only for these constants: shipping 17-hour replay '
        'enforcement lives in structured-log-transport.ts and structured_log_telemetry.rs, while the '
        '2-hour future-skew declaration has no current Desktop production caller. AndroidStructuredLogSpill '
        'preserves the 17-hour shipping behavior and additionally rejects timestamps beyond the declared '
        '2-hour future horizon as Android-adapted hardening. Same-head production/test evidence proves the '
        'Android wiring and hardening only; it does not prove Desktop 2-hour shipping parity.'
    )
    notes = item.get('notes')
    if isinstance(notes, str):
        notes = notes.replace(stale_note, '').strip()
        item['notes'] = (notes + ' ' + note).strip() if note not in notes else notes
    elif isinstance(notes, list):
        notes[:] = [value for value in notes if value != stale_note]
        if note not in notes:
            notes.append(note)
    else:
        item['notes'] = note


def _ensure_mcp_tool_timeout_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Attach Desktop MCP one-hour timeout policy to the shipping Android Host MCP meta owner."""
    source_object = new_by_path.get(MCP_TOOL_TIMEOUT_SOURCE_PATH)
    if source_object is None:
        return
    require(
        source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
        'Desktop MCP tool timeout config has invalid blob/mode/type',
    )
    matches = [
        item for item in responsibilities
        if item.get('responsibility_id') == MCP_TOOL_TIMEOUT_RESPONSIBILITY_ID
    ]
    require(
        len(matches) == 1,
        'MCP tool timeout semantic mapping requires exactly one '
        + MCP_TOOL_TIMEOUT_RESPONSIBILITY_ID,
    )
    item = matches[0]
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    source = existing_sources.get(MCP_TOOL_TIMEOUT_SOURCE_PATH, {})
    source.update({
        'path': MCP_TOOL_TIMEOUT_SOURCE_PATH,
        'blob_sha': source_object['sha'],
        'source_mode': source_object['mode'],
        'source_type': source_object['type'],
        'source_commit': desktop_commit,
        'mapping_quality': 'semantic',
        'symbols': list(MCP_TOOL_TIMEOUT_SOURCE_SPEC['symbols']),
        'focused_tests': list(MCP_TOOL_TIMEOUT_SOURCE_SPEC['focused_tests']),
        'android_targets': list(MCP_TOOL_TIMEOUT_SOURCE_SPEC['android_targets']),
        'source_owner': MCP_TOOL_TIMEOUT_SOURCE_SPEC['source_owner'],
        'android_owner': MCP_TOOL_TIMEOUT_SOURCE_SPEC['android_owner'],
    })
    existing_sources[MCP_TOOL_TIMEOUT_SOURCE_PATH] = source
    item['desktop_sources'] = sorted(
        existing_sources.values(), key=lambda value: value.get('path', '')
    )
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for target in MCP_TOOL_TIMEOUT_SOURCE_SPEC['android_targets']:
        if target not in raw_targets:
            raw_targets.append(target)
    item['android_target'] = raw_targets
    note = (
        'Desktop one-hour MCP tool-call timeout is owned by fabushi-android-mcp-core and consumed '
        'by the shipping Host MCP meta-tool long timeout tier. Android retains additional bounded '
        'tiers without changing the Desktop default/legacy one-hour contract. Same-head production/'
        'test evidence is required for package closure.'
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



def _ensure_automation_schedule_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Refresh the current Desktop schedule source semantically without inheriting verification."""
    path = AUTOMATION_SCHEDULE_SOURCE_PATH
    if path not in new_by_path:
        return
    source_object = new_by_path[path]
    require(
        source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
        'automation schedule Desktop source has invalid blob/mode/type',
    )
    require(
        source_object.get('sha') == AUTOMATION_SCHEDULE_DESKTOP_BLOB,
        'automation schedule Desktop blob does not match the pinned current-main authority',
    )
    item = next(
        (
            value for value in responsibilities
            if value.get('responsibility_id') == AUTOMATION_SCHEDULE_RESPONSIBILITY_ID
        ),
        None,
    )
    require(item is not None, 'automation schedule responsibility is missing')
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    source = existing_sources.get(path, {})
    source.update({
        'path': path,
        'blob_sha': source_object['sha'],
        'source_mode': source_object['mode'],
        'source_type': source_object['type'],
        'source_commit': desktop_commit,
        'mapping_quality': 'semantic',
        'symbols': list(AUTOMATION_SCHEDULE_SOURCE_SPEC['symbols']),
        'android_targets': list(AUTOMATION_SCHEDULE_SOURCE_SPEC['android_targets']),
        'source_owner': AUTOMATION_SCHEDULE_SOURCE_SPEC['source_owner'],
        'android_owner': AUTOMATION_SCHEDULE_SOURCE_SPEC['android_owner'],
    })
    existing_sources[path] = source
    item['desktop_sources'] = sorted(
        existing_sources.values(), key=lambda value: value.get('path', '')
    )
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for target in AUTOMATION_SCHEDULE_SOURCE_SPEC['android_targets']:
        if target not in raw_targets:
            raw_targets.append(target)
    item['android_target'] = raw_targets
    note = (
        'Fresh current-main schedule authority is ' + desktop_commit + '/'
        + AUTOMATION_SCHEDULE_DESKTOP_BLOB
        + '; Desktop and Android carry the same Rust schedule algorithm and Android production callers '
        'are AutomationRuntime plus FileAutomationStore. This mapping remains unreviewed/not-verified '
        'unless the same Android exact HEAD passes the dedicated production and focused test steps; '
        'predecessor verification is never inherited.'
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


def _attach_automation_schedule_exact_head_evidence(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    tracked_android_paths: set[str],
    current: str,
    tree_sha: str,
    android: dict,
) -> None:
    """Attach fresh schedule production/test evidence while preserving not-verified acceptance status."""
    path = AUTOMATION_SCHEDULE_SOURCE_PATH
    if path not in new_by_path:
        return
    responsibility = next(
        item for item in responsibilities
        if item.get('responsibility_id') == AUTOMATION_SCHEDULE_RESPONSIBILITY_ID
    )
    for evidence_key in ('production_evidence', 'test_evidence'):
        existing_evidence = responsibility.get(evidence_key)
        if not isinstance(existing_evidence, list):
            existing_evidence = []
        responsibility[evidence_key] = [
            evidence for evidence in existing_evidence
            if path not in (
                evidence.get('source_paths')
                if isinstance(evidence.get('source_paths'), list)
                else [evidence.get('source_path')]
            )
        ]

    ci_evidence = _latest_exact_head_android_ci_evidence(
        android['android_pr_head_sha'],
        production_step_name=AUTOMATION_SCHEDULE_PRODUCTION_STEP,
        test_step_name=AUTOMATION_SCHEDULE_TEST_STEP,
        evidence_label='automation schedule',
    )
    if ci_evidence is None:
        print(
            'WARNING: no successful same-exact-head automation schedule evidence; '
            'schedule source remains provenance-only',
            file=sys.stderr,
        )
        return

    production_target = AUTOMATION_SCHEDULE_SOURCE_SPEC['production_target']
    test_targets = AUTOMATION_SCHEDULE_SOURCE_SPEC['test_targets']
    required_targets = [
        *AUTOMATION_SCHEDULE_SOURCE_SPEC['android_targets'],
        production_target,
        *test_targets.keys(),
    ]
    missing_targets = sorted({
        target for target in required_targets if target not in tracked_android_paths
    })
    require(
        not missing_targets,
        'automation schedule evidence target is not tracked at exact HEAD: ' + ', '.join(missing_targets),
    )
    common = {
        'source_paths': [path],
        'source_commit': current,
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'source_blob_sha': new_by_path[path]['sha'],
        'android_pr_head_sha': android['android_pr_head_sha'],
        'android_tested_checkout_sha': android['android_checkout_sha'],
        'workflow_sha': android['workflow_sha'],
        'workflow_name': ci_evidence['workflow_name'],
        'workflow_id': ci_evidence['workflow_id'],
        'workflow_path': ci_evidence['workflow_path'],
        'workflow_event': ci_evidence['workflow_event'],
        'workflow_repository': ci_evidence['workflow_repository'],
        'workflow_run_id': ci_evidence['workflow_run_id'],
        'workflow_job_name': ci_evidence['workflow_job_name'],
        'workflow_job_id': ci_evidence['workflow_job_id'],
        'workflow_job_run_attempt': ci_evidence['workflow_job_run_attempt'],
        'run_attempt': ci_evidence['run_attempt'],
        'run_conclusion': ci_evidence['run_conclusion'],
        'result': 'passed',
    }
    responsibility['production_evidence'].append({
        **common,
        'kind': 'production-wiring',
        'workflow_step_name': ci_evidence['production_step_name'],
        'workflow_step_number': ci_evidence['production_step_number'],
        'evidence_ref': production_target + '#automation-schedule-production-owner',
    })
    for test_target, focused_tests in test_targets.items():
        responsibility['test_evidence'].append({
            **common,
            'kind': 'focused-unit-test',
            'workflow_step_name': ci_evidence['test_step_name'],
            'workflow_step_number': ci_evidence['test_step_number'],
            'evidence_ref': test_target + '#automation-schedule-focused-tests',
            'focused_tests': list(focused_tests),
        })



def _ensure_client_side_tool_v2_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Bind current Desktop relay sources to real Android Rust/JNI owners without inheriting verification."""
    if not all(path in new_by_path for path in CLIENT_SIDE_TOOL_V2_SOURCE_SPECS):
        return
    item = next(
        (value for value in responsibilities if value.get('responsibility_id') == CLIENT_SIDE_TOOL_V2_RESPONSIBILITY_ID),
        None,
    )
    require(item is not None, 'client-side-tool-v2 relay responsibility is missing')
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for path, spec in CLIENT_SIDE_TOOL_V2_SOURCE_SPECS.items():
        source_object = new_by_path[path]
        require(
            source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
            'client-side-tool-v2 Desktop source has invalid blob/mode/type: ' + path,
        )
        require(
            source_object.get('sha') == spec['blob_sha'],
            'client-side-tool-v2 Desktop blob does not match pinned current-main authority: ' + path,
        )
        source = existing_sources.get(path, {})
        source.update({
            'path': path,
            'blob_sha': source_object['sha'],
            'source_mode': source_object['mode'],
            'source_type': source_object['type'],
            'source_commit': desktop_commit,
            'mapping_quality': 'semantic',
            'symbols': list(spec['symbols']),
            'android_targets': list(spec['android_targets']),
            'source_owner': spec['source_owner'],
            'android_owner': spec['android_owner'],
        })
        existing_sources[path] = source
        for target in spec['android_targets']:
            if target not in raw_targets:
                raw_targets.append(target)
    item['desktop_sources'] = sorted(existing_sources.values(), key=lambda value: value.get('path', ''))
    item['android_target'] = raw_targets
    note = (
        'Fresh current-main client-side-tool-v2 relay authority is bound to Desktop shipping '
        'Coordinator/shared transport sources and the Android Rust Coordinator/JNI production ingress. '
        'Same-head production and focused test evidence is required; predecessor verification is never inherited.'
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


def _attach_client_side_tool_v2_exact_head_evidence(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    tracked_android_paths: set[str],
    current: str,
    tree_sha: str,
    android: dict,
) -> None:
    """Attach fresh same-HEAD relay evidence while leaving rebaseline acceptance status conservative."""
    source_paths = list(CLIENT_SIDE_TOOL_V2_SOURCE_SPECS)
    if not all(path in new_by_path for path in source_paths):
        return
    responsibility = next(
        item for item in responsibilities
        if item.get('responsibility_id') == CLIENT_SIDE_TOOL_V2_RESPONSIBILITY_ID
    )
    source_path_set = set(source_paths)
    for evidence_key in ('production_evidence', 'test_evidence'):
        existing_evidence = responsibility.get(evidence_key)
        if not isinstance(existing_evidence, list):
            existing_evidence = []
        responsibility[evidence_key] = [
            evidence for evidence in existing_evidence
            if not source_path_set.intersection(
                evidence.get('source_paths')
                if isinstance(evidence.get('source_paths'), list)
                else [evidence.get('source_path')]
            )
        ]
    ci_evidence = _latest_exact_head_android_ci_evidence(
        android['android_pr_head_sha'],
        production_step_name=CLIENT_SIDE_TOOL_V2_PRODUCTION_STEP,
        test_step_name=CLIENT_SIDE_TOOL_V2_TEST_STEP,
        evidence_label='client-side tool v2 relay',
    )
    if ci_evidence is None:
        print(
            'WARNING: no successful same-exact-head client-side-tool-v2 relay evidence; relay sources remain provenance-only',
            file=sys.stderr,
        )
        return
    required_targets = {
        target
        for spec in CLIENT_SIDE_TOOL_V2_SOURCE_SPECS.values()
        for target in spec['android_targets']
    } | set(CLIENT_SIDE_TOOL_V2_TEST_TARGETS)
    missing_targets = sorted(target for target in required_targets if target not in tracked_android_paths)
    require(
        not missing_targets,
        'client-side-tool-v2 evidence target is not tracked at exact HEAD: ' + ', '.join(missing_targets),
    )
    common = {
        'source_paths': source_paths,
        'source_commit': current,
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'source_blob_shas': {path: new_by_path[path]['sha'] for path in source_paths},
        'android_pr_head_sha': android['android_pr_head_sha'],
        'android_tested_checkout_sha': android['android_checkout_sha'],
        'workflow_sha': android['workflow_sha'],
        'workflow_name': ci_evidence['workflow_name'],
        'workflow_id': ci_evidence['workflow_id'],
        'workflow_path': ci_evidence['workflow_path'],
        'workflow_event': ci_evidence['workflow_event'],
        'workflow_repository': ci_evidence['workflow_repository'],
        'workflow_run_id': ci_evidence['workflow_run_id'],
        'workflow_job_name': ci_evidence['workflow_job_name'],
        'workflow_job_id': ci_evidence['workflow_job_id'],
        'workflow_job_run_attempt': ci_evidence['workflow_job_run_attempt'],
        'run_attempt': ci_evidence['run_attempt'],
        'run_conclusion': ci_evidence['run_conclusion'],
        'result': 'passed',
    }
    responsibility['production_evidence'].append({
        **common,
        'kind': 'production-wiring',
        'workflow_step_name': ci_evidence['production_step_name'],
        'workflow_step_number': ci_evidence['production_step_number'],
        'evidence_ref': 'source/mahayana-agent-coordinator/src/main.rs#client-side-tool-v2-production-owner',
    })
    for test_target, focused_tests in CLIENT_SIDE_TOOL_V2_TEST_TARGETS.items():
        responsibility['test_evidence'].append({
            **common,
            'kind': 'focused-unit-test',
            'workflow_step_name': ci_evidence['test_step_name'],
            'workflow_step_number': ci_evidence['test_step_number'],
            'evidence_ref': test_target + '#client-side-tool-v2-focused-tests',
            'focused_tests': list(focused_tests),
        })

def _ensure_app_agent_surface_e2e_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Attach the packaged Desktop App Agent E2E anchor without coupling official-device fixtures."""
    source_object = new_by_path.get(APP_AGENT_SURFACE_E2E_SOURCE_PATH)
    if source_object is None:
        return
    matches = [
        item for item in responsibilities
        if item.get('responsibility_id') == APP_AGENT_SURFACE_E2E_RESPONSIBILITY_ID
    ]
    require(len(matches) == 1, 'App Agent Surface E2E mapping requires exactly one ' + APP_AGENT_SURFACE_E2E_RESPONSIBILITY_ID)
    require(
        source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
        'App Agent Surface E2E Desktop source has invalid blob/mode/type',
    )
    item = matches[0]
    existing_sources = {
        source.get('path'): dict(source)
        for source in item.get('desktop_sources', [])
        if isinstance(source, dict) and isinstance(source.get('path'), str)
    }
    source = existing_sources.get(APP_AGENT_SURFACE_E2E_SOURCE_PATH, {})
    source.update({
        'path': APP_AGENT_SURFACE_E2E_SOURCE_PATH,
        'blob_sha': source_object['sha'],
        'source_mode': source_object['mode'],
        'source_type': source_object['type'],
        'source_commit': desktop_commit,
        'mapping_quality': 'semantic',
        'symbols': list(APP_AGENT_SURFACE_E2E_SOURCE_SPEC['symbols']),
        'android_targets': list(APP_AGENT_SURFACE_E2E_SOURCE_SPEC['android_targets']),
        'source_owner': APP_AGENT_SURFACE_E2E_SOURCE_SPEC['source_owner'],
        'android_owner': APP_AGENT_SURFACE_E2E_SOURCE_SPEC['android_owner'],
    })
    existing_sources[APP_AGENT_SURFACE_E2E_SOURCE_PATH] = source
    for path, spec in APP_AGENT_SURFACE_CROSS_SOURCE_SPECS.items():
        cross_object = new_by_path.get(path)
        require(cross_object is not None, 'App Agent Surface cross-anchor is absent from current Desktop tree: ' + path)
        require(
            cross_object.get('type') == 'blob' and cross_object.get('mode') in ALLOWED_SOURCE_MODES,
            'App Agent Surface cross-anchor has invalid blob/mode/type: ' + path,
        )
        cross = existing_sources.get(path, {})
        cross.update({
            'path': path,
            'blob_sha': cross_object['sha'],
            'source_mode': cross_object['mode'],
            'source_type': cross_object['type'],
            'source_commit': desktop_commit,
            'mapping_quality': 'semantic',
            'symbols': list(spec['symbols']),
            'android_targets': list(spec['android_targets']),
            'source_owner': spec['source_owner'],
            'android_owner': spec['android_owner'],
        })
        existing_sources[path] = cross
    item['desktop_sources'] = sorted(existing_sources.values(), key=lambda value: value.get('path', ''))
    raw_targets = item.get('android_target')
    if not isinstance(raw_targets, list):
        raw_targets = []
    for target in APP_AGENT_SURFACE_E2E_SOURCE_SPEC['android_targets']:
        if target not in raw_targets:
            raw_targets.append(target)
    for spec in APP_AGENT_SURFACE_CROSS_SOURCE_SPECS.values():
        for target in spec['android_targets']:
            if target not in raw_targets:
                raw_targets.append(target)
    item['android_target'] = raw_targets


def _ensure_official_device_registration_semantic_mapping(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    desktop_commit: str,
) -> None:
    """Attach the new Desktop official-device/account production sources without inheriting verification."""
    if not any(path in new_by_path for path in OFFICIAL_DEVICE_SOURCE_SPECS):
        return
    missing = sorted(set(OFFICIAL_DEVICE_SOURCE_SPECS) - set(new_by_path))
    require(not missing, 'current Desktop official-device source set is incomplete: ' + ', '.join(missing))
    by_id = {
        item.get('responsibility_id'): item
        for item in responsibilities
        if item.get('responsibility_id')
    }
    required_ids = {spec['responsibility_id'] for spec in OFFICIAL_DEVICE_SOURCE_SPECS.values()}
    missing_ids = sorted(required_ids - set(by_id))
    require(not missing_ids, 'official-device mapping responsibility is missing: ' + ', '.join(missing_ids))
    for path, spec in OFFICIAL_DEVICE_SOURCE_SPECS.items():
        source_object = new_by_path[path]
        require(
            source_object.get('type') == 'blob' and source_object.get('mode') in ALLOWED_SOURCE_MODES,
            'official-device Desktop source has invalid blob/mode/type: ' + path,
        )
        item = by_id[spec['responsibility_id']]
        existing_sources = {
            source.get('path'): dict(source)
            for source in item.get('desktop_sources', [])
            if isinstance(source, dict) and isinstance(source.get('path'), str)
        }
        source = existing_sources.get(path, {})
        source.update({
            'path': path,
            'blob_sha': source_object['sha'],
            'source_mode': source_object['mode'],
            'source_type': source_object['type'],
            'source_commit': desktop_commit,
            'mapping_quality': 'semantic',
            'symbols': list(spec['symbols']),
            'android_targets': list(spec['android_targets']),
            'source_owner': spec['source_owner'],
            'android_owner': spec['android_owner'],
        })
        existing_sources[path] = source
        item['desktop_sources'] = sorted(
            existing_sources.values(), key=lambda value: value.get('path', '')
        )
        raw_targets = item.get('android_target')
        if not isinstance(raw_targets, list):
            raw_targets = []
        for target in spec['android_targets']:
            if target not in raw_targets:
                raw_targets.append(target)
        item['android_target'] = raw_targets
        note = (
            'Desktop official-device registration authority was added at ' + desktop_commit
            + '; this source mapping is provenance-only and remains unreviewed/not-verified. '
            'Android has a real FabushiRemoteDeviceGateway production owner whose official published '
            'tool surface is now status-only/read-only like Desktop. This mapping remains provenance-only '
            'and not verified until WSS lifecycle, session expiry, account-switch/revoke fencing, token '
            'boundary, read-only preflight, non-retry behavior and exact-head tests are all proven.'
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


def _attach_official_device_registration_exact_head_evidence(
    responsibilities: list[dict],
    new_by_path: dict[str, dict],
    tracked_android_paths: set[str],
    current: str,
    tree_sha: str,
    android: dict,
) -> None:
    """Attach path-scoped same-exact-head Action evidence without upgrading acceptance status."""
    source_path = OFFICIAL_DEVICE_EVIDENCE_SOURCE_PATH
    if source_path not in new_by_path:
        return
    spec = OFFICIAL_DEVICE_SOURCE_SPECS[source_path]
    responsibility = next(
        item for item in responsibilities
        if item.get('responsibility_id') == spec['responsibility_id']
    )
    for evidence_key in ('production_evidence', 'test_evidence'):
        existing_evidence = responsibility.get(evidence_key)
        if not isinstance(existing_evidence, list):
            existing_evidence = []
        responsibility[evidence_key] = [
            evidence for evidence in existing_evidence
            if source_path not in (
                evidence.get('source_paths')
                if isinstance(evidence.get('source_paths'), list)
                else [evidence.get('source_path')]
            )
        ]

    ci_evidence = _latest_exact_head_android_ci_evidence(
        android['android_pr_head_sha'],
        production_step_name=OFFICIAL_DEVICE_PRODUCTION_STEP,
        test_step_name=OFFICIAL_DEVICE_TEST_STEP,
        evidence_label='official-device registration',
    )
    if ci_evidence is None:
        print(
            'WARNING: no successful same-exact-head official-device registration evidence; '
            'official-device source remains provenance-only',
            file=sys.stderr,
        )
        return

    production_target = spec['production_target']
    test_targets = spec['test_targets']
    required_targets = [production_target, *test_targets.keys()]
    missing_targets = [target for target in required_targets if target not in tracked_android_paths]
    require(
        not missing_targets,
        'official-device evidence target is not tracked at exact HEAD: ' + ', '.join(missing_targets),
    )
    common = {
        'source_paths': [source_path],
        'source_commit': current,
        'desktop_commit': current,
        'desktop_root_tree': tree_sha,
        'android_pr_head_sha': android['android_pr_head_sha'],
        'android_tested_checkout_sha': android['android_checkout_sha'],
        'workflow_sha': android['workflow_sha'],
        'workflow_name': ci_evidence['workflow_name'],
        'workflow_id': ci_evidence['workflow_id'],
        'workflow_path': ci_evidence['workflow_path'],
        'workflow_event': ci_evidence['workflow_event'],
        'workflow_repository': ci_evidence['workflow_repository'],
        'workflow_run_id': ci_evidence['workflow_run_id'],
        'workflow_job_name': ci_evidence['workflow_job_name'],
        'workflow_job_id': ci_evidence['workflow_job_id'],
        'workflow_job_run_attempt': ci_evidence['workflow_job_run_attempt'],
        'run_attempt': ci_evidence['run_attempt'],
        'run_conclusion': ci_evidence['run_conclusion'],
        'result': 'passed',
    }
    responsibility['production_evidence'].append({
        **common,
        'kind': 'production-wiring',
        'workflow_step_name': ci_evidence['production_step_name'],
        'workflow_step_number': ci_evidence['production_step_number'],
        'evidence_ref': production_target + '#official-device-registration-production-owner',
    })
    for test_target, focused_tests in test_targets.items():
        responsibility['test_evidence'].append({
            **common,
            'kind': 'focused-unit-test',
            'workflow_step_name': ci_evidence['test_step_name'],
            'workflow_step_number': ci_evidence['test_step_number'],
            'evidence_ref': test_target + '#official-device-registration-focused-tests',
            'focused_tests': list(focused_tests),
        })


def merge_responsibility_mapping(
    responsibility_path: Path,
    ledger_rows: list[dict],
    new_by_path: dict[str, dict],
    current: str,
    tree_sha: str,
    expected_responsibility_count: int | None = None,
    persist_outputs: bool = True,
    deferred_outputs: dict[str, dict] | None = None,
) -> dict:
    predecessor, predecessor_source_rows, mapping_origin, mapping_provenance = load_predecessor_responsibility_mapping(responsibility_path)
    android = current_android_provenance()
    predecessor_authority = predecessor.get('authority') or {}
    pinned_pair = (
        mapping_provenance.get('source_manifest_desktop_commit'),
        mapping_provenance.get('source_manifest_desktop_root_tree'),
    )
    authority_pair = (
        predecessor_authority.get('desktop_commit'),
        predecessor_authority.get('desktop_root_tree'),
    )
    prior_pair = (PRIOR_DESKTOP_COMMIT, PRIOR_DESKTOP_TREE)
    persisted_intermediate_pair = (
        PERSISTED_INTERMEDIATE_DESKTOP_COMMIT,
        PERSISTED_INTERMEDIATE_DESKTOP_TREE,
    )
    current_pair = (current, tree_sha)
    require(
        authority_pair in {pinned_pair, prior_pair, persisted_intermediate_pair, current_pair},
        'responsibility mapping authority is neither the digest-pinned predecessor, '
        'the exact prior accepted authority, nor the current persisted Desktop authority',
    )
    require(mapping_provenance.get('artifact_id') is not None and mapping_provenance.get('artifact_digest'), 'mapping must originate from a verified predecessor artifact')

    responsibilities = json.loads(json.dumps(predecessor.get('responsibilities', [])))
    if any(path.startswith('source/packages/agent-kv/') for path in new_by_path):
        _ensure_agent_kv_semantic_mapping(responsibilities, new_by_path, current)
    if any(path in new_by_path for path in BACKGROUND_COMPLETION_SOURCE_SPECS):
        _ensure_background_completion_semantic_mapping(responsibilities, new_by_path, current)
    if any(path in new_by_path for path in AGENT_CORE_INTERACTION_SOURCE_SPECS):
        _ensure_agent_core_interaction_semantic_mapping(responsibilities, new_by_path, current)
    if SAND_BOX_ARCHIVE_SOURCE_PATH in new_by_path:
        _ensure_sand_box_archive_semantic_mapping(responsibilities, new_by_path, current)
    _keep_cloud_agent_constant_incomplete(responsibilities)
    _keep_agent_store_ids_incomplete(responsibilities)
    _keep_mcp_constant_incomplete(responsibilities)
    if COMPOSER_CONSTANT_SOURCE_PATH in new_by_path:
        _ensure_constants_composer_semantic_mapping(responsibilities, new_by_path, current)
    if AUTO_SPILLOVER_UI_SOURCE_PATH in new_by_path:
        _ensure_auto_spillover_ui_semantic_mapping(responsibilities, new_by_path, current)
    if STRUCTURED_LOG_CONSTANT_SOURCE_PATH in new_by_path:
        _ensure_constants_structured_log_semantic_mapping(responsibilities, new_by_path, current)
    if MCP_TOOL_TIMEOUT_SOURCE_PATH in new_by_path:
        _ensure_mcp_tool_timeout_semantic_mapping(responsibilities, new_by_path, current)
    if AUTOMATION_SCHEDULE_SOURCE_PATH in new_by_path:
        _ensure_automation_schedule_semantic_mapping(responsibilities, new_by_path, current)
    if all(path in new_by_path for path in CLIENT_SIDE_TOOL_V2_SOURCE_SPECS):
        _ensure_client_side_tool_v2_semantic_mapping(responsibilities, new_by_path, current)
    if APP_AGENT_SURFACE_E2E_SOURCE_PATH in new_by_path:
        _ensure_app_agent_surface_e2e_semantic_mapping(responsibilities, new_by_path, current)
    if any(path in new_by_path for path in OFFICIAL_DEVICE_SOURCE_SPECS):
        _ensure_official_device_registration_semantic_mapping(responsibilities, new_by_path, current)
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
    _attach_automation_schedule_exact_head_evidence(
        responsibilities,
        new_by_path,
        tracked_android_paths,
        current,
        tree_sha,
        android,
    )
    _attach_client_side_tool_v2_exact_head_evidence(
        responsibilities,
        new_by_path,
        tracked_android_paths,
        current,
        tree_sha,
        android,
    )
    _attach_official_device_registration_exact_head_evidence(
        responsibilities,
        new_by_path,
        tracked_android_paths,
        current,
        tree_sha,
        android,
    )
    has_agent_kv_inventory = any(path in new_by_path for path in AGENT_KV_SOURCE_SPECS)
    rejected_android_targets = []
    source_targets_by_path: dict[str, list[str]] = {}


    if AUTO_SPILLOVER_UI_SOURCE_PATH in new_by_path:
        auto_spillover_responsibility = next(
            item for item in responsibilities
            if item.get('responsibility_id') == AUTO_SPILLOVER_UI_RESPONSIBILITY_ID
        )
        for evidence_key in ('production_evidence', 'test_evidence'):
            existing_evidence = auto_spillover_responsibility.get(evidence_key)
            if not isinstance(existing_evidence, list):
                existing_evidence = []
            auto_spillover_responsibility[evidence_key] = [
                evidence for evidence in existing_evidence
                if AUTO_SPILLOVER_UI_SOURCE_PATH not in (
                    evidence.get('source_paths')
                    if isinstance(evidence.get('source_paths'), list)
                    else [evidence.get('source_path')]
                )
            ]
        auto_spillover_evidence = _latest_exact_head_android_ci_evidence(
            android['android_pr_head_sha'],
            production_step_name=AUTO_SPILLOVER_UI_PRODUCTION_STEP,
            test_step_name=AUTO_SPILLOVER_UI_TEST_STEP,
            evidence_label='auto-spillover usage UI declaration boundary plus native replacement',
        )
        if auto_spillover_evidence is not None:
            production_target = AUTO_SPILLOVER_UI_SOURCE_SPEC['production_target']
            test_target = AUTO_SPILLOVER_UI_SOURCE_SPEC['test_target']
            require(
                production_target in tracked_android_paths and test_target in tracked_android_paths,
                'auto-spillover-ui evidence target is not tracked at exact HEAD',
            )
            common = {
                'source_paths': [AUTO_SPILLOVER_UI_SOURCE_PATH],
                'source_commit': current,
                'desktop_commit': current,
                'desktop_root_tree': tree_sha,
                'android_pr_head_sha': android['android_pr_head_sha'],
                'android_tested_checkout_sha': android['android_checkout_sha'],
                'workflow_sha': android['workflow_sha'],
                'workflow_name': auto_spillover_evidence['workflow_name'],
                'workflow_id': auto_spillover_evidence['workflow_id'],
                'workflow_path': auto_spillover_evidence['workflow_path'],
                'workflow_event': auto_spillover_evidence['workflow_event'],
                'workflow_repository': auto_spillover_evidence['workflow_repository'],
                'workflow_run_id': auto_spillover_evidence['workflow_run_id'],
                'workflow_job_name': auto_spillover_evidence['workflow_job_name'],
                'workflow_job_id': auto_spillover_evidence['workflow_job_id'],
                'workflow_job_run_attempt': auto_spillover_evidence['workflow_job_run_attempt'],
                'run_attempt': auto_spillover_evidence['run_attempt'],
                'run_conclusion': auto_spillover_evidence['run_conclusion'],
                'result': 'passed',
                'evidence_semantics': AUTO_SPILLOVER_UI_SOURCE_SPEC['evidence_semantics'],
            }
            auto_spillover_responsibility['production_evidence'].append({
                **common,
                'kind': 'production-wiring',
                'workflow_step_name': auto_spillover_evidence['production_step_name'],
                'workflow_step_number': auto_spillover_evidence['production_step_number'],
                'evidence_ref': production_target + '#account-usage-owner-with-declaration-only-ui-copy-boundary',
            })
            auto_spillover_responsibility['test_evidence'].append({
                **common,
                'kind': 'focused-unit-test',
                'workflow_step_name': auto_spillover_evidence['test_step_name'],
                'workflow_step_number': auto_spillover_evidence['test_step_number'],
                'evidence_ref': test_target + '#auto-spillover-declaration-boundary-and-native-usage-tests',
                'focused_tests': list(AUTO_SPILLOVER_UI_SOURCE_SPEC['focused_tests']),
            })
        else:
            print(
                'WARNING: no successful same-exact-head auto-spillover usage UI evidence; '
                'auto-spillover-ui package coverage remains incomplete',
                file=sys.stderr,
            )

    if STRUCTURED_LOG_CONSTANT_SOURCE_PATH in new_by_path:
        structured_responsibility = next(
            item for item in responsibilities
            if item.get('responsibility_id') == STRUCTURED_LOG_CONSTANT_RESPONSIBILITY_ID
        )
        for evidence_key in ('production_evidence', 'test_evidence'):
            existing_evidence = structured_responsibility.get(evidence_key)
            if not isinstance(existing_evidence, list):
                existing_evidence = []
            structured_responsibility[evidence_key] = [
                evidence for evidence in existing_evidence
                if STRUCTURED_LOG_CONSTANT_SOURCE_PATH not in (
                    evidence.get('source_paths')
                    if isinstance(evidence.get('source_paths'), list)
                    else [evidence.get('source_path')]
                )
            ]
        structured_evidence = _latest_exact_head_android_ci_evidence(
            android['android_pr_head_sha'],
            production_step_name=STRUCTURED_LOG_CONSTANT_PRODUCTION_STEP,
            test_step_name=STRUCTURED_LOG_CONSTANT_TEST_STEP,
            evidence_label='constants structured log replay plus Android hardening',
        )
        if structured_evidence is not None:
            production_target = STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['production_target']
            test_target = STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['test_target']
            require(
                production_target in tracked_android_paths and test_target in tracked_android_paths,
                'structured-log constant evidence target is not tracked at exact HEAD',
            )
            common = {
                'source_paths': [STRUCTURED_LOG_CONSTANT_SOURCE_PATH],
                'source_commit': current,
                'desktop_commit': current,
                'desktop_root_tree': tree_sha,
                'android_pr_head_sha': android['android_pr_head_sha'],
                'android_tested_checkout_sha': android['android_checkout_sha'],
                'workflow_sha': android['workflow_sha'],
                'workflow_name': structured_evidence['workflow_name'],
                'workflow_id': structured_evidence['workflow_id'],
                'workflow_path': structured_evidence['workflow_path'],
                'workflow_event': structured_evidence['workflow_event'],
                'workflow_repository': structured_evidence['workflow_repository'],
                'workflow_run_id': structured_evidence['workflow_run_id'],
                'workflow_job_name': structured_evidence['workflow_job_name'],
                'workflow_job_id': structured_evidence['workflow_job_id'],
                'workflow_job_run_attempt': structured_evidence['workflow_job_run_attempt'],
                'run_attempt': structured_evidence['run_attempt'],
                'run_conclusion': structured_evidence['run_conclusion'],
                'result': 'passed',
                'evidence_semantics': STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['evidence_semantics'],
            }
            structured_responsibility['production_evidence'].append({
                **common,
                'kind': 'production-wiring',
                'workflow_step_name': structured_evidence['production_step_name'],
                'workflow_step_number': structured_evidence['production_step_number'],
                'evidence_ref': production_target + '#structured-log-replay-production-and-android-hardening-owner',
            })
            structured_responsibility['test_evidence'].append({
                **common,
                'kind': 'focused-unit-test',
                'workflow_step_name': structured_evidence['test_step_name'],
                'workflow_step_number': structured_evidence['test_step_number'],
                'evidence_ref': test_target + '#structured-log-replay-and-android-hardening-focused-tests',
                'focused_tests': list(STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['focused_tests']),
            })
        else:
            print(
                'WARNING: no successful same-exact-head constants structured-log evidence; '
                'structured-log constant package coverage remains incomplete',
                file=sys.stderr,
            )

    if MCP_TOOL_TIMEOUT_SOURCE_PATH in new_by_path:
        mcp_timeout_responsibility = next(
            item for item in responsibilities
            if item.get('responsibility_id') == MCP_TOOL_TIMEOUT_RESPONSIBILITY_ID
        )
        for evidence_key in ('production_evidence', 'test_evidence'):
            existing_evidence = mcp_timeout_responsibility.get(evidence_key)
            if not isinstance(existing_evidence, list):
                existing_evidence = []
            mcp_timeout_responsibility[evidence_key] = [
                evidence for evidence in existing_evidence
                if MCP_TOOL_TIMEOUT_SOURCE_PATH not in (
                    evidence.get('source_paths')
                    if isinstance(evidence.get('source_paths'), list)
                    else [evidence.get('source_path')]
                )
            ]
        mcp_timeout_evidence = _latest_exact_head_android_ci_evidence(
            android['android_pr_head_sha'],
            production_step_name=MCP_TOOL_TIMEOUT_PRODUCTION_STEP,
            test_step_name=MCP_TOOL_TIMEOUT_TEST_STEP,
            evidence_label='MCP tool timeout',
        )
        if mcp_timeout_evidence is not None:
            production_target = MCP_TOOL_TIMEOUT_SOURCE_SPEC['production_target']
            test_target = MCP_TOOL_TIMEOUT_SOURCE_SPEC['test_target']
            require(
                production_target in tracked_android_paths and test_target in tracked_android_paths,
                'MCP tool timeout evidence target is not tracked at exact HEAD',
            )
            common = {
                'source_paths': [MCP_TOOL_TIMEOUT_SOURCE_PATH],
                'source_commit': current,
                'desktop_commit': current,
                'desktop_root_tree': tree_sha,
                'android_pr_head_sha': android['android_pr_head_sha'],
                'android_tested_checkout_sha': android['android_checkout_sha'],
                'workflow_sha': android['workflow_sha'],
                'workflow_name': mcp_timeout_evidence['workflow_name'],
                'workflow_id': mcp_timeout_evidence['workflow_id'],
                'workflow_path': mcp_timeout_evidence['workflow_path'],
                'workflow_event': mcp_timeout_evidence['workflow_event'],
                'workflow_repository': mcp_timeout_evidence['workflow_repository'],
                'workflow_run_id': mcp_timeout_evidence['workflow_run_id'],
                'workflow_job_name': mcp_timeout_evidence['workflow_job_name'],
                'workflow_job_id': mcp_timeout_evidence['workflow_job_id'],
                'workflow_job_run_attempt': mcp_timeout_evidence['workflow_job_run_attempt'],
                'run_attempt': mcp_timeout_evidence['run_attempt'],
                'run_conclusion': mcp_timeout_evidence['run_conclusion'],
                'result': 'passed',
            }
            mcp_timeout_responsibility['production_evidence'].append({
                **common,
                'kind': 'production-wiring',
                'workflow_step_name': mcp_timeout_evidence['production_step_name'],
                'workflow_step_number': mcp_timeout_evidence['production_step_number'],
                'evidence_ref': production_target + '#desktop-mcp-one-hour-timeout-production-owner',
            })
            mcp_timeout_responsibility['test_evidence'].append({
                **common,
                'kind': 'focused-unit-test',
                'workflow_step_name': mcp_timeout_evidence['test_step_name'],
                'workflow_step_number': mcp_timeout_evidence['test_step_number'],
                'evidence_ref': test_target + '#desktop-mcp-one-hour-timeout-focused-tests',
                'focused_tests': list(MCP_TOOL_TIMEOUT_SOURCE_SPEC['focused_tests']),
            })
        else:
            print(
                'WARNING: no successful same-exact-head MCP tool timeout evidence; '
                'MCP tool timeout package coverage remains incomplete',
                file=sys.stderr,
            )

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
                    'workflow_id': ci_evidence['workflow_id'],
                    'workflow_path': ci_evidence['workflow_path'],
                    'workflow_event': ci_evidence['workflow_event'],
                    'workflow_repository': ci_evidence['workflow_repository'],
                    'workflow_run_id': ci_evidence['workflow_run_id'],
                    'workflow_job_name': ci_evidence['workflow_job_name'],
                    'workflow_job_id': ci_evidence['workflow_job_id'],
                    'workflow_job_run_attempt': ci_evidence['workflow_job_run_attempt'],
                    'run_attempt': ci_evidence['run_attempt'],
                    'run_conclusion': ci_evidence['run_conclusion'],
                    'result': 'passed',
                }
                agent_kv_responsibility['production_evidence'].append({
                    **common,
                    'kind': 'production-wiring',
                    'workflow_step_name': ci_evidence['production_step_name'],
                    'workflow_step_number': ci_evidence['production_step_number'],
                    'evidence_ref': production_target + '#agent-kv-production-owner',
                })
                agent_kv_responsibility['test_evidence'].append({
                    **common,
                    'kind': 'focused-unit-test',
                    'workflow_step_name': ci_evidence['test_step_name'],
                    'workflow_step_number': ci_evidence['test_step_number'],
                    'evidence_ref': test_target + '#agent-kv-focused-tests',
                    'focused_tests': list(spec['focused_tests']),
                })
        else:
            print(
                'WARNING: no successful same-exact-head dedicated agent-kv workflow/job/step evidence; '
                'agent-kv coverage remains incomplete',
                file=sys.stderr,
            )

    if CLOUD_AGENT_CONSTANT_SOURCE_PATH in new_by_path and any(
        isinstance(source, dict) and source.get('path') == CLOUD_AGENT_CONSTANT_SOURCE_PATH
        for item in responsibilities
        for source in (item.get('desktop_sources') or [])
    ):
        cloud_agent_constant_responsibility = next(
            item for item in responsibilities
            if item.get('responsibility_id') == CLOUD_AGENT_CONSTANT_RESPONSIBILITY_ID
        )
        for evidence_key in ('production_evidence', 'test_evidence'):
            existing_evidence = cloud_agent_constant_responsibility.get(evidence_key)
            if not isinstance(existing_evidence, list):
                existing_evidence = []
            cloud_agent_constant_responsibility[evidence_key] = [
                evidence for evidence in existing_evidence
                if CLOUD_AGENT_CONSTANT_SOURCE_PATH not in (
                    evidence.get('source_paths')
                    if isinstance(evidence.get('source_paths'), list)
                    else [evidence.get('source_path')]
                )
            ]
        cloud_agent_constant_evidence = _latest_exact_head_android_ci_evidence(
            android['android_pr_head_sha'],
            production_step_name=CLOUD_AGENT_CONSTANT_PRODUCTION_STEP,
            test_step_name=CLOUD_AGENT_CONSTANT_TEST_STEP,
            evidence_label='cloud-agent artifact prompt',
        )
        if cloud_agent_constant_evidence is not None:
            production_target = CLOUD_AGENT_CONSTANT_SOURCE_SPEC['production_target']
            test_target = CLOUD_AGENT_CONSTANT_SOURCE_SPEC['test_target']
            require(
                production_target in tracked_android_paths and test_target in tracked_android_paths,
                'cloud-agent artifact prompt evidence target is not tracked at exact HEAD',
            )
            common = {
                'source_paths': [CLOUD_AGENT_CONSTANT_SOURCE_PATH],
                'source_commit': current,
                'desktop_commit': current,
                'desktop_root_tree': tree_sha,
                'android_pr_head_sha': android['android_pr_head_sha'],
                'android_tested_checkout_sha': android['android_checkout_sha'],
                'workflow_sha': android['workflow_sha'],
                'workflow_name': cloud_agent_constant_evidence['workflow_name'],
                'workflow_id': cloud_agent_constant_evidence['workflow_id'],
                'workflow_path': cloud_agent_constant_evidence['workflow_path'],
                'workflow_event': cloud_agent_constant_evidence['workflow_event'],
                'workflow_repository': cloud_agent_constant_evidence['workflow_repository'],
                'workflow_run_id': cloud_agent_constant_evidence['workflow_run_id'],
                'workflow_job_name': cloud_agent_constant_evidence['workflow_job_name'],
                'workflow_job_id': cloud_agent_constant_evidence['workflow_job_id'],
                'workflow_job_run_attempt': cloud_agent_constant_evidence['workflow_job_run_attempt'],
                'run_attempt': cloud_agent_constant_evidence['run_attempt'],
                'run_conclusion': cloud_agent_constant_evidence['run_conclusion'],
                'result': 'passed',
            }
            cloud_agent_constant_responsibility['production_evidence'].append({
                **common,
                'kind': 'production-wiring',
                'workflow_step_name': cloud_agent_constant_evidence['production_step_name'],
                'workflow_step_number': cloud_agent_constant_evidence['production_step_number'],
                'evidence_ref': production_target + '#cloud-agent-artifact-prompt-production-owner',
            })
            cloud_agent_constant_responsibility['test_evidence'].append({
                **common,
                'kind': 'focused-unit-test',
                'workflow_step_name': cloud_agent_constant_evidence['test_step_name'],
                'workflow_step_number': cloud_agent_constant_evidence['test_step_number'],
                'evidence_ref': test_target + '#cloud-agent-artifact-prompt-focused-tests',
                'focused_tests': list(CLOUD_AGENT_CONSTANT_SOURCE_SPEC['focused_tests']),
            })
        else:
            print(
                'WARNING: no successful same-exact-head cloud-agent artifact prompt evidence; '
                'cloud-agent artifact prompt package coverage remains incomplete',
                file=sys.stderr,
            )



    if COMPOSER_CONSTANT_SOURCE_PATH in new_by_path:
        composer_responsibility = next(
            item for item in responsibilities
            if item.get('responsibility_id') == COMPOSER_CONSTANT_RESPONSIBILITY_ID
        )
        for evidence_key in ('production_evidence', 'test_evidence'):
            existing_evidence = composer_responsibility.get(evidence_key)
            if not isinstance(existing_evidence, list):
                existing_evidence = []
            composer_responsibility[evidence_key] = [
                evidence for evidence in existing_evidence
                if COMPOSER_CONSTANT_SOURCE_PATH not in (
                    evidence.get('source_paths')
                    if isinstance(evidence.get('source_paths'), list)
                    else [evidence.get('source_path')]
                )
            ]
        composer_evidence = _latest_exact_head_android_ci_evidence(
            android['android_pr_head_sha'],
            production_step_name=COMPOSER_CONSTANT_PRODUCTION_STEP,
            test_step_name=COMPOSER_CONSTANT_TEST_STEP,
            evidence_label='constants composer',
        )
        if composer_evidence is not None:
            production_target = COMPOSER_CONSTANT_SOURCE_SPEC['production_target']
            test_target = COMPOSER_CONSTANT_SOURCE_SPEC['test_target']
            require(
                production_target in tracked_android_paths and test_target in tracked_android_paths,
                'Composer constant evidence target is not tracked at exact HEAD',
            )
            common = {
                'source_paths': [COMPOSER_CONSTANT_SOURCE_PATH],
                'source_commit': current,
                'desktop_commit': current,
                'desktop_root_tree': tree_sha,
                'android_pr_head_sha': android['android_pr_head_sha'],
                'android_tested_checkout_sha': android['android_checkout_sha'],
                'workflow_sha': android['workflow_sha'],
                'workflow_name': composer_evidence['workflow_name'],
                'workflow_id': composer_evidence['workflow_id'],
                'workflow_path': composer_evidence['workflow_path'],
                'workflow_event': composer_evidence['workflow_event'],
                'workflow_repository': composer_evidence['workflow_repository'],
                'workflow_run_id': composer_evidence['workflow_run_id'],
                'workflow_job_name': composer_evidence['workflow_job_name'],
                'workflow_job_id': composer_evidence['workflow_job_id'],
                'workflow_job_run_attempt': composer_evidence['workflow_job_run_attempt'],
                'run_attempt': composer_evidence['run_attempt'],
                'run_conclusion': composer_evidence['run_conclusion'],
                'result': 'passed',
            }
            composer_responsibility['production_evidence'].append({
                **common,
                'kind': 'production-wiring',
                'workflow_step_name': composer_evidence['production_step_name'],
                'workflow_step_number': composer_evidence['production_step_number'],
                'evidence_ref': production_target + '#composer-max-text-size-production-owner',
            })
            composer_responsibility['test_evidence'].append({
                **common,
                'kind': 'focused-unit-test',
                'workflow_step_name': composer_evidence['test_step_name'],
                'workflow_step_number': composer_evidence['test_step_number'],
                'evidence_ref': test_target + '#composer-max-text-size-focused-tests',
                'focused_tests': list(COMPOSER_CONSTANT_SOURCE_SPEC['focused_tests']),
            })
        else:
            print(
                'WARNING: no successful same-exact-head constants composer evidence; '
                'Composer constant package coverage remains incomplete',
                file=sys.stderr,
            )


    if all(path in new_by_path for path in AGENT_CORE_INTERACTION_SOURCE_SPECS):
        interaction_responsibility = next(
            item for item in responsibilities
            if item.get('responsibility_id') == AGENT_CORE_INTERACTION_RESPONSIBILITY_ID
        )
        interaction_paths = set(AGENT_CORE_INTERACTION_SOURCE_SPECS)
        for evidence_key in ('production_evidence', 'test_evidence'):
            existing_evidence = interaction_responsibility.get(evidence_key)
            if not isinstance(existing_evidence, list):
                existing_evidence = []
            interaction_responsibility[evidence_key] = [
                entry for entry in existing_evidence
                if not any(path in interaction_paths for path in (
                    entry.get('source_paths')
                    if isinstance(entry.get('source_paths'), list)
                    else [entry.get('source_path')]
                ))
            ]
        interaction_evidence = _latest_exact_head_android_ci_evidence(
            android['android_pr_head_sha'],
            production_step_name=AGENT_CORE_INTERACTION_PRODUCTION_STEP,
            test_step_name=AGENT_CORE_INTERACTION_TEST_STEP,
            evidence_label='agent-core interaction',
        )
        if interaction_evidence is not None:
            for path, spec in AGENT_CORE_INTERACTION_SOURCE_SPECS.items():
                missing_targets = [target for target in spec['android_targets'] if target not in tracked_android_paths]
                require(
                    not missing_targets,
                    'agent-core interaction Android target is not tracked at exact HEAD for '
                    + path + ': ' + ', '.join(missing_targets),
                )
                common = {
                    'source_paths': [path],
                    'source_commit': current,
                    'desktop_commit': current,
                    'desktop_root_tree': tree_sha,
                    'android_pr_head_sha': android['android_pr_head_sha'],
                    'android_tested_checkout_sha': android['android_checkout_sha'],
                    'workflow_sha': android['workflow_sha'],
                    'workflow_name': interaction_evidence['workflow_name'],
                    'workflow_id': interaction_evidence['workflow_id'],
                    'workflow_path': interaction_evidence['workflow_path'],
                    'workflow_event': interaction_evidence['workflow_event'],
                    'workflow_repository': interaction_evidence['workflow_repository'],
                    'workflow_run_id': interaction_evidence['workflow_run_id'],
                    'workflow_job_name': interaction_evidence['workflow_job_name'],
                    'workflow_job_id': interaction_evidence['workflow_job_id'],
                    'workflow_job_run_attempt': interaction_evidence['workflow_job_run_attempt'],
                    'run_attempt': interaction_evidence['run_attempt'],
                    'run_conclusion': interaction_evidence['run_conclusion'],
                    'result': 'passed',
                }
                interaction_responsibility['production_evidence'].append({
                    **common,
                    'kind': 'production-wiring',
                    'workflow_step_name': interaction_evidence['production_step_name'],
                    'workflow_step_number': interaction_evidence['production_step_number'],
                    'evidence_ref': spec['production_target'] + '#agent-core-interaction-production-owner',
                })
                interaction_responsibility['test_evidence'].append({
                    **common,
                    'kind': 'focused-unit-test',
                    'workflow_step_name': interaction_evidence['test_step_name'],
                    'workflow_step_number': interaction_evidence['test_step_number'],
                    'evidence_ref': spec['test_target'] + '#agent-core-interaction-focused-tests',
                    'focused_tests': list(spec['focused_tests']),
                })
        else:
            print(
                'WARNING: no successful same-exact-head agent-core interaction evidence; '
                'interaction package coverage remains incomplete',
                file=sys.stderr,
            )


    if all(path in new_by_path for path in BACKGROUND_COMPLETION_SOURCE_SPECS):
        background_responsibility = next(
            item for item in responsibilities
            if item.get('responsibility_id') == BACKGROUND_COMPLETION_RESPONSIBILITY_ID
        )
        background_paths = set(BACKGROUND_COMPLETION_SOURCE_SPECS)
        for evidence_key in ('production_evidence', 'test_evidence'):
            existing_evidence = background_responsibility.get(evidence_key)
            if not isinstance(existing_evidence, list):
                existing_evidence = []
            background_responsibility[evidence_key] = [
                entry for entry in existing_evidence
                if not any(path in background_paths for path in (
                    entry.get('source_paths')
                    if isinstance(entry.get('source_paths'), list)
                    else [entry.get('source_path')]
                ))
            ]
        background_evidence = _latest_exact_head_android_ci_evidence(
            android['android_pr_head_sha'],
            production_step_name=BACKGROUND_COMPLETION_PRODUCTION_STEP,
            test_step_name=BACKGROUND_COMPLETION_TEST_STEP,
            evidence_label='background completion',
        )
        if background_evidence is not None:
            for path, spec in BACKGROUND_COMPLETION_SOURCE_SPECS.items():
                production_target = spec['production_target']
                test_target = spec['test_target']
                missing_targets = [
                    target for target in spec['android_targets']
                    if target not in tracked_android_paths
                ]
                require(
                    not missing_targets,
                    'background-completion Android target is not tracked at exact HEAD for '
                    + path + ': ' + ', '.join(missing_targets),
                )
                common = {
                    'source_paths': [path],
                    'source_commit': current,
                    'desktop_commit': current,
                    'desktop_root_tree': tree_sha,
                    'android_pr_head_sha': android['android_pr_head_sha'],
                    'android_tested_checkout_sha': android['android_checkout_sha'],
                    'workflow_sha': android['workflow_sha'],
                    'workflow_name': background_evidence['workflow_name'],
                    'workflow_id': background_evidence['workflow_id'],
                    'workflow_path': background_evidence['workflow_path'],
                    'workflow_event': background_evidence['workflow_event'],
                    'workflow_repository': background_evidence['workflow_repository'],
                    'workflow_run_id': background_evidence['workflow_run_id'],
                    'workflow_job_name': background_evidence['workflow_job_name'],
                    'workflow_job_id': background_evidence['workflow_job_id'],
                    'workflow_job_run_attempt': background_evidence['workflow_job_run_attempt'],
                    'run_attempt': background_evidence['run_attempt'],
                    'run_conclusion': background_evidence['run_conclusion'],
                    'result': 'passed',
                }
                background_responsibility['production_evidence'].append({
                    **common,
                    'kind': 'production-wiring',
                    'workflow_step_name': background_evidence['production_step_name'],
                    'workflow_step_number': background_evidence['production_step_number'],
                    'evidence_ref': production_target + '#background-completion-production-owner',
                })
                background_responsibility['test_evidence'].append({
                    **common,
                    'kind': 'contract-test',
                    'workflow_step_name': background_evidence['test_step_name'],
                    'workflow_step_number': background_evidence['test_step_number'],
                    'evidence_ref': test_target + '#background-completion-focused-tests',
                    'focused_tests': list(spec['focused_tests']),
                })
        else:
            print(
                'WARNING: no successful same-exact-head background completion evidence; '
                'background completion package coverage remains incomplete',
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
        elif path in BACKGROUND_COMPLETION_SOURCE_SPECS:
            row['disposition'] = BACKGROUND_COMPLETION_SOURCE_SPECS[path]['disposition']
        elif path in AGENT_CORE_INTERACTION_SOURCE_SPECS:
            row['disposition'] = AGENT_CORE_INTERACTION_SOURCE_SPECS[path]['disposition']
        elif path == SAND_BOX_ARCHIVE_SOURCE_PATH:
            row['disposition'] = SAND_BOX_ARCHIVE_SOURCE_SPEC['disposition']
        elif path == CLOUD_AGENT_CONSTANT_SOURCE_PATH:
            row['disposition'] = CLOUD_AGENT_CONSTANT_SOURCE_SPEC['disposition']
        elif path == COMPOSER_CONSTANT_SOURCE_PATH:
            row['disposition'] = COMPOSER_CONSTANT_SOURCE_SPEC['disposition']
        elif path == STRUCTURED_LOG_CONSTANT_SOURCE_PATH:
            row['disposition'] = STRUCTURED_LOG_CONSTANT_SOURCE_SPEC['disposition']
        elif path == MCP_TOOL_TIMEOUT_SOURCE_PATH:
            row['disposition'] = MCP_TOOL_TIMEOUT_SOURCE_SPEC['disposition']
        elif path == AUTOMATION_SCHEDULE_SOURCE_PATH:
            row['disposition'] = AUTOMATION_SCHEDULE_SOURCE_SPEC['disposition']
        elif path in CLIENT_SIDE_TOOL_V2_SOURCE_SPECS:
            row['disposition'] = CLIENT_SIDE_TOOL_V2_SOURCE_SPECS[path]['disposition']
        elif path == APP_AGENT_SURFACE_E2E_SOURCE_PATH:
            row['disposition'] = APP_AGENT_SURFACE_E2E_SOURCE_SPEC['disposition']
        elif path in OFFICIAL_DEVICE_SOURCE_SPECS:
            row['disposition'] = OFFICIAL_DEVICE_SOURCE_SPECS[path]['disposition']
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
    generated_outputs = {
        'packages-semantic-coverage.json': package_coverage,
        'rejected-android-targets.json': {
            'status': 'incomplete' if rejected_android_targets else 'passed',
            'count': len(rejected_android_targets),
            'rejected_targets': sorted(
                rejected_android_targets,
                key=lambda item: (item['responsibility_id'], item['target'], item['reason']),
            ),
            'policy': 'Rejected targets are retained for diagnosis and never count as Android parity evidence.',
        },
        'unmapped-source-rows.json': {
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
        },
        'responsibility-mapping-summary.json': {
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
        },
    }
    if deferred_outputs is not None:
        deferred_outputs.update(json.loads(json.dumps(generated_outputs)))
    if persist_outputs:
        for filename, document in generated_outputs.items():
            save(OUT / filename, document)

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
    current = EXPECTED_DESKTOP_COMMIT
    require(SHA.fullmatch(current) is not None, 'Invalid pinned Desktop authority SHA')
    commit = get(f'/repos/{repo}/git/commits/{current}')
    require(commit.get('sha') == current, 'Pinned Desktop authority commit lookup mismatch')
    tree_sha = commit['tree']['sha']
    require(SHA.fullmatch(tree_sha) is not None, 'Invalid Desktop root tree SHA')
    entries, fallback = complete_tree(repo, tree_sha)
    files = [item for item in entries if item['type'] != 'tree']
    require(files, 'Empty source inventory during rebaseline')
    require(current == EXPECTED_DESKTOP_COMMIT and tree_sha == EXPECTED_DESKTOP_TREE,
            'Desktop authority commit/tree differs from the pinned current authority lock')
    require(len(entries) == EXPECTED_TREE_ENTRY_COUNT,
            f'current Desktop recursive tree count mismatch: expected {EXPECTED_TREE_ENTRY_COUNT}, got {len(entries)}')
    require(len(files) == EXPECTED_TRACKED_NON_TREE_COUNT,
            f'current Desktop non-tree count mismatch: expected {EXPECTED_TRACKED_NON_TREE_COUNT}, got {len(files)}')
    require(manifest_path.is_file() and ledger_path.is_file(),
            'persisted source manifest and ledger are required for a guarded rebaseline')
    prior_manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    prior_ledger_text = ledger_path.read_text(encoding='utf-8')
    predecessor_source_rows = None
    if (
        prior_manifest.get('desktop_commit') == PREDECESSOR_DESKTOP_COMMIT
        and prior_manifest.get('desktop_root_tree') == PREDECESSOR_DESKTOP_TREE
    ):
        _, predecessor_source_rows, _ = _load_pinned_responsibility_mapping()
    old_rows = _validate_rebaseline_boundary(
        baseline, prior_manifest, prior_ledger_text, current, tree_sha, len(entries), len(files),
        predecessor_source_rows=predecessor_source_rows,
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
    prior_reverse_links: dict[str, set[str]] = {}
    prior_anchor_count = 0
    for responsibility in prior_responsibilities:
        rid = responsibility['responsibility_id']
        for source in responsibility.get('desktop_sources') or []:
            if not _is_semantic_source_anchor(rid, source):
                continue
            path = source.get('path')
            require(path in old_by_path,
                    'persisted responsibility source anchor is absent from persisted source ledger: ' + str(path))
            row = old_by_path[path]
            require(source.get('blob_sha') == row.get('source_blob_sha')
                    and source.get('source_mode') == row.get('source_mode')
                    and source.get('source_type') == row.get('source_type')
                    and source.get('source_commit') == prior_manifest.get('desktop_commit'),
                    'persisted responsibility source anchor object differs from persisted source ledger: ' + str(path))
            prior_reverse_links.setdefault(path, set()).add(rid)
            prior_anchor_count += 1
    if prior_manifest.get('semantic_source_anchor_entries') is not None:
        require(prior_anchor_count == prior_manifest.get('semantic_source_anchor_entries'),
                'persisted responsibility source-anchor count differs from source manifest')
    for path, row in old_by_path.items():
        linked = row.get('responsibilities') or []
        require(set(linked) == prior_reverse_links.get(path, set()),
                'persisted source ledger responsibility links disagree with responsibility source anchors: ' + path)
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
        'desktop_main_before': prior_manifest.get('desktop_commit'),
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
    baseline['desktop_main_before'] = PREDECESSOR_DESKTOP_COMMIT
    baseline['desktop_main_after'] = current
    require(responsibility_path.is_file(), 'Persistent responsibility ledger is missing')

    touched_responsibilities = 0
    deleted_anchor_count = 0
    if responsibility_path.is_file():
        mapping_outputs: dict[str, dict] = {}
        responsibility_doc = merge_responsibility_mapping(
            responsibility_path,
            new_rows,
            new_by_path,
            current,
            tree_sha,
            expected_responsibility_count=EXPECTED_RESPONSIBILITY_COUNT,
            persist_outputs=False,
            deferred_outputs=mapping_outputs,
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
            row['responsibilities'] = sorted(reverse_links.get(row['source_path'], set()))
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
        require(SHA.fullmatch(latest_desktop_main) is not None,
                'Invalid observed live Desktop main SHA during rebaseline')
        ledger_path.write_text(ledger_text, encoding='utf-8')
        save(responsibility_path, responsibility_doc)
        save(manifest_path, manifest)
        save(baseline_path, baseline)
        for filename, document in mapping_outputs.items():
            save(OUT / filename, document)

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
    live_before = get(f'/repos/{repo}/branches/main')['commit']['sha']
    require(SHA.fullmatch(live_before) is not None, 'Invalid observed live Desktop main SHA')
    require(live_before == sha, 'Desktop main does not match the pinned current authority before inventory')
    before = live_before
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
    live_after = get(f'/repos/{repo}/branches/main')['commit']['sha']
    require(SHA.fullmatch(live_after) is not None, 'Invalid observed live Desktop main SHA')
    after = live_after
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
            f'Persistent source inventory manifest must preserve all {EXPECTED_TREE_ENTRY_COUNT} recursive tree entries')
    require(authority_manifest.get('tracked_non_tree_entries') == EXPECTED_TRACKED_NON_TREE_COUNT == len(files),
            f'Persistent source inventory manifest must preserve all {EXPECTED_TRACKED_NON_TREE_COUNT} tracked non-tree entries')
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
    _, _, predecessor_validation = _load_pinned_responsibility_mapping()
    actual_digest = predecessor_validation['artifact_digest']
    require(actual_digest == expected_digest,
            'predecessor artifact SHA-256 mismatch: expected ' + expected_digest + ', got ' + actual_digest)

    tree_path = OUT / 'desktop-complete-tree.json'
    root_tree_path = OUT / 'desktop-root-tree.json'
    inputs = {
        'baseline': DOC / 'authority/baseline.json',
        'source_inventory_manifest': DOC / 'authority/source-inventory-manifest.json',
        'source_inventory_ledger': DOC / 'authority/source-inventory-ledger.jsonl',
        'responsibility_ledger': DOC / 'authority/responsibility-ledger.json',
        'package_semantic_coverage': OUT / 'packages-semantic-coverage.json',
        'desktop_complete_tree': tree_path,
        'desktop_root_tree': root_tree_path,
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
            'receipt manifest does not preserve the current authority boundary')
    tree_doc = docs['desktop_complete_tree']
    root_tree_doc = docs['desktop_root_tree']
    require(tree_doc.get('sha') == EXPECTED_DESKTOP_TREE
            and tree_doc.get('sha') == manifest.get('desktop_root_tree')
            and tree_doc.get('truncated') is False,
            'receipt complete-tree evidence is truncated or does not match the current Desktop root tree')
    tree_entries = tree_doc.get('tree')
    require(isinstance(tree_entries, list) and len(tree_entries) == EXPECTED_TREE_ENTRY_COUNT,
            'receipt complete-tree evidence has the wrong recursive entry count')
    require(root_tree_doc.get('sha') == EXPECTED_DESKTOP_TREE
            and root_tree_doc.get('truncated') is False,
            'receipt root-tree evidence is truncated or does not match the current Desktop root tree')

    def tree_signature(entries: list[dict], label: str) -> list[tuple[str, str, str, str]]:
        require(isinstance(entries, list), label + ' must be a list')
        paths, signature = [], []
        for item in entries:
            require(isinstance(item, dict), label + ' contains a non-object entry')
            path = item.get('path')
            require(isinstance(path, str) and path and not PurePosixPath(path).is_absolute()
                    and '..' not in PurePosixPath(path).parts,
                    label + ' contains an unsafe or missing path')
            require(SHA.fullmatch(str(item.get('sha') or '')) is not None,
                    label + ' contains an invalid object SHA: ' + path)
            require(item.get('mode') in {'040000', '100644', '100755', '120000', '160000'}
                    and item.get('type') in {'tree', 'blob', 'commit'},
                    label + ' contains an invalid mode/type: ' + path)
            paths.append(path)
            signature.append((path, item['sha'], item['mode'], item['type']))
        require(len(paths) == len(set(paths)), label + ' contains duplicate paths')
        return sorted(signature)

    desktop_repo = baseline['desktop_repository']
    live_main_before = get(f'/repos/{desktop_repo}/branches/main')['commit']['sha']
    require(live_main_before == EXPECTED_DESKTOP_COMMIT,
            'receipt current Desktop main does not match the pinned commit')
    commit_doc = get(f'/repos/{desktop_repo}/git/commits/{EXPECTED_DESKTOP_COMMIT}')
    require(commit_doc.get('sha') == EXPECTED_DESKTOP_COMMIT
            and commit_doc.get('tree', {}).get('sha') == EXPECTED_DESKTOP_TREE,
            'receipt current Desktop commit/tree lock mismatch')
    remote_tree = get(f'/repos/{desktop_repo}/git/trees/{EXPECTED_DESKTOP_TREE}?recursive=1')
    require(remote_tree.get('sha') == EXPECTED_DESKTOP_TREE
            and remote_tree.get('truncated') is False,
            'receipt current Desktop recursive API tree is truncated or has the wrong root identity')
    remote_entries = remote_tree.get('tree')
    require(isinstance(remote_entries, list) and len(remote_entries) == EXPECTED_TREE_ENTRY_COUNT,
            'receipt current Desktop recursive API tree has the wrong entry count')
    require(sum(item.get('type') != 'tree' for item in remote_entries if isinstance(item, dict))
            == EXPECTED_TRACKED_NON_TREE_COUNT,
            'receipt current Desktop recursive API tree has the wrong non-tree count')
    saved_signature = tree_signature(tree_entries, 'saved Desktop complete tree')
    remote_signature = tree_signature(remote_entries, 'current Desktop recursive API tree')
    require(saved_signature == remote_signature,
            'receipt saved complete-tree evidence differs from current GitHub API path/object/mode/type data')
    root_entries = root_tree_doc.get('tree')
    remote_root_entries = [item for item in remote_entries if isinstance(item, dict) and '/' not in item.get('path', '')]
    require(tree_signature(root_entries, 'saved Desktop root tree')
            == tree_signature(remote_root_entries, 'current Desktop root entries'),
            'receipt saved root-tree evidence differs from current GitHub API root entries')
    live_main_after = get(f'/repos/{desktop_repo}/branches/main')['commit']['sha']
    require(live_main_after == EXPECTED_DESKTOP_COMMIT,
            'receipt Desktop main moved during complete-tree verification')
    require(manifest.get('desktop_main_before') in {
                PRIOR_DESKTOP_COMMIT, PERSISTED_INTERMEDIATE_DESKTOP_COMMIT,
                PREDECESSOR_DESKTOP_COMMIT, EXPECTED_DESKTOP_COMMIT
            }
            and manifest.get('desktop_main_after') == EXPECTED_DESKTOP_COMMIT
            and manifest.get('authority_current') is True,
            'receipt manifest does not bind the rebaseline to the current Desktop authority')

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
            f'receipt requires {EXPECTED_TRACKED_NON_TREE_COUNT} unique current source_path values')
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
        'predecessor_artifact': {
            'artifact_id': int(artifact_id),
            'downloaded_bytes_sha256': actual_digest,
            'expected_sha256': expected_digest,
            'zip_integrity_verified': True,
            'entry_count': predecessor_validation['zip_member_count'],
            'validated_tree_entries': predecessor_validation['validated_tree_entries'],
            'validated_source_rows': predecessor_validation['validated_source_rows'],
            'validated_responsibility_ids': predecessor_validation['validated_responsibility_ids'],
            'validated_semantic_source_anchors': predecessor_validation['validated_semantic_source_anchors'],
        },
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