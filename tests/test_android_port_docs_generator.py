import hashlib
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


def padded_responsibilities(primary):
    return [primary] + [
        {"responsibility_id": f"RESP-{index:03d}", "desktop_sources": []}
        for index in range(2, 307)
    ]


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

    def test_local_snapshot_fallback_is_digest_verified_and_not_parity_evidence(self):
        module = load_generator()
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            responsibility_path = root / "responsibility-ledger.json"
            source_commit = "d" * 40
            source_tree = "t" * 40
            rows = []
            for index in range(8258):
                rows.append({
                    "source_path": f"source/generated-{index:04d}.rs",
                    "source_commit": source_commit,
                    "source_blob_sha": f"{index + 1:040x}",
                    "source_type": "blob",
                    "source_mode": "100644",
                    "responsibilities": [],
                    "android_targets": [],
                    "disposition": None,
                    "implementation_status": "unreviewed",
                    "evidence": [],
                })
            ledger_text = "".join(
                json.dumps(row, ensure_ascii=False, sort_keys=True) + "\\n"
                for row in rows
            )
            (root / "source-inventory-ledger.jsonl").write_text(ledger_text, encoding="utf-8")
            manifest = {
                "authority_current": True,
                "desktop_commit": source_commit,
                "desktop_root_tree": source_tree,
                "tree_entries": 9412,
                "tracked_non_tree_entries": 8258,
                "ledger_sha256": hashlib.sha256(ledger_text.encode("utf-8")).hexdigest(),
                "workflow_run_id": 456,
                "android_pr_head_sha": "h" * 40,
            }
            (root / "source-inventory-manifest.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            candidate = {
                "authority": {
                    "desktop_commit": source_commit,
                    "desktop_root_tree": source_tree,
                },
                "responsibilities": padded_responsibilities({
                    "responsibility_id": "RESP-001",
                    "desktop_sources": [{
                        "path": "source/generated-0000.rs",
                        "blob_sha": rows[0]["source_blob_sha"],
                        "source_mode": "100644",
                        "source_type": "blob",
                    }],
                }),
            }
            responsibility_path.write_text(json.dumps(candidate), encoding="utf-8")
            with patch.object(
                module,
                "_load_pinned_responsibility_mapping",
                side_effect=RuntimeError(
                    "failed to recover predecessor responsibility artifact 12053467058 (HTTP 404)"
                ),
            ):
                recovered, source_rows, origin, provenance = (
                    module.load_predecessor_responsibility_mapping(responsibility_path)
                )

            self.assertEqual(origin, "current-responsibility-ledger+validated-local-coverage")
            self.assertEqual(8258, len(source_rows))
            self.assertEqual("validated-local-persisted-snapshot", provenance["mapping_source"])
            self.assertIsNone(provenance["artifact_id"])
            self.assertIsNone(provenance["artifact_digest"])
            self.assertEqual("unreviewed", source_rows["source/generated-0000.rs"]["implementation_status"])
            self.assertEqual(306, len(recovered["responsibilities"]))

    def test_local_snapshot_fallback_rejects_a_stale_ledger_digest(self):
        module = load_generator()
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            responsibility_path = root / "responsibility-ledger.json"
            responsibility_path.write_text(json.dumps({
                "authority": {"desktop_commit": "d" * 40, "desktop_root_tree": "t" * 40},
                "responsibilities": padded_responsibilities({"responsibility_id": "RESP-001"}),
            }), encoding="utf-8")
            (root / "source-inventory-ledger.jsonl").write_text("", encoding="utf-8")
            (root / "source-inventory-manifest.json").write_text(json.dumps({
                "authority_current": True,
                "desktop_commit": "d" * 40,
                "desktop_root_tree": "t" * 40,
                "tree_entries": 9412,
                "tracked_non_tree_entries": 8258,
                "ledger_sha256": "0" * 64,
            }), encoding="utf-8")
            with patch.object(
                module,
                "_load_pinned_responsibility_mapping",
                side_effect=RuntimeError("failed to recover predecessor responsibility artifact 12053467058"),
            ):
                with self.assertRaisesRegex(ValueError, "ledger digest"):
                    module.load_predecessor_responsibility_mapping(responsibility_path)


if __name__ == "__main__":
    unittest.main()
