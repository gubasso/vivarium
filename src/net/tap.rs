//! Tap configuration inside the pair, via the pinned iproute2 `ip`.
//!
//! The tap is a launch-time one-shot a netlink crate would overserve: four `ip`
//! invocations run through `nsenter`, then an assertion over `ip -j` output. The
//! interface reports no carrier until a VMM opens the device by name — Q-005's spike
//! recorded that state as expected, so the assertion here distinguishes it from a
//! link that failed to come up rather than reading it as failure.

use serde_json::Value;
use thiserror::Error;

/// The `ip` argv sequences, in apply order, that configure the namespace's side of
/// the guest link: loopback up, the tap created, addressed as the guest's gateway,
/// and administratively up.
#[must_use]
pub fn setup_sequences(tap: &str, gateway_cidr: &str) -> Vec<Vec<String>> {
    [
        vec!["link", "set", "lo", "up"],
        vec!["tuntap", "add", "mode", "tap", "name", tap],
        vec!["addr", "add", gateway_cidr, "dev", tap],
        vec!["link", "set", tap, "up"],
    ]
    .into_iter()
    .map(|args| args.into_iter().map(ToString::to_string).collect())
    .collect()
}

/// The observed state of a configured tap, read from `ip -j link show`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TapState {
    /// Whether something has opened the device. `false` is the expected state at
    /// launch time, before the VMM attaches; it becomes `true` when the guest's
    /// device opens the tap by name.
    pub carrier: bool,
}

/// A tap that `ip -j link show` does not report as configured and up.
#[derive(Debug, Error)]
pub enum TapError {
    /// The output is not the JSON array `ip -j` emits.
    #[error("`ip -j link show` output was unparsable: {reason}")]
    Unparsable { reason: String },
    /// No interface with the tap's name is present.
    #[error("tap {tap:?} is absent from the namespace's links")]
    Missing { tap: String },
    /// The interface exists but is not administratively up.
    #[error("tap {tap:?} exists but is not up (flags {flags:?})")]
    NotUp { tap: String, flags: Vec<String> },
}

/// Assert over `ip -j link show` output that the tap exists and is up, reporting
/// whether a carrier is present.
///
/// # Errors
///
/// Returns [`TapError`] when the output is unparsable, the tap is absent, or the
/// tap is not administratively up. No carrier is not an error: that is the expected
/// pre-attach state and the caller reads it from [`TapState`].
pub fn assert_tap_up(ip_json: &str, tap: &str) -> Result<TapState, TapError> {
    let links: Value = serde_json::from_str(ip_json).map_err(|error| TapError::Unparsable {
        reason: error.to_string(),
    })?;
    let links = links.as_array().ok_or_else(|| TapError::Unparsable {
        reason: "expected a JSON array of links".to_owned(),
    })?;
    let link = links
        .iter()
        .find(|link| link.get("ifname").and_then(Value::as_str) == Some(tap))
        .ok_or_else(|| TapError::Missing {
            tap: tap.to_owned(),
        })?;
    let flags: Vec<String> = link
        .get("flags")
        .and_then(Value::as_array)
        .map(|flags| {
            flags
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    if !flags.iter().any(|flag| flag == "UP") {
        return Err(TapError::NotUp {
            tap: tap.to_owned(),
            flags,
        });
    }
    Ok(TapState {
        carrier: !flags.iter().any(|flag| flag == "NO-CARRIER"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_setup_sequences_render_the_spike_order() {
        let rendered = setup_sequences("viv-tap0", "10.177.0.1/24");
        let expected: Vec<Vec<String>> = [
            vec!["link", "set", "lo", "up"],
            vec!["tuntap", "add", "mode", "tap", "name", "viv-tap0"],
            vec!["addr", "add", "10.177.0.1/24", "dev", "viv-tap0"],
            vec!["link", "set", "viv-tap0", "up"],
        ]
        .into_iter()
        .map(|args| args.into_iter().map(ToString::to_string).collect())
        .collect();
        assert_eq!(rendered, expected);
    }

    #[test]
    fn a_pre_attach_tap_is_up_without_carrier_and_that_is_not_failure() {
        // The exact flag shape Q-005's spike recorded before any VMM opened the
        // device: administratively up, no carrier, operstate DOWN.
        let output = r#"[
            {"ifindex": 1, "ifname": "lo",
                "flags": ["LOOPBACK", "UP", "LOWER_UP"], "operstate": "UNKNOWN"},
            {"ifindex": 2, "ifname": "viv-tap0",
                "flags": ["NO-CARRIER", "BROADCAST", "MULTICAST", "UP"], "operstate": "DOWN"}
        ]"#;
        let state = assert_tap_up(output, "viv-tap0").unwrap();
        assert!(!state.carrier);
    }

    #[test]
    fn an_attached_tap_reports_carrier() {
        let output = r#"[{"ifname": "viv-tap0",
                "flags": ["BROADCAST", "MULTICAST", "UP", "LOWER_UP"], "operstate": "UP"}]"#;
        let state = assert_tap_up(output, "viv-tap0").unwrap();
        assert!(state.carrier);
    }

    #[test]
    fn a_missing_or_down_tap_is_an_error_not_a_state() {
        let missing = r#"[{"ifname": "lo", "flags": ["LOOPBACK", "UP"]}]"#;
        assert!(matches!(
            assert_tap_up(missing, "viv-tap0"),
            Err(TapError::Missing { .. })
        ));
        let down = r#"[{"ifname": "viv-tap0", "flags": ["BROADCAST", "MULTICAST"]}]"#;
        assert!(matches!(
            assert_tap_up(down, "viv-tap0"),
            Err(TapError::NotUp { .. })
        ));
        assert!(matches!(
            assert_tap_up("not json", "viv-tap0"),
            Err(TapError::Unparsable { .. })
        ));
    }
}
