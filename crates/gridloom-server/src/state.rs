use crate::recovery::Recovery;
use gridloom_core::workbook::Workbook;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub workbook: Arc<RwLock<Workbook>>,
    pub(crate) ledgers: Arc<crate::ledger::Ledgers>,
    pub(crate) demo: Arc<crate::ledger_demo::Demo>,
    pub recovery: Option<Arc<Mutex<Recovery>>>,
    pub mutation_gate: Arc<Mutex<()>>,
}

impl AppState {
    pub fn new(workbook: Workbook) -> Self {
        Self {
            workbook: Arc::new(RwLock::new(workbook)),
            ledgers: Arc::default(),
            demo: Arc::default(),
            recovery: None,
            mutation_gate: Default::default(),
        }
    }
}
