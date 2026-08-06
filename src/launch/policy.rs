//! Closed N20 command construction. Callers supply resources, never policy switches.

use crate::launch::{CommandSpec, LaunchError, LaunchSpec, ShareSpec};
use std::path::Path;

const SETPRIV_POLICY: [&str; 4] = [
    "--no-new-privs",
    "--bounding-set=-all",
    "--ambient-caps=-all",
    "--inh-caps=-all",
];

#[derive(Clone, Debug)]
pub struct ConfinementProfile<'a> {
    spec: &'a LaunchSpec,
}

impl<'a> ConfinementProfile<'a> {
    /// Validate a launch contract and bind it to the only policy constructors.
    ///
    /// # Errors
    ///
    /// Returns an error when the launch contract is unsafe or malformed.
    pub fn new(spec: &'a LaunchSpec) -> Result<Self, LaunchError> {
        spec.validate()?;
        Ok(Self { spec })
    }

    #[must_use]
    pub fn vmm(&self) -> CommandSpec {
        let mut child = vec![
            "--api-socket".into(),
            format!("path={}", self.spec.runtime_paths.api_socket.display()),
            "--seccomp".into(),
            "true".into(),
        ];
        if self.spec.landlock_available {
            child.push("--landlock".into());
        }
        wrapped(
            &self.spec.backend_programs.setpriv,
            &self.spec.backend_programs.cloud_hypervisor,
            child,
        )
    }

    #[must_use]
    pub fn shares(&self) -> Vec<CommandSpec> {
        self.spec
            .shares
            .iter()
            .map(|share| self.virtiofsd(share))
            .collect()
    }

    fn virtiofsd(&self, share: &ShareSpec) -> CommandSpec {
        let ids = self.spec.identity_translation;
        let mut child = vec![
            "--socket-path".into(),
            share.socket.display().to_string(),
            "--shared-dir".into(),
            share.source.display().to_string(),
            "--sandbox".into(),
            "namespace".into(),
            "--seccomp".into(),
            "kill".into(),
            "--inode-file-handles=never".into(),
            "--thread-pool-size".into(),
            self.spec.descriptor_budget.worker_pool_size.to_string(),
            "--cache".into(),
            share.cache.clone(),
            format!("--rlimit-nofile={}", self.spec.descriptor_budget.limit),
        ];
        add_translation(
            &mut child,
            "--translate-uid",
            ids.guest_uid,
            ids.host_uid,
            ids.overflow_uid,
            ids.id_max,
        );
        add_translation(
            &mut child,
            "--translate-gid",
            ids.guest_gid,
            ids.host_gid,
            ids.overflow_gid,
            ids.id_max,
        );
        if share.read_only {
            child.push("--readonly".into());
        }
        child.extend(share.extra_args.clone());
        wrapped(
            &self.spec.backend_programs.setpriv,
            &self.spec.backend_programs.virtiofsd,
            child,
        )
    }

    pub(crate) fn validate_rendered(
        vmm: &CommandSpec,
        shares: &[CommandSpec],
    ) -> Result<(), LaunchError> {
        validate_wrapper(vmm)?;
        require_pair(vmm.args(), "--seccomp", "true")?;
        for command in shares {
            validate_wrapper(command)?;
            require_pair(command.args(), "--sandbox", "namespace")?;
            require_pair(command.args(), "--seccomp", "kill")?;
            require_pair(command.args(), "--shared-dir", "")?;
            require_pair(command.args(), "--socket-path", "")?;
            if !command
                .args()
                .iter()
                .any(|arg| arg == "--inode-file-handles=never")
            {
                return Err(LaunchError::Policy("virtiofsd inode handle policy missing"));
            }
            if !command
                .args()
                .iter()
                .any(|arg| arg.starts_with("--rlimit-nofile="))
            {
                return Err(LaunchError::Policy("virtiofsd descriptor limit missing"));
            }
        }
        Ok(())
    }
}

fn wrapped(setpriv: &Path, child: &Path, child_args: Vec<String>) -> CommandSpec {
    let mut args = SETPRIV_POLICY
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    args.push("--".into());
    args.push(child.display().to_string());
    args.extend(child_args);
    CommandSpec::new(setpriv.to_path_buf(), args)
}

