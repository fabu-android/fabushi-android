from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def test_android_replaces_desktop_agent_v1_gateway_wire_with_typed_runner_contract() -> None:
    execution = read("source/shared/src/execution.rs")
    local_runner = read("source/local-exec-daemon/src/lib.rs")
    remote_transport = read("source/box-exec-daemon/src/transport.rs")

    # Desktop's generated agent.v1 Gateway binding is an Electron/local-provider mechanism.
    # Android has one typed execution contract instead of keeping a second protobuf owner.
    assert "pub enum ExecutionTarget" in execution
    assert "AndroidLocal" in execution
    assert "RemoteBox" in execution
    assert "pub struct ExecutionRequest" in execution
    assert "pub struct ExecutionResult" in execution
    assert "MAX_ANDROID_LOCAL_EXECUTION_TIMEOUT_MS" in execution

    # The local target is deliberately bounded and cannot impersonate the Remote Runner.
    assert "local runner accepts only AndroidLocal capabilities" in local_runner
    assert "cancellable local capability requires an independently reachable cancellation boundary" in local_runner
    assert "unrestricted shell" in local_runner

    # Desktop-class execution is preserved through the separately authenticated remote transport.
    assert "RemoteTransportPolicy" in remote_transport
    assert "execute" in remote_transport
    assert "reconcile" in remote_transport

    assert not (ROOT / "source/host/src/extensions/local_exec/agent_v1_exec_wire.rs").exists()


def test_typed_android_execution_wire_rejects_unbounded_or_malformed_payloads() -> None:
    execution = read("source/shared/src/execution.rs")

    for contract in (
        "MAX_EXECUTION_ID_BYTES",
        "MAX_EXECUTION_INPUT_JSON_BYTES",
        "MAX_EXECUTION_OUTPUT_JSON_BYTES",
        "timeout_ms must be between",
        "input_json must be valid JSON",
        "output_json must be valid JSON",
        "execution_request_rejects_unbounded_or_invalid_wire_inputs",
        "execution_request_rejects_unbounded_local_timeout",
        "execution_result_rejects_invalid_or_unbounded_json",
    ):
        assert contract in execution
