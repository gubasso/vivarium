use thiserror::Error;
use vivarium::protocol::validate_boot_identity;

const KEY: &str = "vivarium.boot_identity=";

#[derive(Debug, Error)]
pub enum BootError {
    #[error("missing boot identity")]
    Missing,
    #[error("duplicate boot identity")]
    Duplicate,
    #[error("invalid boot identity")]
    Invalid,
}

pub fn parse_boot_identity(cmdline: &str) -> Result<String, BootError> {
    let mut values = cmdline
        .split_ascii_whitespace()
        .filter_map(|token| token.strip_prefix(KEY));
    let value = values.next().ok_or(BootError::Missing)?;
    if values.next().is_some() {
        return Err(BootError::Duplicate);
    }
    validate_boot_identity(value).map_err(|_| BootError::Invalid)?;
    Ok(value.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const ID: &str = "01234567-89ab-cdef-0123-456789abcdef";

    #[test]
    fn parses_exactly_one_identity() {
        assert_eq!(
            parse_boot_identity(&format!("quiet {KEY}{ID} panic=-1")).unwrap(),
            ID
        );
        assert!(matches!(
            parse_boot_identity("quiet"),
            Err(BootError::Missing)
        ));
        assert!(matches!(
            parse_boot_identity(&format!("{KEY}{ID} {KEY}{ID}")),
            Err(BootError::Duplicate)
        ));
        assert!(matches!(parse_boot_identity(KEY), Err(BootError::Invalid)));
        assert!(matches!(
            parse_boot_identity(&format!("{KEY}BAD")),
            Err(BootError::Invalid)
        ));
    }
}
