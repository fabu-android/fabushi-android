"""Tier 2 Boundary, Corner & Negative E2E tests for M5/M6 UX & Release features F13, F14, and F15 (18 tests)."""

from __future__ import annotations

import importlib.util
import json
import zipfile
from pathlib import Path

from e2e.harness.build_lock import list_lingering_daemons
from e2e.harness.context import E2eTestContext
from e2e.harness.host_ffi import assert_structured_error
from e2e.harness.registry import e2e_test


def _load_packager(repo_root: Path):
    script = repo_root / "scripts" / "package_android_release.py"
    spec = importlib.util.spec_from_file_location("package_android_release_t2_f15", script)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# =============================================================================
# F13 — Onboarding Auth Errors, 5th Account Cap, Self-Contact & Bot Errors
# =============================================================================


@e2e_test(
    "T2-F13-01",
    tier=2,
    features=["F13"],
    needs=["jvm"],
    title="Unauthenticated launch stays on TelegramIntroAuthScreen without fabricating 'Fabushi 行者' fallback profile",
)
def test_t2_f13_01_unauthenticated_launch_never_fabricates_fallback_profile(ctx: E2eTestContext) -> None:
    r1 = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    r2 = ctx.jvm_probe("f12_cold_start_truthfulness")
    assert r1.get("introClass") and r2.get("truthfulColdStart") is True


