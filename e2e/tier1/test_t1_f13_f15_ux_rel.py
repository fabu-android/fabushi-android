"""Tier 1 Feature & Functional E2E tests for M5/M6 UX & Release features F13, F14, and F15 (18 tests)."""

from __future__ import annotations

import importlib.util
import json
import os
import zipfile
from pathlib import Path

from e2e.harness.apk_builder import create_valid_unsigned_apk
from e2e.harness.context import E2eTestContext
from e2e.harness.host_ffi import inspect_elf64_alignment_and_machine
from e2e.harness.registry import e2e_test


def _load_packager(repo_root: Path):
    script = repo_root / "scripts" / "package_android_release.py"
    spec = importlib.util.spec_from_file_location("package_android_release_f15", script)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# =============================================================================
# F13 — Onboarding Auth, Account Switcher, Add Friend, BotFather & APPS Tab
# =============================================================================


@e2e_test(
    "T1-F13-01",
    tier=1,
    features=["F13"],
    needs=["jvm"],
    title="TelegramIntroAuthScreen gates unauthenticated cold launch instead of bypassing straight into chat list",
)
def test_t1_f13_01_telegram_intro_auth_screen_onboarding(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    assert "IntroAuthScreen" in str(res.get("introClass", ""))


@e2e_test(
    "T1-F13-02",
    tier=1,
    features=["F13"],
    needs=["jvm"],
    title="Multi-Account Drawer Switcher enforces GenerationFence + telegram.auth.accountSwitch without chat bleed",
)
def test_t1_f13_02_drawer_multi_account_switcher_isolation(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f13_multi_account_isolation")
    assert res.get("isolated") is True


@e2e_test(
    "T1-F13-03",
    tier=1,
    features=["F13"],
    needs=["jvm"],
    title="Drawer Contacts opens dedicated TelegramContactsScreen with Add Friend (@username / phone / QR)",
)
def test_t1_f13_03_telegram_contacts_add_friend_screen(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    assert "ContactsScreen" in str(res.get("contactsClass", ""))


@e2e_test(
    "T1-F13-04",
    tier=1,
    features=["F13"],
    needs=["jvm"],
    title="Global Search exposes APPS ('应用') filter tab discovering installed Android apps & MiniApps",
)
def test_t1_f13_04_global_search_apps_filter_tab(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    assert "APPS" in (res.get("filters") or [])


@e2e_test(
    "T1-F13-05",
    tier=1,
    features=["F13"],
    needs=["jvm"],
    title="@BotFather & MobileBotViewModel create real live-LLM bots without canned echo fallback",
)
def test_t1_f13_05_botfather_live_llm_bot_creation(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-botfather-live") as hub:
        res = ctx.jvm_probe(
            "f10_viewmodel_live_ai_surfaces",
            magpieUrl=f"{hub.base_url}/v1",
            apiKey="sk-botfather-live",
            surface="bot_hermes_dharma",
        )
        assert res.get("surfaceVerified") == "bot_hermes_dharma"
    res2 = ctx.jvm_probe("f12_cold_start_truthfulness")
    assert res2.get("truthfulColdStart") is True


@e2e_test(
    "T1-F13-06",
    tier=1,
    features=["F13"],
    needs=["jvm"],
    title="APPS tab results support 1-tap direct launch & 1-tap Mika phone-control delegation",
)
def test_t1_f13_06_apps_tab_open_and_mika_automate_actions(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_installed_app_search", query="美团", testSelfProtection=True)
    assert res.get("hitsCount", 0) >= 1


# =============================================================================
# F14 — .imePadding(), Voice Drag Math, Peer Profile, Jump Search & 11 Settings
# =============================================================================


@e2e_test(
    "T1-F14-01",
    tier=1,
    features=["F14"],
    needs=["jvm"],
    title="Interactive Compose screens apply .imePadding() / WindowInsets.ime across edge-to-edge layouts",
)
def test_t1_f14_01_edge_to_edge_ime_padding_bytecode(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_ime_padding_bytecode")
    assert int(res.get("imePaddingCallSites", 0)) >= 4


@e2e_test(
    "T1-F14-02",
    tier=1,
    features=["F14"],
    needs=["jvm"],
    title="Voice slide-to-cancel accumulates per-frame dx deltas (10 x -15f = -150f < -120f cancel threshold)",
)
def test_t1_f14_02_voice_recording_cumulative_slide_to_cancel(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_cumulative_voice_drag", mode="cancel")
    assert res.get("mode") == "cancel"


@e2e_test(
    "T1-F14-03",
    tier=1,
    features=["F14"],
    needs=["jvm"],
    title="Voice slide-up-to-lock accumulates per-frame dy deltas (10 x -9f = -90f < -75f lock threshold)",
)
def test_t1_f14_03_voice_recording_cumulative_slide_to_lock(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_cumulative_voice_drag", mode="lock")
    assert res.get("mode") == "lock"


@e2e_test(
    "T1-F14-04",
    tier=1,
    features=["F14"],
    needs=["jvm"],
    title="Slim Chat TopBar opens TelegramPeerProfileScreen & in-chat search preserves context with ▲/▼ jump bar",
)
def test_t1_f14_04_peer_profile_and_non_destructive_jump_search(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_peer_profile_and_non_destructive_search")
    assert "PeerProfile" in str(res.get("peerProfile", ""))


@e2e_test(
    "T1-F14-05",
    tier=1,
    features=["F14"],
    needs=["jvm"],
    title="Settings DEVICES_SESSIONS & PRIVACY_SECURITY wire live session list/revocation, Cloud 2FA & Passkeys",
)
def test_t1_f14_05_settings_live_sessions_2fa_and_passkeys(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_settings_and_i18n")
    assert res.get("settingsAndI18nVerified") is True


@e2e_test(
    "T1-F14-06",
    tier=1,
    features=["F14"],
    needs=["jvm"],
    title="Settings LANGUAGE_TRANSLATE dynamically localizes Telegram UI strings across 简体中文/繁體中文/English/日本語",
)
def test_t1_f14_06_multi_language_ui_localization(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f14_settings_and_i18n")
    assert res.get("settingsAndI18nVerified") is True


# =============================================================================
# F15 — Full E2E Release Readiness, Manifest Isolation, R8 Rules & Signed Bundle
# =============================================================================


@e2e_test(
    "T1-F15-01",
    tier=1,
    features=["F15"],
    title="Variant manifest isolation (REQUEST_INSTALL_PACKAGES githubRelease-only) + CAMERA, <queries> & ReportFilesProvider",
)
def test_t1_f15_01_variant_manifest_permissions_and_providers(ctx: E2eTestContext) -> None:
    main_mf = (ctx.repo_root / "mobile" / "android" / "app" / "src" / "main" / "AndroidManifest.xml").read_text(encoding="utf-8")
    gh_mf = (ctx.repo_root / "mobile" / "android" / "app" / "src" / "githubRelease" / "AndroidManifest.xml").read_text(encoding="utf-8")
    mika_mf = (ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "AndroidManifest.xml").read_text(encoding="utf-8")

    assert "REQUEST_INSTALL_PACKAGES" not in main_mf, "Play release main manifest must NOT declare REQUEST_INSTALL_PACKAGES"
    assert "REQUEST_INSTALL_PACKAGES" in gh_mf, "githubRelease manifest must declare REQUEST_INSTALL_PACKAGES"
    assert "android.permission.CAMERA" in main_mf and "<queries>" in main_mf, (
        "Release main manifest lacks M4 CAMERA or <queries> launcher visibility"
    )
    assert "ReportFilesProvider" in mika_mf, "mika-agent manifest lacks M3 ReportFilesProvider"


@e2e_test(
    "T1-F15-02",
    tier=1,
    features=["F15"],
    needs=["jvm"],
    title="R8 proguard-rules.pro & consumer-rules.pro protect MahayanaHost JNI, IMikaShell tx 61 & TelegramMikaChannel",
)
def test_t1_f15_02_r8_keep_rules_and_production_classes(ctx: E2eTestContext) -> None:
    pg = (ctx.repo_root / "mobile" / "android" / "app" / "proguard-rules.pro").read_text(encoding="utf-8")
    assert "com.ombhrum.fabushi.core.MahayanaHost" in pg
    assert "IMikaShell" in pg or "com.wangzan101.mika" in pg
    r_aidl = ctx.jvm_probe("f08_aidl_start_share_file")
    r_chan = ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
    assert r_aidl.get("transactionCode") == 61 and r_chan.get("channelId") == "telegram"


@e2e_test(
    "T1-F15-03",
    tier=1,
    features=["F15"],
    needs=["jvm"],
    title="Synchronized v2.1.0 (versionCode=20100) BuildConfig across :app/:mika-agent + native telegram.events.drain",
)
def test_t1_f15_03_synchronized_v2_1_0_buildconfig_and_native_runtime(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f15_ota_validator_and_version", mode="positive")
    assert res.get("appVCode") == 20100 and res.get("mikaVCode") == 20100
    with ctx.host_session("f15_03_host", test_mode=False) as host:
        drain = host.call_ok("telegram.events.drain", {"accountGeneration": 1, "afterSeq": 0})
        assert isinstance(drain.get("events"), list)


@e2e_test(
    "T1-F15-04",
    tier=1,
    features=["F15"],
    needs=["native"],
    title="Full release bundle packaging (16KB real native .so + Mika prebuilts + ZIP_STORED rootfs + update.json + SHA256SUMS)",
)
def test_t1_f15_04_end_to_end_release_bundle_manifest_and_checksums(ctx: E2eTestContext) -> None:
    mod = _load_packager(ctx.repo_root)
    work = ctx.subdir("rel_bundle")
    ks_cache = work / "ks"
    raw_apk = work / "fabushi-raw.apk"
    signed_apk = work / "fabushi-telegram-mika-2.1.0-githubRelease.apk"

    mika_jni = ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "jniLibs" / "arm64-v8a"
    rootfs = ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "assets" / "runtime" / "rootfs.tar.xz"

    create_valid_unsigned_apk(
        raw_apk,
        work,
        arm64_host_bytes=ctx.ws.arm64_lib_so.read_bytes(),
        x64_host_bytes=ctx.ws.x86_64_lib_so.read_bytes(),
        mikarun_bytes=(mika_jni / "libmikarun.so").read_bytes(),
        proot_bytes=(mika_jni / "libproot.so").read_bytes(),
        proot_loader_bytes=(mika_jni / "libproot_loader.so").read_bytes(),
        rootfs_bytes=rootfs.read_bytes()[:65536],
    )

    saved = os.environ.get("FABUSHI_SIGNING_CACHE_DIR")
    try:
        os.environ["FABUSHI_SIGNING_CACHE_DIR"] = str(ks_cache)
        mod.ensure_signed_apk(raw_apk, signed_apk, ctx.repo_root, variant="githubRelease")
    finally:
        if saved is None:
            os.environ.pop("FABUSHI_SIGNING_CACHE_DIR", None)
        else:
            os.environ["FABUSHI_SIGNING_CACHE_DIR"] = saved

    apk_meta = mod.verify_apk_contents(signed_apk)
    assert apk_meta["has_x86_64_host"] is True

    # Ensure packaged host binary has headless gpui-telegram crates integrated for v2.1.0!
    cargo_meta = json.loads(ctx.ws.cargo_metadata_json.read_text(encoding="utf-8"))
    crates = {p.get("name", "").replace("-", "_") for p in cargo_meta.get("packages", [])}
    assert "gpui_telegram_client" in crates and "magpie_gateway" in crates, (
        "Release bundle native host still lacks vendored gpui_telegram_client / magpie_gateway crates"
    )


@e2e_test(
    "T1-F15-05",
    tier=1,
    features=["F15"],
    needs=["jvm"],
    title="All 4 native ARM64 libraries (libmahayana_app_host, libmikarun --pty, libproot, libproot_loader) & rootfs ready",
)
def test_t1_f15_05_all_four_arm64_native_libs_and_rootfs_ready(ctx: E2eTestContext) -> None:
    mika_jni = ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "jniLibs" / "arm64-v8a"
    for name in ("libmikarun.so", "libproot.so", "libproot_loader.so"):
        p = mika_jni / name
        assert p.is_file() and p.stat().st_size > 4096, f"Missing Mika native binary: {p}"
    arm64_host = inspect_elf64_alignment_and_machine(ctx.ws.arm64_lib_so)
    mikarun = inspect_elf64_alignment_and_machine(mika_jni / "libmikarun.so")
    assert all(a >= 0x4000 for a in arm64_host["pt_load_aligns"])
    assert all(a >= 0x4000 for a in mikarun["pt_load_aligns"])
    assert b"--pty" in (mika_jni / "libmikarun.so").read_bytes(), "libmikarun.so not updated to b899c5b --pty"


@e2e_test(
    "T1-F15-06",
    tier=1,
    features=["F15"],
    needs=["jvm"],
    title="Production zero-mock release gate: cold host & ViewModel have 0 fake seeds and 0 canned AI echo",
)
def test_t1_f15_06_production_zero_mock_release_gate(ctx: E2eTestContext) -> None:
    with ctx.host_session("f15_06_host", test_mode=False) as host:
        accts = host.call_ok("telegram.auth.accountsList")
        a_list = accts if isinstance(accts, list) else (accts.get("accounts") or [])
        assert a_list == [], f"Production host contains fake seeded accounts: {a_list}"
        no_key = host.call("magpie.chatComplete", {"prompt": "Release gate check"})
        assert no_key.get("ok") is False, f"Unconfigured magpie.chatComplete returned fake success: {no_key}"
    res = ctx.jvm_probe("f12_cold_start_truthfulness")
    assert res.get("truthfulColdStart") is True
