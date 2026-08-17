//! The palette: every color the product uses, named by role in one place.
//!
//! A `Palette` is a plain value with two constructors. `plain()` carries attribute-free styles
//! that emit nothing, and `colored()` carries the scheme with styling forced — forced because an
//! unforced `console::Style` consults that crate's own global detection at apply time, which
//! would silently override the first-party chain in [`super::chain`]. Every renderer takes a
//! `&Palette` and never asks whether color is on; the palette itself is the answer.
//!
//! One rule is structural: a renderer parameterized by the palette produces one text whose
//! escape sequences differ and whose bytes otherwise do not (spec/14 "the text is identical with
//! and without it"). `plain()` is the `Display` default, which is what keeps every byte-exact
//! assertion in `diagnostic` true without knowing this module exists.

use console::Style;

/// The roles, not the colors: a renderer names what a span is, and the palette decides how that
/// role looks — or that it looks like nothing.
#[derive(Clone, Debug)]
pub struct Palette {
    /// The `error` word ahead of a diagnostic id.
    pub error: Style,
    /// A stable identifier: a diagnostic id, a check id.
    pub id: Style,
    /// The one line that must stand alone: a diagnostic's `what`.
    pub what: Style,
    /// Where something happened: the `-->` arrow and the locus beside it.
    pub locus: Style,
    /// A label ahead of a value: `why:`, `hint:`, a table's left column.
    pub label: Style,
    /// The remedy: a hint's body.
    pub hint: Style,
    /// A value worth the eye: a manifest name, a state.
    pub accent: Style,
    /// A healthy reading.
    pub good: Style,
    /// A reading that wants attention without being a failure.
    pub warn: Style,
}

impl Palette {
    /// Attribute-free styles: applying them changes nothing, byte for byte.
    #[must_use]
    pub const fn plain() -> Self {
        Self {
            error: Style::new(),
            id: Style::new(),
            what: Style::new(),
            locus: Style::new(),
            label: Style::new(),
            hint: Style::new(),
            accent: Style::new(),
            good: Style::new(),
            warn: Style::new(),
        }
    }

    /// The scheme, forced so the chain's answer is final.
    #[must_use]
    pub fn colored() -> Self {
        let force = |style: Style| style.force_styling(true);
        Self {
            error: force(Style::new().red().bold()),
            id: force(Style::new().dim()),
            what: force(Style::new().bold()),
            locus: force(Style::new().cyan()),
            label: force(Style::new().dim()),
            hint: force(Style::new().green()),
            accent: force(Style::new().cyan()),
            good: force(Style::new().green()),
            warn: force(Style::new().yellow()),
        }
    }

    /// The palette the chain's answer selects.
    #[must_use]
    pub fn resolve(enabled: bool) -> Self {
        if enabled {
            Self::colored()
        } else {
            Self::plain()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plain_palette_changes_no_bytes() {
        let palette = Palette::plain();
        assert_eq!(palette.error.apply_to("error").to_string(), "error");
        assert_eq!(palette.what.apply_to("a message").to_string(), "a message");
    }

    #[test]
    fn the_colored_palette_survives_a_pipe() {
        // Forced styling is the point: the chain decides, not the crate's own detection, and this
        // test runs off a TTY — where an unforced style would silently emit nothing.
        let rendered = Palette::colored().error.apply_to("error").to_string();
        assert!(rendered.contains('\u{1b}'), "{rendered:?}");
        assert_eq!(console::strip_ansi_codes(&rendered), "error");
    }
}
