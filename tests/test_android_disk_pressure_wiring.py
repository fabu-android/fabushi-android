from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def test_android_disk_pressure_has_one_platform_sampler_and_rust_owner() -> None:
    host = read("source/android-main/src/main/kotlin/com/ombhrum/fabushi/core/MahayanaHost.kt")
    runtime = read("source/host/src/android_json_runtime.rs")
    extension = read("source/host/src/extensions/forever_box/extension.rs")

    # Android owns only the platform storage measurement. It must sample the app-private
    # Host root and project raw capacity values into Rust rather than classifying pressure in Kotlin.
    assert "StatFs(appDataDir)" in host
    assert '.put("totalBytes", stats.totalBytes.coerceAtLeast(0L))' in host
    assert '.put("availableBytes", stats.availableBytes.coerceAtLeast(0L))' in host
    assert '.put("method", "feature.agent.diskPressure.observe")' in host
    assert "DISK_PRESSURE_SAMPLE_INTERVAL_MS = 60_000L" in host

    # Rust remains the sole canonical pressure/reminder owner and fences observations by account.
    assert '"feature.agent.diskPressure.observe" => self.agent_disk_pressure_observe(params)' in runtime
    assert "fn agent_disk_pressure_observe" in runtime
    observe = runtime.split("fn agent_disk_pressure_observe", 1)[1].split(
        "fn agent_disk_pressure_record", 1
    )[0]
    assert "self.current_turn_account_fence()?" in observe
    assert "observe_disk_pressure_sample" in observe

    # Desktop Forever Box execution must not silently become arbitrary local Android execution.
    assert "local_desktop_box_available" in extension
    assert "false" in extension.split("local_desktop_box_available", 1)[1].split("}", 1)[0]
    assert '"bounded-local-runner-or-authorized-remote-runner"' in extension


def test_disk_pressure_port_keeps_desktop_hysteresis_and_durable_episode_tests() -> None:
    guard = read("source/host/src/extensions/forever_box/disk_pressure_guard.rs")
    reminders = read("source/host/src/extensions/forever_box/disk_pressure.rs")

    for expected in (
        "soft_available_bytes: 8 * GIB",
        "hard_available_bytes: 2 * GIB",
        "soft_recovery_bytes: 10 * GIB",
        "hard_recovery_bytes: 3 * GIB",
        "DISK_PRESSURE_HEARTBEAT_MS: u64 = 5 * 60_000",
        "classifies_thresholds_with_hysteresis",
    ):
        assert expected in guard

    for expected in (
        "reminder_episode_survives_reopen_and_claim_release_commit_is_idempotent",
        "restored_episode_is_not_cleared_by_incomplete_healthy_sample",
    ):
        assert expected in reminders
