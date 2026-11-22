pub mod ast;
pub mod chart;
pub mod close;
pub mod db;
pub mod depgraph;
pub mod dependency_navigation;
pub mod error;
pub mod evaluator;
pub mod find;
pub mod functions;
pub mod lineage;
pub mod parser;
pub mod pivot;
pub mod range_view;
pub mod review;
pub mod spill;
pub mod row_insert;
pub mod spreadsheet;
pub mod selection_summary;
pub mod table;
pub mod value;
pub mod virtual_view;
pub mod workbook;
pub mod worksheet_navigation;
pub mod writeback;

pub use error::CellError;
pub use value::{CellCoord, CellRange, Value};
pub use workbook::Workbook;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub mod live_table;

mod workbook_store;
mod workbook_file_guard;
