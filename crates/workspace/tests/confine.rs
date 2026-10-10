//! Confinement: workspace-relative paths cannot escape the root through traversal,
//! absolute or drive-letter paths, UNC paths, alternate data streams, symbolic
//! links, or junctions.

mod common;

use std::fs;

use aidex_core::ServiceErrorCode;
use aidex_workspace::{Access, ConfinedRoot, WorkspaceError, parse_workspace_path};
use common::{canonical, dir_link, file_link, project, wp};

#[test]
fn lexical_escapes_are_rejected_before_touching_the_filesystem() {
    for escape in [
        "../secret",
        "a/../../secret",
        "..",
        "/etc/passwd",
        r"C:\Windows\System32",
        "C:/Windows",
        "C:relative",
        r"\\server\share\file",
        "//server/share/file",
        r"dir\..\..\secret",
        "file.txt:hidden-stream",
        "file.txt::$DATA",
        "a//b",
        "./a",
        "",
    ] {
        let error = parse_workspace_path(escape).unwrap_err();
        assert!(
            matches!(error, WorkspaceError::InvalidPath { .. }),
            "{escape:?}: {error}"
        );
        assert_eq!(error.code(), ServiceErrorCode::InvalidArgument);
    }
    assert_eq!(
        parse_workspace_path("src/main.rs").unwrap().as_str(),
        "src/main.rs"
    );
}

#[test]
fn paths_inside_the_root_resolve_for_each_access() {
    let repo = project(&[("src/main.rs", "fn main() {}")]);
    let root = ConfinedRoot::new(repo.path()).unwrap();
    assert_eq!(root.root(), canonical(repo.path()));

    let read = root.resolve(&wp("src/main.rs"), Access::Read).unwrap();
    assert_eq!(read, canonical(&repo.path().join("src/main.rs")));
    root.resolve(&wp("src"), Access::Write).unwrap();
    let created = root
        .resolve(&wp("src/new/deep/file.rs"), Access::Create)
        .unwrap();
    assert!(root.contains(&created));
    assert!(created.ends_with("src/new/deep/file.rs"));

    let missing = root
        .resolve(&wp("src/missing.rs"), Access::Read)
        .unwrap_err();
    assert!(
        matches!(missing, WorkspaceError::PathNotFound(_)),
        "{missing}"
    );
    let missing = root.resolve(&wp("nope/x"), Access::Write).unwrap_err();
    assert!(
        matches!(missing, WorkspaceError::PathNotFound(_)),
        "{missing}"
    );
    let under_file = root
        .resolve(&wp("src/main.rs/child"), Access::Create)
        .unwrap_err();
    assert!(
        matches!(under_file, WorkspaceError::InvalidPath { .. }),
        "{under_file}"
    );
}

#[test]
fn relativize_maps_absolute_paths_back_and_rejects_outside_paths() {
    let repo = project(&[("src/lib.rs", "")]);
    let outside = project(&[("secret.txt", "")]);
    let root = ConfinedRoot::new(repo.path()).unwrap();
    assert_eq!(
        root.relativize(&repo.path().join("src").join("lib.rs"))
            .unwrap(),
        wp("src/lib.rs")
    );
    assert_eq!(
        root.relativize(&repo.path().join("src").join("new.rs"))
            .unwrap(),
        wp("src/new.rs")
    );
    for path in [
        outside.path().join("secret.txt"),
        repo.path().join("..").join("elsewhere"),
        repo.path().join("src").join("..").join("..").join("x"),
    ] {
        let error = root.relativize(&path).unwrap_err();
        assert!(
            matches!(error, WorkspaceError::EscapesRoot(_)),
            "{}: {error}",
            path.display()
        );
    }
    assert!(
        root.relativize(repo.path()).is_err(),
        "the root has no relative path"
    );
    assert!(root.relativize(std::path::Path::new("relative")).is_err());
}

#[test]
fn directory_links_that_leave_the_root_are_rejected() {
    let repo = project(&[("inside/ok.txt", "ok")]);
    let outside = project(&[("secret.txt", "secret")]);
    if !dir_link(outside.path(), &repo.path().join("escape")) {
        return;
    }
    let root = ConfinedRoot::new(repo.path()).unwrap();
    for access in [Access::Read, Access::Write, Access::Create] {
        let error = root.resolve(&wp("escape/secret.txt"), access).unwrap_err();
        assert!(
            matches!(error, WorkspaceError::EscapesRoot(_)),
            "{access:?}: {error}"
        );
        assert_eq!(error.code(), ServiceErrorCode::PermissionDenied);
    }
    let error = root
        .resolve(&wp("escape/new/file.txt"), Access::Create)
        .unwrap_err();
    assert!(matches!(error, WorkspaceError::EscapesRoot(_)), "{error}");
    let error = root.resolve(&wp("escape"), Access::Read).unwrap_err();
    assert!(matches!(error, WorkspaceError::EscapesRoot(_)), "{error}");
    let error = root
        .relativize(&repo.path().join("escape").join("secret.txt"))
        .unwrap_err();
    assert!(matches!(error, WorkspaceError::EscapesRoot(_)), "{error}");
    assert_eq!(
        fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
        "secret"
    );
}

#[test]
fn links_within_the_root_resolve_to_their_checked_target() {
    let repo = project(&[("real/data.txt", "data")]);
    if !dir_link(&repo.path().join("real"), &repo.path().join("alias")) {
        return;
    }
    let root = ConfinedRoot::new(repo.path()).unwrap();
    let resolved = root.resolve(&wp("alias/data.txt"), Access::Read).unwrap();
    assert_eq!(resolved, canonical(&repo.path().join("real/data.txt")));
    // Creating through an inside link is allowed; replacing the link itself is not.
    root.resolve(&wp("alias/new.txt"), Access::Create).unwrap();
    let error = root.resolve(&wp("alias"), Access::Create).unwrap_err();
    assert!(matches!(error, WorkspaceError::SymlinkLeaf(_)), "{error}");
}

