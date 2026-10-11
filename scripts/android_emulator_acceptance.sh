#!/usr/bin/env bash
set -euo pipefail

: "${SOURCE_SHA:?SOURCE_SHA is required}"
: "${FABUSHI_CI_ACCOUNT_SESSION_FILE:?FABUSHI_CI_ACCOUNT_SESSION_FILE is required}"
: "${DEVICE_ID:?DEVICE_ID is required}"

PACKAGE=com.ombhrum.fabushi.ci
TEST_PACKAGE=com.ombhrum.fabushi.ci.test
APK="../../exact-head-apk/fabushi-android-ci-acceptance-${SOURCE_SHA}.apk"
TEST_APK="../../exact-head-apk/fabushi-android-ci-acceptance-test-${SOURCE_SHA}.apk"
EVIDENCE="../../evidence/emulator"
REMOTE_EXTERNAL="/sdcard/Android/data/${PACKAGE}/files"
REMOTE_SESSION="${REMOTE_EXTERNAL}/fabushi-ci-session.json"

mkdir -p "$EVIDENCE"
test -s "$APK"
test -s "$TEST_APK"
test -s "$FABUSHI_CI_ACCOUNT_SESSION_FILE"

stage_session() {
  adb shell mkdir -p "$REMOTE_EXTERNAL"
  adb push "$FABUSHI_CI_ACCOUNT_SESSION_FILE" "$REMOTE_SESSION" >/dev/null
}

run_instrumentation_class() {
  local class_name="$1"
  local output="$2"
  adb shell am instrument -w -r -e class "$class_name"     "$TEST_PACKAGE/androidx.test.runner.AndroidJUnitRunner"     | tee "$output"
  grep -q "OK (" "$output"
}

adb uninstall "$PACKAGE" >/dev/null 2>&1 || true
adb install "$APK"
adb install -r "$TEST_APK"
stage_session

adb logcat -c
adb shell rm -f /sdcard/fabushi-acceptance.mp4
adb shell screenrecord --time-limit 120 /sdcard/fabushi-acceptance.mp4 >/dev/null 2>&1 &
SCREENRECORD_ADB_PID=$!
trap 'adb shell pkill -INT screenrecord >/dev/null 2>&1 || true; wait "$SCREENRECORD_ADB_PID" >/dev/null 2>&1 || true' EXIT

# Existing instrumentation remains useful, but packaged production evidence is a
# separate class so deterministic featureHostTest coverage cannot substitute for it.
run_instrumentation_class   "com.ombhrum.fabushi.MahayanaFeatureHostTest,com.ombhrum.fabushi.FabushiScreenTest,com.ombhrum.fabushi.AndroidAccountSessionStoreInstrumentedTest"   "$EVIDENCE/instrumentation-contracts.txt"

# The production-path test requires a real bounded account session, backend-confirmed
# App-owned remote-device registration, live Coordinator generation/sequence metadata,
# chat stream settlement, cancellation, MCP OAuth, WebAuthn provider wire, and logout.
stage_session
run_instrumentation_class   "com.ombhrum.fabushi.PackagedProductionAcceptanceTest"   "$EVIDENCE/instrumentation-packaged-production.txt"

adb shell pkill -INT screenrecord >/dev/null 2>&1 || true
wait "$SCREENRECORD_ADB_PID" >/dev/null 2>&1 || true
trap - EXIT
adb pull /sdcard/fabushi-acceptance.mp4 "$EVIDENCE/android-session.mp4" >/dev/null 2>&1 || true
test -s "$EVIDENCE/android-session.mp4"

adb pull "$REMOTE_EXTERNAL/device-gateway-trace.jsonl" "$EVIDENCE/device-gateway-trace.jsonl" >/dev/null
test -s "$EVIDENCE/device-gateway-trace.jsonl"
python - "$EVIDENCE/device-gateway-trace.jsonl" "$DEVICE_ID" <<'PY'
import json,sys
path,device_id=sys.argv[1:3]
records=[]
with open(path,encoding="utf-8") as handle:
    for line in handle:
        try:
            records.append(json.loads(line))
        except Exception:
            pass
assert any(r.get("phase")=="registered" and r.get("deviceId")==device_id for r in records)
for record in records:
    serialized=json.dumps(record,ensure_ascii=False)
    assert "accessToken" not in serialized
    assert "refreshToken" not in serialized
PY

# Process death/relaunch must advance the persisted Coordinator generation.
adb shell monkey -p "$PACKAGE" -c android.intent.category.LAUNCHER 1 >/dev/null
sleep 3
PID_BEFORE="$(adb shell pidof "$PACKAGE" | tr -d '\r')"
test -n "$PID_BEFORE"
adb shell run-as "$PACKAGE" cat shared_prefs/fabushi-coordinator-runtime.xml > "$EVIDENCE/coordinator-before.xml"
GEN_BEFORE="$(sed -n 's/.*name="generation" value="\([0-9][0-9]*\)".*/\1/p' "$EVIDENCE/coordinator-before.xml" | head -n 1)"
test -n "$GEN_BEFORE"

adb shell am force-stop "$PACKAGE"
stage_session
adb shell monkey -p "$PACKAGE" -c android.intent.category.LAUNCHER 1 >/dev/null
sleep 3
PID_AFTER="$(adb shell pidof "$PACKAGE" | tr -d '\r')"
test -n "$PID_AFTER"
test "$PID_BEFORE" != "$PID_AFTER"
adb shell run-as "$PACKAGE" cat shared_prefs/fabushi-coordinator-runtime.xml > "$EVIDENCE/coordinator-after.xml"
GEN_AFTER="$(sed -n 's/.*name="generation" value="\([0-9][0-9]*\)".*/\1/p' "$EVIDENCE/coordinator-after.xml" | head -n 1)"
test -n "$GEN_AFTER"
test "$GEN_AFTER" -gt "$GEN_BEFORE"

adb exec-out screencap -p > "$EVIDENCE/relaunch.png"
adb logcat -d > "$EVIDENCE/logcat.txt"
sha256sum "$APK" "$TEST_APK" > "$EVIDENCE/installed-apks.sha256"
printf 'head=%s\npackage=%s\npid_before=%s\npid_after=%s\ngeneration_before=%s\ngeneration_after=%s\ndevice_id=%s\n'   "$SOURCE_SHA" "$PACKAGE" "$PID_BEFORE" "$PID_AFTER" "$GEN_BEFORE" "$GEN_AFTER" "$DEVICE_ID"   > "$EVIDENCE/process-recreation.txt"
printf 'head=%s\npackage=%s\nproduction_path=true\nauthenticated=true\nremote_registered=true\n'   "$SOURCE_SHA" "$PACKAGE" > "$EVIDENCE/identity.txt"
