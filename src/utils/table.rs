use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{ContentArrangement, Table};

/// Returns the detected terminal width in columns, with a sensible fallback.
pub fn get_terminal_width() -> u16 {
    crossterm::terminal::size()
        .map(|(w, _)| w)
        .unwrap_or(80)
        .max(40) // Minimum readable boundary
}

/// Creates a styled `comfy_table::Table` configured for the current terminal width
/// with dynamic content wrapping so it never exceeds the screen boundary or shatters.
pub fn create_styled_table() -> Table {
    create_styled_table_with_width(get_terminal_width())
}

/// Creates a styled table with an explicitly specified terminal width (useful for testing).
pub fn create_styled_table_with_width(width: u16) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(width);
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use comfy_table::Cell;

    #[test]
    fn test_get_terminal_width_bounds() {
        let width = get_terminal_width();
        assert!(width >= 40);
    }

    #[test]
    fn test_create_styled_table() {
        let mut table = create_styled_table_with_width(80);
        table.set_header(vec![Cell::new("Col1"), Cell::new("Col2")]);
        table.add_row(vec![
            Cell::new("Very long content that should be dynamically wrapped inside the table cells"),
            Cell::new("Short content"),
        ]);
        let rendered = table.to_string();
        for line in rendered.lines() {
            assert!(
                line.chars().count() <= 80,
                "Line length {} exceeded max 80: {}",
                line.chars().count(),
                line
            );
        }
    }
}
