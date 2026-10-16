import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "android-port-docs.py"


def load_generator():
    spec = importlib.util.spec_from_file_location("android_port_docs", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class AndroidPortDocsGeneratorTest(unittest.TestCase):
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

    def test_merge_preserves_unmapped_rows_without_fabricating_closure(self):
        module = load_generator()
        with tempfile.TemporaryDirectory() as temp:
            module.OUT = Path(temp)
            predecessor = {
                "authority": {
                    "desktop_commit": "d" * 40,
                    "desktop_root_tree": "t" * 40,
                },
                "responsibilities": [
                    {
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
                    }
                ],
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
                                             "source_manifest_run_id": 1, "source_manifest_android_head": "h" * 40})),                  patch.object(module, "current_android_provenance", return_value=provenance):
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
                "responsibilities": [
                    {
                        "responsibility_id": "RESP-001",
                        "source_owner": "canonical",
                        "android_owner": "Rust Host",
                        "android_target": ["source/host/src/lib.rs"],
                        "desktop_sources": [
                            {"path": "source/host/src/lib.rs", "blob_sha": "old",
                             "symbols": ["symbol"], "source_mode": "100644", "source_type": "blob"}
                        ],
                    }
                ],
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
                                            {"artifact_id": None, "artifact_digest": None,
                                             "source_manifest_run_id": None, "source_manifest_android_head": None})),                  patch.object(module, "current_android_provenance", return_value=provenance):
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


if __name__ == "__main__":
    unittest.main()
