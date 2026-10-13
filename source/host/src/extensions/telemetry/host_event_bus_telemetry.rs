use std::collections::BTreeMap;

use crate::host_event_bus::HostEventFailure;

use super::HostTelemetryProjection;

pub const HOST_EVENT_BUS_EVENT: &str = "sand.host.event_bus";

pub fn host_event_bus_telemetry(report: &HostEventFailure) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("kind".into(), report.kind.clone()),
        ("error_class".into(), report.error_class.clone()),
    ]);
    if let Some(topic) = report.topic.as_ref() {
        metadata.insert("topic".into(), topic.clone());
    }
    HostTelemetryProjection {
        level: Some("error"),
        event: Some(HOST_EVENT_BUS_EVENT),
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_projection_is_bounded_to_kind_topic_and_error_class() {
        let projection = host_event_bus_telemetry(&HostEventFailure {
            kind: "subscriber_failed".into(),
            topic: Some("tool".into()),
            error_class: "panic".into(),
        });
        assert_eq!(projection.level, Some("error"));
        assert_eq!(projection.event, Some("sand.host.event_bus"));
        assert_eq!(projection.metadata["kind"], "subscriber_failed");
        assert_eq!(projection.metadata["topic"], "tool");
        assert_eq!(projection.metadata["error_class"], "panic");
        assert_eq!(projection.metadata.len(), 3);
    }
}
