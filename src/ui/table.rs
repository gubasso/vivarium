//! Fixed-column stdout layout, first-party and ANSI-aware.
//!
//! Every table on the surface has known columns, so this is deliberately small: left-aligned
//! cells sized to the widest visible text, and a label-value record form. Widths are measured
//! with `console::measure_text_width`, never `len()`, because a cell may already carry escape
//! sequences and format-width padding counts their bytes as if they showed.

use std::fmt::Write as _;

use console::measure_text_width;

use super::style::Palette;

/// Left-aligned columns sized to the widest cell, two spaces apart, trailing space trimmed.
///
/// Cells arrive already styled where the caller wants style; the measurement sees through it.
#[must_use]
pub fn columns(rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = Vec::new();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            let width = measure_text_width(cell);
            if index == widths.len() {
                widths.push(width);
            } else if widths[index] < width {
                widths[index] = width;
            }
        }
    }
    let mut rendered = String::new();
    for row in rows {
        let mut line = String::new();
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                line.push_str("  ");
            }
            line.push_str(cell);
            if index + 1 < row.len() {
                let pad = widths[index].saturating_sub(measure_text_width(cell));
                line.push_str(&" ".repeat(pad));
            }
        }
        let _ = writeln!(rendered, "{}", line.trim_end());
    }
    rendered
}

/// Label-value rows: dim labels padded to one width, values verbatim.
#[must_use]
pub fn record(palette: &Palette, rows: &[(&str, String)]) -> String {
    let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
    let mut rendered = String::new();
    for (label, value) in rows {
        let _ = writeln!(
            rendered,
            "{}  {value}",
            palette.label.apply_to(format!("{label:<width$}")),
        );
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_align_on_visible_width_not_bytes() {
        let styled = Palette::colored().accent.apply_to("ab").to_string();
        let rendered = columns(&[
            vec![styled, "x".to_owned()],
            vec!["abcd".to_owned(), "y".to_owned()],
        ]);
        let lines: Vec<&str> = rendered.lines().collect();
        // Both `x` and `y` land in the same visible column despite the escape bytes.
        assert_eq!(
            measure_text_width(lines[0]),
            measure_text_width(lines[1]),
            "{rendered:?}"
        );
    }

    #[test]
    fn a_record_pads_its_labels_to_one_width() {
        let rendered = record(
            &Palette::plain(),
            &[
                ("state", "running".to_owned()),
                ("build", "/nix/store/x".to_owned()),
            ],
        );
        assert_eq!(rendered, "state  running\nbuild  /nix/store/x\n");
    }
}
