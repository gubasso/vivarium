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
            // `mixed`, not `control-group`, and the difference is N18 rather than taste.
            // `control-group` sends the stop signal to every process in the unit at once, so
            // `systemctl stop` reaches cloud-hypervisor directly and the VM dies where it stands —
            // the guest never runs its shutdown transaction, never unmounts its volumes, and
            // anything it had not committed is lost. Measured: a file written into the home volume
            // and not explicitly `sync`ed was absent after the next `viv start`, while a synced one
            // survived. `mixed` sends it to the supervisor alone, which is what lets the supervisor
            // walk spec/10's ladder and power the guest down properly.
            //
            // Nothing is given up. `KillMode=mixed` still SIGKILLs whatever remains in the cgroup
            // when the stop timeout expires, so a supervisor that hangs or dies leaves no strays —
            // the guarantee `control-group` was here for.
            "--property=KillMode=mixed".into(),
            // No `CPUAccounting=`. systemd deprecated it — v261 answers the
            // assignment with "D-Bus property CPUAccounting is deprecated,
            // ignoring assignment" and stops reporting the property at all — and
            // under unified cgroups the accounting it once switched on is
            // unconditional: a transient unit started without it still reports a
            // non-zero `CPUUsageNSec`. Setting it bought nothing and cost a
            // warning on every launch, so it is removed rather than left inert.
            // `MemoryAccounting` and `IOAccounting` are not deprecated and are
            // still load-bearing: a unit started without them reports
            // `IOAccounting=no`.
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
        // Rendered from the real spec, not from a second hand-written list. The
        // earlier form joined its own literals and asserted they contained those
        // literals, so it would have passed with `new` returning nothing at all —
        // and it did pass while the property set drifted from what systemd
        // accepts.
        let rendered = TransientUnitSpec::new(&crate::launch::spec::tests::fixture())
            .command()
            .args()
            .join(" ");
        let required = [
            "--service-type=exec",
            "--collect",
            "--slice=vivarium.slice",
            "CollectMode=inactive-or-failed",
            "KillMode=mixed",
            "MemoryAccounting=yes",
            "IOAccounting=yes",
            "CPUWeight=",
            "LimitNOFILE=",
        ];
        // `CPUAccounting` is forbidden rather than merely absent: systemd
        // deprecated it, so setting it is a warning on every launch and buys
        // accounting that unified cgroups already provide unconditionally.
        // `KillMode=control-group` is forbidden rather than merely replaced: it is the value
        // that made `viv stop` lose a guest's uncommitted writes, and nothing else in the product
        // would catch its return.
        let forbidden = [
            "CPUAccounting=",
            "MemoryMax=",
            "MemoryHigh=",
            "IOWeight=",
            "KillMode=control-group",
        ];
        for value in required {
            assert!(rendered.contains(value), "missing {value} in {rendered}");
        }
        for value in forbidden {
            assert!(!rendered.contains(value), "present {value} in {rendered}");
        }
    }
}
