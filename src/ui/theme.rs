//! The vivarium look for the clack gutter — the only file that knows cliclack's shape.
//!
//! The stock clack theme is already close to the restrained brief; what this trims is the
//! always-cyan gutter, so the vertical bar stays quiet and color marks state changes alone: cyan
//! only on the live symbol, green on a submitted step, red and yellow where they already mean
//! what they mean.
//!
//! Installed once at the process boundary through [`install`]. The theme and console's
//! per-stream color globals are the only global state this crate touches, and confining both
//! the trait impl and the setter here keeps the impurity in a file whose whole job is rendering.

use cliclack::{Theme, ThemeState};
use console::Style;

/// The restrained clack: quiet gutter, stock state symbols.
struct VivariumTheme;

impl Theme for VivariumTheme {
    fn bar_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Cancel => Style::new().red(),
            ThemeState::Error(_) => Style::new().yellow(),
            // The stock theme paints the active gutter cyan; a frame is not the news, so the
            // gutter stays dim in every calm state and the symbol carries the color.
            ThemeState::Active | ThemeState::Submit => Style::new().bright().black(),
        }
    }
}

/// Installs the theme and pins console's per-stream color globals, once, from the process
/// boundary.
///
/// The pinning is what makes ADR-0103's "forced or absent" hold across the borrowed renderers:
/// the theme's styles and indicatif's spinner tint consult console's global detection at apply
/// time, and that detection reads `CLICOLOR`/`CLICOLOR_FORCE` rather than the first-party
/// `NO_COLOR > FORCE_COLOR > isatty` chain — under `NO_COLOR=1 CLICOLOR_FORCE=1` it would even
/// color what the chain declared plain. Overriding both globals with the chain's answers makes
/// that answer the one every unforced style sees.
pub fn install(stdout_colors: bool, stderr_colors: bool) {
    console::set_colors_enabled(stdout_colors);
    console::set_colors_enabled_stderr(stderr_colors);
    cliclack::set_theme(VivariumTheme);
}
