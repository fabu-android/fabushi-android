import hashlib
import importlib.util
import io
import json
import tempfile
import unittest
import zipfile
from email.message import Message
from pathlib import Path
from urllib.error import HTTPError
from unittest.mock import Mock, patch


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "android-port-docs.py"
CURRENT_DESKTOP_COMMIT = "12887d2a44ba450b13ca8390066a39418c52e0c9"
CURRENT_DESKTOP_TREE = "4bc59314edd76a51bd8f0e2d0654935d9e8ffb41"


def load_generator():
    spec = importlib.util.spec_from_file_location("android_port_docs", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def padded_responsibilities(primary):
    return [primary] + [
        {"responsibility_id": f"RESP-{index:03d}", "desktop_sources": []}
        for index in range(2, 307)
    ]


class HttpBytes(io.BytesIO):
    def __init__(self, payload, status=200, headers=None):
        super().__init__(payload)
        self.status = status
        self.headers = headers or Message()

    def getcode(self):
        return self.status

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()
        return False


def valid_zip_bytes():
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("predecessor.json", "{}")
    return output.getvalue()


def pinned_predecessor_zip_bytes(module, responsibilities=2, anchors=1):
    predecessor_commit = "1" * 40
    predecessor_tree = "2" * 40
    current_commit = "3" * 40
    current_tree = "4" * 40
    source_path = "source/example.ts"
    source_sha = "5" * 40
    row = {
        "source_path": source_path,
        "source_commit": predecessor_commit,
        "source_blob_sha": source_sha,
        "source_mode": "100644",
        "source_type": "blob",
        "responsibilities": ["RESP-001"] if anchors else [],
    }
    ledger_text = json.dumps(row, sort_keys=True) + "\n"
    manifest = {
        "authority_current": True,
        "desktop_commit": predecessor_commit,
        "source_commit": predecessor_commit,
        "desktop_root_tree": predecessor_tree,
        "desktop_main_before": predecessor_commit,
        "desktop_main_after": predecessor_commit,
        "tree_entries": 2,
        "tracked_non_tree_entries": 1,
        "authority_ledger_rows": 1,
        "ledger_sha256": hashlib.sha256(ledger_text.encode("utf-8")).hexdigest(),
        "semantic_source_anchor_entries": anchors,
        "semantic_source_anchor_unique_paths": 1 if anchors else 0,
        "workflow_run_id": 9,
        "android_pr_head_sha": "a" * 40,
    }
    baseline = {
        "desktop_repository": "bhrumom/fabushi-desktop",
        "desktop_commit": current_commit,
        "desktop_root_tree": current_tree,
        "rebaseline_note": (
            f"Historical predecessor authority {predecessor_commit}/tree {predecessor_tree} "
            "had 1 non-tree/2 recursive entries."
        ),
    }
    tree = {
        "sha": predecessor_tree,
        "truncated": False,
        "tree": [
            {"path": "source", "sha": "6" * 40, "mode": "040000", "type": "tree"},
            {"path": source_path, "sha": source_sha, "mode": "100644", "type": "blob"},
        ],
    }
    responsibility_rows = []
    for index in range(responsibilities):
        item = {"responsibility_id": f"RESP-{index + 1:03d}", "desktop_sources": []}
        if index == 0 and anchors:
            item["desktop_sources"] = [{
                "path": source_path,
                "blob_sha": source_sha,
                "source_mode": "100644",
                "source_type": "blob",
                "source_commit": predecessor_commit,
                "mapping_quality": "semantic",
            }]
        responsibility_rows.append(item)
    responsibility = {
        "source_commit": predecessor_commit,
        "authority": {"desktop_commit": predecessor_commit, "desktop_root_tree": predecessor_tree},
        "responsibilities": responsibility_rows,
    }
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("docs/android-port/authority/baseline.json", json.dumps(baseline))
        archive.writestr("docs/android-port/authority/source-inventory-manifest.json", json.dumps(manifest))
        archive.writestr("docs/android-port/authority/source-inventory-ledger.jsonl", ledger_text)
        archive.writestr("docs/android-port/authority/responsibility-ledger.json", json.dumps(responsibility))
        archive.writestr("docs/android-port/generated/desktop-recursive-api.json", json.dumps(tree))
        complete_tree = dict(tree)
        complete_tree["tree"] = list(reversed(tree["tree"]))
        archive.writestr("docs/android-port/generated/desktop-complete-tree.json", json.dumps(complete_tree))
    return output.getvalue(), {
        "predecessor_commit": predecessor_commit, "predecessor_tree": predecessor_tree,
        "current_commit": current_commit, "current_tree": current_tree,
        "source_path": source_path, "source_sha": source_sha,
    }


class AndroidPortDocsGeneratorTest(unittest.TestCase):

    def test_background_completion_semantic_mapping_is_narrow_and_source_specific(self):
        module = load_generator()
        responsibility = {
            "responsibility_id": module.BACKGROUND_COMPLETION_RESPONSIBILITY_ID,
            "desktop_sources": [],
            "android_target": [],
            "notes": "",
        }
        responsibilities = padded_responsibilities(responsibility)
        inventory = {
            path: {"sha": str(index + 1) * 40, "mode": "100644", "type": "blob"}
            for index, path in enumerate(module.BACKGROUND_COMPLETION_SOURCE_SPECS)
        }
        module._ensure_background_completion_semantic_mapping(
            responsibilities, inventory, CURRENT_DESKTOP_COMMIT
        )
        mapped = next(
            item for item in responsibilities
            if item["responsibility_id"] == module.BACKGROUND_COMPLETION_RESPONSIBILITY_ID
        )
        mapped_by_path = {source["path"]: source for source in mapped["desktop_sources"]}
        self.assertEqual(
            set(mapped_by_path),
            set(module.BACKGROUND_COMPLETION_SOURCE_SPECS),
        )
        for path, spec in module.BACKGROUND_COMPLETION_SOURCE_SPECS.items():
            source = mapped_by_path[path]
            self.assertEqual(source["mapping_quality"], "semantic")
            self.assertEqual(source["source_owner"], spec["source_owner"])
            self.assertEqual(source["android_owner"], spec["android_owner"])
            self.assertEqual(source["android_targets"], spec["android_targets"])
            self.assertTrue(source["focused_tests"])
        self.assertNotIn(
            "source/packages/agent/actions/goal-continuation-action-handler.ts",
            mapped_by_path,
        )
        self.assertNotIn(
            "source/packages/agent/actions/user-message-action/synthetic-user-message.ts",
            mapped_by_path,
        )
        self.assertIn("goal-continuation", mapped["notes"])
        self.assertIn("agent/state", mapped["notes"])

    def test_agent_kv_semantic_mapping_is_optional_for_generic_inventory_fixtures(self):
        module = load_generator()
        responsibilities = padded_responsibilities({"responsibility_id": "RESP-001", "desktop_sources": []})
        module._ensure_agent_kv_semantic_mapping(
            responsibilities,
            {"source/generated.rs": {"sha": "1" * 40, "mode": "100644", "type": "blob"}},
            CURRENT_DESKTOP_COMMIT,
        )
        self.assertNotIn("AGENT-RUN-LIFECYCLE-001", [r["responsibility_id"] for r in responsibilities])

    def test_merge_without_agent_kv_inventory_does_not_require_agent_kv_responsibility(self):
        module = load_generator()
        with tempfile.TemporaryDirectory() as temp:
            module.OUT = Path(temp)
            predecessor = {
                "authority": {"desktop_commit": CURRENT_DESKTOP_COMMIT, "desktop_root_tree": CURRENT_DESKTOP_TREE},
                "responsibilities": padded_responsibilities({
                    "responsibility_id": "RESP-001",
                    "source_owner": "canonical Host owner",
                    "android_owner": "Rust Host",
                    "android_target": ["source/host/src/lib.rs"],
                    "desktop_sources": [{
                        "path": "source/host/src/lib.rs",
                        "blob_sha": "old",
                        "symbols": ["Host::dispatch"],
                        "source_mode": "100644",
                        "source_type": "blob",
                    }],
                }),
            }
            android_head = "a" * 40
            provenance = {
                "android_repository": "fabu-android/fabushi-android",
                "android_pr": 3,
                "android_pr_head_sha": android_head,
                "android_pr_head_ref": "spec/grok-bot-018-android-parity-20260922",
                "android_pr_base_sha": "b" * 40,
                "android_pr_base_ref": "main",
                "android_checkout_sha": android_head,
                "workflow_event": "push",
                "workflow_run_id": 456,
                "run_attempt": 1,
                "workflow_sha": android_head,
                "workflow_ref": "ref",
                "run_url": "https://example.invalid/run/456",
                "generated_at_utc": "2026-10-17T00:00:00Z",
            }
            source_path = "source/host/src/lib.rs"
            blob = "1" * 40
            rows = [{
                "source_path": source_path,
                "source_commit": CURRENT_DESKTOP_COMMIT,
                "source_blob_sha": blob,
                "source_type": "blob",
                "source_mode": "100644",
                "disposition": "direct-port",
                "implementation_status": "unreviewed",
                "evidence": [],
            }]
            inventory = {source_path: {"path": source_path, "sha": blob, "type": "blob", "mode": "100644"}}
            with patch.object(
                module, "load_predecessor_responsibility_mapping",
                return_value=(predecessor, {}, "validated-fixture", {
                    "artifact_id": 12072277149, "artifact_digest": "sha256:" + "f" * 64,
                    "source_manifest_run_id": 456, "source_manifest_android_head": android_head,
                    "source_manifest_desktop_commit": CURRENT_DESKTOP_COMMIT,
                    "source_manifest_desktop_root_tree": CURRENT_DESKTOP_TREE,
                }),
            ), patch.object(module, "current_android_provenance", return_value=provenance):
                result = module.merge_responsibility_mapping(
                    Path("responsibility-ledger.json"), rows, inventory,
                    CURRENT_DESKTOP_COMMIT, CURRENT_DESKTOP_TREE,
                )
            self.assertEqual(result["authority"]["responsibility_count"], 306)
            self.assertEqual(rows[0]["responsibilities"], ["RESP-001"])
            self.assertEqual(rows[0]["implementation_status"], "unreviewed")
            self.assertEqual(rows[0]["evidence"][0]["status"], "provenance-only")

    def test_agent_kv_inventory_requires_all_seven_current_desktop_sources(self):
        module = load_generator()
        responsibilities = padded_responsibilities({"responsibility_id": "AGENT-RUN-LIFECYCLE-001", "desktop_sources": []})
        with self.assertRaisesRegex(ValueError, "inventory is incomplete"):
            module._ensure_agent_kv_semantic_mapping(
                responsibilities,
                {"source/packages/agent-kv/agent-store.ts": {"sha": "1" * 40, "mode": "100644", "type": "blob"}},
                CURRENT_DESKTOP_COMMIT,
            )

    def test_automation_schedule_current_source_maps_without_inheriting_verification(self):
        module = load_generator()
        responsibility = {
            "responsibility_id": module.AUTOMATION_SCHEDULE_RESPONSIBILITY_ID,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [{
                "path": module.AUTOMATION_SCHEDULE_SOURCE_PATH,
                "blob_sha": "0" * 40,
                "source_commit": "0" * 40,
            }],
            "android_target": [],
        }
        responsibilities = padded_responsibilities(responsibility)
        inventory = {
            module.AUTOMATION_SCHEDULE_SOURCE_PATH: {
                "sha": module.AUTOMATION_SCHEDULE_DESKTOP_BLOB,
                "mode": "100644",
                "type": "blob",
            }
        }
        module._ensure_automation_schedule_semantic_mapping(
            responsibilities, inventory, CURRENT_DESKTOP_COMMIT
        )
        item = next(
            value for value in responsibilities
            if value["responsibility_id"] == module.AUTOMATION_SCHEDULE_RESPONSIBILITY_ID
        )
        source = next(
            value for value in item["desktop_sources"]
            if value["path"] == module.AUTOMATION_SCHEDULE_SOURCE_PATH
        )
        self.assertEqual(source["source_commit"], CURRENT_DESKTOP_COMMIT)
        self.assertEqual(source["blob_sha"], module.AUTOMATION_SCHEDULE_DESKTOP_BLOB)
        self.assertEqual(source["mapping_quality"], "semantic")
        self.assertEqual(
            source["android_targets"], module.AUTOMATION_SCHEDULE_SOURCE_SPEC["android_targets"]
        )
        self.assertEqual(item["implementation_status"], "unreviewed")
        self.assertEqual(item["verification_status"], "not-verified")

    def test_automation_schedule_attaches_fresh_exact_head_evidence_without_promoting_status(self):
        module = load_generator()
        responsibility = {
            "responsibility_id": module.AUTOMATION_SCHEDULE_RESPONSIBILITY_ID,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [],
            "production_evidence": [{
                "source_paths": [module.AUTOMATION_SCHEDULE_SOURCE_PATH],
                "android_pr_head_sha": "0" * 40,
                "result": "passed",
            }],
            "test_evidence": [],
        }
        android_head = "a" * 40
        android = {
            "android_pr_head_sha": android_head,
            "android_checkout_sha": android_head,
            "workflow_sha": android_head,
        }
        spec = module.AUTOMATION_SCHEDULE_SOURCE_SPEC
        tracked = set(spec["android_targets"])
        module._latest_exact_head_android_ci_evidence = lambda *args, **kwargs: {
            "workflow_name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "workflow_path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "workflow_event": "pull_request",
            "workflow_repository": module.ANDROID_REPOSITORY,
            "workflow_run_id": 39045053681,
            "workflow_job_name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME,
            "workflow_job_id": 117432007563,
            "workflow_job_run_attempt": 1,
            "run_attempt": 1,
            "run_conclusion": "success",
            "production_step_name": module.AUTOMATION_SCHEDULE_PRODUCTION_STEP,
            "production_step_number": 15,
            "test_step_name": module.AUTOMATION_SCHEDULE_TEST_STEP,
            "test_step_number": 16,
        }
        module._attach_automation_schedule_exact_head_evidence(
            [responsibility],
            {module.AUTOMATION_SCHEDULE_SOURCE_PATH: {
                "sha": module.AUTOMATION_SCHEDULE_DESKTOP_BLOB,
                "mode": "100644",
                "type": "blob",
            }},
            tracked,
            CURRENT_DESKTOP_COMMIT,
            CURRENT_DESKTOP_TREE,
            android,
        )
        self.assertEqual(responsibility["implementation_status"], "unreviewed")
        self.assertEqual(responsibility["verification_status"], "not-verified")
        self.assertEqual(len(responsibility["production_evidence"]), 1)
        self.assertEqual(
            responsibility["production_evidence"][0]["android_pr_head_sha"], android_head
        )
        self.assertEqual(
            responsibility["production_evidence"][0]["source_blob_sha"],
            module.AUTOMATION_SCHEDULE_DESKTOP_BLOB,
        )
        self.assertEqual(
            responsibility["production_evidence"][0]["workflow_step_name"],
            module.AUTOMATION_SCHEDULE_PRODUCTION_STEP,
        )
        self.assertEqual(
            len(responsibility["test_evidence"]), len(spec["test_targets"])
        )
        for evidence in responsibility["test_evidence"]:
            self.assertEqual(evidence["source_paths"], [module.AUTOMATION_SCHEDULE_SOURCE_PATH])
            self.assertEqual(evidence["android_tested_checkout_sha"], android_head)
            self.assertEqual(evidence["workflow_step_name"], module.AUTOMATION_SCHEDULE_TEST_STEP)
            self.assertTrue(evidence["focused_tests"])


    def test_client_side_tool_v2_current_sources_map_without_inheriting_verification(self):
        module = load_generator()
        responsibility = {
            "responsibility_id": module.CLIENT_SIDE_TOOL_V2_RESPONSIBILITY_ID,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [],
            "android_target": [],
        }
        responsibilities = padded_responsibilities(responsibility)
        inventory = {
            path: {"sha": spec["blob_sha"], "mode": "100644", "type": "blob"}
            for path, spec in module.CLIENT_SIDE_TOOL_V2_SOURCE_SPECS.items()
        }
        module._ensure_client_side_tool_v2_semantic_mapping(
            responsibilities, inventory, CURRENT_DESKTOP_COMMIT
        )
        item = next(
            value for value in responsibilities
            if value["responsibility_id"] == module.CLIENT_SIDE_TOOL_V2_RESPONSIBILITY_ID
        )
        by_path = {source["path"]: source for source in item["desktop_sources"]}
        for path, spec in module.CLIENT_SIDE_TOOL_V2_SOURCE_SPECS.items():
            source = by_path[path]
            self.assertEqual(source["source_commit"], CURRENT_DESKTOP_COMMIT)
            self.assertEqual(source["blob_sha"], spec["blob_sha"])
            self.assertEqual(source["mapping_quality"], "semantic")
            self.assertEqual(source["android_targets"], spec["android_targets"])
            self.assertTrue(source["source_owner"])
            self.assertTrue(source["android_owner"])
        self.assertEqual(item["implementation_status"], "unreviewed")
        self.assertEqual(item["verification_status"], "not-verified")

    def test_client_side_tool_v2_attaches_fresh_exact_head_evidence_without_promoting_status(self):
        module = load_generator()
        responsibility = {
            "responsibility_id": module.CLIENT_SIDE_TOOL_V2_RESPONSIBILITY_ID,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [],
            "production_evidence": [],
            "test_evidence": [],
        }
        android_head = "b" * 40
        android = {
            "android_pr_head_sha": android_head,
            "android_checkout_sha": android_head,
            "workflow_sha": android_head,
        }
        inventory = {
            path: {"sha": spec["blob_sha"], "mode": "100644", "type": "blob"}
            for path, spec in module.CLIENT_SIDE_TOOL_V2_SOURCE_SPECS.items()
        }
        tracked = {
            target
            for spec in module.CLIENT_SIDE_TOOL_V2_SOURCE_SPECS.values()
            for target in spec["android_targets"]
        } | set(module.CLIENT_SIDE_TOOL_V2_TEST_TARGETS)
        module._latest_exact_head_android_ci_evidence = lambda *args, **kwargs: {
            "workflow_name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "workflow_path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "workflow_event": "pull_request",
            "workflow_repository": module.ANDROID_REPOSITORY,
            "workflow_run_id": 39046993371,
            "workflow_job_name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME,
            "workflow_job_id": 117437585855,
            "workflow_job_run_attempt": 1,
            "run_attempt": 1,
            "run_conclusion": "success",
            "production_step_name": module.CLIENT_SIDE_TOOL_V2_PRODUCTION_STEP,
            "production_step_number": 19,
            "test_step_name": module.CLIENT_SIDE_TOOL_V2_TEST_STEP,
            "test_step_number": 20,
        }
        module._attach_client_side_tool_v2_exact_head_evidence(
            [responsibility],
            inventory,
            tracked,
            CURRENT_DESKTOP_COMMIT,
            CURRENT_DESKTOP_TREE,
            android,
        )
        self.assertEqual(responsibility["implementation_status"], "unreviewed")
        self.assertEqual(responsibility["verification_status"], "not-verified")
        self.assertEqual(len(responsibility["production_evidence"]), 1)
        self.assertEqual(
            responsibility["production_evidence"][0]["android_pr_head_sha"], android_head
        )
        self.assertEqual(
            responsibility["production_evidence"][0]["workflow_step_name"],
            module.CLIENT_SIDE_TOOL_V2_PRODUCTION_STEP,
        )
        self.assertEqual(
            len(responsibility["test_evidence"]), len(module.CLIENT_SIDE_TOOL_V2_TEST_TARGETS)
        )
        focused = [
            test
            for evidence in responsibility["test_evidence"]
            for test in evidence["focused_tests"]
        ]
        self.assertTrue(any(
            "malformed_wire_version_account_base64_type_and_identity_fail_closed" in test
            for test in focused
        ))
        self.assertTrue(any(
            "duplicate_out_of_order_inconsistent_result_and_stale_epoch_are_rejected" in test
            for test in focused
        ))
        self.assertTrue(any(
            "client_side_tool_v2_replay_survives_process_reopen_and_account_retirement_is_durable" in test
            for test in focused
        ))
        self.assertTrue(any(
            "client_side_tool_v2_wire_ingress_is_reachable_only_through_rust_coordinator" in test
            for test in focused
        ))

    def test_official_device_registration_sources_map_without_claiming_verification(self):
        module = load_generator()
        responsibilities = [
            {"responsibility_id": "ACCOUNT-SESSION-001", "desktop_sources": [], "android_target": []},
            {"responsibility_id": "REMOTE-COMPUTER-001", "desktop_sources": [], "android_target": []},
        ] + [
            {"responsibility_id": f"RESP-{index:03d}", "desktop_sources": []}
            for index in range(3, 307)
        ]
        inventory = {
            path: {
                "sha": (f"{index:040x}")[-40:],
                "mode": "100644",
                "type": "blob",
            }
            for index, path in enumerate(module.OFFICIAL_DEVICE_SOURCE_SPECS, start=1)
        }
        module._ensure_official_device_registration_semantic_mapping(
            responsibilities, inventory, CURRENT_DESKTOP_COMMIT
        )
        by_id = {item["responsibility_id"]: item for item in responsibilities}
        self.assertEqual(len(responsibilities), 306)
        for path, spec in module.OFFICIAL_DEVICE_SOURCE_SPECS.items():
            source = next(
                entry
                for entry in by_id[spec["responsibility_id"]]["desktop_sources"]
                if entry["path"] == path
            )
            self.assertEqual(source["source_commit"], CURRENT_DESKTOP_COMMIT)
            self.assertEqual(source["blob_sha"], inventory[path]["sha"])
            self.assertEqual(source["mapping_quality"], "semantic")
            self.assertEqual(source["android_targets"], spec["android_targets"])
            self.assertTrue(source["source_owner"])
            self.assertTrue(source["android_owner"])

    def test_official_device_registration_attaches_path_scoped_exact_head_evidence_without_promoting_status(self):
        module = load_generator()
        source_path = module.OFFICIAL_DEVICE_EVIDENCE_SOURCE_PATH
        spec = module.OFFICIAL_DEVICE_SOURCE_SPECS[source_path]
        responsibility = {
            "responsibility_id": spec["responsibility_id"],
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [],
            "production_evidence": [],
            "test_evidence": [],
        }
        android_head = "a" * 40
        android = {
            "android_pr_head_sha": android_head,
            "android_checkout_sha": android_head,
            "workflow_sha": android_head,
        }
        tracked = {spec["production_target"], *spec["test_targets"].keys()}
        module._latest_exact_head_android_ci_evidence = lambda *args, **kwargs: {
            "workflow_name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "workflow_path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "workflow_event": "pull_request",
            "workflow_repository": module.ANDROID_REPOSITORY,
            "workflow_run_id": 123,
            "workflow_job_name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME,
            "workflow_job_id": 456,
            "workflow_job_run_attempt": 1,
            "run_attempt": 1,
            "run_conclusion": "success",
            "production_step_name": module.OFFICIAL_DEVICE_PRODUCTION_STEP,
            "production_step_number": 13,
            "test_step_name": module.OFFICIAL_DEVICE_TEST_STEP,
            "test_step_number": 14,
        }
        module._attach_official_device_registration_exact_head_evidence(
            [responsibility],
            {source_path: {"sha": "1" * 40, "mode": "100644", "type": "blob"}},
            tracked,
            CURRENT_DESKTOP_COMMIT,
            CURRENT_DESKTOP_TREE,
            android,
        )
        self.assertEqual(responsibility["implementation_status"], "unreviewed")
        self.assertEqual(responsibility["verification_status"], "not-verified")
        self.assertEqual(len(responsibility["production_evidence"]), 1)
        self.assertEqual(len(responsibility["test_evidence"]), len(spec["test_targets"]))
        production = responsibility["production_evidence"][0]
        self.assertEqual(production["source_paths"], [source_path])
        self.assertEqual(production["android_pr_head_sha"], android_head)
        self.assertEqual(production["workflow_step_name"], module.OFFICIAL_DEVICE_PRODUCTION_STEP)
        self.assertIn(production["evidence_ref"].split("#", 1)[0], tracked)
        for evidence in responsibility["test_evidence"]:
            self.assertEqual(evidence["source_paths"], [source_path])
            self.assertEqual(evidence["android_tested_checkout_sha"], android_head)
            self.assertEqual(evidence["workflow_step_name"], module.OFFICIAL_DEVICE_TEST_STEP)
            self.assertIn(evidence["evidence_ref"].split("#", 1)[0], tracked)
            self.assertTrue(evidence["focused_tests"])

    def test_legacy_top_level_fallback_is_not_a_semantic_anchor(self):
        module = load_generator()
        fallback = {
            "responsibility_id": "RUST-ANDROID-RUNTIME-GATE-001",
            "path": "source/packages/example.ts",
        }
        explicit = {
            "responsibility_id": "RUST-ANDROID-RUNTIME-GATE-001",
            "path": "source/host/src/lib.rs",
            "source_mode": "100644",
            "source_type": "blob",
        }
        authored = {
            "responsibility_id": "CUSTOM-RESPONSIBILITY-001",
            "path": "source/packages/example.ts",
        }
        self.assertFalse(module._is_semantic_source_anchor(fallback["responsibility_id"], fallback))
        self.assertTrue(module._is_semantic_source_anchor(explicit["responsibility_id"], explicit))
        self.assertTrue(module._is_semantic_source_anchor(authored["responsibility_id"], authored))


    def test_exact_head_provenance_requires_manifest_ledger_and_responsibility_agreement(self):
        module = load_generator()
        android_head = "a" * 40
        source_path = "source/host/src/lib.rs"
        source_blob = "1" * 40
        row = {
            "source_path": source_path,
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "source_blob_sha": source_blob,
            "source_type": "blob",
            "source_mode": "100644",
            "android_pr_head_sha": android_head,
            "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "evidence": [{
                "source_path": source_path,
                "source_blob_sha": source_blob,
                "desktop_commit": CURRENT_DESKTOP_COMMIT,
                "desktop_root_tree": CURRENT_DESKTOP_TREE,
                "source_commit": CURRENT_DESKTOP_COMMIT,
                "android_head": android_head,
                "android_pr_head_sha": android_head,
                "android_tested_checkout_sha": android_head,
                "workflow_sha": android_head,
                "workflow_run_id": 456,
            }],
        }
        row["responsibilities"] = ["RESP-001"]
        manifest = {
            "desktop_commit": CURRENT_DESKTOP_COMMIT,
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "desktop_root_tree": CURRENT_DESKTOP_TREE,
            "android_pr_head_sha": android_head,
            "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head,
            "workflow_run_id": 456,
        }
        responsibilities = [{
            "responsibility_id": "RESP-001",
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "android_head_at_update": android_head,
            "desktop_sources": [{
                "path": source_path,
                "source_commit": CURRENT_DESKTOP_COMMIT,
                "blob_sha": source_blob,
                "source_mode": "100644",
                "source_type": "blob",
            }],
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
        }]
        responsibilities.extend({
            "responsibility_id": f"RESP-{index:03d}",
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "android_head_at_update": android_head,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [],
        } for index in range(2, 307))
        responsibility_doc = {
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "android_pr_head_sha": android_head,
            "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head,
            "authority": {
                "desktop_commit": CURRENT_DESKTOP_COMMIT,
                "desktop_root_tree": CURRENT_DESKTOP_TREE,
            },
            "responsibilities": responsibilities,
        }
        module.validate_exact_head_provenance(manifest, [row], responsibility_doc, android_head)

        for field in ("android_pr_head_sha", "android_tested_checkout_sha", "workflow_sha"):
            stale_manifest = dict(manifest)
            stale_manifest[field] = "b" * 40
            with self.assertRaisesRegex(ValueError, field):
                module.validate_exact_head_provenance(stale_manifest, [row], responsibility_doc, android_head)

        stale_rows = json.loads(json.dumps([row]))
        stale_rows[0]["evidence"][0]["android_head"] = "b" * 40
        with self.assertRaisesRegex(ValueError, "source evidence has stale Android head"):
            module.validate_exact_head_provenance(manifest, stale_rows, responsibility_doc, android_head)

        stale_responsibilities = json.loads(json.dumps(responsibility_doc))
        stale_responsibilities["responsibilities"][0]["android_head_at_update"] = "b" * 40
        with self.assertRaisesRegex(ValueError, "responsibility has stale Android head"):
            module.validate_exact_head_provenance(manifest, [row], stale_responsibilities, android_head)

    def test_non_target_consumer_labels_are_filtered_without_verification_promotion(self):
        module = load_generator()
        with tempfile.TemporaryDirectory() as temp:
            module.OUT = Path(temp)
            android_head = "a" * 40
            source_path = "source/host/src/lib.rs"
            source_blob = "1" * 40
            real_target = "source/host/src/lib.rs"
            invalid_targets = [
                "source/android-host-jni typed Host dispatch path",
                "frontend MessagingViewModel JSON projection consumer",
            ]
            predecessor = {
                "authority": {
                    "desktop_commit": CURRENT_DESKTOP_COMMIT,
                    "desktop_root_tree": CURRENT_DESKTOP_TREE,
                },
                "responsibilities": padded_responsibilities({
                    "responsibility_id": "RESP-001",
                    "source_owner": "canonical Rust Host owner",
                    "android_owner": "Android Host dispatch",
                    "android_target": [real_target] + invalid_targets,
                    "desktop_sources": [{
                        "path": source_path,
                        "blob_sha": source_blob,
                        "symbols": ["Host::dispatch"],
                        "source_mode": "100644",
                        "source_type": "blob",
                    }],
                }),
            }
            provenance = {
                "android_repository": "fabu-android/fabushi-android",
                "android_pr": 3,
                "android_pr_head_sha": android_head,
                "android_pr_head_ref": "spec/grok-bot-018-android-parity-20260922",
                "android_pr_base_sha": "b" * 40,
                "android_pr_base_ref": "main",
                "android_checkout_sha": android_head,
                "workflow_event": "push",
                "workflow_run_id": 456,
                "run_attempt": 1,
                "workflow_sha": android_head,
                "workflow_ref": "ref",
                "run_url": "https://example.invalid/run/456",
                "generated_at_utc": "2026-10-17T00:00:00Z",
            }
            rows = [{
                "source_path": source_path,
                "source_commit": CURRENT_DESKTOP_COMMIT,
                "source_blob_sha": source_blob,
                "source_type": "blob",
                "source_mode": "100644",
                "disposition": "direct-port",
                "implementation_status": "unreviewed",
                "evidence": [],
            }]
            inventory = {
                source_path: {
                    "path": source_path,
                    "sha": source_blob,
                    "type": "blob",
                    "mode": "100644",
                }
            }
            with patch.object(
                module, "load_predecessor_responsibility_mapping",
                return_value=(predecessor, {}, "current-responsibility-ledger+validated-local-coverage",
                              {"artifact_id": 12072277149, "artifact_digest": "sha256:" + "f" * 64,
                               "source_manifest_run_id": 456, "source_manifest_android_head": android_head,
                               "source_manifest_desktop_commit": CURRENT_DESKTOP_COMMIT,
                               "source_manifest_desktop_root_tree": CURRENT_DESKTOP_TREE}),
            ), patch.object(module, "current_android_provenance", return_value=provenance):
                result = module.merge_responsibility_mapping(
                    Path("responsibility-ledger.json"), rows, inventory,
                    CURRENT_DESKTOP_COMMIT, CURRENT_DESKTOP_TREE,
                )

            self.assertEqual(result["responsibilities"][0]["android_target"], [real_target])
            self.assertEqual(rows[0]["android_targets"], [real_target])
            self.assertEqual(rows[0]["implementation_status"], "unreviewed")
            self.assertEqual(rows[0]["evidence"][0]["status"], "provenance-only")
            self.assertEqual(rows[0]["evidence"][0]["verification_status"], "not-verified")
            self.assertEqual(result["responsibilities"][0]["verification_status"], "not-verified")
            diagnostics = json.loads((Path(temp) / "rejected-android-targets.json").read_text())
            self.assertEqual(diagnostics["count"], 2)
            self.assertEqual(
                {item["reason"] for item in diagnostics["rejected_targets"]},
                {"legacy-consumer-label-not-a-path"},
            )

    def test_merge_preserves_unmapped_rows_without_fabricating_closure(self):
        module = load_generator()
        with tempfile.TemporaryDirectory() as temp:
            module.OUT = Path(temp)
            predecessor = {
                "authority": {
                    "desktop_commit": "d" * 40,
                    "desktop_root_tree": "t" * 40,
                },
                "responsibilities": padded_responsibilities({
                    "responsibility_id": "RESP-001",
                    "source_owner": "source/host canonical owner",
                    "android_owner": "Rust Host",
                    "android_target": ["source/host/src/lib.rs"],
                    "desktop_sources": [
                        {
                            "path": "source/host/src/lib.rs",
                            "blob_sha": "old",
                            "symbols": ["real_symbol"],
                            "source_mode": "100644",
                            "source_type": "blob",
                        }
                    ],
                }),
            }
            source_rows = {
                "source/host/src/lib.rs": {
                    "source_path": "source/host/src/lib.rs",
                    "source_blob_sha": "a" * 40,
                    "disposition": "direct-port",
                    "responsibilities": ["RESP-001"],
                }
            }
            ledger_rows = [
                {
                    "source_path": "source/host/src/lib.rs",
                    "source_blob_sha": "a" * 40,
                    "source_type": "blob",
                    "source_mode": "100644",
                    "disposition": None,
                },
                {
                    "source_path": "source/packages/unmapped.ts",
                    "source_blob_sha": "b" * 40,
                    "source_type": "blob",
                    "source_mode": "100644",
                    "disposition": None,
                },
            ]
            new_by_path = {
                "source/host/src/lib.rs": {
                    "path": "source/host/src/lib.rs",
                    "sha": "a" * 40,
                    "type": "blob",
                    "mode": "100644",
                },
                "source/packages/unmapped.ts": {
                    "path": "source/packages/unmapped.ts",
                    "sha": "b" * 40,
                    "type": "blob",
                    "mode": "100644",
                },
            }
            provenance = {
                "android_repository": "fabu-android/fabushi-android",
                "android_pr": 3,
                "android_pr_head_sha": "h" * 40,
                "android_pr_head_ref": "spec/grok-bot-018-android-parity-20260922",
                "android_pr_base_sha": "m" * 40,
                "android_pr_base_ref": "main",
                "android_checkout_sha": "h" * 40,
                "workflow_event": "push",
                "workflow_run_id": 123,
                "run_attempt": 1,
                "workflow_sha": "w" * 40,
                "workflow_ref": "ref",
                "run_url": "https://example.invalid/run/123",
                "generated_at_utc": "2026-10-17T00:00:00Z",
            }
            with patch.object(module, "load_predecessor_responsibility_mapping",
                              return_value=(predecessor, source_rows, "pinned-predecessor-artifact",
                                            {"artifact_id": 12053467058, "artifact_digest": "sha256:test",
                                             "source_manifest_run_id": 1, "source_manifest_android_head": "h" * 40,
                                             "source_manifest_desktop_commit": "d" * 40,
                                             "source_manifest_desktop_root_tree": "t" * 40})),                  patch.object(module, "current_android_provenance", return_value=provenance):
                result = module.merge_responsibility_mapping(
                    Path("responsibility-ledger.json"),
                    ledger_rows,
                    new_by_path,
                    "d" * 40,
                    "t" * 40,
                )

            self.assertEqual(result["authority"]["mapped_source_rows"], 1)
            self.assertEqual(result["authority"]["unmapped_source_rows"], 1)
            self.assertEqual(ledger_rows[0]["responsibilities"], ["RESP-001"])
            self.assertEqual(ledger_rows[0]["disposition"], "direct-port")
            self.assertEqual(ledger_rows[0]["source_symbols"], ["real_symbol"])
            self.assertEqual(ledger_rows[0]["android_targets"], ["source/host/src/lib.rs"])
            self.assertEqual(ledger_rows[1]["responsibilities"], [])
            self.assertEqual(ledger_rows[1]["implementation_status"], "unreviewed")
            self.assertEqual(ledger_rows[1]["mapping_status"], "unmapped")
            self.assertEqual(ledger_rows[1]["evidence"][0]["status"], "missing-semantic-mapping")

            summary = json.loads((Path(temp) / "responsibility-mapping-summary.json").read_text())
            gaps = json.loads((Path(temp) / "unmapped-source-rows.json").read_text())
            self.assertEqual(summary["status"], "incomplete")
            self.assertEqual(gaps["count"], 1)
            self.assertEqual(gaps["paths"], ["source/packages/unmapped.ts"])

    def test_semantic_anchor_metadata_is_refreshed_from_current_inventory(self):
        module = load_generator()
        with tempfile.TemporaryDirectory() as temp:
            module.OUT = Path(temp)
            predecessor = {
                "authority": {"desktop_commit": "d" * 40, "desktop_root_tree": "t" * 40},
                "responsibilities": padded_responsibilities({
                    "responsibility_id": "RESP-001",
                    "source_owner": "canonical",
                    "android_owner": "Rust Host",
                    "android_target": ["source/host/src/lib.rs"],
                    "desktop_sources": [
                        {"path": "source/host/src/lib.rs", "blob_sha": "old",
                         "symbols": ["symbol"], "source_mode": "100644", "source_type": "blob"}
                    ],
                }),
            }
            provenance = {
                "android_repository": "fabu-android/fabushi-android",
                "android_pr": 3,
                "android_pr_head_sha": "h" * 40,
                "android_pr_head_ref": "branch",
                "android_pr_base_sha": "m" * 40,
                "android_pr_base_ref": "main",
                "android_checkout_sha": "h" * 40,
                "workflow_event": "push",
                "workflow_run_id": 456,
                "run_attempt": 1,
                "workflow_sha": "w" * 40,
                "workflow_ref": "ref",
                "run_url": "https://example.invalid/run/456",
                "generated_at_utc": "2026-10-17T00:00:00Z",
            }
            rows = [{
                "source_path": "source/host/src/lib.rs",
                "source_blob_sha": "n" * 40,
                "source_type": "blob",
                "source_mode": "100755",
                "disposition": "android-adapted",
            }]
            inventory = {
                "source/host/src/lib.rs": {
                    "path": "source/host/src/lib.rs",
                    "sha": "n" * 40,
                    "type": "blob",
                    "mode": "100755",
                }
            }
            with patch.object(module, "load_predecessor_responsibility_mapping",
                              return_value=(predecessor, {}, "current-responsibility-ledger",
                                            {"artifact_id": 12072277149, "artifact_digest": "sha256:" + "f" * 64,
                                             "source_manifest_run_id": 1, "source_manifest_android_head": "h" * 40,
                                             "source_manifest_desktop_commit": "d" * 40,
                                             "source_manifest_desktop_root_tree": "t" * 40})),                  patch.object(module, "current_android_provenance", return_value=provenance):
                result = module.merge_responsibility_mapping(
                    Path("responsibility-ledger.json"),
                    rows,
                    inventory,
                    "d" * 40,
                    "t" * 40,
                )
            source = result["responsibilities"][0]["desktop_sources"][0]
            self.assertEqual(source["blob_sha"], "n" * 40)
            self.assertEqual(source["source_mode"], "100755")
            self.assertEqual(source["source_type"], "blob")
            self.assertEqual(result["authority"]["semantic_source_anchor_unique_paths"], 1)

    def test_predecessor_artifact_missing_http_error_and_digest_mismatch_fail_closed(self):
        module=load_generator()
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/"responsibility-ledger.json"
            path.write_text(json.dumps({"responsibilities":[]}),encoding="utf-8")
            with patch.dict("os.environ",{"GH_TOKEN":""}):
                with self.assertRaisesRegex(ValueError,"GH_TOKEN is required"):
                    module.load_predecessor_responsibility_mapping(path)
            with patch.dict("os.environ",{"GH_TOKEN":"token"}),patch.object(module,"_download_predecessor_bytes",side_effect=RuntimeError("failed to recover predecessor responsibility artifact 12072277149 (HTTP 404)")):
                with self.assertRaisesRegex(RuntimeError,"HTTP 404"):
                    module.load_predecessor_responsibility_mapping(path)
            with patch.dict("os.environ",{"GH_TOKEN":"token","ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST":"sha256:"+"0"*64}),patch.object(module,"_download_predecessor_bytes",return_value=b"PK\x03\x04invalid"):
                with self.assertRaisesRegex(ValueError,"digest mismatch"):
                    module.load_predecessor_responsibility_mapping(path)


    def test_rebaseline_boundary_distinguishes_historical_and_current_counts(self):
        module = load_generator()
        self.assertEqual(
            (module.PREDECESSOR_TRACKED_NON_TREE_COUNT, module.PREDECESSOR_TREE_ENTRY_COUNT,
             module.EXPECTED_TRACKED_NON_TREE_COUNT, module.EXPECTED_TREE_ENTRY_COUNT),
            (8258, 9412, 8286, 9441),
        )
        current_commit, current_tree = "d" * 40, "e" * 40
        predecessor_commit, predecessor_tree = "p" * 40, "q" * 40
        with patch.object(module, "EXPECTED_DESKTOP_COMMIT", current_commit), \
             patch.object(module, "EXPECTED_DESKTOP_TREE", current_tree), \
             patch.object(module, "PREDECESSOR_DESKTOP_COMMIT", predecessor_commit), \
             patch.object(module, "PREDECESSOR_DESKTOP_TREE", predecessor_tree), \
             patch.object(module, "EXPECTED_TRACKED_NON_TREE_COUNT", 2), \
             patch.object(module, "EXPECTED_TREE_ENTRY_COUNT", 3), \
             patch.object(module, "PREDECESSOR_TRACKED_NON_TREE_COUNT", 1), \
             patch.object(module, "PREDECESSOR_TREE_ENTRY_COUNT", 2):
            baseline = {
                "desktop_repository": "bhrumom/fabushi-desktop",
                "desktop_commit": current_commit,
                "desktop_root_tree": current_tree,
                "desktop_main_before": predecessor_commit,
                "desktop_main_after": current_commit,
            }
            old_row = {
                "source_path": "old/path.rs", "source_commit": predecessor_commit,
                "source_blob_sha": "1" * 40, "source_type": "blob", "source_mode": "100644",
            }
            old_text = json.dumps(old_row, sort_keys=True) + "\n"
            old_manifest = {
                "authority_current": True, "desktop_commit": predecessor_commit,
                "source_commit": predecessor_commit, "desktop_root_tree": predecessor_tree,
                "desktop_main_before": predecessor_commit, "desktop_main_after": predecessor_commit,
                "tree_entries": 2, "tracked_non_tree_entries": 1, "authority_ledger_rows": 1,
                "ledger_sha256": hashlib.sha256(old_text.encode("utf-8")).hexdigest(),
            }
            old_rows = module._validate_rebaseline_boundary(
                baseline, old_manifest, old_text, current_commit, current_tree, 3, 2,
                predecessor_source_rows={
                    "old/path.rs": {
                        "source_blob_sha": "1" * 40, "source_mode": "100644", "source_type": "blob",
                    }
                },
            )
            self.assertEqual(old_rows[0]["source_path"], "old/path.rs")

            current_rows = [
                {"source_path": "AGENTS.md", "source_commit": current_commit,
                 "source_blob_sha": "2" * 40, "source_type": "blob", "source_mode": "100644"},
                {"source_path": "source/file.rs", "source_commit": current_commit,
                 "source_blob_sha": "3" * 40, "source_type": "blob", "source_mode": "100755"},
            ]
            current_text = "".join(json.dumps(row, sort_keys=True) + "\n" for row in current_rows)
            current_manifest = {
                "authority_current": True, "desktop_commit": current_commit,
                "source_commit": current_commit, "desktop_root_tree": current_tree,
                "desktop_main_before": predecessor_commit, "desktop_main_after": current_commit,
                "tree_entries": 3, "tracked_non_tree_entries": 2, "authority_ledger_rows": 2,
                "ledger_sha256": hashlib.sha256(current_text.encode("utf-8")).hexdigest(),
            }
            accepted = module._validate_rebaseline_boundary(
                baseline, current_manifest, current_text, current_commit, current_tree, 3, 2
            )
            self.assertEqual(len(accepted), 2)

            bad_manifest = dict(current_manifest)
            bad_manifest["tree_entries"] = 9412
            with self.assertRaisesRegex(ValueError, "tree count does not match its authority boundary"):
                module._validate_rebaseline_boundary(
                    baseline, bad_manifest, current_text, current_commit, current_tree, 3, 2
                )
            with self.assertRaisesRegex(ValueError, "digest does not match"):
                module._validate_rebaseline_boundary(
                    baseline, current_manifest, current_text + " ", current_commit, current_tree, 3, 2
                )

    def test_rebaseline_boundary_accepts_exact_prior_current_authority(self):
        module = load_generator()
        current_commit, current_tree = "d" * 40, "e" * 40
        prior_commit, prior_tree = "c" * 40, "b" * 40
        predecessor_commit, predecessor_tree = "a" * 40, "9" * 40
        with patch.object(module, "EXPECTED_DESKTOP_COMMIT", current_commit), \
             patch.object(module, "EXPECTED_DESKTOP_TREE", current_tree), \
             patch.object(module, "PRIOR_DESKTOP_COMMIT", prior_commit), \
             patch.object(module, "PRIOR_DESKTOP_TREE", prior_tree), \
             patch.object(module, "PREDECESSOR_DESKTOP_COMMIT", predecessor_commit), \
             patch.object(module, "PREDECESSOR_DESKTOP_TREE", predecessor_tree), \
             patch.object(module, "EXPECTED_TRACKED_NON_TREE_COUNT", 2), \
             patch.object(module, "EXPECTED_TREE_ENTRY_COUNT", 3), \
             patch.object(module, "PRIOR_TRACKED_NON_TREE_COUNT", 2), \
             patch.object(module, "PRIOR_TREE_ENTRY_COUNT", 3):
            baseline = {
                "desktop_repository": "bhrumom/fabushi-desktop",
                "desktop_commit": current_commit,
                "desktop_root_tree": current_tree,
                "desktop_main_before": prior_commit,
                "desktop_main_after": current_commit,
            }
            prior_rows = [
                {"source_path": "AGENTS.md", "source_commit": prior_commit,
                 "source_blob_sha": "1" * 40, "source_type": "blob", "source_mode": "100644"},
                {"source_path": "source/file.rs", "source_commit": prior_commit,
                 "source_blob_sha": "2" * 40, "source_type": "blob", "source_mode": "100755"},
            ]
            prior_text = "".join(json.dumps(row, sort_keys=True) + "\n" for row in prior_rows)
            prior_manifest = {
                "authority_current": True,
                "desktop_commit": prior_commit,
                "source_commit": prior_commit,
                "desktop_root_tree": prior_tree,
                "desktop_main_before": predecessor_commit,
                "desktop_main_after": prior_commit,
                "tree_entries": 3,
                "tracked_non_tree_entries": 2,
                "authority_ledger_rows": 2,
                "ledger_sha256": hashlib.sha256(prior_text.encode("utf-8")).hexdigest(),
            }
            accepted = module._validate_rebaseline_boundary(
                baseline, prior_manifest, prior_text, current_commit, current_tree, 3, 2
            )
            self.assertEqual([row["source_path"] for row in accepted], ["AGENTS.md", "source/file.rs"])

            stale = dict(prior_manifest)
            stale["desktop_root_tree"] = "8" * 40
            with self.assertRaisesRegex(ValueError, "neither the pinned historical predecessor"):
                module._validate_rebaseline_boundary(
                    baseline, stale, prior_text, current_commit, current_tree, 3, 2
                )


    def test_predecessor_redirect_does_not_forward_bearer_and_uses_real_zip_magic(self):
        module = load_generator()
        endpoint = "https://api.github.com/repos/fabu-android/fabushi-android/actions/artifacts/123/zip"
        signed_url = "https://artifact-storage.example.test/signed.zip"
        headers = Message()
        headers["Location"] = signed_url
        redirect_error = HTTPError(endpoint, 302, "Found", headers, None)
        opener = Mock()
        opener.open.side_effect = redirect_error
        payload = valid_zip_bytes()
        with patch.object(module.urllib.request, "build_opener", return_value=opener) as build_opener, \
             patch.object(module, "urlopen", return_value=HttpBytes(payload, 200)) as signed_download:
            downloaded = module._download_predecessor_bytes("123", "sensitive-token")
        self.assertEqual(downloaded, payload)
        self.assertTrue(downloaded.startswith(b"PK\x03\x04"))
        initial_request = opener.open.call_args.args[0]
        self.assertEqual(initial_request.get_header("Authorization"), "Bearer sensitive-token")
        redirected_request = signed_download.call_args.args[0]
        self.assertEqual(redirected_request.full_url, signed_url)
        self.assertIsNone(redirected_request.get_header("Authorization"))
        self.assertIsInstance(build_opener.call_args.args[0], module._NoRedirect)

    def test_predecessor_download_http_401_fails_closed(self):
        module = load_generator()
        endpoint = "https://api.github.com/repos/fabu-android/fabushi-android/actions/artifacts/123/zip"
        opener = Mock()
        opener.open.side_effect = HTTPError(endpoint, 401, "Unauthorized", Message(), None)
        with patch.object(module.urllib.request, "build_opener", return_value=opener):
            with self.assertRaisesRegex(RuntimeError, "HTTP 401"):
                module._download_predecessor_bytes("123", "invalid-token")

    def test_predecessor_download_rejects_invalid_zip_magic(self):
        module = load_generator()
        opener = Mock()
        opener.open.return_value = HttpBytes(b"not-a-zip", 200)
        with patch.object(module.urllib.request, "build_opener", return_value=opener):
            with self.assertRaisesRegex(ValueError, "not a ZIP local-file header"):
                module._download_predecessor_bytes("123", "token")


    def test_predecessor_download_rejects_non_200_and_content_length_mismatch(self):
        module = load_generator()
        opener = Mock()
        opener.open.return_value = HttpBytes(b"PK\\x03\\x04data", 206)
        with patch.object(module.urllib.request, "build_opener", return_value=opener):
            with self.assertRaisesRegex(ValueError, "unexpected HTTP status"):
                module._download_predecessor_bytes("123", "token")
        headers = Message()
        headers["Content-Length"] = "999"
        opener.open.return_value = HttpBytes(b"PK\\x03\\x04data", 200, headers)
        with patch.object(module.urllib.request, "build_opener", return_value=opener):
            with self.assertRaisesRegex(ValueError, "Content-Length mismatch"):
                module._download_predecessor_bytes("123", "token")

    def test_pinned_predecessor_validates_tree_rows_responsibilities_and_all_semantic_anchors(self):
        module = load_generator()
        payload, fixture = pinned_predecessor_zip_bytes(module)
        digest = "sha256:" + hashlib.sha256(payload).hexdigest()
        patches = [
            patch.object(module, "PREDECESSOR_DESKTOP_COMMIT", fixture["predecessor_commit"]),
            patch.object(module, "PREDECESSOR_DESKTOP_TREE", fixture["predecessor_tree"]),
            patch.object(module, "EXPECTED_DESKTOP_COMMIT", fixture["current_commit"]),
            patch.object(module, "EXPECTED_DESKTOP_TREE", fixture["current_tree"]),
            patch.object(module, "PREDECESSOR_TRACKED_NON_TREE_COUNT", 1),
            patch.object(module, "PREDECESSOR_TREE_ENTRY_COUNT", 2),
            patch.object(module, "EXPECTED_RESPONSIBILITY_COUNT", 2),
            patch.object(module, "PREDECESSOR_SEMANTIC_SOURCE_ANCHOR_COUNT", 1),
            patch.object(module, "_download_predecessor_bytes", return_value=payload),
            patch.dict("os.environ", {
                "GH_TOKEN": "token",
                "ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT": "123",
                "ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST": digest,
            }),
        ]
        with patches[0], patches[1], patches[2], patches[3], patches[4], patches[5], patches[6], patches[7], patches[8], patches[9]:
            candidate, rows, provenance = module._load_pinned_responsibility_mapping()
        self.assertEqual(len(candidate["responsibilities"]), 2)
        self.assertEqual(rows[fixture["source_path"]]["source_blob_sha"], fixture["source_sha"])
        self.assertEqual(provenance["validated_tree_entries"], 2)
        self.assertEqual(provenance["validated_source_rows"], 1)
        self.assertEqual(provenance["validated_responsibility_ids"], 2)
        self.assertEqual(provenance["validated_semantic_source_anchors"], 1)

        # Corrupt the only source object without changing the tree and require a hard failure.
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            contents = {name: archive.read(name) for name in archive.namelist()}
        bad_row = {
            "source_path": fixture["source_path"], "source_commit": fixture["predecessor_commit"],
            "source_blob_sha": "7" * 40, "source_mode": "100644", "source_type": "blob",
        }
        bad_ledger = json.dumps(bad_row, sort_keys=True) + "\n"
        manifest = json.loads(contents["docs/android-port/authority/source-inventory-manifest.json"])
        manifest["ledger_sha256"] = hashlib.sha256(bad_ledger.encode("utf-8")).hexdigest()
        output = io.BytesIO()
        with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, body in contents.items():
                if name.endswith("source-inventory-ledger.jsonl"):
                    archive.writestr(name, bad_ledger)
                elif name.endswith("source-inventory-manifest.json"):
                    archive.writestr(name, json.dumps(manifest))
                else:
                    archive.writestr(name, body)
        corrupt = output.getvalue()
        corrupt_digest = "sha256:" + hashlib.sha256(corrupt).hexdigest()
        with patch.object(module, "PREDECESSOR_DESKTOP_COMMIT", fixture["predecessor_commit"]), \
             patch.object(module, "PREDECESSOR_DESKTOP_TREE", fixture["predecessor_tree"]), \
             patch.object(module, "EXPECTED_DESKTOP_COMMIT", fixture["current_commit"]), \
             patch.object(module, "EXPECTED_DESKTOP_TREE", fixture["current_tree"]), \
             patch.object(module, "PREDECESSOR_TRACKED_NON_TREE_COUNT", 1), \
             patch.object(module, "PREDECESSOR_TREE_ENTRY_COUNT", 2), \
             patch.object(module, "EXPECTED_RESPONSIBILITY_COUNT", 2), \
             patch.object(module, "PREDECESSOR_SEMANTIC_SOURCE_ANCHOR_COUNT", 1), \
             patch.object(module, "_download_predecessor_bytes", return_value=corrupt), \
             patch.dict("os.environ", {
                 "GH_TOKEN": "token", "ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT": "123",
                 "ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST": corrupt_digest,
             }):
            with self.assertRaisesRegex(ValueError, "object does not match historical tree"):
                module._load_pinned_responsibility_mapping()

    def test_audit_receipt_is_written_to_runner_temp_with_real_json_newline(self):
        module = load_generator()
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            doc = root / "docs" / "android-port"
            authority = doc / "authority"
            out = doc / "generated"
            runner_temp = root / "runner-temp"
            authority.mkdir(parents=True)
            out.mkdir(parents=True)
            runner_temp.mkdir(parents=True)
            desktop_commit, desktop_tree, android_head = "d" * 40, "e" * 40, "a" * 40
            tree_entries = [
                {"path": "AGENTS.md", "sha": "1" * 40, "type": "blob", "mode": "100644"},
                {"path": "source", "sha": "2" * 40, "type": "tree", "mode": "040000"},
                {"path": "source/host.rs", "sha": "3" * 40, "type": "blob", "mode": "100644"},
            ]
            (out / "desktop-complete-tree.json").write_text(
                json.dumps({"sha": desktop_tree, "truncated": False, "tree": tree_entries}),
                encoding="utf-8",
            )
            (out / "desktop-root-tree.json").write_text(
                json.dumps({
                    "sha": desktop_tree,
                    "truncated": False,
                    "tree": [item for item in tree_entries if "/" not in item["path"]],
                }),
                encoding="utf-8",
            )
            source_rows = []
            responsibility_rows = []
            for index, (path, blob_sha) in enumerate(
                [("AGENTS.md", "1" * 40), ("source/host.rs", "3" * 40)], start=1
            ):
                responsibility_id = f"RESP-{index:03d}"
                evidence = {
                    "source_path": path, "source_blob_sha": blob_sha,
                    "desktop_commit": desktop_commit, "desktop_root_tree": desktop_tree,
                    "source_commit": desktop_commit, "android_head": android_head,
                    "android_pr_head_sha": android_head, "android_tested_checkout_sha": android_head,
                    "workflow_sha": android_head, "workflow_run_id": 456,
                }
                source_rows.append({
                    "source_repository": "bhrumom/fabushi-desktop",
                    "source_commit": desktop_commit, "source_path": path,
                    "source_blob_sha": blob_sha, "source_type": "blob", "source_mode": "100644",
                    "android_pr_head_sha": android_head, "android_tested_checkout_sha": android_head,
                    "workflow_sha": android_head,
                    "responsibilities": [responsibility_id], "android_targets": ["source/host.rs"],
                    "disposition": "android-adapted", "implementation_status": "unreviewed",
                    "verification_status": "not-verified", "evidence": [evidence],
                })
                responsibility_rows.append({
                    "responsibility_id": responsibility_id, "source_commit": desktop_commit,
                    "android_head_at_update": android_head, "implementation_status": "unreviewed",
                    "verification_status": "not-verified", "android_target": ["source/host.rs"],
                    "desktop_sources": [{
                        "path": path, "source_commit": desktop_commit, "blob_sha": blob_sha,
                        "source_mode": "100644", "source_type": "blob",
                    }],
                })
            ledger_text = "".join(json.dumps(row, sort_keys=True) + "\n" for row in source_rows)
            manifest = {
                "authority_current": True, "desktop_commit": desktop_commit,
                "source_commit": desktop_commit, "desktop_root_tree": desktop_tree,
                "desktop_main_before": desktop_commit, "desktop_main_after": desktop_commit,
                "tree_entries": 3, "tracked_non_tree_entries": 2, "authority_ledger_rows": 2,
                "ledger_sha256": hashlib.sha256(ledger_text.encode("utf-8")).hexdigest(),
                "android_pr_head_sha": android_head, "android_tested_checkout_sha": android_head,
                "workflow_sha": android_head, "workflow_run_id": 456,
            }
            baseline = {
                "desktop_repository": "bhrumom/fabushi-desktop",
                "desktop_commit": desktop_commit, "desktop_root_tree": desktop_tree,
            }
            responsibility = {
                "source_commit": desktop_commit, "android_pr_head_sha": android_head,
                "android_tested_checkout_sha": android_head, "workflow_sha": android_head,
                "authority": {"desktop_commit": desktop_commit, "desktop_root_tree": desktop_tree},
                "responsibilities": responsibility_rows,
            }
            coverage = {
                "status": "passed", "expected_source_rows": 0, "observed_source_rows": 0,
                "complete_rows": 0, "strict_incomplete_rows": 0, "gaps": [],
            }
            (authority / "baseline.json").write_text(json.dumps(baseline), encoding="utf-8")
            (authority / "source-inventory-manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
            (authority / "source-inventory-ledger.jsonl").write_text(ledger_text, encoding="utf-8")
            (authority / "responsibility-ledger.json").write_text(json.dumps(responsibility), encoding="utf-8")
            (out / "packages-semantic-coverage.json").write_text(json.dumps(coverage), encoding="utf-8")
            (out / "rejected-android-targets.json").write_text(
                json.dumps({"status": "passed", "count": 0, "rejected_targets": []}), encoding="utf-8"
            )
            payload, predecessor_fixture = pinned_predecessor_zip_bytes(module, responsibilities=2, anchors=1)
            # Rebind the tiny predecessor artifact baseline's current authority to this receipt fixture.
            with zipfile.ZipFile(io.BytesIO(payload)) as archive:
                contents = {name: archive.read(name) for name in archive.namelist()}
            predecessor_baseline = json.loads(contents["docs/android-port/authority/baseline.json"])
            predecessor_baseline["desktop_commit"] = desktop_commit
            predecessor_baseline["desktop_root_tree"] = desktop_tree
            predecessor_baseline["rebaseline_note"] = (
                f"Historical predecessor authority {predecessor_fixture['predecessor_commit']}/tree "
                f"{predecessor_fixture['predecessor_tree']} had 1 non-tree/2 recursive entries."
            )
            rebuilt = io.BytesIO()
            with zipfile.ZipFile(rebuilt, "w", zipfile.ZIP_DEFLATED) as archive:
                for name, body in contents.items():
                    if name.endswith("baseline.json"):
                        archive.writestr(name, json.dumps(predecessor_baseline))
                    else:
                        archive.writestr(name, body)
            payload = rebuilt.getvalue()
            provenance = {
                "android_repository": "fabu-android/fabushi-android", "android_pr": 3,
                "android_pr_head_sha": android_head, "android_pr_head_ref": "branch",
                "android_pr_base_sha": "b" * 40, "android_pr_base_ref": "main",
                "android_checkout_sha": android_head, "workflow_event": "pull_request",
                "workflow_run_id": 456, "run_attempt": 1, "workflow_sha": android_head,
                "workflow_ref": "ref", "run_url": "https://example.invalid/run/456",
                "generated_at_utc": "2026-10-17T00:00:00Z",
            }
            digest = "sha256:" + hashlib.sha256(payload).hexdigest()
            def fake_github_get(endpoint):
                if endpoint == "/repos/bhrumom/fabushi-desktop/branches/main":
                    return {"commit": {"sha": desktop_commit}}
                if endpoint == f"/repos/bhrumom/fabushi-desktop/git/commits/{desktop_commit}":
                    return {"sha": desktop_commit, "tree": {"sha": desktop_tree}}
                if endpoint == f"/repos/bhrumom/fabushi-desktop/git/trees/{desktop_tree}?recursive=1":
                    return {"sha": desktop_tree, "truncated": False, "tree": tree_entries}
                raise AssertionError(f"unexpected GitHub API request in receipt fixture: {endpoint}")
            with patch.object(module, "ROOT", root), patch.object(module, "DOC", doc), \
                 patch.object(module, "OUT", out), \
                 patch.object(module, "EXPECTED_DESKTOP_COMMIT", desktop_commit), \
                 patch.object(module, "EXPECTED_DESKTOP_TREE", desktop_tree), \
                 patch.object(module, "EXPECTED_TREE_ENTRY_COUNT", 3), \
                 patch.object(module, "EXPECTED_TRACKED_NON_TREE_COUNT", 2), \
                 patch.object(module, "EXPECTED_RESPONSIBILITY_COUNT", 2), \
                 patch.object(module, "EXPECTED_PACKAGES_SOURCE_COUNT", 0), \
                 patch.object(module, "PREDECESSOR_DESKTOP_COMMIT", predecessor_fixture["predecessor_commit"]), \
                 patch.object(module, "PREDECESSOR_DESKTOP_TREE", predecessor_fixture["predecessor_tree"]), \
                 patch.object(module, "PREDECESSOR_TRACKED_NON_TREE_COUNT", 1), \
                 patch.object(module, "PREDECESSOR_TREE_ENTRY_COUNT", 2), \
                 patch.object(module, "PREDECESSOR_SEMANTIC_SOURCE_ANCHOR_COUNT", 1), \
                 patch.object(module, "_download_predecessor_bytes", return_value=payload), \
                 patch.object(module, "_current_android_tracked_files", return_value={"source/host.rs"}), \
                 patch.object(module, "current_android_provenance", return_value=provenance), \
                 patch.object(module, "get", side_effect=fake_github_get), \
                 patch.dict("os.environ", {
                     "RUNNER_TEMP": str(runner_temp), "GH_TOKEN": "token",
                     "ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT": "123",
                     "ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST": digest,
                 }):
                module.audit_receipt()
            receipt_path = runner_temp / "android-port-audit-receipt.json"
            raw_receipt = receipt_path.read_bytes()
            self.assertTrue(raw_receipt.endswith(b"\n"))
            self.assertFalse((out / "android-port-audit-receipt.json").exists())
            receipt = json.loads(raw_receipt.decode("utf-8"))
            self.assertEqual(receipt["predecessor_artifact"]["downloaded_bytes_sha256"], digest)
            self.assertEqual(receipt["inventory"]["unique_source_path_count"], 2)
            self.assertEqual(receipt["inventory"]["unique_responsibility_id_count"], 2)

    def test_inventory_serializer_is_canonical_and_digestable(self):
        module = load_generator()
        rows = [
            {"source_path": "source/a.rs", "source_commit": CURRENT_DESKTOP_COMMIT},
            {"source_path": "source/b.rs", "source_commit": CURRENT_DESKTOP_COMMIT},
        ]
        serialized = module.serialize_source_inventory_ledger(rows)
        self.assertEqual(
            serialized,
            "".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in rows),
        )
        digest = hashlib.sha256(serialized.encode("utf-8")).hexdigest()
        self.assertEqual(digest, hashlib.sha256(serialized.encode("utf-8")).hexdigest())
        with self.assertRaisesRegex(ValueError, "canonical sorted path order"):
            module.serialize_source_inventory_ledger(list(reversed(rows)))


    def test_android_targets_must_be_tracked_files_and_rejections_are_explicit(self):
        module = load_generator()
        accepted, rejected = module._filter_android_targets(
            [
                "source/host/src/lib.rs",
                "source/host/src/not-tracked.rs",
                "frontend MessagingViewModel JSON projection consumer",
            ],
            {"source/host/src/lib.rs"},
        )
        self.assertEqual(accepted, ["source/host/src/lib.rs"])
        self.assertEqual(len(rejected), 2)
        self.assertEqual(
            {item["reason"] for item in rejected},
            {"not-in-current-android-tracked-file-tree", "legacy-consumer-label-not-a-path"},
        )

    def test_packages_semantic_coverage_preserves_852_rows_and_requires_all_closure_fields(self):
        module = load_generator()
        target = "source/host/src/lib.rs"
        test_target = "tests/HostContractTest.kt"
        paths = [f"source/packages/package-{index:03d}/lib.rs" for index in range(852)]
        rows = [{
            "source_path": path, "responsibilities": [], "mapping_status": "unmapped",
            "android_targets": [], "disposition": None,
        } for path in paths]
        owner = padded_responsibilities({"responsibility_id": "PKG-RESP-001"})
        incomplete = module._package_semantic_coverage(rows, owner, {target, test_target})
        self.assertEqual(incomplete["expected_source_rows"], 852)
        self.assertEqual(incomplete["observed_source_rows"], 852)
        self.assertEqual(incomplete["strict_incomplete_rows"], 852)
        self.assertEqual(incomplete["semantic_responsibility_rows"], 0)

        android_head = "a" * 40
        semantic_sources = [
            {"path": path, "mapping_quality": "semantic", "source_mode": "100644", "source_type": "blob"}
            for path in paths
        ]
        production_evidence = [{
            "kind": "production-wiring", "source_path": path,
            "source_commit": CURRENT_DESKTOP_COMMIT, "desktop_commit": CURRENT_DESKTOP_COMMIT,
            "android_pr_head_sha": android_head, "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head, "evidence_ref": target + "#production",
            "workflow_name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "workflow_path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "workflow_event": "pull_request", "workflow_repository": module.ANDROID_REPOSITORY,
            "workflow_run_id": 456, "workflow_job_id": 789,
            "workflow_job_name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME,
            "workflow_job_run_attempt": 1, "run_attempt": 1, "run_conclusion": "success",
            "workflow_step_number": 7, "workflow_step_name": module.AGENT_KV_PRODUCTION_STEP,
            "result": "passed",
        } for path in paths]
        test_evidence = [{
            "kind": "contract-test", "source_path": path,
            "source_commit": CURRENT_DESKTOP_COMMIT, "desktop_commit": CURRENT_DESKTOP_COMMIT,
            "android_pr_head_sha": android_head, "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head, "evidence_ref": test_target + "#test",
            "workflow_name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "workflow_path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "workflow_event": "pull_request", "workflow_repository": module.ANDROID_REPOSITORY,
            "workflow_run_id": 456, "workflow_job_id": 789,
            "workflow_job_name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME,
            "workflow_job_run_attempt": 1, "run_attempt": 1, "run_conclusion": "success",
            "workflow_step_number": 8, "workflow_step_name": module.AGENT_KV_TEST_STEP,
            "result": "passed",
        } for path in paths]
        responsibility = {
            "responsibility_id": "PKG-RESP-001",
            "source_owner": "Desktop package source owner",
            "android_owner": "Android Mahayana package runtime owner",
            "android_target": [target], "desktop_sources": semantic_sources,
            "production_evidence": production_evidence, "test_evidence": test_evidence,
        }
        complete_rows = [{
            "source_path": path, "source_commit": CURRENT_DESKTOP_COMMIT,
            "android_pr_head_sha": android_head, "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head, "responsibilities": ["PKG-RESP-001"],
            "mapping_status": "semantic", "android_targets": [target], "disposition": "direct-port",
        } for path in paths]

        def github_api(run_conclusion, production_conclusion, test_conclusion):
            def get_ci(endpoint):
                if endpoint == "/repos/fabu-android/fabushi-android/actions/runs/456":
                    return {"name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
                            "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
                            "path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
                            "event": "pull_request",
                            "repository": {"full_name": module.ANDROID_REPOSITORY},
                            "head_sha": android_head, "run_attempt": 1,
                            "status": "completed", "conclusion": run_conclusion}
                if endpoint == "/repos/fabu-android/fabushi-android/actions/runs/456/jobs?filter=latest&per_page=100&page=1":
                    return {"jobs": [{
                        "id": 789, "run_id": 456, "run_attempt": 1,
                        "name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME,
                        "status": "completed", "conclusion": "success",
                        "steps": [
                            {"name": module.AGENT_KV_PRODUCTION_STEP,
                             "number": 7, "status": "completed", "conclusion": production_conclusion},
                            {"name": module.AGENT_KV_TEST_STEP,
                             "number": 8, "status": "completed", "conclusion": test_conclusion},
                        ],
                    }]}
                raise AssertionError("Unexpected GitHub API endpoint: " + endpoint)
            return get_ci

        tracked = {target, test_target}
        with patch.object(module, "get", side_effect=github_api("success", "success", "success")):
            complete = module._package_semantic_coverage(
                complete_rows, padded_responsibilities(responsibility), tracked
            )
        self.assertEqual(complete["status"], "passed")
        self.assertEqual(complete["strict_incomplete_rows"], 0)
        self.assertEqual(complete["complete_rows"], 852)

        with patch.object(module, "get", side_effect=github_api("failure", "success", "success")):
            successful_steps_in_failed_run = module._package_semantic_coverage(
                json.loads(json.dumps(complete_rows)), padded_responsibilities(responsibility), tracked
            )
        self.assertEqual(successful_steps_in_failed_run["strict_incomplete_rows"], 852)
        self.assertEqual(successful_steps_in_failed_run["status"], "incomplete")
        self.assertIn("missing-exact-head-production-evidence", successful_steps_in_failed_run["gaps"][0]["reasons"])

        with patch.object(module, "get", side_effect=github_api("cancelled", "success", "success")):
            cancelled_run = module._package_semantic_coverage(
                json.loads(json.dumps(complete_rows)), padded_responsibilities(responsibility), tracked
            )
        self.assertEqual(cancelled_run["strict_incomplete_rows"], 852)
        self.assertEqual(cancelled_run["status"], "incomplete")
        self.assertIn("missing-exact-head-production-evidence", cancelled_run["gaps"][0]["reasons"])
        with patch.object(module, "get", side_effect=github_api("success", "failure", "cancelled")):
            failed_jobs = module._package_semantic_coverage(
                json.loads(json.dumps(complete_rows)), padded_responsibilities(responsibility), tracked
            )
        self.assertEqual(failed_jobs["strict_incomplete_rows"], 852)
        self.assertIn("missing-exact-head-test-evidence", failed_jobs["gaps"][0]["reasons"])

        coverage_only_owner = {**responsibility, "desktop_sources": [
            {**source, "mapping_quality": "coverage-only"} for source in semantic_sources
        ]}
        coverage_only_rows = [{
            "source_path": path, "responsibilities": ["PKG-RESP-001"],
            "mapping_status": "coverage-only", "android_targets": [target],
            "disposition": "direct-port",
        } for path in paths]
        coverage_only = module._package_semantic_coverage(
            coverage_only_rows, padded_responsibilities(coverage_only_owner), tracked
        )
        self.assertEqual(coverage_only["semantic_responsibility_rows"], 0)
        self.assertGreater(coverage_only["strict_incomplete_rows"], 0)
        self.assertIn("missing-semantic-responsibility", coverage_only["gaps"][0]["reasons"])

    def test_exact_head_validation_rejects_promoted_status_and_stale_ledger_metadata(self):
        module = load_generator()
        android_head = "a" * 40
        source_path = "source/host/src/lib.rs"
        source_blob = "1" * 40
        row = {
            "source_path": source_path,
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "source_blob_sha": source_blob,
            "source_type": "blob",
            "source_mode": "100644",
            "android_pr_head_sha": android_head,
            "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "evidence": [{
                "source_path": source_path,
                "source_blob_sha": source_blob,
                "desktop_commit": CURRENT_DESKTOP_COMMIT,
                "desktop_root_tree": CURRENT_DESKTOP_TREE,
                "source_commit": CURRENT_DESKTOP_COMMIT,
                "android_head": android_head,
                "android_pr_head_sha": android_head,
                "android_tested_checkout_sha": android_head,
                "workflow_sha": android_head,
                "workflow_run_id": 456,
            }],
        }
        row["responsibilities"] = ["RESP-001"]
        manifest = {
            "desktop_commit": CURRENT_DESKTOP_COMMIT,
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "desktop_root_tree": CURRENT_DESKTOP_TREE,
            "android_pr_head_sha": android_head,
            "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head,
            "workflow_run_id": 456,
        }
        responsibilities = [{
            "responsibility_id": "RESP-001",
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "android_head_at_update": android_head,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [{
                "path": source_path,
                "source_commit": CURRENT_DESKTOP_COMMIT,
                "blob_sha": source_blob,
                "source_mode": "100644",
                "source_type": "blob",
            }],
        }]
        responsibilities.extend({
            "responsibility_id": f"RESP-{index:03d}",
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "android_head_at_update": android_head,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [],
        } for index in range(2, 307))
        doc = {
            "source_commit": CURRENT_DESKTOP_COMMIT,
            "android_pr_head_sha": android_head,
            "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head,
            "authority": {
                "desktop_commit": CURRENT_DESKTOP_COMMIT,
                "desktop_root_tree": CURRENT_DESKTOP_TREE,
            },
            "responsibilities": responsibilities,
        }
        module.validate_exact_head_provenance(manifest, [row], doc, android_head)

        promoted = json.loads(json.dumps([row]))
        promoted[0]["implementation_status"] = "implemented"
        with self.assertRaisesRegex(ValueError, "status was promoted"):
            module.validate_exact_head_provenance(manifest, promoted, doc, android_head)

        stale_ledger = json.loads(json.dumps([row]))
        stale_ledger[0]["workflow_sha"] = "b" * 40
        with self.assertRaisesRegex(ValueError, "source ledger row mismatch for workflow_sha"):
            module.validate_exact_head_provenance(manifest, stale_ledger, doc, android_head)

        stale_doc = json.loads(json.dumps(doc))
        stale_doc["android_tested_checkout_sha"] = "b" * 40
        with self.assertRaisesRegex(ValueError, "responsibility ledger mismatch for android_tested_checkout_sha"):
            module.validate_exact_head_provenance(manifest, [row], stale_doc, android_head)

    def test_merge_lineage_allows_persisted_current_authority_with_historical_pin(self):
        generator = SCRIPT.read_text(encoding="utf-8")
        merge_start = generator.index("def merge_responsibility_mapping(")
        rebaseline_start = generator.index("def rebaseline()")
        merge_body = generator[merge_start:rebaseline_start]
        self.assertIn("prior_pair = (PRIOR_DESKTOP_COMMIT, PRIOR_DESKTOP_TREE)", merge_body)
        self.assertIn("authority_pair in {pinned_pair, prior_pair, current_pair}", merge_body)
        self.assertNotIn("predecessor_authority.get('desktop_commit') == mapping_provenance.get('source_manifest_desktop_commit')", merge_body)
        self.assertIn("mapping must originate from a verified predecessor artifact", merge_body)

    def test_merge_lineage_accepts_exact_prior_authority_pair(self):
        module = load_generator()
        current_commit, current_tree = "d" * 40, "e" * 40
        prior_commit, prior_tree = "c" * 40, "b" * 40
        android_head = "a" * 40
        predecessor = {
            "authority": {"desktop_commit": prior_commit, "desktop_root_tree": prior_tree},
            "responsibilities": padded_responsibilities({
                "responsibility_id": "RESP-001",
                "source_owner": "canonical owner",
                "android_owner": "Rust Host",
                "android_target": ["source/host/src/lib.rs"],
                "desktop_sources": [],
            }),
        }
        ledger_rows = [{
            "source_path": "source/unmapped.rs",
            "source_commit": current_commit,
            "source_blob_sha": "1" * 40,
            "source_type": "blob",
            "source_mode": "100644",
            "implementation_status": "unreviewed",
            "evidence": [],
        }]
        inventory = {
            "source/unmapped.rs": {
                "path": "source/unmapped.rs",
                "sha": "1" * 40,
                "type": "blob",
                "mode": "100644",
            }
        }
        provenance = {
            "android_repository": "fabu-android/fabushi-android",
            "android_pr": 3,
            "android_pr_head_sha": android_head,
            "android_pr_head_ref": "branch",
            "android_pr_base_sha": "2" * 40,
            "android_pr_base_ref": "main",
            "android_checkout_sha": android_head,
            "workflow_event": "pull_request",
            "workflow_run_id": 456,
            "run_attempt": 1,
            "workflow_sha": android_head,
            "workflow_ref": "ref",
            "run_url": "https://example.invalid/run/456",
            "generated_at_utc": "2026-10-17T00:00:00Z",
        }
        with tempfile.TemporaryDirectory() as temp, \
             patch.object(module, "OUT", Path(temp)), \
             patch.object(module, "PRIOR_DESKTOP_COMMIT", prior_commit), \
             patch.object(module, "PRIOR_DESKTOP_TREE", prior_tree), \
             patch.object(module, "load_predecessor_responsibility_mapping",
                          return_value=(predecessor, {}, "current-responsibility-ledger", {
                              "artifact_id": 12072277149,
                              "artifact_digest": "sha256:" + "f" * 64,
                              "source_manifest_run_id": 1,
                              "source_manifest_android_head": android_head,
                              "source_manifest_desktop_commit": "9" * 40,
                              "source_manifest_desktop_root_tree": "8" * 40,
                          })), \
             patch.object(module, "current_android_provenance", return_value=provenance):
            result = module.merge_responsibility_mapping(
                Path("responsibility-ledger.json"),
                ledger_rows,
                inventory,
                current_commit,
                current_tree,
            )
        self.assertEqual(result["authority"]["desktop_commit"], current_commit)
        self.assertEqual(ledger_rows[0]["mapping_status"], "unmapped")


    def test_rebaseline_defers_generated_mapping_writes_until_after_validation(self):
        generator = SCRIPT.read_text(encoding="utf-8")
        merge_start = generator.index("def merge_responsibility_mapping(")
        rebaseline_start = generator.index("def rebaseline()")
        merge_body = generator[merge_start:rebaseline_start]
        rebaseline_body = generator[rebaseline_start:generator.index("\ndef inventory()", rebaseline_start)]
        self.assertIn("persist_outputs: bool = True", merge_body)
        self.assertIn("deferred_outputs: dict[str, dict] | None = None", merge_body)
        self.assertIn("if deferred_outputs is not None:", merge_body)
        self.assertIn("if persist_outputs:", merge_body)
        self.assertIn("persist_outputs=False", rebaseline_body)
        self.assertIn("deferred_outputs=mapping_outputs", rebaseline_body)
        self.assertIn("for filename, document in mapping_outputs.items():", rebaseline_body)
        first_write = rebaseline_body.index("ledger_path.write_text(")
        self.assertLess(rebaseline_body.index("validate_exact_head_provenance("), first_write)
        self.assertLess(rebaseline_body.index("latest_desktop_main ="), first_write)
        self.assertGreater(rebaseline_body.index("for filename, document in mapping_outputs.items():"), first_write)

    def test_predecessor_fixture_records_reverse_links_for_every_anchor(self):
        module = load_generator()
        payload, _ = pinned_predecessor_zip_bytes(module)
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            row = json.loads(archive.read(
                "docs/android-port/authority/source-inventory-ledger.jsonl"
            ))
            responsibility = json.loads(archive.read(
                "docs/android-port/authority/responsibility-ledger.json"
            ))
        self.assertEqual(row["responsibilities"], ["RESP-001"])
        self.assertEqual(responsibility["responsibilities"][0]["responsibility_id"], "RESP-001")

    def test_workflow_persists_generated_package_coverage_and_diagnostics(self):
        workflow = (ROOT / '.github' / 'workflows' / 'android-port-docs.yml').read_text(encoding='utf-8')
        generator = SCRIPT.read_text(encoding='utf-8')
        self.assertIn("'packages-semantic-coverage.json': package_coverage", generator)
        self.assertIn("for filename, document in generated_outputs.items():", generator)
        self.assertIn("save(OUT / filename, document)", generator)
        self.assertIn('docs/android-port/generated/packages-semantic-coverage.json', workflow)
        self.assertIn('docs/android-port/generated/rejected-android-targets.json', workflow)
        self.assertIn("assert coverage['complete_rows'] + coverage['strict_incomplete_rows'] == 852", workflow)
        push_trigger = workflow.split('  workflow_dispatch:', 1)[0]
        self.assertIn('  push:', push_trigger)
        self.assertIn('  pull_request:', push_trigger)
        self.assertIn("'spec/grok-bot-018-android-parity-20260922'", push_trigger)
        self.assertIn('github.event.pull_request.head.sha || github.sha', workflow)
        self.assertIn('pull-requests: read', workflow)
        self.assertIn('      - \'source/**\'', push_trigger)
        self.assertIn('      - \'native/**\'', push_trigger)
        self.assertIn('      - \'tests/**\'', push_trigger)
        self.assertNotIn('      - \'docs/android-port/**\'', push_trigger,
                         'the generated-doc commit must not trigger an endless persistence loop')


    def test_artifact_only_docs_run_does_not_create_persistence_loop(self):
        workflow = (ROOT / ".github" / "workflows" / "android-port-docs.yml").read_text(encoding="utf-8")
        self.assertIn("group: android-port-docs-${{ github.event.pull_request.head.ref || github.ref_name }}", workflow)
        self.assertIn("Upload source inventory and documentation", workflow)
        self.assertNotIn("gh workflow run android-port-docs.yml", workflow)
        self.assertNotIn("chore(android): persist exact Desktop main authority evidence", workflow)
        push_trigger = workflow.split("  workflow_dispatch:", 1)[0]
        self.assertIn("'source/**'", push_trigger)
        self.assertIn("'tests/**'", push_trigger)
        self.assertNotIn("'docs/android-port/**'", push_trigger)

    def test_agent_kv_semantic_mapping_uses_exact_desktop_objects_and_tracked_android_targets(self):
        module = load_generator()
        primary = {
            "responsibility_id": module.AGENT_KV_RESPONSIBILITY_ID,
            "desktop_sources": [],
            "source_owner": "old placeholder",
            "android_owner": "old placeholder",
            "android_target": [],
        }
        responsibilities = padded_responsibilities(primary)
        new_by_path = {
            path: {"sha": f"{index + 1:040x}", "mode": "100644", "type": "blob"}
            for index, path in enumerate(module.AGENT_KV_SOURCE_SPECS)
        }
        module._ensure_agent_kv_semantic_mapping(
            responsibilities, new_by_path, CURRENT_DESKTOP_COMMIT
        )
        self.assertEqual(len(responsibilities), 306)
        owner = next(item for item in responsibilities
                     if item["responsibility_id"] == module.AGENT_KV_RESPONSIBILITY_ID)
        self.assertIn("AgentStore2", owner["source_owner"])
        self.assertIn("ProductionAgentStore", owner["android_owner"])
        self.assertEqual(len({
            source["path"] for source in owner["desktop_sources"]
            if source["path"] in module.AGENT_KV_SOURCE_SPECS
        }), 7)
        for path, source_spec in module.AGENT_KV_SOURCE_SPECS.items():
            source = next(item for item in owner["desktop_sources"] if item["path"] == path)
            self.assertEqual(source["blob_sha"], new_by_path[path]["sha"])
            self.assertEqual(source["source_commit"], CURRENT_DESKTOP_COMMIT)
            self.assertEqual(source["source_mode"], "100644")
            self.assertEqual(source["source_type"], "blob")
            self.assertEqual(source["mapping_quality"], "semantic")
            self.assertEqual(source["symbols"], source_spec["symbols"])
            self.assertEqual(source["focused_tests"], source_spec["focused_tests"])
            self.assertTrue(source["android_targets"])
            self.assertEqual(source_spec["disposition"], "android-adapted")
            self.assertIn(source_spec["production_target"], source["android_targets"])
            self.assertIn(source_spec["test_target"], source["android_targets"])
            self.assertTrue(all(target.startswith("source/") for target in source["android_targets"]))

    def test_constants_composer_mapping_uses_desktop_limit_and_real_host_owner(self):
        module = load_generator()
        source_path = module.COMPOSER_CONSTANT_SOURCE_PATH
        primary = {
            "responsibility_id": module.COMPOSER_CONSTANT_RESPONSIBILITY_ID,
            "desktop_sources": [],
            "source_owner": "Canonical draft owner",
            "android_owner": "Host DurableComposerDraftOwner",
            "android_target": ["source/host/src/composer_draft.rs"],
        }
        responsibilities = padded_responsibilities(primary)
        source_object = {"sha": "d" * 40, "mode": "100644", "type": "blob"}
        module._ensure_constants_composer_semantic_mapping(
            responsibilities,
            {source_path: source_object},
            CURRENT_DESKTOP_COMMIT,
        )
        owner = next(
            item for item in responsibilities
            if item["responsibility_id"] == module.COMPOSER_CONSTANT_RESPONSIBILITY_ID
        )
        source = next(item for item in owner["desktop_sources"] if item["path"] == source_path)
        self.assertEqual(source["blob_sha"], source_object["sha"])
        self.assertEqual(source["source_commit"], CURRENT_DESKTOP_COMMIT)
        self.assertEqual(source["source_mode"], "100644")
        self.assertEqual(source["source_type"], "blob")
        self.assertEqual(source["mapping_quality"], "semantic")
        self.assertEqual(source["symbols"], ["MAX_TEXT_SIZE"])
        self.assertIn("source/packages/constants/src/composer.rs", source["android_targets"])
        self.assertIn("source/host/src/composer_draft.rs", source["android_targets"])
        self.assertTrue(source["focused_tests"])
        self.assertIn("8 MiB", source["source_owner"])
        self.assertIn("DurableComposerDraftOwner", source["android_owner"])

    def test_constants_composer_contract_is_enforced_by_host_and_exact_head_workflow(self):
        host = (ROOT / "source" / "host" / "src" / "composer_draft.rs").read_text(encoding="utf-8")
        workflow = (ROOT / ".github" / "workflows" / "grok-android-architecture.yml").read_text(encoding="utf-8")
        self.assertIn("use fabushi_constants::composer::text_size_allowed;", host)
        self.assertIn("validate_prompt(prompt)?;", host)
        self.assertNotIn("const MAX_PROMPT_BYTES: usize = 2 * 1024 * 1024;", host)
        self.assertIn("prompt_size_limit_matches_desktop_composer_contract", host)
        self.assertIn("Verify exact-HEAD constants composer production owner", workflow)
        self.assertIn("Test exact-HEAD constants composer production owner", workflow)
        self.assertIn("cargo test -p fabushi-constants composer::tests::text_limit_is_inclusive -- --exact", workflow)
        self.assertIn("cargo test -p fabushi-mahayana-host composer_draft::tests::prompt_size_limit_matches_desktop_composer_contract -- --exact", workflow)

    def test_exact_head_ci_discovery_supports_constants_composer_evidence_steps(self):
        module = load_generator()
        android_head = "a" * 40
        run = {
            "id": 456, "name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "head_sha": android_head, "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "event": "pull_request", "repository": {"full_name": module.ANDROID_REPOSITORY},
            "status": "completed", "conclusion": "success", "run_number": 12, "run_attempt": 1,
        }
        job = {
            "id": 789, "name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME, "run_attempt": 1,
            "status": "completed", "conclusion": "success",
            "steps": [
                {"name": module.COMPOSER_CONSTANT_PRODUCTION_STEP, "number": 9,
                 "status": "completed", "conclusion": "success"},
                {"name": module.COMPOSER_CONSTANT_TEST_STEP, "number": 10,
                 "status": "completed", "conclusion": "success"},
            ],
        }
        with patch.dict("os.environ", {"GH_TOKEN": "token", "ANDROID_CI_EVIDENCE_WAIT_SECONDS": "0"}), \
             patch.object(module, "get", side_effect=[{"workflow_runs": [run]}, {"jobs": [job]}]):
            evidence = module._latest_exact_head_android_ci_evidence(
                android_head,
                production_step_name=module.COMPOSER_CONSTANT_PRODUCTION_STEP,
                test_step_name=module.COMPOSER_CONSTANT_TEST_STEP,
                evidence_label="constants composer",
            )
        self.assertIsNotNone(evidence)
        self.assertEqual(evidence["production_step_name"], module.COMPOSER_CONSTANT_PRODUCTION_STEP)
        self.assertEqual(evidence["test_step_name"], module.COMPOSER_CONSTANT_TEST_STEP)

    def test_constants_structured_log_mapping_uses_desktop_policy_and_real_host_owner(self):
        module = load_generator()
        source_path = module.STRUCTURED_LOG_CONSTANT_SOURCE_PATH
        primary = {
            "responsibility_id": module.STRUCTURED_LOG_CONSTANT_RESPONSIBILITY_ID,
            "desktop_sources": [],
            "source_owner": "Desktop structured telemetry owner",
            "android_owner": "Host structured telemetry owner",
            "android_target": ["source/host/src/android_structured_log_spill.rs"],
        }
        responsibilities = padded_responsibilities(primary)
        source_object = {"sha": "e" * 40, "mode": "100644", "type": "blob"}
        module._ensure_constants_structured_log_semantic_mapping(
            responsibilities,
            {source_path: source_object},
            CURRENT_DESKTOP_COMMIT,
        )
        owner = next(
            item for item in responsibilities
            if item["responsibility_id"] == module.STRUCTURED_LOG_CONSTANT_RESPONSIBILITY_ID
        )
        source = next(item for item in owner["desktop_sources"] if item["path"] == source_path)
        self.assertEqual(source["blob_sha"], source_object["sha"])
        self.assertEqual(source["source_commit"], CURRENT_DESKTOP_COMMIT)
        self.assertEqual(source["source_mode"], "100644")
        self.assertEqual(source["source_type"], "blob")
        self.assertEqual(source["mapping_quality"], "semantic")
        self.assertEqual(
            source["symbols"],
            ["STRUCTURED_LOG_REPLAY_MAX_AGE_MS", "STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS"],
        )
        self.assertIn("source/packages/constants/src/structured_log.rs", source["android_targets"])
        self.assertIn("source/host/src/android_structured_log_spill.rs", source["android_targets"])
        self.assertTrue(source["focused_tests"])
        self.assertIn("17-hour", source["source_owner"])
        self.assertIn("AndroidStructuredLogSpill", source["android_owner"])

    def test_constants_structured_log_contract_is_enforced_by_host_and_exact_head_workflow(self):
        host = (ROOT / "source" / "host" / "src" / "android_structured_log_spill.rs").read_text(encoding="utf-8")
        workflow = (ROOT / ".github" / "workflows" / "grok-android-architecture.yml").read_text(encoding="utf-8")
        self.assertIn(
            "fabushi_constants::structured_log::STRUCTURED_LOG_REPLAY_MAX_AGE_MS",
            host,
        )
        self.assertIn(
            "fabushi_constants::structured_log::STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS",
            host,
        )
        self.assertIn(
            "fabushi_constants::structured_log::structured_log_timestamp_allowed",
            host,
        )
        self.assertNotIn(
            "pub const ANDROID_STRUCTURED_LOG_SPILL_MAX_AGE_MS: u64 = 17 * 60 * 60 * 1_000;",
            host,
        )
        self.assertIn("spill_rejects_far_future_records_under_desktop_skew_contract", host)
        self.assertIn("Verify exact-HEAD constants structured log production owner", workflow)
        self.assertIn("Test exact-HEAD constants structured log production owner", workflow)
        self.assertIn(
            "cargo test -p fabushi-constants structured_log::tests::replay_window_rejects_old_and_far_future_entries -- --exact",
            workflow,
        )
        self.assertIn(
            "cargo test -p fabushi-mahayana-host android_structured_log_spill::tests::spill_rejects_far_future_records_under_desktop_skew_contract -- --exact",
            workflow,
        )

    def test_mcp_tool_timeout_mapping_uses_desktop_policy_and_real_host_owner(self):
        module = load_generator()
        source_path = module.MCP_TOOL_TIMEOUT_SOURCE_PATH
        primary = {
            "responsibility_id": module.MCP_TOOL_TIMEOUT_RESPONSIBILITY_ID,
            "desktop_sources": [],
            "source_owner": "Desktop MCP owner",
            "android_owner": "Host MCP owner",
            "android_target": ["source/host/src/runner/mcp_meta_routed_tools.rs"],
        }
        responsibilities = padded_responsibilities(primary)
        source_object = {"sha": "f" * 40, "mode": "100644", "type": "blob"}
        module._ensure_mcp_tool_timeout_semantic_mapping(
            responsibilities,
            {source_path: source_object},
            CURRENT_DESKTOP_COMMIT,
        )
        owner = next(
            item for item in responsibilities
            if item["responsibility_id"] == module.MCP_TOOL_TIMEOUT_RESPONSIBILITY_ID
        )
        source = next(item for item in owner["desktop_sources"] if item["path"] == source_path)
        self.assertEqual(source["blob_sha"], source_object["sha"])
        self.assertEqual(source["source_commit"], CURRENT_DESKTOP_COMMIT)
        self.assertEqual(source["mapping_quality"], "semantic")
        self.assertEqual(
            source["symbols"],
            ["DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS", "LEGACY_MCP_TOOL_CALL_TIMEOUT_MS"],
        )
        self.assertIn(
            "source/packages/mcp-core/src/config/mcp_tool_call_timeout.rs",
            source["android_targets"],
        )
        self.assertIn("source/host/src/runner/mcp_meta_routed_tools.rs", source["android_targets"])
        self.assertIn("one-hour", source["source_owner"])
        self.assertIn("MCP meta-tool", source["android_owner"])

    def test_mcp_tool_timeout_contract_is_consumed_by_host_and_exact_head_workflow(self):
        package = (
            ROOT / "source" / "packages" / "mcp-core" / "src" / "config" /
            "mcp_tool_call_timeout.rs"
        ).read_text(encoding="utf-8")
        host = (
            ROOT / "source" / "host" / "src" / "runner" / "mcp_meta_routed_tools.rs"
        ).read_text(encoding="utf-8")
        workflow = (
            ROOT / ".github" / "workflows" / "grok-android-architecture.yml"
        ).read_text(encoding="utf-8")
        host_cargo = (ROOT / "source" / "host" / "Cargo.toml").read_text(encoding="utf-8")
        self.assertIn(
            "pub const DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS: u64 = 60 * 60_000;",
            package,
        )
        self.assertIn(
            "pub const LEGACY_MCP_TOOL_CALL_TIMEOUT_MS: u64 = "
            "DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS;",
            package,
        )
        self.assertIn(
            "fabushi-android-mcp-core = { path = \"../packages/mcp-core\" }",
            host_cargo,
        )
        self.assertIn(
            "fabushi_android_mcp_core::config::mcp_tool_call_timeout::"
            "DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS",
            host,
        )
        self.assertIn(
            "pub const LONG_TOOL_TIMEOUT_MS: u64 = DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS;",
            host,
        )
        self.assertNotIn("pub const LONG_TOOL_TIMEOUT_MS: u64 = 60 * 60 * 1_000;", host)
        self.assertIn("long_timeout_tier_uses_desktop_mcp_default_contract", host)
        self.assertIn("Verify exact-HEAD MCP tool timeout production owner", workflow)
        self.assertIn("Test exact-HEAD MCP tool timeout production owner", workflow)
        self.assertIn(
            "cargo test -p fabushi-android-mcp-core "
            "config::mcp_tool_call_timeout::tests::desktop_default_and_legacy_timeouts_are_one_hour "
            "-- --exact",
            workflow,
        )
        self.assertIn(
            "cargo test -p fabushi-mahayana-host "
            "runner::mcp_meta_routed_tools::tests::long_timeout_tier_uses_desktop_mcp_default_contract "
            "-- --exact",
            workflow,
        )

    def test_current_android_provenance_prefers_android_workflow_sha_and_rejects_stale_fallback(self):
        module = load_generator()
        head = "a" * 40
        pr = {
            "number": 3,
            "head": {"sha": head, "ref": "spec/grok-bot-018-android-parity-20260922",
                     "repo": {"full_name": "fabu-android/fabushi-android"}},
            "base": {"sha": "b" * 40, "ref": "main"},
        }
        env = {
            "ANDROID_PR_HEAD_SHA": head,
            "ANDROID_WORKFLOW_SHA": head,
            "GITHUB_WORKFLOW_SHA": "c" * 40,
            "GITHUB_REPOSITORY": "fabu-android/fabushi-android",
            "ANDROID_PR_NUMBER": "3",
        }
        with patch.dict("os.environ", env, clear=True), \
             patch.object(module.subprocess, "check_output", return_value=head + "\n"), \
             patch.object(module, "get", return_value=pr):
            provenance = module.current_android_provenance()
        self.assertEqual(provenance["workflow_sha"], head)

        env.pop("ANDROID_WORKFLOW_SHA")
        with patch.dict("os.environ", env, clear=True), \
             patch.object(module.subprocess, "check_output", return_value=head + "\n"), \
             patch.object(module, "get", return_value=pr):
            with self.assertRaisesRegex(ValueError, "workflow SHA/checkout mismatch"):
                module.current_android_provenance()

    def test_package_action_evidence_requires_successful_exact_head_run_job_step_attempt_and_anchor(self):
        module = load_generator()
        android_head = "a" * 40
        target = "source/packages/agent-kv/src/serde.rs"
        run = {
            "name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "event": "pull_request",
            "repository": {"full_name": module.ANDROID_REPOSITORY},
            "head_sha": android_head, "run_attempt": 2,
            "status": "completed", "conclusion": "success",
        }
        job = {
            "id": 789, "name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME,
            "run_attempt": 2, "status": "completed", "conclusion": "success",
            "steps": [
                {"name": module.AGENT_KV_PRODUCTION_STEP, "number": 6,
                 "status": "completed", "conclusion": "success"},
                {"name": module.AGENT_KV_TEST_STEP, "number": 7,
                 "status": "completed", "conclusion": "success"},
            ],
        }
        entry = {
            "workflow_name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "workflow_path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "workflow_event": "pull_request",
            "workflow_repository": module.ANDROID_REPOSITORY,
            "workflow_run_id": 456, "run_attempt": 2, "run_conclusion": "success",
            "workflow_job_id": 789, "workflow_job_name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME,
            "workflow_job_run_attempt": 2,
            "workflow_step_name": module.AGENT_KV_TEST_STEP, "workflow_step_number": 7,
            "evidence_ref": target + "#agent-kv-focused-tests",
        }
        with patch.object(module, "get", side_effect=[run, {"jobs": [job]}]):
            self.assertTrue(module._package_action_job_passed(
                entry, android_head=android_head, tracked_android_paths={target},
                action_job_cache={},
            ))

        entry_mutations = {
            "run-attempt": {"run_attempt": 1},
            "job-attempt": {"workflow_job_run_attempt": 1},
            "workflow-id": {"workflow_id": 1},
            "workflow-path": {"workflow_path": ".github/workflows/other.yml"},
            "workflow-event": {"workflow_event": "workflow_dispatch"},
            "workflow-repository": {"workflow_repository": "other/repo"},
            "run-conclusion": {"run_conclusion": "failure"},
            "job-name": {"workflow_job_name": "other"},
            "evidence-no-anchor": {"evidence_ref": target},
            "evidence-empty-anchor": {"evidence_ref": target + "#"},
            "evidence-untracked": {"evidence_ref": "source/missing.rs#test"},
        }
        for label, mutation in entry_mutations.items():
            with self.subTest(label=label):
                mutated = {**entry, **mutation}
                with patch.object(module, "get") as get_mock:
                    self.assertFalse(module._package_action_job_passed(
                        mutated, android_head=android_head, tracked_android_paths={target},
                        action_job_cache={},
                    ))
                    get_mock.assert_not_called()

        wrong_step_number = {**entry, "workflow_step_number": 99}
        with patch.object(module, "get", side_effect=[run, {"jobs": [job]}]):
            self.assertFalse(module._package_action_job_passed(
                wrong_step_number, android_head=android_head, tracked_android_paths={target},
                action_job_cache={},
            ))

        run_mutations = {
            "name": {"name": "other"}, "workflow-id": {"workflow_id": 1},
            "path": {"path": ".github/workflows/other.yml"}, "event": {"event": "workflow_dispatch"},
            "repository": {"repository": {"full_name": "other/repo"}},
            "head": {"head_sha": "b" * 40}, "status": {"status": "queued"},
            "failure": {"conclusion": "failure"}, "cancelled": {"conclusion": "cancelled"},
            "attempt": {"run_attempt": 1},
        }
        for label, mutation in run_mutations.items():
            with self.subTest(run=label), patch.object(module, "get", return_value={**run, **mutation}):
                self.assertFalse(module._package_action_job_passed(
                    entry, android_head=android_head, tracked_android_paths={target},
                    action_job_cache={},
                ))

        for label, mutation in {
            "name": {"name": "other"}, "status": {"status": "in_progress"},
            "failure": {"conclusion": "failure"}, "cancelled": {"conclusion": "cancelled"},
            "attempt": {"run_attempt": 1},
        }.items():
            bad_job = {**job, **mutation}
            with self.subTest(job=label), patch.object(module, "get", side_effect=[run, {"jobs": [bad_job]}]):
                self.assertFalse(module._package_action_job_passed(
                    entry, android_head=android_head, tracked_android_paths={target},
                    action_job_cache={},
                ))

        for conclusion in ("failure", "cancelled", "skipped"):
            bad_job = json.loads(json.dumps(job))
            bad_job["steps"][1]["conclusion"] = conclusion
            with self.subTest(step=conclusion), patch.object(module, "get", side_effect=[run, {"jobs": [bad_job]}]):
                self.assertFalse(module._package_action_job_passed(
                    entry, android_head=android_head, tracked_android_paths={target},
                    action_job_cache={},
                ))

    def test_exact_head_ci_discovery_requires_successful_overall_dedicated_run(self):
        module = load_generator()
        android_head = "a" * 40
        success = {
            "id": 456, "name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "head_sha": android_head, "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "event": "pull_request", "repository": {"full_name": module.ANDROID_REPOSITORY},
            "status": "completed", "conclusion": "success", "run_number": 12, "run_attempt": 1,
        }
        job = {
            "id": 789, "name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME, "run_attempt": 1,
            "status": "completed", "conclusion": "success",
            "steps": [
                {"name": module.AGENT_KV_PRODUCTION_STEP, "number": 6,
                 "status": "completed", "conclusion": "success"},
                {"name": module.AGENT_KV_TEST_STEP, "number": 7,
                 "status": "completed", "conclusion": "success"},
            ],
        }
        with patch.dict("os.environ", {"GH_TOKEN": "token", "ANDROID_CI_EVIDENCE_WAIT_SECONDS": "0"}), \
             patch.object(module, "get", side_effect=[{"workflow_runs": [success]}, {"jobs": [job]}]):
            evidence = module._latest_exact_head_android_ci_evidence(android_head)
        self.assertIsNotNone(evidence)
        self.assertEqual(evidence["workflow_run_id"], 456)
        self.assertEqual(evidence["workflow_job_run_attempt"], 1)
        self.assertEqual(evidence["run_conclusion"], "success")
        self.assertEqual(evidence["production_step_number"], 6)
        self.assertEqual(evidence["test_step_number"], 7)

        for conclusion in ("failure", "cancelled", "skipped"):
            failed = {**success, "conclusion": conclusion}
            with self.subTest(conclusion=conclusion), \
                 patch.dict("os.environ", {"GH_TOKEN": "token", "ANDROID_CI_EVIDENCE_WAIT_SECONDS": "0"}), \
                 patch.object(module, "get", return_value={"workflow_runs": [failed]}):
                self.assertIsNone(module._latest_exact_head_android_ci_evidence(android_head))


    def test_exact_head_ci_discovery_retries_after_stale_terminal_snapshot(self):
        module = load_generator()
        android_head = "a" * 40
        run = {
            "id": 456, "name": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_NAME,
            "head_sha": android_head, "workflow_id": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID,
            "path": module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_PATH,
            "event": "pull_request", "repository": {"full_name": module.ANDROID_REPOSITORY},
            "status": "completed", "conclusion": "success", "run_number": 12, "run_attempt": 1,
        }
        job = {
            "id": 789, "name": module.ANDROID_AGENT_KV_EVIDENCE_JOB_NAME, "run_attempt": 1,
            "status": "completed", "conclusion": "success",
            "steps": [
                {"name": module.AGENT_KV_PRODUCTION_STEP,
                 "number": 6, "status": "completed", "conclusion": "success"},
                {"name": module.AGENT_KV_TEST_STEP,
                 "number": 7, "status": "completed", "conclusion": "success"},
            ],
        }
        with patch.dict("os.environ", {"GH_TOKEN": "token", "ANDROID_CI_EVIDENCE_WAIT_SECONDS": "60"}):
            with patch.object(module, "get", side_effect=[
                {"workflow_runs": [run]}, {"jobs": []},
                {"workflow_runs": [run]}, {"jobs": [job]},
            ]), patch.object(module.time, "monotonic", side_effect=[0, 0, 1]):
                with patch.object(module.time, "sleep"):
                    evidence = module._latest_exact_head_android_ci_evidence(android_head)
        self.assertIsNotNone(evidence)
        self.assertEqual(evidence["workflow_run_id"], 456)
        self.assertEqual(evidence["production_step_number"], 6)
        self.assertEqual(evidence["test_step_number"], 7)
        self.assertEqual(evidence["workflow_id"], module.ANDROID_AGENT_KV_EVIDENCE_WORKFLOW_ID)

    def _exact_head_provenance_fixture(self, row_links, mapping_quality="semantic"):
        module = load_generator()
        android_head = "a" * 40
        desktop_commit = "b" * 40
        desktop_tree = "c" * 40
        source_blob = "d" * 40
        source_path = "source/example.ts"
        run_id = 987654
        source = {
            "path": source_path,
            "source_commit": desktop_commit,
            "blob_sha": source_blob,
            "source_mode": "100644",
            "source_type": "blob",
            "mapping_quality": mapping_quality,
        }
        row = {
            "source_path": source_path,
            "source_commit": desktop_commit,
            "source_blob_sha": source_blob,
            "source_mode": "100644",
            "source_type": "blob",
            "responsibilities": list(row_links),
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "android_pr_head_sha": android_head,
            "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head,
            "evidence": [{
                "source_path": source_path,
                "source_blob_sha": source_blob,
                "desktop_commit": desktop_commit,
                "desktop_root_tree": desktop_tree,
                "source_commit": desktop_commit,
                "android_head": android_head,
                "android_pr_head_sha": android_head,
                "android_tested_checkout_sha": android_head,
                "workflow_sha": android_head,
                "workflow_run_id": run_id,
            }],
        }
        responsibilities = [{
            "responsibility_id": "RESP-001",
            "source_commit": desktop_commit,
            "android_head_at_update": android_head,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [source],
        }]
        responsibilities.extend({
            "responsibility_id": f"RESP-{index:03d}",
            "source_commit": desktop_commit,
            "android_head_at_update": android_head,
            "implementation_status": "unreviewed",
            "verification_status": "not-verified",
            "desktop_sources": [],
        } for index in range(2, 307))
        manifest = {
            "desktop_commit": desktop_commit,
            "source_commit": desktop_commit,
            "desktop_root_tree": desktop_tree,
            "workflow_run_id": run_id,
            "android_pr_head_sha": android_head,
            "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head,
        }
        responsibility_doc = {
            "source_commit": desktop_commit,
            "android_pr_head_sha": android_head,
            "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head,
            "authority": {
                "desktop_commit": desktop_commit,
                "desktop_root_tree": desktop_tree,
            },
            "responsibilities": responsibilities,
        }
        return module, manifest, row, responsibility_doc, android_head

    def test_exact_head_provenance_accepts_bidirectional_semantic_responsibility_links(self):
        module, manifest, row, responsibility_doc, android_head = self._exact_head_provenance_fixture(["RESP-001"])
        module.validate_exact_head_provenance(manifest, [row], responsibility_doc, android_head)

    def test_exact_head_provenance_rejects_missing_or_extra_reverse_responsibility_links(self):
        for row_links in ([], ["RESP-002"], ["RESP-001", "RESP-002"]):
            with self.subTest(row_links=row_links):
                module, manifest, row, responsibility_doc, android_head = self._exact_head_provenance_fixture(row_links)
                with self.assertRaisesRegex(ValueError, "responsibility links disagree"):
                    module.validate_exact_head_provenance(manifest, [row], responsibility_doc, android_head)

    def test_exact_head_provenance_allows_explicit_coverage_only_anchors_without_reverse_links(self):
        module, manifest, row, responsibility_doc, android_head = self._exact_head_provenance_fixture([], "coverage-only")
        module.validate_exact_head_provenance(manifest, [row], responsibility_doc, android_head)

if __name__ == "__main__":
    unittest.main()
