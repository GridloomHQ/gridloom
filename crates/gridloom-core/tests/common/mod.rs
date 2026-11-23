use std::path::Path;

pub fn edit_workbook_metadata(path: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
    let db = rusqlite::Connection::open(path).unwrap();
    let payload: String = db
        .query_row("SELECT json FROM workbook WHERE singleton = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    let mut json = serde_json::from_str(&payload).unwrap();
    edit(&mut json);
    db.execute(
        "UPDATE workbook SET json = ?1 WHERE singleton = 1",
        [json.to_string()],
    )
    .unwrap();
}
