from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / "source" / "host" / "src" / "android_json_runtime.rs"
BACKEND = ROOT / "source" / "host" / "src" / "mcp_auth" / "dashboard_mcp_auth_backend.rs"


class AndroidStructuredLogShippingContractTest(unittest.TestCase):
    def test_durable_record_immediately_kicks_single_flight_shipper(self) -> None:
        runtime = RUNTIME.read_text(encoding="utf-8")
        record = re.search(
            r"fn record_structured_log\([\s\S]*?\n    \}\n\n    fn rotate_structured_log_account",
            runtime,
        )
        self.assertIsNotNone(record)
        body = record.group(0)
        self.assertIn(".record(level, message, metadata, timestamp_ms)", body)
        self.assertIn("self.schedule_structured_log_ship();", body)
        self.assertLess(
            body.index(".record(level, message, metadata, timestamp_ms)"),
            body.index("self.schedule_structured_log_ship();"),
        )

        ship = re.search(
            r"fn schedule_structured_log_ship\(&self\) \{[\s\S]*?\n    \}\n\n    fn schedule_account_telemetry_ship",
            runtime,
        )
        self.assertIsNotNone(ship)
        body = ship.group(0)
        for anchor in (
            "compare_exchange(false, true",
            "current_turn_account_fence()",
            "owner.account_slot() == account_slot",
            "backend.submit_structured_logs(&batch)",
            "owner.settle_success(processed, dropped",
            "owner.mark_ship_failed",
        ):
            self.assertIn(anchor, body)

        # Never drain before a backend receipt. The successful settlement must occur after submit.
        self.assertLess(
            body.index("backend.submit_structured_logs(&batch)"),
            body.index("owner.settle_success(processed, dropped"),
        )

    def test_backend_uses_desktop_submit_logs_wire_and_exact_receipt_settlement(self) -> None:
        backend = BACKEND.read_text(encoding="utf-8")
        self.assertIn(
            'pub const ANALYTICS_SUBMIT_LOGS_PATH: &str =\n    "/aiserver.v1.AnalyticsService/SubmitLogs";',
            backend,
        )
        method = re.search(
            r"pub fn submit_structured_logs\([\s\S]*?\n    \}\n\n    pub fn list_server_snapshots",
            backend,
        )
        self.assertIsNotNone(method)
        body = method.group(0)
        for anchor in (
            "encode_structured_logs_submit_request",
            "ANALYTICS_SUBMIT_LOGS_PATH",
            "decode_submit_logs_response",
            "receipt.logs_processed.saturating_add(receipt.logs_dropped) != expected",
        ):
            self.assertIn(anchor, body)


if __name__ == "__main__":
    unittest.main()
