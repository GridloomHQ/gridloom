use crate::value::CellCoord;
use crate::workbook::Sheet;
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JumpDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Jump to the end of a contiguous run, across a gap to the next occupied cell,
/// or to the sheet edge if nothing is occupied ahead. Table and spill extents
/// are occupied regions, including NULLs in a connected table. Only sparse
/// cells and extent boundaries are visited; no viewport or SQL query is needed.
pub fn jump(sheet: &Sheet, origin: CellCoord, direction: JumpDirection) -> Option<CellCoord> {
    if origin.row >= sheet.max_rows || origin.col >= sheet.max_cols {
        return None;
    }
    let vertical = matches!(direction, JumpDirection::Up | JumpDirection::Down);
    let forward = matches!(direction, JumpDirection::Down | JumpDirection::Right);
    let (at, cross, max) = if vertical {
        (origin.row, origin.col, sheet.max_rows - 1)
    } else {
        (origin.col, origin.row, sheet.max_cols - 1)
    };
    let mut runs = Vec::new();
    let extents = sheet
        .tables
        .values()
        .map(|table| table.range)
        .chain(sheet.spills.active_spills.values().map(|spill| spill.range));
    for range in extents {
        let (from, to, cross_from, cross_to) = if vertical {
            (
                range.start_row,
                range.end_row,
                range.start_col,
                range.end_col,
            )
        } else {
            (
                range.start_col,
                range.end_col,
                range.start_row,
                range.end_row,
            )
        };
        if cross_from <= cross && cross <= cross_to && from <= max {
            runs.push((from, to.min(max)));
        }
    }
    for cell in sheet.cells.values() {
        let (axis, fixed) = if vertical {
            (cell.coord.row, cell.coord.col)
        } else {
            (cell.coord.col, cell.coord.row)
        };
        if fixed == cross
            && axis <= max
            && (cell.formula.is_some()
                || !cell.raw_input.is_empty()
                || !cell.evaluated_value.is_blank())
        {
            runs.push((axis, axis));
        }
    }

    // Merge overlapping and adjoining ranges in-place in a sorted single pass.
    runs.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (from, to) in runs {
        if let Some(prior) = merged.last_mut() {
            if from <= prior.1.saturating_add(1) {
                prior.1 = prior.1.max(to);
                continue;
            }
        }
        merged.push((from, to));
    }
    let destination = if forward {
        merged
            .into_iter()
            .find_map(|(lo, hi)| {
                if at < lo {
                    Some(lo)
                } else if at < hi {
                    Some(hi)
                } else {
                    None
                }
            })
            .unwrap_or(max)
    } else {
        merged
            .into_iter()
            .rev()
            .find_map(|(lo, hi)| {
                if at > hi {
                    Some(hi)
                } else if at > lo {
                    Some(lo)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    };
    Some(if vertical {
        CellCoord::new(destination, cross)
    } else {
        CellCoord::new(cross, destination)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbook::CellData;

    #[test]
    fn follows_contiguous_runs_gaps_and_edges_beyond_the_viewport() {
        let mut sheet = Sheet::new("Sheet1".into());
        for (row, col, raw) in [
            (0, 0, "1"),
            (1, 0, "2"),
            (2, 0, "=IF(FALSE,1,\"\")"),
            (5, 0, ""),
            (70, 0, "0"),
            (80, 0, "3"),
            (2, 50, "4"),
        ] {
            let coord = CellCoord::new(row, col);
            sheet
                .cells
                .insert(coord.to_a1(), CellData::new(coord, raw.into()));
        }
        for (row, expected) in [(0, 2), (2, 70), (7, 70), (70, 80), (80, 999), (999, 999)] {
            assert_eq!(
                jump(&sheet, CellCoord::new(row, 0), JumpDirection::Down),
                Some(CellCoord::new(expected, 0))
            );
        }
        for (row, expected) in [(999, 80), (70, 2), (2, 0), (0, 0)] {
            assert_eq!(
                jump(&sheet, CellCoord::new(row, 0), JumpDirection::Up),
                Some(CellCoord::new(expected, 0))
            );
        }
        assert_eq!(
            jump(&sheet, CellCoord::new(2, 0), JumpDirection::Right),
            Some(CellCoord::new(2, 50))
        );
        assert_eq!(
            jump(&sheet, CellCoord::new(2, 50), JumpDirection::Left),
            Some(CellCoord::new(2, 0))
        );
        assert_eq!(
            jump(&sheet, CellCoord::new(1000, 0), JumpDirection::Down),
            None
        );
    }

    #[test]
    fn sparse_extent_jumps_do_not_expand_large_spills() {
        let mut sheet = Sheet::new("Sheet1".into());
        sheet.max_rows = 2_000_000;
        sheet
            .spills
            .register_spill(CellCoord::new(1, 1), 1_000_000, 2);
        assert_eq!(
            jump(&sheet, CellCoord::new(1, 2), JumpDirection::Down),
            Some(CellCoord::new(1_000_000, 2))
        );
        assert_eq!(
            jump(&sheet, CellCoord::new(1_000_001, 2), JumpDirection::Up),
            Some(CellCoord::new(1_000_000, 2))
        );
        assert_eq!(
            jump(&sheet, CellCoord::new(1, 3), JumpDirection::Down),
            Some(CellCoord::new(1_999_999, 3))
        );
    }
    #[test]
    fn table_extents_join_adjacent_values_without_fetching_the_source() {
        let mut sheet = Sheet::new("Sheet1".into());
        sheet.max_rows = 2_000_000;
        let table = crate::table::WorksheetTable::new(
            "Ledger".into(),
            "OfflineSource".into(),
            CellCoord::new(42, 1),
            vec!["Amount".into(), "Memo".into()],
            1_000_000,
        );
        sheet.tables.insert(table.name.clone(), table);
        let tail = CellCoord::new(1_000_043, 1);
        sheet
            .cells
            .insert(tail.to_a1(), CellData::new(tail, "0".into()));
        assert_eq!(
            jump(&sheet, CellCoord::new(0, 1), JumpDirection::Down),
            Some(CellCoord::new(42, 1))
        );
        assert_eq!(
            jump(&sheet, CellCoord::new(42, 1), JumpDirection::Down),
            Some(tail)
        );
        assert_eq!(
            jump(&sheet, CellCoord::new(42, 1), JumpDirection::Right),
            Some(CellCoord::new(42, 2))
        );
    }
}
