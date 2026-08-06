//! Rendering of the manager-owned transient user service handoff.

use crate::launch::{CommandSpec, LaunchSpec};

#[derive(Clone, Debug)]
pub struct TransientUnitSpec {
    command: CommandSpec,
    unit_name: String,
}

impl TransientUnitSpec {
    #[must_use]
    pub fn new(spec: &LaunchSpec) -> Self {
        let unit_name = format!(
            "vivarium-{}-{}.service",
            escape_component(&spec.project_id),
            escape_component(&spec.target)
        );
        let args = vec![
            "--user".into(),
            format!("--unit={unit_name}"),
            "--service-type=exec".into(),
            "--collect".into(),
            "--no-block".into(),
            "--slice=vivarium.slice".into(),
            "--property=CollectMode=inactive-or-failed".into(),
            "--property=KillMode=control-group".into(),
            "--property=CPUAccounting=yes".into(),
            "--property=MemoryAccounting=yes".into(),
            "--property=IOAccounting=yes".into(),
            format!("--property=CPUWeight={}", spec.resources.cpu_weight),
            format!("--property=LimitNOFILE={}", spec.descriptor_budget.limit),
            "--".into(),
            spec.backend_programs.supervisor.display().to_string(),
            "--spec".into(),
            spec.runtime_paths.launch_spec.display().to_string(),
            "--ready-socket".into(),
            spec.runtime_paths.ready_socket.display().to_string(),
        ];
        Self {
            command: CommandSpec::new(spec.backend_programs.systemd_run.clone(), args),
            unit_name,
        }
    }

    #[must_use]
    pub const fn command(&self) -> &CommandSpec {
        &self.command
    }

    #[must_use]
    pub fn unit_name(&self) -> &str {
        &self.unit_name
    }
}

fn escape_component(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') {
            output.push(char::from(byte));
        } else {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            output.push('x');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    if output.is_empty() {
        "unnamed".into()
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_names_are_stable() {
        assert_eq!(escape_component("hello/world"), "hellox2fworld");
    }

    #[test]
    fn required_and_forbidden_properties_are_explicit() {
        let required = [
            "--service-type=exec",
            "--collect",
            "--slice=vivarium.slice",
            "CollectMode=inactive-or-failed",
            "KillMode=control-group",
            "CPUAccounting=yes",
            "MemoryAccounting=yes",
            "IOAccounting=yes",
            "CPUWeight=",
            "LimitNOFILE=",
        ];
        let forbidden = ["MemoryMax=", "MemoryHigh=", "IOWeight="];
        let rendered = [
            "--service-type=exec",
            "--collect",
            "--slice=vivarium.slice",
            "--property=CollectMode=inactive-or-failed",
            "--property=KillMode=control-group",
            "--property=CPUAccounting=yes",
            "--property=MemoryAccounting=yes",
            "--property=IOAccounting=yes",
            "--property=CPUWeight=100",
            "--property=LimitNOFILE=524288",
        ]
        .join(" ");
        for value in required {
            assert!(rendered.contains(value));
        }
        for value in forbidden {
            assert!(!rendered.contains(value));
        }
    }
}
