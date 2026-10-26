use crate::{query::Query, sys};
use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::Path};

#[test]
fn index_paths_preserve_case_and_non_utf8_names() {
    let bytes = b"/home/user/Report\xff.txt";
    let path = Path::new(OsStr::from_bytes(bytes));
    assert_eq!(sys::index_path(path), bytes);
    assert_eq!(sys::os_path(bytes), path);
    assert_ne!(sys::index_path(Path::new("/home/user/Report")), sys::index_path(Path::new("/home/user/report")));
}

#[test]
fn query_scope_expands_linux_home() {
    let query = Query::parse("in:~/Documents report", "/home/alice").unwrap();
    assert_eq!(query.scope.as_deref(), Some(b"/home/alice/Documents".as_slice()));
}
