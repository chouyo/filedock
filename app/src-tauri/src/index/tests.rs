//! The index must list exactly what `scan.rs` finds, before and after
//! changes on disk and to the rules.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

use super::*;
use crate::config::{Category, TargetRule};
use crate::scan::scan_category;

fn target(dir: &Path, pattern: &str, recursive: bool) -> TargetRule {
    TargetRule {
        id: String::new(),
        dir: dir.to_string_lossy().into_owned(),
        match_type: "glob".into(),
        pattern: pattern.into(),
        recursive,
    }
}

fn category(id: &str, targets: Vec<TargetRule>) -> Category {
    Category {
        id: id.into(),
        name: id.into(),
        order: 0,
        targets,
        created_at: 0,
        updated_at: 0,
    }
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn sorted(mut v: Vec<FileEntry>) -> Vec<(String, u64, i64, String, String)> {
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v.into_iter()
        .map(|e| (e.path, e.size, e.modified_at, e.parent_dir, e.extension))
        .collect()
}

/// Lists through the index as `list_files` does (build cold roots first).
fn list(index: &Index, c: &Category) -> Vec<FileEntry> {
    index.ensure_indexed(&index.roots_for(c)).unwrap();
    index.list(c).unwrap()
}

fn assert_same(index: &Index, c: &Category) {
    assert_eq!(
        sorted(list(index, c)),
        sorted(scan_category(c)),
        "category {}",
        c.id
    );
}

fn reconcile_at(index: &Index, level: Level) -> bool {
    let mut changed = false;
    for r in index.roots().0 {
        changed |= index.reconcile(&r, level, Duration::ZERO).unwrap();
    }
    changed
}

fn reconcile_all(index: &Index) -> bool {
    reconcile_at(index, Level::Full)
}

fn paths(v: Vec<FileEntry>) -> Vec<String> {
    let mut p: Vec<String> = v.into_iter().map(|e| e.path).collect();
    p.sort();
    p
}

fn tree(root: &Path) {
    write(&root.join("a.png"), "1");
    write(&root.join("B.PNG"), "22");
    write(&root.join(".png"), "dot");
    write(&root.join("notes.txt"), "t");
    write(&root.join("sub/c.png"), "333");
    write(&root.join("sub/deep/d.jpg"), "4444");
    write(&root.join("other/e.png"), "5");
}

#[test]
fn matches_scan_and_tracks_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    tree(&data);
    let cats = vec![
        category("png-rec", vec![target(&data, "*.png", true)]),
        category("top", vec![target(&data, "*", false)]),
        category(
            "regex",
            vec![TargetRule {
                match_type: "regex".into(),
                ..target(&data.join("sub"), r"^[cd]\.", true)
            }],
        ),
        // Trailing separator and two overlapping targets.
        category(
            "mixed",
            vec![
                target(
                    Path::new(&format!("{}{}", data.display(), std::path::MAIN_SEPARATOR)),
                    "*.jpg;*.png",
                    true,
                ),
                target(&data.join("sub"), "*", false),
            ],
        ),
    ];
    let index = Index::open(&tmp.path().join("index.db"), &cats).unwrap();
    assert_eq!(
        index.roots().0.len(),
        1,
        "all targets share one recursive root"
    );
    for c in &cats {
        assert_same(&index, c);
    }

    // Offline changes: add, delete, modify in place, rename, delete a
    // directory, add a nested directory.
    write(&data.join("new.png"), "new");
    fs::remove_file(data.join("a.png")).unwrap();
    write(&data.join("B.PNG"), "changed size");
    fs::rename(data.join("sub/c.png"), data.join("sub/c2.png")).unwrap();
    fs::remove_dir_all(data.join("other")).unwrap();
    write(&data.join("sub/x/y/z.png"), "zz");

    assert!(reconcile_all(&index));
    for c in &cats {
        assert_same(&index, c);
    }
    assert!(!reconcile_all(&index), "nothing left to change");
}

#[test]
fn non_recursive_root_keeps_nested_roots_apart() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    tree(&data);
    let top = category("top", vec![target(&data, "*", false)]);
    let sub = category("sub", vec![target(&data.join("sub"), "*", true)]);
    let cats = vec![top.clone(), sub.clone()];
    let index = Index::open(&tmp.path().join("index.db"), &cats).unwrap();
    assert_eq!(index.roots().0.len(), 2);
    assert_same(&index, &top);
    assert_same(&index, &sub);

    write(&data.join("sub/later.txt"), "l");
    write(&data.join("later.txt"), "l");
    reconcile_all(&index);
    assert_same(&index, &top);
    assert_same(&index, &sub);
    // The top root's reconcile must not re-walk or delete the nested root.
    assert!(!reconcile_all(&index));
}

