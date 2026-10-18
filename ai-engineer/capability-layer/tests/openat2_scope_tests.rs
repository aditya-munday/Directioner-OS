use capability_layer::scope::ScopedPath;
use rustix::fs::{Mode, OFlags};
use std::fs;
use std::os::unix::fs::symlink;
use tempfile::tempdir;

#[test]
fn test_parent_traversal_prohibited() {
    let dir = tempdir().unwrap();
    let scope = ScopedPath::new(dir.path()).unwrap();

    let res = scope.resolve("../outside.txt");
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("parent component"));

    let nested_res = scope.resolve("sub/../../outside.txt");
    assert!(nested_res.is_err());
}

#[test]
fn test_symlink_prohibited_in_resolution() {
    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_file = outside.path().join("victim.txt");
    fs::write(&outside_file, "secret").unwrap();

    let scope = ScopedPath::new(dir.path()).unwrap();
    let evil_link = dir.path().join("evil");
    symlink(&outside_file, &evil_link).unwrap();

    let res = scope.resolve("evil");
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("symlink"));
}

#[test]
fn test_open_scoped_valid_file() {
    let dir = tempdir().unwrap();
    let scope = ScopedPath::new(dir.path()).unwrap();
    let target = dir.path().join("test.txt");
    fs::write(&target, "hello scoped openat2").unwrap();

    let fd = scope.open_scoped("test.txt", OFlags::RDONLY, Mode::empty());
    assert!(fd.is_ok());
}