fn add_translation(
    args: &mut Vec<String>,
    flag: &str,
    guest: u32,
    host: u32,
    overflow: u32,
    id_max: u32,
) {
    args.extend([flag.into(), format!("map:{guest}:{host}:1")]);
    if guest > 0 {
        args.extend([flag.into(), format!("forbid-guest:0:{guest}")]);
    }
    if guest < id_max {
        args.extend([
            flag.into(),
            format!("forbid-guest:{}:{}", guest + 1, id_max - guest),
        ]);
    }
    if host > 0 {
        args.extend([flag.into(), format!("squash-host:0:{overflow}:{host}")]);
    }
    if host < id_max {
        args.extend([
            flag.into(),
            format!("squash-host:{}:{overflow}:{}", host + 1, id_max - host),
        ]);
    }
}

fn validate_wrapper(command: &CommandSpec) -> Result<(), LaunchError> {
    for mandatory in SETPRIV_POLICY {
        if !command.args().iter().any(|arg| arg == mandatory) {
            return Err(LaunchError::Policy(
                "capability/no-new-privileges wrapper incomplete",
            ));
        }
    }
    Ok(())
}

fn require_pair(args: &[String], flag: &str, value: &str) -> Result<(), LaunchError> {
    let found = args
        .windows(2)
        .any(|pair| pair[0] == flag && (value.is_empty() || pair[1] == value));
    if found {
        Ok(())
    } else {
        Err(LaunchError::Policy(
            "mandatory confinement argument missing",
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn command(args: &[&str]) -> CommandSpec {
        CommandSpec::new(
            std::path::PathBuf::from("/nix/store/setpriv"),
            args.iter().map(ToString::to_string).collect(),
        )
    }

    #[test]
    fn mutation_table_rejects_every_mandatory_leg() {
        let base_vmm = [
            "--no-new-privs",
            "--bounding-set=-all",
            "--ambient-caps=-all",
            "--inh-caps=-all",
            "--",
            "/nix/store/ch",
            "--seccomp",
            "true",
        ];
        let base_share = [
            "--no-new-privs",
            "--bounding-set=-all",
            "--ambient-caps=-all",
            "--inh-caps=-all",
            "--",
            "/nix/store/virtiofsd",
            "--sandbox",
            "namespace",
            "--seccomp",
            "kill",
            "--shared-dir",
            "/work",
            "--socket-path",
            "/run/s",
            "--inode-file-handles=never",
            "--rlimit-nofile=524288",
        ];
        let vmm = command(&base_vmm);
        let share = command(&base_share);
        assert_eq!(1, [share.clone()].len());
        ConfinementProfile::validate_rendered(&vmm, &[share]).unwrap();
        for removed in [
            "--no-new-privs",
            "--bounding-set=-all",
            "--ambient-caps=-all",
            "--inh-caps=-all",
            "--seccomp",
        ] {
            let mutated = command(
                &base_vmm
                    .iter()
                    .copied()
                    .filter(|arg| *arg != removed)
                    .collect::<Vec<_>>(),
            );
            assert!(
                ConfinementProfile::validate_rendered(&mutated, &[command(&base_share)]).is_err()
            );
        }
        let mut false_seccomp = base_vmm.map(str::to_owned);
        false_seccomp[7] = "false".into();
        assert!(
            ConfinementProfile::validate_rendered(
                &CommandSpec::new(
                    std::path::PathBuf::from("/nix/store/setpriv"),
                    false_seccomp.into_iter().collect(),
                ),
                &[command(&base_share)],
            )
            .is_err()
        );
        for (flag, replacement) in [
            ("--seccomp", "none"),
            ("--seccomp", "log"),
            ("--sandbox", "none"),
        ] {
            let mut args = base_share.map(str::to_owned);
            let index = args.iter().position(|arg| arg == flag).unwrap();
            args[index + 1] = replacement.into();
            let mutated = CommandSpec::new(
                std::path::PathBuf::from("/nix/store/setpriv"),
                args.into_iter().collect(),
            );
            assert!(ConfinementProfile::validate_rendered(&vmm, &[mutated]).is_err());
        }
        for removed in ["--seccomp", "--ambient-caps=-all"] {
            let mutated = command(
                &base_share
                    .iter()
                    .copied()
                    .filter(|arg| *arg != removed)
                    .collect::<Vec<_>>(),
            );
            assert!(ConfinementProfile::validate_rendered(&vmm, &[mutated]).is_err());
        }
    }
}