#[test]
fn rule_changes_rebuild_only_new_roots_and_drop_old_data() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    tree(&data);
    let db = tmp.path().join("index.db");
    let sub = category("c", vec![target(&data.join("sub"), "*", true)]);
    let index = Index::open(&db, std::slice::from_ref(&sub)).unwrap();
    assert_same(&index, &sub);

    // Pattern change: same root, no rebuild needed.
    let png = category("c", vec![target(&data.join("sub"), "*.png", true)]);
    index.set_categories(std::slice::from_ref(&png));
    assert!(index.cold_roots().unwrap().is_empty());
    assert_same(&index, &png);

    // Widen to the parent: the old root is absorbed by a new, cold one.
    let wide = category("c", vec![target(&data, "*", true)]);
    index.set_categories(std::slice::from_ref(&wide));
    index.collect_garbage().unwrap();
    assert_eq!(index.cold_roots().unwrap().len(), 1);
    assert_same(&index, &wide);

    // Narrow to a sibling: data outside it is collected.
    let other = category("c", vec![target(&data.join("other"), "*", false)]);
    index.set_categories(std::slice::from_ref(&other));
    index.collect_garbage().unwrap();
    assert_same(&index, &other);
    let dirs: i64 = lock(&index.reader)
        .query_row("SELECT COUNT(*) FROM dirs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dirs, 1);
}

#[test]
fn survives_restart_and_missing_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let db = tmp.path().join("index.db");
    let c = category("c", vec![target(&data, "*", true)]);
    {
        // Directory does not exist yet.
        let index = Index::open(&db, std::slice::from_ref(&c)).unwrap();
        assert!(list(&index, &c).is_empty());
    }
    tree(&data);
    {
        let index = Index::open(&db, std::slice::from_ref(&c)).unwrap();
        assert!(
            index.cold_roots().unwrap().is_empty(),
            "indexed state persisted"
        );
        assert!(reconcile_all(&index), "appeared directory gets built");
        assert_same(&index, &c);
    }
    {
        // Reopened: served from storage without walking.
        let index = Index::open(&db, std::slice::from_ref(&c)).unwrap();
        assert_eq!(index.list(&c).unwrap().len(), scan_category(&c).len());
        fs::remove_dir_all(&data).unwrap();
        assert!(reconcile_all(&index));
        assert!(index.list(&c).unwrap().is_empty());
    }
}

#[test]
fn outdated_schema_is_rebuilt() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("index.db");
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE junk (x); PRAGMA user_version = 999;")
            .unwrap();
    }
    let index = Index::open(&db, &[]).unwrap();
    let version: i64 = lock(&index.reader)
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, store::SCHEMA_VERSION);
}

#[test]
fn level2_tracks_the_file_set_but_not_in_place_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    tree(&data);
    let c = category("c", vec![target(&data, "*", true)]);
    let index = Index::open(&tmp.path().join("index.db"), std::slice::from_ref(&c)).unwrap();
    assert_same(&index, &c);
    assert!(
        !reconcile_at(&index, Level::Dirs),
        "unchanged tree is skipped"
    );

    write(&data.join("sub/deep/new.txt"), "n");
    fs::remove_file(data.join("a.png")).unwrap();
    fs::rename(data.join("sub/c.png"), data.join("sub/renamed.png")).unwrap();
    fs::remove_dir_all(data.join("other")).unwrap();
    write(&data.join("fresh/nested/f.txt"), "f");
    assert!(reconcile_at(&index, Level::Dirs));
    assert_same(&index, &c);

    // Rewriting content leaves the directory's mtime alone: level 2 keeps
    // the old size, level 3 picks it up.
    write(&data.join("notes.txt"), "much longer content");
    assert!(!reconcile_at(&index, Level::Dirs));
    assert_eq!(paths(index.list(&c).unwrap()), paths(scan_category(&c)));
    assert_ne!(sorted(index.list(&c).unwrap()), sorted(scan_category(&c)));
    assert!(reconcile_at(&index, Level::Full));
    assert_same(&index, &c);
}

#[test]
fn coarse_directory_times_force_a_full_check() {
    let root = Root {
        path: PathBuf::from("r"),
        recursive: true,
    };
    let dir = |mtime| reconcile::StoredDir {
        id: 1,
        path: PathBuf::from("r"),
        mtime,
    };
    assert!(reconcile::coarse_times(
        &[dir(1_700_000_000_000_000_000)],
        &root
    ));
    assert!(reconcile::coarse_times(
        &[dir(1_700_000_000_010_000_000)],
        &root
    ));
    assert!(!reconcile::coarse_times(
        &[dir(1_700_000_000_123_456_789)],
        &root
    ));
}

