use gridloom_core::{CellCoord, Workbook};
fn edit(wb: &mut Workbook, input: &str) {
    wb.set_cell("Sheet1", CellCoord::new(0, 0), input.into())
        .unwrap();
}
#[test]
fn replacement_roundtrips_without_touching_unrelated_fixed_temp() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("book.gridloom");
    let fixed_temp = target.with_extension("gridloom.tmp");
    std::fs::write(&fixed_temp, b"unrelated existing data").unwrap();
    let mut wb = Workbook::new();
    edit(&mut wb, "old");
    wb.save_to_file(&target).unwrap();
    edit(&mut wb, "new");
    wb.save_to_file(&target).unwrap();
    let loaded = Workbook::open_from_file(&target).unwrap();
    assert_eq!(loaded.sheets["Sheet1"].cells["A1"].raw_input, "new");
    assert_eq!(
        std::fs::read(&fixed_temp).unwrap(),
        b"unrelated existing data"
    );
    assert_eq!(wb.file_path.as_ref(), Some(&target));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
#[test]
fn precommit_failures_preserve_original_bytes_and_path_and_clean_owned_temp() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.gridloom");
    let mut wb = Workbook::new();
    edit(&mut wb, "original");
    wb.save_to_file(&original).unwrap();
    let bytes = std::fs::read(&original).unwrap();
    edit(&mut wb, "unsaved changes");
    let directory_target = dir.path().join("blocked.gridloom");
    std::fs::create_dir(&directory_target).unwrap();
    std::fs::write(directory_target.join("keep"), b"original directory content").unwrap();
    assert!(wb.save_to_file(&directory_target).is_err());
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    assert_eq!(wb.file_path.as_ref(), Some(&original));
    assert_eq!(
        std::fs::read(directory_target.join("keep")).unwrap(),
        b"original directory content"
    );
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        2,
        "failed-save temp must be removed"
    );
    assert!(wb
        .save_to_file(dir.path().join("missing").join("book.gridloom"))
        .is_err());
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    assert_eq!(wb.file_path.as_ref(), Some(&original));
}
#[cfg(unix)]
#[test]
fn old_fixed_temp_symlink_is_not_followed() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("book.gridloom");
    let victim = dir.path().join("unrelated.txt");
    std::fs::write(&victim, b"keep me").unwrap();
    symlink(&victim, target.with_extension("gridloom.tmp")).unwrap();
    Workbook::new().save_to_file(&target).unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");
    assert!(
        std::fs::symlink_metadata(target.with_extension("gridloom.tmp"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn new_workbooks_are_private_and_replacement_preserves_permission_bits() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("private.gridloom");
    let mut wb = Workbook::new();
    wb.save_to_file(&target).unwrap();
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    for mode in [0o600, 0o640] {
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode)).unwrap();
        edit(&mut wb, "replacement");
        wb.save_to_file(&target).unwrap();
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
}