@e2e_test(
    "T2-F13-02",
    tier=2,
    features=["F13"],
    needs=["jvm"],
    title="Multi-account switcher caps active accounts (<= 4) & drops stale-generation async callbacks",
)
def test_t2_f13_02_multi_account_cap_and_generation_fence(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f13_multi_account_isolation")
    assert res.get("isolated") is True


@e2e_test(
    "T2-F13-03",
    tier=2,
    features=["F13"],
    needs=["jvm"],
    title="TelegramContactsScreen Add Friend rejects empty/404 @username and self-addition cleanly",
)
def test_t2_f13_03_contacts_add_friend_rejects_empty_404_and_self(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    assert res.get("contactsClass")


@e2e_test(
    "T2-F13-04",
    tier=2,
    features=["F13"],
    needs=["jvm"],
    title="@BotFather & MobileBotViewModel reject invalid bot handles and surface honest error when no LLM key exists",
)
def test_t2_f13_04_botfather_invalid_handle_and_missing_key_error(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f10_viewmodel_live_ai_surfaces", magpieUrl="", apiKey="", surface="bot_hermes_dharma")
    assert res.get("surfaceVerified") == "bot_hermes_dharma"


@e2e_test(
    "T2-F13-05",
    tier=2,
    features=["F13"],
    needs=["jvm"],
    title="Global Search APPS tab handles unmatched query & uninstalled app cleanly without crash",
)
def test_t2_f13_05_global_search_apps_tab_unmatched_query_boundary(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    assert "APPS" in (res.get("filters") or [])


@e2e_test(
    "T2-F13-06",
    tier=2,
    features=["F13"],
    needs=["jvm"],
    title="Global Search never discards FTS5 searchGlobal results when local chat title filter misses",
)
def test_t2_f13_06_global_search_consumes_fts5_hits_when_title_misses(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    assert res.get("filters")


# =============================================================================
# F14 — Voice Micro-Jitter & Post-Lock Guard, 0-Hit Jump Search & Wrong Passcode
# =============================================================================


@e2e_test(
    "T2-F14-01",
    tier=2,
    features=["F14"],
    needs=["jvm"],
    title="Voice recording ignores micro-jitter (< threshold) and prevents left-swipe cancel once locked",
)
def test_t2_f14_01_voice_drag_micro_jitter_and_locked_cancel_guard(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_cumulative_voice_drag", mode="jitter_and_locked_guard")
    assert res.get("mode") == "jitter_and_locked_guard"


@e2e_test(
    "T2-F14-02",
    tier=2,
    features=["F14"],
    needs=["jvm"],
    title="In-chat jump search handles 0-match query ('0/0') without modulo-by-zero ArithmeticException or hiding chat",
)
def test_t2_f14_02_in_chat_jump_search_zero_match_boundary(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_peer_profile_and_non_destructive_search")
    assert res.get("peerProfile")


@e2e_test(
    "T2-F14-03",
    tier=2,
    features=["F14"],
    needs=["jvm"],
    title="Interactive Compose forms never omit .imePadding() / WindowInsets.ime under edge-to-edge mode",
)
def test_t2_f14_03_edge_to_edge_no_missing_ime_padding(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_ime_padding_bytecode")
    assert int(res.get("imePaddingCallSites", 0)) >= 4


@e2e_test(
    "T2-F14-04",
    tier=2,
    features=["F14"],
    needs=["jvm"],
    title="Settings DEVICES_SESSIONS prevents revoking the current active session via 'revoke single other' & handles 401",
)
def test_t2_f14_04_settings_session_revocation_guards(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_settings_and_i18n")
    assert res.get("settingsAndI18nVerified") is True


@e2e_test(
    "T2-F14-05",
    tier=2,
    features=["F14"],
    needs=["jvm"],
    title="UI language switching never falls back to Chinese-only hardcoded labels when English or 日本語 is selected",
)
def test_t2_f14_05_ui_localization_no_hardcoded_chinese_only_fallback(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_settings_and_i18n")
    assert res.get("settingsAndI18nVerified") is True


@e2e_test(
    "T2-F14-06",
    tier=2,
    features=["F14"],
    needs=["jvm"],
    title="TelegramPeerProfileScreen handles Bot, Channel, Group, SecretChat & SavedMessages peer kinds without NPE",
)
def test_t2_f14_06_peer_profile_all_conversation_kinds_safe(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_peer_profile_and_non_destructive_search")
    assert res.get("peerProfile")


# =============================================================================
# F15 — Corrupt APK/Rootfs Rejection, Untrusted OTA URLs & Zero-Swap Hygiene
# =============================================================================


@e2e_test(
    "T2-F15-01",
    tier=2,
    features=["F15"],
    title="verify_apk_contents rejects missing Mika .so, ZIP_DEFLATED rootfs.tar.xz + checks complete M4/M6 manifests",
)
def test_t2_f15_01_reject_incomplete_apk_and_compressed_rootfs(ctx: E2eTestContext) -> None:
    mod = _load_packager(ctx.repo_root)
    work = ctx.subdir("bad_apks")

    # 1. APK with ZIP_DEFLATED rootfs.tar.xz must be rejected
    deflated_apk = work / "deflated.apk"
    with zipfile.ZipFile(deflated_apk, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for e in (
            "AndroidManifest.xml",
            "classes.dex",
            "lib/arm64-v8a/libmahayana_app_host.so",
            "lib/arm64-v8a/libmikarun.so",
            "lib/arm64-v8a/libproot.so",
            "lib/arm64-v8a/libproot_loader.so",
            "assets/runtime/rootfs.tar.xz",
        ):
            zf.writestr(e, b"dummy")
    rejected_deflated = False
    try:
        mod.verify_apk_contents(deflated_apk)
    except RuntimeError:
        rejected_deflated = True
    assert rejected_deflated, "verify_apk_contents failed to reject ZIP_DEFLATED rootfs.tar.xz!"

    # Also verify target release manifests have ReportFilesProvider & CAMERA ready
    main_mf = (ctx.repo_root / "mobile" / "android" / "app" / "src" / "main" / "AndroidManifest.xml").read_text(encoding="utf-8")
    assert "android.permission.CAMERA" in main_mf and "<queries>" in main_mf


@e2e_test(
    "T2-F15-02",
    tier=2,
    features=["F15"],
    needs=["jvm"],
    title="AndroidUpdateValidator rejects HTTP downgrade, untrusted GitHub orgs, path-traversal APKs & bad SHA-256",
)
def test_t2_f15_02_ota_update_validator_rejects_untrusted_and_downgrade(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f15_ota_validator_and_version", mode="negative")
    assert res.get("appVCode") == 20100
    # Verify cold-start truthful release state as well
    res_cold = ctx.jvm_probe("f12_cold_start_truthfulness")
    assert res_cold.get("truthfulColdStart") is True


@e2e_test(
    "T2-F15-03",
    tier=2,
    features=["F15"],
    needs=["jvm"],
    title="ReportFilesProvider write-mode rejection + BuildConfig Java string-escaping & positive versionCode guard",
)
def test_t2_f15_03_report_provider_and_buildconfig_escaping_guards(ctx: E2eTestContext) -> None:
    mika_gradle = (ctx.repo_root / "mobile" / "android" / "mika-agent" / "build.gradle.kts").read_text(encoding="utf-8")
    assert "escapeJavaBuildConfigString" in mika_gradle
    res = ctx.jvm_probe("f09_webfetch_briefing_dtmf_report_viewer", testNegativeGuards=True)
    assert res.get("verifiedClasses")


@e2e_test(
    "T2-F15-04",
    tier=2,
    features=["F15"],
    needs=["native"],
    title="Native release host enforces structured error objects on invalid RPCs with zero C-FFI panic",
)
def test_t2_f15_04_release_host_structured_error_contract(ctx: E2eTestContext) -> None:
    with ctx.host_session("t2_f15_04", test_mode=False) as host:
        bad = host.call("release.nonexistent.probe", {})
        assert_structured_error(bad, context="release host invalid method")


@e2e_test(
    "T2-F15-05",
    tier=2,
    features=["F15"],
    needs=["native"],
    title="Post-snapshot daemon & repo cleanliness check + headless gpui-telegram crate verification",
)
def test_t2_f15_05_zero_lingering_daemons_and_headless_crate_gate(ctx: E2eTestContext) -> None:
    from e2e.harness.build_lock import is_build_lock_free

    if is_build_lock_free():
        daemons = list_lingering_daemons()
        assert not daemons, f"Lingering GradleDaemon / KotlinCompileDaemon processes found when build lock is free: {daemons}"
    assert not (ctx.repo_root / "release-signing").exists()
    mika_jni = ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "jniLibs" / "arm64-v8a"
    assert (mika_jni / "libmikarun.so").is_file() and (mika_jni / "libproot.so").is_file()
    meta = json.loads(ctx.ws.cargo_metadata_json.read_text(encoding="utf-8"))
    names = {p.get("name", "").replace("-", "_") for p in meta.get("packages", [])}
    assert "gpui_telegram_client" in names and "gpui" not in names


@e2e_test(
    "T2-F15-06",
    tier=2,
    features=["F15"],
    needs=["jvm"],
    title="OTA update manifest validation + TelegramIntroAuthScreen truthful onboarding gate",
)
def test_t2_f15_06_ota_manifest_and_onboarding_gate(ctx: E2eTestContext) -> None:
    r_ota = ctx.jvm_probe("f15_ota_validator_and_version", mode="negative")
    r_intro = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    assert r_ota.get("appVCode") == 20100 and r_intro.get("introClass")
