#[test]
fn automatic_recreate_decision_is_host_owned_and_precedes_android_side_effect() {
    let host = include_str!("../src/android_json_runtime.rs");
    assert!(host.contains(
        r#""feature.telemetry.boxRecreateDecided" => self.box_recreate_decided_telemetry(params)"#
    ));
    assert!(host.contains(
        r#"("mode".into(), "pod_recreate".into())"#
    ));
    assert!(host.contains(
        r#"("preserved".into(), "true".into())"#
    ));
    assert!(host.contains(
        r#""auto_update" | "hibernation_auto_update""#
    ));

    let route = host
        .split("fn box_recreate_decided_telemetry")
        .nth(1)
        .and_then(|part| part.split("fn recovery_telemetry").next())
        .expect("recreate decision route body");
    for legacy_field in [
        "\"requestId\"",
        "\"decision\"",
        "\"kind\"",
        "\"preserveData\"",
        "\"forceRecreate\"",
        "\"forceUpdate\"",
        "\"hasOperationId\"",
    ] {
        assert!(
            !route.contains(legacy_field),
            "{legacy_field} must not reuse the Desktop automatic recreate-decision taxonomy",
        );
    }

    let coordinator = include_str!(
        "../../android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt"
    );
    assert_eq!(
        coordinator.matches("feature.telemetry.boxRecreateDecided").count(),
        1,
        "only the automatic queued-update producer may report the Desktop recreate decision event",
    );
    let queued = coordinator
        .split("private fun resumeQueuedComputerUpdateIfNeeded()")
        .nth(1)
        .and_then(|part| part.split("private fun resumeComputerRebuildMigrationIfNeeded()").next())
        .expect("queued update producer body");
    let event = queued
        .find("feature.telemetry.boxRecreateDecided")
        .expect("automatic recreate decision telemetry");
    for gate in [
        "if (!snapshot.queuedWhenIdle",
        "if (current.imageUpdateAvailable == false)",
        "if (current.imageUpdateAvailable != true)",
        "if (hasWorkingAgent) return@execute",
    ] {
        assert!(
            queued.find(gate).expect("automatic update gate") < event,
            "{gate} must settle before the recreate decision event",
        );
    }
    assert!(
        event < queued.find("reserveQueuedRequest").expect("durable request reservation"),
        "decision telemetry must be emitted before reserving/dispatching the recreate side effect",
    );
    assert!(
        event < queued.find("dispatchComputerRebuildRequest(").expect("rebuild dispatch"),
        "decision telemetry must precede the rebuild transport side effect",
    );

    let manual = coordinator
        .split("override fun computerRebuildRequest(")
        .nth(1)
        .and_then(|part| part.split("override fun computerRebuildForceUpdate()").next())
        .expect("manual rebuild body");
    assert!(!manual.contains("feature.telemetry.boxRecreateDecided"));
}

#[test]
fn recreate_decision_mapper_keeps_desktop_event_taxonomy() {
    let mapper = include_str!("../src/extensions/telemetry/box_recreate_decided_telemetry.rs");
    assert_eq!(mapper.matches("sand.box.recreate_decided").count(), 1);
    assert!(mapper.contains(r#"level: Some("info")"#));
    assert!(mapper.contains("event: Some(BOX_RECREATE_DECIDED_EVENT)"));
}
