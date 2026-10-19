from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HOST = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/core/MahayanaHost.kt"
COORDINATOR = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt"
RUST_HOST = ROOT / "source/host/src/android_json_runtime.rs"


def test_android_storage_pressure_has_one_platform_producer():
    host = HOST.read_text(encoding="utf-8")
    coordinator = COORDINATOR.read_text(encoding="utf-8")
    rust_host = RUST_HOST.read_text(encoding="utf-8")

    # MahayanaHost owns shared JNI lifetime/event fanout, not a second Android storage sampler.
    assert "feature.agent.diskPressure.observe" not in host
    assert "DISK_PRESSURE_SAMPLE_INTERVAL_MS" not in host

    # Android platform runtime is the one OS-storage producer and explicitly tears it down.
    assert coordinator.count('"feature.agent.diskPressure.observe"') == 1
    assert "storagePressureExecutor.scheduleWithFixedDelay" in coordinator
    assert "storagePressureExecutor.shutdownNow()" in coordinator
    assert "StatFs(application.filesDir.absolutePath)" in coordinator

    # Rust Host remains the canonical account-fenced pressure/episode state owner.
    assert "fn agent_disk_pressure_observe" in rust_host
    assert "current_turn_account_fence" in rust_host
