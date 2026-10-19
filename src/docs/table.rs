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

/// A table being read: its rows so far, the names of its columns (its first row's cells, each
/// over the columns of the grid it spans), and the cells of the row being read.
#[derive(Default)]
pub struct Table {
    rows: usize,
    /// Each name with the columns of the grid it names, `start..end`, sorted.
    names: Vec<(usize, usize, String)>,
    cells: Vec<Cell>,
    /// The column of the grid the row's next cell starts at.
    next: usize,
    /// The columns of the last cell's span that covered cells have not taken yet.
    inside: usize,
    /// The rows the row being read stands for, when more than one.
    repeat: Option<usize>,
}

/// A cell of the row being read: its text, and the columns of the grid it spans.
struct Cell {
    text: String,
    col: usize,
    width: usize,
}

impl Table {
    /// Starts a cell of the row being read, one column of the grid wide.
    pub fn push_cell(&mut self) {
        self.cells.push(Cell {
            text: String::new(),
            col: self.next,
            width: 1,
        });
        self.next = self.next.saturating_add(1);
        self.inside = 0;
    }

    /// The last cell spans `n` columns of the grid, which the cells after it in the row do not
    /// hold (Word's `w:gridSpan`), or which as many covered cells hold (OpenDocument's
    /// `number-columns-spanned`, PowerPoint's `gridSpan`: `cover`).
    pub fn span(&mut self, n: usize) {
        if let Some(c) = self.cells.last_mut() {
            c.width = n.max(1);
            self.next = c.col.saturating_add(c.width);
            self.inside = c.width - 1;
        }
    }

    /// `n` covered cells, which have no text: the ones the last cell's span holds take no
    /// column, and the rest one column each (a cell merged down from the row above).
    pub fn cover(&mut self, n: usize) {
        let inside = n.min(self.inside);
        self.inside -= inside;
        self.skip(n - inside);
    }

    /// The row being read stands for `n` rows (OpenDocument's `number-rows-repeated`).
    pub fn repeat(&mut self, n: usize) {
        self.repeat = Some(n);
    }

    /// The row's next cell starts `n` columns of the grid later (Word's `w:gridBefore`).
    pub fn skip(&mut self, n: usize) {
        self.next = self.next.saturating_add(n);
    }

    /// Adds a paragraph's text to the last cell, after a space.
    pub fn add_text(&mut self, s: &str) {
        if let Some(c) = self.cells.last_mut() {
            let c = &mut c.text;
            if !c.is_empty() && !s.trim().is_empty() {
                c.push(' ');
            }
            c.push_str(s.trim());
        }
    }

    /// The text of the row being read, its cells joined by spaces (a nested table's row).
    pub fn take_row(&mut self) -> String {
        let cells = self.take_cells();
        cells
            .into_iter()
            .map(|c| c.text)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn take_cells(&mut self) -> Vec<Cell> {
        (self.next, self.inside, self.repeat) = (0, 0, None);
        std::mem::take(&mut self.cells)
    }

    /// Writes the row just read: in the first row the cells name the columns of the grid they
    /// span and have no Segment (they are the columns' names, as a CSV header row); in the
    /// others each cell is a `column` Segment named by the column of the grid it starts at
    /// (Codex M4 code r9 #2), or `column K` when that name is empty. An empty row writes no
    /// line but still counts, so the rows after it keep their numbers, and an empty first row
    /// still names the columns, every name empty (Codex r8 #2).
    pub fn end_row(&mut self, b: &mut Builder) {
        // One row more than a document has bytes cannot take the count past the largest one.
        let _ = self.end_rows(b, 1);
    }

    /// `end_row` for a row that stands for more (`repeat`): each is written as one, at most
    /// `writes` times, and they all count (Codex M4 code r9 #4). None when they take the count
    /// past the largest number.
    pub fn end_rows(&mut self, b: &mut Builder, writes: usize) -> Option<()> {
        let n = self.repeat.unwrap_or(1);
        let cells = self.take_cells();
        let first = self.rows;
        self.rows = self.rows.checked_add(n)?;
        if first == 0 {
            self.names = cells
                .iter()
                .map(|c| (c.col, c.col.saturating_add(c.width), cut(&c.text)))
                .collect();
        }
        if cells.iter().all(|c| c.text.trim().is_empty()) {
            return Some(());
        }
        for row in first + 1..=first + n.min(writes) {
            b.start_row(Some(&format!("row {row}")));
            for c in &cells {
                let name = if row == 1 {
                    String::new()
                } else {
                    let i = self.names.partition_point(|n| n.0 <= c.col);
                    match i.checked_sub(1).map(|i| &self.names[i]) {
                        Some((_, end, n)) if c.col < *end && !n.is_empty() => n.clone(),
                        _ => format!("column {}", c.col.saturating_add(1)),
                    }
                };
                b.cell("column", &name, &c.text);
            }
            b.end_row();
        }
        Some(())
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

    /// A cell names the column of the grid it starts at, past spans, covered cells and
    /// skipped columns of any width (Codex M4 code r9 #2).
    #[test]
    fn cells_are_named_by_their_column_of_the_grid() {
        let cell = |t: &mut Table, s: &str| {
            t.push_cell();
            t.add_text(s);
        };
        // B over columns 2–4, C from column 5 on.
        for (skip, cover, want) in [(0, 0, "B"), (2, 0, "B"), (3, 0, "C"), (0, 9, "C")] {
            let mut b = Builder::new(super::super::MAX_TEXT);
            let mut t = Table::default();
            cell(&mut t, "A");
            cell(&mut t, "B");
            t.span(3);
            t.cover(2);
            cell(&mut t, "C");
            t.span(usize::MAX);
            t.end_row(&mut b);
            cell(&mut t, "x");
            t.skip(skip);
            t.cover(cover);
            cell(&mut t, "needle");
            t.end_row(&mut b);
            let e = b.finish();
            let last = &e.outline.segments.last().unwrap().segment;
            assert_eq!(last.name, want, "{skip} {cover}");
        }
    }
}
