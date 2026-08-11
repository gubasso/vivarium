/// Produces the unsuffixed project id fixed by spec/15.
#[must_use]
pub fn sanitize_project_name(name: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_was_hyphen = false;

    for character in name.chars().flat_map(char::to_lowercase) {
        let character = if character.is_ascii_lowercase() || character.is_ascii_digit() {
            character
        } else {
            '-'
        };

        if character == '-' {
            if !sanitized.is_empty() && !previous_was_hyphen {
                sanitized.push(character);
            }
            previous_was_hyphen = true;
        } else {
            sanitized.push(character);
            previous_was_hyphen = false;
        }
    }

    while sanitized.ends_with('-') {
        sanitized.pop();
    }
    sanitized.truncate(48);
    while sanitized.ends_with('-') {
        sanitized.pop();
    }

    if sanitized.is_empty() {
        "project".to_owned()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_project_name;

    /// Pins lowercase, replacement, collapse, and both-end trimming in spec/15's order.
    #[test]
    fn project_name_sanitization_follows_the_fixed_pipeline() {
        let rows = [
            ("API", "api"),
            ("Mixed name_value!", "mixed-name-value"),
            ("alpha___...beta", "alpha-beta"),
            ("---trim me---", "trim-me"),
        ];
        for (input, expected) in rows {
            assert_eq!(sanitize_project_name(input), expected);
        }
    }

    /// Pins the 48-character cap, trailing-hyphen retrim, and unsuffixed collision behavior.
    #[test]
    fn project_name_sanitization_caps_and_retrims_at_48_characters() {
        let forty_eight = "a".repeat(48);
        assert_eq!(
            sanitize_project_name(&format!("{forty_eight}extra")),
            forty_eight
        );

        let cut_on_hyphen = format!("{}-tail", "b".repeat(47));
        assert_eq!(sanitize_project_name(&cut_on_hyphen), "b".repeat(47));

        let shared_prefix = "c".repeat(48);
        assert_eq!(
            sanitize_project_name(&format!("{shared_prefix}one")),
            sanitize_project_name(&format!("{shared_prefix}two"))
        );
    }

    /// Pins spec/15's fallback when sanitization produces no usable characters.
    #[test]
    fn project_name_sanitization_uses_project_for_empty_output() {
        assert_eq!(sanitize_project_name(""), "project");
        assert_eq!(sanitize_project_name("___!!!"), "project");
    }
}
