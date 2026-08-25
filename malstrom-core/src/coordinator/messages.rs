//! Typed messages which workers and coordinator may send to each other
use crate::types::WorkerId;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

/// The Coordinator sends this to the Worker on startup
/// to give the worker the info it needs for building
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct BuildInformation {
    /// Workers in cluster
    pub(crate) worker_set: IndexSet<WorkerId>,
    /// snapshot which the workers shall load
    /// or none if starting fresh
    pub(crate) resume_snapshot: Option<u64>,
    /// CLuster config version to resmue
    pub(crate) config_version: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StartBuild(pub(crate) BuildInformation);

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StartExecution;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum RuntimeMessage {
    Snapshot(u64),
    Reconfigure((IndexSet<WorkerId>, u64)),
    ExecutionComplete,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ExecutionComplete;

#[cfg(test)]
mod tests {
    use super::{BuildInformation, RuntimeMessage, StartBuild, StartExecution};
    use crate::types::distributable::Distributable;
    use indexmap::IndexSet;

    /// The coordinator→worker protocol messages must round-trip. This pins the wire
    /// format (tuple struct `StartBuild`/unit struct `StartExecution`; enum
    /// `RuntimeMessage`) — the coordination task decodes exactly these types, so a
    /// format change breaks worker startup or rescale.
    #[test]
    fn startup_messages_round_trip() {
        let info = BuildInformation {
            worker_set: IndexSet::from([0, 1, 2]),
            resume_snapshot: Some(4),
            config_version: 3,
        };
        let start_build = StartBuild(info);
        let decoded = StartBuild::decode(&start_build.encode());
        assert_eq!(decoded.0.worker_set, IndexSet::from([0, 1, 2]));
        assert_eq!(decoded.0.resume_snapshot, Some(4));
        assert_eq!(decoded.0.config_version, 3);

        // unit struct — a zero-field message
        let decoded_exec = StartExecution::decode(&StartExecution.encode());
        let _ = decoded_exec;
    }

    #[test]
    fn runtime_messages_round_trip() {
        let snap = RuntimeMessage::Snapshot(9);
        assert!(matches!(RuntimeMessage::decode(&snap.encode()), RuntimeMessage::Snapshot(9)));

        let reconfig = RuntimeMessage::Reconfigure((IndexSet::from([0]), 2));
        match RuntimeMessage::decode(&reconfig.encode()) {
            RuntimeMessage::Reconfigure((set, version)) => {
                assert_eq!(set, IndexSet::from([0]));
                assert_eq!(version, 2);
            }
            _ => panic!("expected Reconfigure"),
        }

        let complete = RuntimeMessage::ExecutionComplete;
        assert!(matches!(
            RuntimeMessage::decode(&complete.encode()),
            RuntimeMessage::ExecutionComplete
        ));
    }
}
