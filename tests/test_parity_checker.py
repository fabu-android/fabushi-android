import importlib.util
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts/check_grok_android_parity.py"
SPEC = importlib.util.spec_from_file_location("check_grok_android_parity", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)

class ParityCheckerTest(unittest.TestCase):
    def test_phase0_inventory_is_complete(self):
        result = MODULE.run_checks(strict=False)
        self.assertEqual([], result.errors)
        self.assertEqual(2046, result.summary["inventory_files"])
        self.assertEqual(2046, result.summary["ledger_rows"])
        self.assertEqual(0, result.summary["legacy_monoliths_present"])
        self.assertEqual(0, result.summary["legacy_android_product_files"])
        self.assertEqual(0, result.summary["presentation_runtime_bypasses"])
        self.assertEqual(0, result.summary["presentation_host_bypasses"])
        self.assertEqual(0, result.summary["frontend_android_main_dependencies"])
        self.assertEqual(0, result.summary["presentation_feature_receive_bypasses"])
        self.assertEqual(0, result.summary["native_host_bridge_missing"])
        self.assertTrue(result.summary["native_host_ci_wired"])
        self.assertTrue(result.summary["release_ci_account_session_import_disabled"])
        self.assertTrue(result.summary["ci_acceptance_session_import_enabled"])

    def test_strict_gate_reports_remaining_real_migration_work(self):
        result = MODULE.run_checks(strict=True)
        self.assertGreater(len(result.errors), 0)
        self.assertEqual(0, result.summary["legacy_monoliths_present"])
        self.assertEqual(0, result.summary["presentation_runtime_bypasses"])
        self.assertEqual(0, result.summary["frontend_android_main_dependencies"])
        self.assertEqual(0, result.summary["architecture_scope_markers"])
        self.assertGreater(result.summary["status_counts"].get("mapped", 0), 0)

if __name__ == "__main__":
    unittest.main()
