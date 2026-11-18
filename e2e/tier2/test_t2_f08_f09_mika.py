"""Tier 2 Boundary, Corner & Negative E2E tests for M3 Upstream Mika Sync features F8 and F9 (12 tests)."""

from __future__ import annotations

from e2e.harness.context import E2eTestContext
from e2e.harness.registry import e2e_test


# =============================================================================
# F8 — Blind-UI StopRules (#237), Non-Transient Vision, False-Positive Back & Shell Guards
# =============================================================================


@e2e_test(
    "T2-F08-01",
    tier=2,
    features=["F8"],
    needs=["jvm"],
    title="StopRules.guard(no_progress, treeBlind=true) emits #237 blind-UI stop message instead of stale wording",
)
def test_t2_f08_01_stop_rules_blind_ui_no_progress(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_stop_rules_blind_ui")
    assert "no_progress" in str(res.get("reason", ""))


@e2e_test(
    "T2-F08-02",
    tier=2,
    features=["F8"],
    needs=["jvm"],
    title="BackAffordance rejects false-positive 'feedback'/'backup' substrings and bottom-right corner buttons",
)
def test_t2_f08_02_back_affordance_rejects_false_positives(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_back_affordance_negative")
    assert res.get("emptyReturnsNull") is True


@e2e_test(
    "T2-F08-03",
    tier=2,
    features=["F8"],
    needs=["jvm"],
    title="Locator does NOT retry non-transient 'name_mismatch' or 'protected_content' failures (#474)",
)
def test_t2_f08_03_locator_non_transient_no_retry(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_locator_non_transient_no_retry")
    assert res.get("nonTransientVerified") is True


@e2e_test(
    "T2-F08-04",
    tier=2,
    features=["F8"],
    needs=["jvm"],
    title="ShellService audioMuted never falls back to legacy appops 'ignore' and restores 'allow' on crash",
)
def test_t2_f08_04_shell_audio_deny_never_ignore(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_shell_uid_back_and_audio_deny")
    assert res.get("audioDeny") is True


@e2e_test(
    "T2-F08-05",
    tier=2,
    features=["F8"],
    needs=["jvm"],
    title="IMikaShell startShareFile enforces VirtualDisplay displayId > 0 and single-file URI authority boundary",
)
def test_t2_f08_05_aidl_start_share_file_display_boundary(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_aidl_start_share_file")
    assert res.get("transactionCode") == 61


@e2e_test(
    "T2-F08-06",
    tier=2,
    features=["F8"],
    needs=["jvm"],
    title="Light model in ModelRouter is forbidden from executing Verb.ListFolderFiles (escalates to Main model)",
)
def test_t2_f08_06_light_model_blocked_on_list_folder_files(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_model_router_and_persona_loop")
    assert res.get("mainOnlyListFolderFiles") is True


# =============================================================================
# F9 — Path Traversal, Prompt Injection, Tampered Skill, Read-Only Provider & DTMF Cap
# =============================================================================


@e2e_test(
    "T2-F09-01",
    tier=2,
    features=["F9"],
    needs=["jvm"],
    title="FolderFiles.parts rejects '/folders/../secret.txt' traversal, backslashes & bare '/folders' root writes",
)
def test_t2_f09_01_saf_folder_path_traversal_rejected(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f09_saf_folder_files", testTraversal=True)
    assert res.get("grantedClass")


@e2e_test(
    "T2-F09-02",
    tier=2,
    features=["F9"],
    needs=["jvm"],
    title="PersonaPrompt neutralizes '</persona>' breakout tags and strips Unicode bidi override U+202E",
)
def test_t2_f09_02_persona_prompt_tag_breakout_and_bidi_stripped(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f09_persona_and_skill_market")
    assert res.get("personaEscaped") is True


@e2e_test(
    "T2-F09-03",
    tier=2,
    features=["F9"],
    needs=["jvm"],
    title="ReportFilesProvider is unexported in AndroidManifest.xml and rejects write modes ('w'/'rw') with SecurityException",
)
def test_t2_f09_03_report_files_provider_readonly_and_manifest_unexported(ctx: E2eTestContext) -> None:
    mika_manifest = (ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "AndroidManifest.xml").read_text(
        encoding="utf-8"
    )
    assert "ReportFilesProvider" in mika_manifest, "mika-agent AndroidManifest.xml is missing ReportFilesProvider"
    assert 'android:exported="false"' in mika_manifest and 'android:grantUriPermissions="true"' in mika_manifest
    res = ctx.jvm_probe("f09_webfetch_briefing_dtmf_report_viewer", testNegativeGuards=True)
    assert res.get("verifiedClasses")


@e2e_test(
    "T2-F09-04",
    tier=2,
    features=["F9"],
    needs=["jvm"],
    title="PocketGuard LockTriggerPolicy suppresses lock prompts during active Running or Held(waiting_unlock) phases",
)
def test_t2_f09_04_pocket_guard_active_run_suppression(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f09_pocket_guard")
    assert res.get("pocketGuardVerified") is True


@e2e_test(
    "T2-F09-05",
    tier=2,
    features=["F9"],
    needs=["jvm"],
    title="Audio & Transcribe tools fail closed with honest error when speech/transcription capability is unconfigured",
)
def test_t2_f09_05_audio_and_transcribe_unconfigured_fail_closed(ctx: E2eTestContext) -> None:
    res_tts = ctx.jvm_probe("f09_audio_tts_models", magpieUrl="", apiKey="")
    res_asr = ctx.jvm_probe("f09_audio_transcribe")
    assert res_tts.get("audioTtsToolsVerified") is True and res_asr.get("transcribeClass")


@e2e_test(
    "T2-F09-06",
    tier=2,
    features=["F9"],
    needs=["jvm"],
    title="WebFetch rejects embedded userinfo/non-HTTP schemes & CallDtmfBudget caps per-call tone burst length",
)
def test_t2_f09_06_webfetch_url_guard_and_dtmf_budget_cap(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f09_webfetch_briefing_dtmf_report_viewer", testNegativeGuards=True)
    assert len(res.get("verifiedClasses") or []) == 6
