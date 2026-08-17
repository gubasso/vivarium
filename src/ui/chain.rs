//! The color decision: `NO_COLOR > FORCE_COLOR > isatty`, first-party.
//!
//! spec/01 fixes this exact order and ADR-0015 fixes that there is no `--color` flag, and no
//! crate implements the chain as written — `anstream` skips `FORCE_COLOR`, `console` consults its
//! own globals — so the decision is made here and every style downstream is forced or absent,
//! never left to a library's own detection.
//!
//! Both variables follow the no-color.org convention for what "set" means: present with a
//! non-empty value. An empty string is ignored rather than read as intent, because shells produce
//! `VAR=` in ways users do not mean (`env -i`, an unset expansion exported anyway), and the two
//! variables should disagree about emptiness in no direction.

use crate::config::Environment;

/// Whether one stream gets color, per the fixed chain.
///
/// Per stream rather than per process: spec/01 splits stdout and stderr by role, and a
/// `viv status | less` colors its stderr progress while its piped result stays plain.
pub fn colors_enabled<E: Environment>(is_tty: bool, environment: &E) -> bool {
    if set(environment, "NO_COLOR") {
        return false;
    }
    if set(environment, "FORCE_COLOR") {
        return true;
    }
    is_tty
}

/// Present with a non-empty value.
fn set<E: Environment>(environment: &E, name: &'static str) -> bool {
    environment
        .variable(name)
        .is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    /// A closure is an `Environment` by the blanket impl in `config::roots`.
    fn env(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&'static str) -> Option<OsString> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn the_default_is_the_stream_fact() {
        assert!(colors_enabled(true, &env(&[])));
        assert!(!colors_enabled(false, &env(&[])));
    }

    #[test]
    fn force_color_overrides_a_pipe() {
        assert!(colors_enabled(false, &env(&[("FORCE_COLOR", "1")])));
    }

    #[test]
    fn no_color_wins_over_everything() {
        let both = env(&[("NO_COLOR", "1"), ("FORCE_COLOR", "1")]);
        assert!(!colors_enabled(true, &both));
        assert!(!colors_enabled(false, &both));
    }

    #[test]
    fn an_empty_value_is_not_set() {
        assert!(colors_enabled(true, &env(&[("NO_COLOR", "")])));
        assert!(!colors_enabled(false, &env(&[("FORCE_COLOR", "")])));
    }
}