#[test]
fn file_links_are_never_create_targets() {
    let repo = project(&[("target.txt", "t")]);
    let outside = project(&[("secret.txt", "secret")]);
    let inside_link = repo.path().join("inside-link");
    if !file_link(&repo.path().join("target.txt"), &inside_link) {
        return;
    }
    assert!(file_link(
        &outside.path().join("secret.txt"),
        &repo.path().join("outside-link")
    ));
    assert!(file_link(
        &outside.path().join("does-not-exist"),
        &repo.path().join("dangling")
    ));
    let root = ConfinedRoot::new(repo.path()).unwrap();
    let error = root
        .resolve(&wp("inside-link"), Access::Create)
        .unwrap_err();
    assert!(matches!(error, WorkspaceError::SymlinkLeaf(_)), "{error}");
    root.resolve(&wp("inside-link"), Access::Read).unwrap();
    for (path, access) in [
        ("outside-link", Access::Read),
        ("outside-link", Access::Create),
        ("dangling", Access::Read),
        ("dangling", Access::Create),
    ] {
        let error = root.resolve(&wp(path), access).unwrap_err();
        assert!(
            matches!(error, WorkspaceError::EscapesRoot(_)),
            "{path} {access:?}: {error}"
        );
    }
}

#[test]
fn a_missing_root_is_unavailable_not_recreated() {
    let repo = project(&[]);
    let path = repo.path().to_path_buf();
    drop(repo);
    let error = ConfinedRoot::new(&path).unwrap_err();
    assert!(
        matches!(error, WorkspaceError::RootUnavailable { .. }),
        "{error}"
    );
    assert!(!path.exists());

    let repo = project(&[("a", "")]);
    let root = ConfinedRoot::new(repo.path()).unwrap();
    let path = repo.path().to_path_buf();
    drop(repo);
    let error = root.resolve(&wp("a"), Access::Create).unwrap_err();
    assert!(
        matches!(error, WorkspaceError::RootUnavailable { .. }),
        "{error}"
    );
    assert!(!path.exists());
}

#[cfg(windows)]
#[test]
fn windows_reserved_names_and_aliasing_spellings_are_rejected() {
    let repo = project(&[]);
    let root = ConfinedRoot::new(repo.path()).unwrap();
    for name in [
        "CON",
        "nul.txt",
        "dir/aux",
        "COM1.log",
        "lpt9",
        "trailing.",
        "trailing ",
        "dir./file",
        "a?b",
        "a*b",
    ] {
        let path = aidex_core::WorkspacePath::new(name).unwrap();
        let error = root.resolve(&path, Access::Create).unwrap_err();
        assert!(
            matches!(error, WorkspaceError::InvalidPath { .. }),
            "{name}: {error}"
        );
        assert!(parse_workspace_path(name).is_err(), "{name}");
    }
}

#[cfg(windows)]
#[test]
fn windows_drive_letter_and_verbatim_roots_resolve_identically() {
    let repo = project(&[("Src/Main.rs", "")]);
    let plain = aidex_workspace::display_path(&canonical(repo.path()));
    assert!(!plain.to_string_lossy().starts_with(r"\\?\"));
    let verbatim = ConfinedRoot::new(format!(r"\\?\{}", plain.display())).unwrap();
    let lettered = ConfinedRoot::new(&plain).unwrap();
    assert_eq!(verbatim, lettered);
    // Case-insensitive comparison: a differently cased absolute path is inside.
    let upper = std::path::PathBuf::from(plain.to_string_lossy().to_uppercase()).join("SRC");
    assert_eq!(
        lettered.relativize(&upper.join("MAIN.RS")).unwrap(),
        wp("Src/Main.rs")
    );
    assert!(lettered.contains(&upper));
    // Another drive letter is never inside.
    let letter = plain.to_string_lossy().as_bytes()[0];
    let other = if letter == b'Z' { 'Y' } else { 'Z' };
    let foreign = std::path::PathBuf::from(format!(r"{other}:\{}", &plain.to_string_lossy()[3..]));
    assert!(!lettered.contains(&foreign));
}

#[cfg(unix)]
#[test]
fn unix_symlink_loops_fail_closed() {
    let repo = project(&[]);
    std::os::unix::fs::symlink("loop-b", repo.path().join("loop-a")).unwrap();
    std::os::unix::fs::symlink("loop-a", repo.path().join("loop-b")).unwrap();
    let root = ConfinedRoot::new(repo.path()).unwrap();
    for access in [Access::Read, Access::Create] {
        let error = root.resolve(&wp("loop-a/x"), access).unwrap_err();
        assert!(
            matches!(error, WorkspaceError::EscapesRoot(_)),
            "{access:?}: {error}"
        );
    }
}

#[test]
fn a_root_replaced_by_a_link_after_resolution_is_refused() {
    let parent = project(&[("repo/a.txt", "a")]);
    let outside = project(&[("a.txt", "outside")]);
    let root = ConfinedRoot::new(parent.path().join("repo")).unwrap();
    fs::rename(parent.path().join("repo"), parent.path().join("moved")).unwrap();
    if !dir_link(outside.path(), &parent.path().join("repo")) {
        return;
    }
    for access in [Access::Read, Access::Create] {
        let error = root.resolve(&wp("a.txt"), access).unwrap_err();
        assert!(
            matches!(error, WorkspaceError::EscapesRoot(_)),
            "{access:?}: {error}"
        );
    }
}
