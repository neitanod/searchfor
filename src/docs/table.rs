//! The tables of Word, OpenDocument text and presentations (spec §5.8): `table N › row R ›
//! Column`, the first row naming the columns, as a CSV header row (M4 plan, D20).
use super::Builder;

/// The characters of a table column's name, cut with `…` past them (as M3's `name_chars`).
pub const NAME_CHARS: usize = 40;

pub fn cut(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match s.char_indices().nth(NAME_CHARS) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s,
    }
}

/// A table being read: its number, its rows so far, the names of its columns (its first row's
/// cells), and the cells of the row being read.
#[derive(Default)]
pub struct Table {
    rows: usize,
    names: Vec<String>,
    cells: Vec<String>,
}

impl Table {
    /// Starts a cell of the row being read.
    pub fn push_cell(&mut self) {
        self.cells.push(String::new());
    }

    /// Adds a paragraph's text to the last cell, after a space.
    pub fn add_text(&mut self, s: &str) {
        if let Some(c) = self.cells.last_mut() {
            if !c.is_empty() && !s.trim().is_empty() {
                c.push(' ');
            }
            c.push_str(s.trim());
        }
    }

    /// The text of the row being read, its cells joined by spaces (a nested table's row).
    pub fn take_row(&mut self) -> String {
        std::mem::take(&mut self.cells).join(" ")
    }

    /// Writes the row just read: in the first row the cells name the columns and have no
    /// Segment (they are the columns' names, as a CSV header row); in the others each cell is
    /// a `column` Segment named by its column, or `column K` when that name is empty.
    pub fn end_row(&mut self, b: &mut Builder) {
        let cells = std::mem::take(&mut self.cells);
        if cells.iter().all(|c| c.trim().is_empty()) {
            return;
        }
        self.rows += 1;
        b.start_row(Some(&format!("row {}", self.rows)));
        if self.rows == 1 {
            self.names = cells.iter().map(|c| cut(c)).collect();
            for c in &cells {
                b.cell("column", "", c);
            }
        } else {
            for (k, c) in cells.iter().enumerate() {
                let name = match self.names.get(k) {
                    Some(n) if !n.is_empty() => n.clone(),
                    _ => format!("column {}", k + 1),
                };
                b.cell("column", &name, c);
            }
        }
        b.end_row();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cut_at_forty_characters() {
        assert_eq!(cut("  Unit   price "), "Unit price");
        assert_eq!(cut(&"x".repeat(45)), format!("{}…", "x".repeat(40)));
    }

    #[test]
    fn the_first_row_names_the_columns() {
        let mut b = Builder::new(super::super::MAX_TEXT);
        let mut t = Table::default();
        for row in [
            &["Item", "", "Price"][..],
            &["pens", "x", "1.50 needle"],
            &["", ""],
        ] {
            for c in row {
                t.push_cell();
                t.add_text(c);
            }
            t.end_row(&mut b);
        }
        let e = b.finish();
        assert_eq!(e.text, b"Item\t\tPrice\npens\tx\t1.50 needle\n");
        let names: Vec<_> = e
            .outline
            .segments
            .iter()
            .map(|s| (s.segment.kind.as_str(), s.segment.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("row", "row 1"),
                ("row", "row 2"),
                ("column", "Item"),
                ("column", "column 2"),
                ("column", "Price")
            ]
        );
    }
}
