//! The readiness report the supervisor hands back to the launcher.
//!
//! This is the one message crossing the handoff socket, and it is the launcher's only evidence
//! that the supervisor got as far as a running guest. It is a type rather than an ad hoc JSON
//! object on each side so that the two halves cannot drift: the version the supervisor writes is
//! the version the launcher checks, and a status the launcher does not know fails to parse instead
//! of being read as a failure it can describe.

use serde::{Deserialize, Serialize};

/// The only schema version this build writes, and the only one it accepts.
pub const SCHEMA_VERSION: u32 = 1;

/// What the supervisor reached before reporting.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadinessStatus {
    /// Every supervised process started and the guest was handed its boot request.
    ProcessReady,
    /// Supervision ended before readiness; the supervisor's own diagnostic is in the journal.
    Failed,
}

/// One readiness report, as it appears on the wire.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReadinessReport {
    /// The wire schema this report was written against.
    pub schema_version: u32,
    /// What the supervisor reached.
    pub status: ReadinessStatus,
}

/// Why a readiness report could not be produced or understood.
#[derive(Debug, thiserror::Error)]
pub enum ReadinessError {
    /// The report could not be serialized.
    #[error("cannot encode the readiness report")]
    Encode(#[source] serde_json::Error),
    /// The bytes are not a readiness report.
    #[error("the readiness report is not valid JSON in the expected shape")]
    Decode(#[source] serde_json::Error),
    /// The report is well formed but written against a schema this build does not know.
    #[error("the readiness report declares unsupported schema version {0}")]
    UnsupportedSchema(u32),
}

impl ReadinessReport {
    /// The report for a supervisor that reached a running guest.
    #[must_use]
    pub const fn process_ready() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            status: ReadinessStatus::ProcessReady,
        }
    }

    /// The report for a supervisor that ended before readiness.
    #[must_use]
    pub const fn failed() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            status: ReadinessStatus::Failed,
        }
    }

    /// Serializes the report for the handoff socket.
    ///
    /// # Errors
    ///
    /// Returns [`ReadinessError::Encode`] if serialization fails, which for this fixed shape would
    /// be a defect rather than a condition of the input.
    pub fn encode(&self) -> Result<Vec<u8>, ReadinessError> {
        serde_json::to_vec(self).map_err(ReadinessError::Encode)
    }

    /// Parses a readiness report, refusing a schema version this build does not know.
    ///
    /// Refusing rather than ignoring is the point of carrying a version at all: a launcher that
    /// read only the fields it recognized would report a stale meaning as a current one.
    ///
    /// # Errors
    ///
    /// Returns [`ReadinessError::Decode`] when the bytes are not a report in this shape, and
    /// [`ReadinessError::UnsupportedSchema`] when they are, but from a schema this build cannot
    /// interpret.
    pub fn decode(bytes: &[u8]) -> Result<Self, ReadinessError> {
        let report: Self = serde_json::from_slice(bytes).map_err(ReadinessError::Decode)?;
        if report.schema_version == SCHEMA_VERSION {
            Ok(report)
        } else {
            Err(ReadinessError::UnsupportedSchema(report.schema_version))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ReadinessError, ReadinessReport, ReadinessStatus};

    /// The wire format is the contract between two processes, so the bytes are asserted rather
    /// than only the round trip: a rename that still round-trips would still break the pair.
    #[test]
    fn a_ready_report_has_the_agreed_wire_form() {
        let encoded = ReadinessReport::process_ready().encode();
        assert_eq!(
            encoded.ok().as_deref(),
            Some(br#"{"schemaVersion":1,"status":"process-ready"}"#.as_slice())
        );
    }

    #[test]
    fn both_statuses_survive_a_round_trip() {
        for report in [ReadinessReport::process_ready(), ReadinessReport::failed()] {
            let decoded = report
                .encode()
                .ok()
                .and_then(|bytes| ReadinessReport::decode(&bytes).ok());
            assert_eq!(decoded, Some(report));
        }
    }

    /// A version the launcher cannot interpret must fail loudly. Reading such a report as a
    /// failure would attribute the launcher's own ignorance to the supervisor.
    #[test]
    fn an_unknown_schema_version_is_refused() {
        let decoded = ReadinessReport::decode(br#"{"schemaVersion":2,"status":"process-ready"}"#);
        assert!(
            matches!(decoded, Err(ReadinessError::UnsupportedSchema(2))),
            "accepted a report from an unknown schema"
        );
    }

    /// `deny_unknown_fields` and the closed status enum are what make the shape a contract rather
    /// than a suggestion.
    #[test]
    fn a_report_outside_the_agreed_shape_is_refused() {
        for malformed in [
            br#"{"schemaVersion":1,"status":"process-ready","extra":true}"#.as_slice(),
            br#"{"schemaVersion":1,"status":"almost-ready"}"#.as_slice(),
            br#"{"status":"process-ready"}"#.as_slice(),
            b"not json".as_slice(),
        ] {
            assert!(
                matches!(
                    ReadinessReport::decode(malformed),
                    Err(ReadinessError::Decode(_))
                ),
                "accepted a malformed report: {}",
                String::from_utf8_lossy(malformed)
            );
        }
    }

    #[test]
    fn the_two_constructors_differ_only_in_status() {
        assert_eq!(
            ReadinessReport::process_ready().status,
            ReadinessStatus::ProcessReady
        );
        assert_eq!(ReadinessReport::failed().status, ReadinessStatus::Failed);
        assert_eq!(
            ReadinessReport::process_ready().schema_version,
            ReadinessReport::failed().schema_version
        );
    }
}
