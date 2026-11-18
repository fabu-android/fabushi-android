"""Tier 1 Feature & Functional E2E tests for M3 Upstream Mika Sync features F8 and F9 (12 tests)."""

from __future__ import annotations

from e2e.harness.context import E2eTestContext
from e2e.harness.host_ffi import inspect_elf64_alignment_and_machine
from e2e.harness.registry import e2e_test


# =============================================================================
# F8 — Upstream Mika@b899c5b Core, Shell, VirtualDisplay, Vision & Sandbox Sync
# =============================================================================


@e2e_test(
    "T1-F08-01",
    tier=1,
    features=["F8"],
    needs=["jvm"],
    title="IMikaShell.aidl & IMikaShell.Stub expose transaction 61 startShareFile(int, String, String, String, String)",
)
def test_t1_f08_01_aidl_tx61_start_share_file(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_aidl_start_share_file")
    assert res.get("transactionCode") == 61


@e2e_test(
    "T1-F08-02",
    tier=1,
    features=["F8"],
    needs=["jvm"],
    title="ShellService targets VirtualDisplay front-app UID for Back (#622) & enforces appops PLAY_AUDIO deny (#483)",
)
def test_t1_f08_02_shell_uid_back_and_audio_deny(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_shell_uid_back_and_audio_deny")
    assert res.get("uidInject") is True and res.get("audioDeny") is True


@e2e_test(
    "T1-F08-03",
    tier=1,
    features=["F8"],
    needs=["jvm"],
    title="BackAffordance (#622) resolves on-screen Back labels ('返回', 'navigate up') & top-left corner ImageButtons",
)
def test_t1_f08_03_back_affordance_positive_resolution(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_back_affordance_positive")
    assert res.get("verified") is True


@e2e_test(
    "T1-F08-04",
    tier=1,
    features=["F8"],
    needs=["jvm"],
    title="Locator (#474) retries transient vision locate/confirm failures with VISION_ATTEMPTS >= 2",
)
def test_t1_f08_04_locator_transient_vision_retry(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_locator_vision_retry")
    assert res.get("visionRetry") is True


@e2e_test(
    "T1-F08-05",
    tier=1,
    features=["F8"],
    needs=["jvm"],
    title="ModelRouter routes Verb.ListFolderFiles to main-only & AgentLoop emits personaLabel trace state",
)
def test_t1_f08_05_model_router_folder_verb_and_persona_loop(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f08_model_router_and_persona_loop")
    assert res.get("mainOnlyListFolderFiles") is True


@e2e_test(
    "T1-F08-06",
    tier=1,
    features=["F8"],
    needs=["jvm"],
    title="Synced native/mikarun.c & arm64-v8a/libmikarun.so have 16KB PT_LOAD alignment and --pty interactive TTY",
)
def test_t1_f08_06_mikarun_c_pty_and_16kb_elf_alignment(ctx: E2eTestContext) -> None:
    mikarun_c = ctx.repo_root / "mobile" / "android" / "mika-agent" / "native" / "mikarun.c"
    mikarun_so = ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "jniLibs" / "arm64-v8a" / "libmikarun.so"
    assert mikarun_c.is_file(), f"Missing upstream Mika@b899c5b native/mikarun.c source file at {mikarun_c}"
    elf = inspect_elf64_alignment_and_machine(mikarun_so)
    assert elf["e_machine"] == 183 and elf["pt_load_aligns"], f"Invalid libmikarun.so ELF64: {elf}"
    assert all(a >= 0x4000 and (a & (a - 1)) == 0 for a in elf["pt_load_aligns"]), (
        f"libmikarun.so PT_LOAD alignment is not 16KB (>=0x4000): {elf['pt_load_aligns']}"
    )
    raw = mikarun_so.read_bytes()
    assert b"--pty" in raw, "libmikarun.so binary is pre-b899c5b and lacks interactive '--pty' TTY mode"


# =============================================================================
# F9 — All 10 New Upstream Mika Capabilities (6 Tests)
# =============================================================================


@e2e_test(
    "T1-F09-01",
    tier=1,
    features=["F9"],
    needs=["jvm"],
    title="AudioCapability (OpenAiSpeechModel + GeminiTtsModel) & audio_generate/audio_save/speak/phone_share_audio tools",
)
def test_t1_f09_01_audio_tts_models_and_tools(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-tts-live") as hub:
        res = ctx.jvm_probe("f09_audio_tts_models", magpieUrl=f"{hub.base_url}/v1", apiKey="sk-tts-live")
        assert res.get("audioTtsToolsVerified") is True


@e2e_test(
    "T1-F09-02",
    tier=1,
    features=["F9"],
    needs=["jvm"],
    title="TranscribeCapability & audio_transcribe tool dispatch in :mika-core / :mika-agent",
)
def test_t1_f09_02_transcribe_capability_and_tool(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f09_audio_transcribe")
    assert res.get("transcribeClass")


@e2e_test(
    "T1-F09-03",
    tier=1,
    features=["F9"],
    needs=["jvm"],
    title="PocketGuardManager SensorEventListener, 2000ms PocketDetector debounce & LockTriggerPolicy phase rules",
)
def test_t1_f09_03_pocket_guard_manager_and_detector(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f09_pocket_guard")
    assert res.get("pocketGuardVerified") is True


@e2e_test(
    "T1-F09-04",
    tier=1,
    features=["F9"],
    needs=["jvm"],
    title="SAF GrantedFolderFiles & FolderFiles virtual /folders/<Alias> mount with optimistic SHA-256 writes",
)
def test_t1_f09_04_saf_granted_folder_files(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f09_saf_folder_files", testTraversal=False)
    assert res.get("grantedClass")


@e2e_test(
    "T1-F09-05",
    tier=1,
    features=["F9"],
    needs=["jvm"],
    title="PersonaPrompt XML/bidi sanitization, PersonaStore persistence & GitHub SkillMarket installer",
)
def test_t1_f09_05_persona_store_and_skill_market(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f09_persona_and_skill_market")
    assert res.get("personaEscaped") is True


@e2e_test(
    "T1-F09-06",
    tier=1,
    features=["F9"],
    needs=["jvm"],
    title="WebFetch + TinyFish, VoiceBriefing + CallDtmfExecutor, ReportFilesProvider & ReportViewer FindState",
)
def test_t1_f09_06_webfetch_briefing_dtmf_and_report_provider(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f09_webfetch_briefing_dtmf_report_viewer", testNegativeGuards=False)
    assert len(res.get("verifiedClasses") or []) == 6
