#![cfg(unix)]
use ceh_desktop::replace_appimage;
use std::os::unix::fs::PermissionsExt;

#[test]
fn the_installed_appimage_is_replaced_atomically_by_the_verified_download() {
    let dir = tempfile::tempdir().unwrap();
    let installed = dir.path().join("Apps/SantaCita.AppImage");
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    std::fs::write(&installed, "old").unwrap();
    let downloads = dir.path().join("data/updates");
    std::fs::create_dir_all(&downloads).unwrap();
    let package = downloads.join("SantaCita-0.4.0-beta.3-x86_64.AppImage");
    std::fs::write(&package, "new").unwrap();
    replace_appimage(&package, &installed).unwrap();
    assert_eq!(std::fs::read_to_string(&installed).unwrap(), "new");
    assert_eq!(std::fs::metadata(&installed).unwrap().permissions().mode() & 0o111, 0o111, "executable");
    assert!(!package.exists(), "the download is removed");
    let leftovers: Vec<_> = std::fs::read_dir(installed.parent().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(leftovers.len(), 1, "no temporary file stays beside it: {leftovers:?}");
}

#[test]
fn a_folder_that_cannot_be_written_leaves_the_installed_file_and_the_download_alone() {
    let dir = tempfile::tempdir().unwrap();
    let installed = dir.path().join("missing-folder/SantaCita.AppImage");
    let package = dir.path().join("SantaCita-x.AppImage");
    std::fs::write(&package, "new").unwrap();
    assert_eq!(replace_appimage(&package, &installed), Err("update_install_failed"));
    assert!(package.exists());
}
