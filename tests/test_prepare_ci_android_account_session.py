import importlib.util
import pathlib
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts/prepare_ci_android_account_session.py"
SPEC = importlib.util.spec_from_file_location("prepare_ci_android_account_session", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class PrepareCiAndroidAccountSessionTest(unittest.TestCase):
    def source(self, now=1_000_000):
        return {
            "accessToken": "a" * 40,
            "refreshToken": "r" * 40,
            "tokenType": "Bearer",
            "accessTokenExpiresAt": now + 3600,
            "refreshTokenExpiresAt": now + 86400,
            "sessionId": "ordinary-session",
            "deviceId": "gha-12345-2-interactive",
            "username": "ci-user",
            "userId": "user-1",
            "user": {"id": "user-1", "username": "ci-user"},
            "provider": "official",
            "ciRunner": False,
        }

    def test_exports_bound_refresh_token_free_session(self):
        exported = MODULE.bounded_session(
            self.source(),
            device_id="gha-12345-2-interactive",
            run_id="12345",
            run_attempt="2",
            now=1_000_000,
        )
        self.assertEqual("ci-runner:12345:2", exported["sessionId"])
        self.assertEqual("github-actions", exported["provider"])
        self.assertTrue(exported["ciRunner"])
        self.assertNotIn("refreshToken", exported)
        self.assertEqual("a" * 40, exported["accessToken"])

    def test_rejects_identity_mismatch_and_unbounded_expiry(self):
        with self.assertRaises(ValueError):
            MODULE.bounded_session(
                self.source(),
                device_id="gha-12345-3-interactive",
                run_id="12345",
                run_attempt="2",
                now=1_000_000,
            )
        source = self.source()
        source["accessTokenExpiresAt"] = 1_000_000 + MODULE.MAX_LIFETIME_SECONDS + 1
        with self.assertRaises(ValueError):
            MODULE.bounded_session(
                source,
                device_id="gha-12345-2-interactive",
                run_id="12345",
                run_attempt="2",
                now=1_000_000,
            )

    def test_private_writer_uses_owner_only_permissions(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "private" / "session.json"
            MODULE.atomic_private_write(path, {"accessToken": "a" * 40})
            self.assertEqual(0, path.stat().st_mode & 0o077)

    def test_login_http_error_reports_only_server_reason(self):
        import io
        import urllib.error
        from unittest import mock

        body = b'{"error":{"code":"policy_blocked","message":"CI login denied"}}'
        error = urllib.error.HTTPError(
            "https://example.invalid/api/auth/login",
            403,
            "Forbidden",
            {},
            io.BytesIO(body),
        )
        with mock.patch.object(MODULE.urllib.request, "urlopen", side_effect=error):
            with self.assertRaisesRegex(
                RuntimeError,
                r"HTTP 403: policy_blocked CI login denied",
            ) as raised:
                MODULE.login(
                    base_url="https://example.invalid",
                    username="secret-user",
                    password="secret-password",
                    device_id="gha-12345-2-interactive",
                )
        message = str(raised.exception)
        self.assertNotIn("secret-user", message)
        self.assertNotIn("secret-password", message)

    def test_ci_auth_origin_is_separate_and_https_only(self):
        self.assertEqual(
            "https://issuer.example",
            MODULE.ci_auth_base_url({
                "FABUSHI_CI_AUTH_BASE_URL": "https://issuer.example/",
                "FABUSHI_API_BASE_URL": "https://app.example",
            }),
        )
        self.assertEqual(
            "https://app.example",
            MODULE.ci_auth_base_url({"FABUSHI_API_BASE_URL": "https://app.example"}),
        )
        self.assertEqual(
            "https://mahayana-platform.bhrumom.workers.dev",
            MODULE.ci_auth_base_url({}),
        )
        with self.assertRaises(ValueError):
            MODULE.ci_auth_base_url({"FABUSHI_CI_AUTH_BASE_URL": "http://issuer.example"})


if __name__ == "__main__":
    unittest.main()
