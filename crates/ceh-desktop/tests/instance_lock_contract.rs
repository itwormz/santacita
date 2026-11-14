use ceh_desktop::instance_lock::{Acquire, InstanceLock};
use std::time::Duration;

#[test]
fn a_second_copy_on_the_same_data_folder_is_refused_and_asks_the_first_to_show_itself() {
    let root = tempfile::tempdir().unwrap();
    let first = match InstanceLock::acquire(root.path(), Duration::ZERO) {
        Acquire::Owned(lock) => lock,
        _ => panic!("the first copy owns the folder"),
    };
    assert!(matches!(InstanceLock::acquire(root.path(), Duration::ZERO), Acquire::AlreadyRunning));
    assert!(first.take_activation_request(), "the refused copy left a request");
    assert!(!first.take_activation_request(), "a request is read once");
    drop(first);
    assert!(matches!(InstanceLock::acquire(root.path(), Duration::ZERO), Acquire::Owned(_)), "free again after the first exits");
}

#[test]
fn a_restarted_copy_waits_for_the_previous_one_to_let_go() {
    let root = tempfile::tempdir().unwrap();
    let Acquire::Owned(first) = InstanceLock::acquire(root.path(), Duration::ZERO) else { panic!() };
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        drop(first);
    });
    assert!(matches!(InstanceLock::acquire(root.path(), Duration::from_secs(3)), Acquire::Owned(_)));
    release.join().unwrap();
}

#[test]
fn an_unusable_folder_never_blocks_the_start() {
    let missing = std::path::Path::new("/definitely/not/here");
    assert!(matches!(InstanceLock::acquire(missing, Duration::ZERO), Acquire::Unavailable));
}