fn set(paths: &[PathBuf]) -> HashSet<PathBuf> {
    paths.iter().cloned().collect()
}

#[test]
fn watcher_events_are_applied_path_by_path() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    tree(&data);
    let all = category("all", vec![target(&data, "*", true)]);
    let top = category("top", vec![target(&data, "*", false)]);
    let deep = category("deep", vec![target(&data.join("sub/deep"), "*", true)]);
    let cats = vec![all.clone(), top.clone(), deep.clone()];
    let index = Index::open(&tmp.path().join("index.db"), &cats).unwrap();
    for c in &cats {
        assert_same(&index, c);
    }
    let check = |changes: &HashSet<Change>| {
        for c in &cats {
            assert_same(&index, c);
        }
        let hit = |c: &Category| {
            c.targets.iter().any(|t| {
                changes
                    .iter()
                    .any(|ch| ch.affects(&normalize_dir(&t.dir), t.recursive))
            })
        };
        (hit(&all), hit(&top), hit(&deep))
    };

    // New file and in-place edit at the top level.
    write(&data.join("new.txt"), "n");
    write(&data.join("notes.txt"), "rewritten in place");
    let ch = index
        .apply_paths(&set(&[data.join("new.txt"), data.join("notes.txt")]))
        .unwrap();
    assert_eq!(check(&ch), (true, true, false));

    // Rename inside a nested directory: both paths are reported.
    fs::rename(data.join("sub/deep/d.jpg"), data.join("sub/deep/d2.jpg")).unwrap();
    let ch = index
        .apply_paths(&set(&[
            data.join("sub/deep/d.jpg"),
            data.join("sub/deep/d2.jpg"),
        ]))
        .unwrap();
    assert_eq!(check(&ch), (true, false, true));

    // A directory moved in with content: only the directory is reported.
    let outside = tmp.path().join("outside");
    write(&outside.join("x/y.txt"), "y");
    fs::rename(&outside, data.join("moved")).unwrap();
    let ch = index.apply_paths(&set(&[data.join("moved")])).unwrap();
    assert_eq!(check(&ch), (true, false, false));

    // The parent of the watched `deep` target removed.
    fs::remove_dir_all(data.join("sub")).unwrap();
    let ch = index.apply_paths(&set(&[data.join("sub")])).unwrap();
    assert_eq!(check(&ch), (true, false, true));

    // A file replaced by a directory of the same name.
    fs::remove_file(data.join("new.txt")).unwrap();
    write(&data.join("new.txt/inner.txt"), "i");
    let ch = index.apply_paths(&set(&[data.join("new.txt")])).unwrap();
    assert_eq!(check(&ch), (true, true, false));

    // Paths outside every root, and events for unchanged files, change nothing.
    write(&tmp.path().join("elsewhere.txt"), "e");
    let ch = index
        .apply_paths(&set(&[
            tmp.path().join("elsewhere.txt"),
            data.join("B.PNG"),
        ]))
        .unwrap();
    assert!(ch.is_empty());
}

/// Timing on a real directory, e.g.
/// `FILEDOCK_INDEX_BENCH_DIR=~/FileDockBench cargo test --release --lib index_timing -- --ignored --nocapture`
#[test]
#[ignore]
fn index_timing() {
    use std::time::Instant;
    let Ok(dir) = std::env::var("FILEDOCK_INDEX_BENCH_DIR") else {
        eprintln!("FILEDOCK_INDEX_BENCH_DIR not set");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let c = category("c", vec![target(Path::new(&dir), "*", true)]);
    let index = Index::open(&tmp.path().join("index.db"), std::slice::from_ref(&c)).unwrap();
    let t = Instant::now();
    index.ensure_indexed(&index.roots_for(&c)).unwrap();
    eprintln!("build:   {:>8.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    let t = Instant::now();
    let n = index.list(&c).unwrap().len();
    eprintln!(
        "list:    {:>8.0} ms ({n} files)",
        t.elapsed().as_secs_f64() * 1000.0
    );
    for level in [Level::Dirs, Level::Full, Level::Dirs] {
        let t = Instant::now();
        let changed = reconcile_at(&index, level);
        eprintln!(
            "{level:?}:{:>9.0} ms (changed: {changed})",
            t.elapsed().as_secs_f64() * 1000.0
        );
    }
}
