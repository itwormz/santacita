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

#[test]
fn a_downloaded_appimage_is_started_as_the_successor_so_it_waits_for_the_data_folder() {
    use ceh_desktop::appimage_command;
    let command = appimage_command(std::path::Path::new("/data/updates/SantaCita-0.4.0-x86_64.AppImage"));
    let marker = command.get_envs().find(|(name, _)| *name == "CEH_AFTER_UPDATE").and_then(|(_, value)| value);
    assert_eq!(marker, Some(std::ffi::OsStr::new("1")), "without it the new copy meets the old copy's instance lock and exits at once");
    assert_eq!(command.get_program(), "/data/updates/SantaCita-0.4.0-x86_64.AppImage");
}

#[test]
fn every_replacement_stages_its_own_file_so_two_clicks_cannot_corrupt_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let installed = dir.path().join("SantaCita.AppImage");
    std::fs::write(&installed, "old").unwrap();
    // The fixed staging name an earlier version used, as if another replacement were in the middle of copying.
    let foreign = dir.path().join(".SantaCita.AppImage.update");
    std::fs::write(&foreign, "someone else's half-written copy").unwrap();
    let package = dir.path().join("package.AppImage");
    std::fs::write(&package, "new").unwrap();
    replace_appimage(&package, &installed).unwrap();
    assert_eq!(std::fs::read_to_string(&installed).unwrap(), "new");
    assert_eq!(std::fs::read_to_string(&foreign).unwrap(), "someone else's half-written copy", "another staging file is never touched");
}

#[test]
fn a_staging_name_that_is_taken_is_skipped_and_never_deleted() {
    use ceh_desktop::create_staging_file;
    let dir = tempfile::tempdir().unwrap();
    let taken = dir.path().join("taken");
    std::fs::write(&taken, "another call's file").unwrap();
    let names = ["taken".to_owned(), "taken".to_owned(), "free".to_owned()];
    let (path, _file) = create_staging_file(dir.path(), names.into_iter()).unwrap();
    assert_eq!(path, dir.path().join("free"), "the first name that could be created exclusively");
    assert_eq!(std::fs::read_to_string(&taken).unwrap(), "another call's file", "a name that was already there is left alone");
    assert!(create_staging_file(dir.path(), ["taken".to_owned()].into_iter()).is_err(), "no free name: an error, and still nothing deleted");
    assert!(taken.exists());
}
