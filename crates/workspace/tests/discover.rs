//! Discovery reports enclosing and nested repositories without changing the
//! selected root.

mod common;

use std::fs;

use aidex_workspace::{MAX_SCAN_DEPTH, MAX_SCAN_ENTRIES, Vcs, WorkspaceError, discover};
use common::{canonical, dir_link, project};

#[test]
fn nested_repositories_are_listed_relative_to_the_selected_root() {
    let root = project(&[
        ("README.md", "monorepo"),
        ("services/api/.git/HEAD", "ref"),
        ("services/web/.git", "gitdir: ../../.git/worktrees/web"),
        ("libs/core/src/lib.rs", ""),
        ("vendor/a/b/c/.git/HEAD", "ref"),
    ]);
    let found = discover(root.path()).unwrap();
    assert_eq!(canonical(&found.selected_root), canonical(root.path()));
    assert_eq!(found.vcs, Vcs::None);
    assert!(found.enclosing_repository.is_none());
    let nested: Vec<_> = found
        .nested_repositories
        .iter()
        .map(|path| path.as_str())
        .collect();
    assert_eq!(nested, ["services/api", "services/web", "vendor/a/b/c"]);
    assert!(!found.scan_truncated);
}

#[test]
fn the_enclosing_repository_is_reported_for_a_subdirectory() {
    let repo = project(&[(".git/HEAD", "ref"), ("pkg/inner/.git/HEAD", "ref")]);
    let found = discover(&repo.path().join("pkg")).unwrap();
    assert_eq!(found.vcs, Vcs::Git);
    assert_eq!(
        canonical(found.enclosing_repository.as_ref().unwrap()),
        canonical(repo.path())
    );
    assert_eq!(
        canonical(&found.selected_root),
        canonical(&repo.path().join("pkg"))
    );
    assert_eq!(found.nested_repositories.len(), 1);
    assert_eq!(found.nested_repositories[0].as_str(), "inner");
}

#[test]
fn the_scan_is_bounded_by_depth_and_entry_count() {
    let deep = format!("{}/.git/HEAD", ["d"; MAX_SCAN_DEPTH + 1].join("/"));
    let root = project(&[(&deep, "ref")]);
    let found = discover(root.path()).unwrap();
    assert!(
        found.nested_repositories.is_empty(),
        "beyond the depth bound"
    );

    let wide = project(&[]);
    for index in 0..=MAX_SCAN_ENTRIES {
        fs::create_dir(wide.path().join(format!("dir{index:05}"))).unwrap();
    }
    assert!(discover(wide.path()).unwrap().scan_truncated);
}

#[test]
fn links_are_not_followed_while_scanning() {
    let root = project(&[]);
    let elsewhere = project(&[("repo/.git/HEAD", "ref")]);
    if !dir_link(elsewhere.path(), &root.path().join("link")) {
        return;
    }
    assert!(
        discover(root.path())
            .unwrap()
            .nested_repositories
            .is_empty()
    );
}

#[test]
fn discovery_requires_an_existing_absolute_directory() {
    let root = project(&[("file.txt", "")]);
    for path in [
        root.path().join("missing"),
        root.path().join("file.txt"),
        std::path::PathBuf::from("relative"),
    ] {
        assert!(
            matches!(discover(&path), Err(WorkspaceError::InvalidPath { .. })),
            "{}",
            path.display()
        );
    }
}
