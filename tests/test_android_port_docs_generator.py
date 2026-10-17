import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "android-port-docs.py"
CURRENT_DESKTOP_COMMIT = "dc81eac2df50f6c08ae4daf98988076bfdc956d9"
CURRENT_DESKTOP_TREE = "04473e188c5088b66a7f984e5e89e6d47e75eafb"



def load_generator()
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
                    "artifact_id": None, "artifact_digest": None,
                    "source_manifest_run_id": 456, "source_manifest_android_head": android_head,
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
                              {"artifact_id": None, "artifact_digest": None,
                               "source_manifest_run_id": 456, "source_manifest_android_head": android_head}),
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
            with patch.dict("os.environ",{"GH_TOKEN":"token","ANDROID_RESPONSIBILITY_PREDECESSOR_ARTIFACT_DIGEST":"sha256:"+"0"*64}),patch.object(module,"_download_predecessor_bytes",return_value=b"PK\\x03\\x04invalid"):
                with self.assertRaisesRegex(ValueError,"digest mismatch"):
                    module.load_predecessor_responsibility_mapping(path)

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
            "workflow_run_id": 456, "workflow_job_id": 789, "workflow_job_name": "android-build-unit-lint",
            "workflow_step_name": "Build exact-HEAD native Mahayana Coordinator Host",
            "workflow_name": "Android Parity Full CI", "result": "passed",
        } for path in paths]
        test_evidence = [{
            "kind": "contract-test", "source_path": path,
            "source_commit": CURRENT_DESKTOP_COMMIT, "desktop_commit": CURRENT_DESKTOP_COMMIT,
            "android_pr_head_sha": android_head, "android_tested_checkout_sha": android_head,
            "workflow_sha": android_head, "evidence_ref": test_target + "#test",
            "workflow_run_id": 456, "workflow_job_id": 789, "workflow_job_name": "android-build-unit-lint",
            "workflow_step_name": "Compile, unit-test, lint, and assemble non-release acceptance APKs",
            "workflow_name": "Android Parity Full CI", "result": "passed",
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
                    return {"name": "Android Parity Full CI",
                            "repository": {"full_name": "fabu-android/fabushi-android"},
                            "head_sha": android_head, "status": "completed", "conclusion": run_conclusion}
                if endpoint == "/repos/fabu-android/fabushi-android/actions/runs/456/jobs?per_page=100":
                    return {"jobs": [{
                        "id": 789, "name": "android-build-unit-lint",
                        "status": "completed", "conclusion": "success",
                        "steps": [
                            {"name": "Build exact-HEAD native Mahayana Coordinator Host",
                             "status": "completed", "conclusion": production_conclusion},
                            {"name": "Compile, unit-test, lint, and assemble non-release acceptance APKs",
                             "status": "completed", "conclusion": test_conclusion},
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
        self.assertEqual(successful_steps_in_failed_run["strict_incomplete_rows"], 0)
        self.assertEqual(successful_steps_in_failed_run["status"], "passed")

        with patch.object(module, "get", side_effect=github_api("cancelled", "success", "success")):
            cancelled_run = module._package_semantic_coverage(
                json.loads(json.dumps(complete_rows)), padded_responsibilities(responsibility), tracked
            )
        self.assertEqual(cancelled_run["strict_incomplete_rows"], 852)
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

    def test_workflow_persists_generated_package_coverage_and_diagnostics(self):
        workflow = (ROOT / '.github' / 'workflows' / 'android-port-docs.yml').read_text(encoding='utf-8')
        generator = SCRIPT.read_text(encoding='utf-8')
        self.assertIn("save(OUT / 'packages-semantic-coverage.json', package_coverage)", generator)
        self.assertIn('docs/android-port/generated/packages-semantic-coverage.json', workflow)
        self.assertIn('docs/android-port/generated/rejected-android-targets.json', workflow)
        self.assertIn("assert coverage['complete_rows'] + coverage['strict_incomplete_rows'] == 852", workflow)
        push_trigger = workflow.split('  workflow_dispatch:', 1)[0]
        self.assertIn('  push:', push_trigger)
        self.assertIn("'spec/grok-bot-018-android-parity-20260922'", push_trigger)
        self.assertIn('      - \'source/**\'', push_trigger)
        self.assertIn('      - \'native/**\'', push_trigger)
        self.assertIn('      - \'tests/**\'', push_trigger)
        self.assertNotIn('      - \'docs/android-port/**\'', push_trigger,
                         'the generated-doc commit must not trigger an endless persistence loop')


    def test_artifact_only_docs_run_does_not_create_persistence_loop(self):
        workflow = (ROOT / ".github" / "workflows" / "android-port-docs.yml").read_text(encoding="utf-8")
        self.assertIn("group: android-port-docs-${{ github.ref_name }}", workflow)
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

    def test_package_action_evidence_requires_a_successful_exact_head_job_and_named_step(self):
        module = load_generator()
        android_head = "a" * 40
        target = "source/packages/agent-kv/src/serde.rs"
        run = {
            "name": "Android Parity Full CI",
            "repository": {"full_name": "fabu-android/fabushi-android"},
            "head_sha": android_head,
            "status": "completed",
            "conclusion": "failure",
        }
        job = {
            "id": 789,
            "name": "android-build-unit-lint",
            "status": "completed",
            "conclusion": "success",
            "steps": [
                {"name": "Build exact-HEAD native Mahayana Coordinator Host",
                 "status": "completed", "conclusion": "success"},
                {"name": "Compile, unit-test, lint, and assemble non-release acceptance APKs",
                 "status": "completed", "conclusion": "success"},
            ],
        }
        entry = {
            "workflow_run_id": 456,
            "workflow_job_id": 789,
            "workflow_job_name": "android-build-unit-lint",
            "workflow_step_name": "Compile, unit-test, lint, and assemble non-release acceptance APKs",
            "evidence_ref": target,
        }
        with patch.object(module, "get", side_effect=[run, {"jobs": [job]}]):
            self.assertTrue(module._package_action_job_passed(
                entry, android_head=android_head, tracked_android_paths={target},
                action_job_cache={},
            ))
        bad_step = json.loads(json.dumps(job))
        bad_step["steps"][1]["conclusion"] = "cancelled"
        with patch.object(module, "get", side_effect=[run, {"jobs": [bad_step]}]):
            self.assertFalse(module._package_action_job_passed(
                entry, android_head=android_head, tracked_android_paths={target},
                action_job_cache={},
            ))
        wrong_head = {**run, "head_sha": "b" * 40}
        with patch.object(module, "get", return_value=wrong_head):
            self.assertFalse(module._package_action_job_passed(
                entry, android_head=android_head, tracked_android_paths={target},
                action_job_cache={},
            ))

    def test_exact_head_ci_discovery_accepts_failed_overall_run_only_when_required_job_steps_pass(self):
        module = load_generator()
        android_head = "a" * 40
        run = {
            "id": 456, "name": "Android Parity Full CI", "head_sha": android_head,
            "status": "completed", "conclusion": "failure", "run_number": 12, "run_attempt": 1,
        }
        job = {
            "id": 789, "name": "android-build-unit-lint",
            "status": "completed", "conclusion": "success",
            "steps": [
                {"name": "Build exact-HEAD native Mahayana Coordinator Host",
                 "number": 9, "status": "completed", "conclusion": "success"},
                {"name": "Compile, unit-test, lint, and assemble non-release acceptance APKs",
                 "number": 10, "status": "completed", "conclusion": "success"},
            ],
        }
        with patch.dict("os.environ", {"GH_TOKEN": "token", "ANDROID_CI_EVIDENCE_WAIT_SECONDS": "0"}):
            with patch.object(module, "get", side_effect=[
                {"workflow_runs": [run]}, {"jobs": [job]},
            ]):
                evidence = module._latest_exact_head_android_ci_evidence(android_head)
        self.assertIsNotNone(evidence)
        self.assertEqual(evidence["workflow_run_id"], 456)
        self.assertEqual(evidence["workflow_job_id"], 789)
        self.assertEqual(evidence["run_conclusion"], "failure")
        self.assertEqual(evidence["production_step_number"], 9)
        self.assertEqual(evidence["test_step_number"], 10)

        cancelled = {**run, "conclusion": "cancelled"}
        with patch.dict("os.environ", {"GH_TOKEN": "token", "ANDROID_CI_EVIDENCE_WAIT_SECONDS": "0"}):
            with patch.object(module, "get", return_value={"workflow_runs": [cancelled]}):
                self.assertIsNone(module._latest_exact_head_android_ci_evidence(android_head))

if __name__ == "__main__":
    unittest.main()
