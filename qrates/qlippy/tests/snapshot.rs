use std::fs;

use qargo_tools::adapter;
use qargo_tools::report::coordinates;
use qargo_tools::snapshot::{Files, FrozenSources, collect_tree, digest_files, materialize};

const IDENTITY: &str = "pub unitary fn identity(q: Q<Bit>) -> Q<Bit> { q }\n";

#[test]
fn changed_working_copies_cannot_change_the_subject_or_later_checks() {
    let sources = FrozenSources::from_files(Files::from([(
        "library.qli".into(),
        IDENTITY.as_bytes().to_vec(),
    )]))
    .unwrap();
    let original_id = sources.source_id().to_owned();
    let first = sources.stage().unwrap();
    fs::write(first.root().join("library.qli"), "invalid syntax").unwrap();
    fs::write(first.root().join("extra.qli"), "invalid syntax").unwrap();

    let second = sources.stage().unwrap();
    assert_ne!(first.root(), second.root());
    assert_eq!(collect_tree(second.root(), None).unwrap(), *sources.files());
    assert_eq!(sources.source_id(), original_id);
    assert_eq!(sources.files()["library.qli"], IDENTITY.as_bytes());
    let checked = adapter::check(&sources).unwrap();
    assert_eq!(checked.qleisli_check()["status"], "passed");
    assert_eq!(checked.source_count(), 1);
    assert_eq!(checked.module_index()[0]["path"], "library.qli");
}

#[test]
fn empty_capture_has_no_compiler_or_bundled_module_claim() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".gitkeep"), b"").unwrap();
    let sources = FrozenSources::capture(root.path()).unwrap();
    let checked = adapter::check(&sources).unwrap();
    assert_eq!(checked.source_count(), 0);
    assert!(checked.project().is_none());
    assert_eq!(checked.qleisli_check()["status"], "not_run");
    assert_eq!(checked.qleisli_check()["reason"], "no_sources");
    assert_eq!(checked.module_index(), &serde_json::json!([]));
}

#[test]
fn frozen_bytes_survive_mutation_and_deletion_of_original_inputs() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("library.qli");
    fs::write(&file, IDENTITY).unwrap();
    let sources = FrozenSources::capture(root.path()).unwrap();
    let original_id = sources.source_id().to_owned();
    fs::write(&file, "unitary fn broken(q: Q<Bit>) -> Q<Bit> { q; q }").unwrap();
    let changed = FrozenSources::capture(root.path()).unwrap();
    assert_ne!(changed.source_id(), original_id);
    assert!(adapter::check(&changed).is_err());
    fs::remove_file(file).unwrap();
    let checked = adapter::check(&sources).unwrap();
    assert_eq!(checked.qleisli_check()["status"], "passed");
    assert_eq!(checked.module_index()[0]["path"], "library.qli");
    assert_eq!(
        checked.module_index()[0]["declarations"][0]["name"],
        "identity"
    );
}

#[test]
fn identity_is_portable_order_independent_and_byte_exact() {
    let mut files = Files::new();
    files.insert("nested/b.qli".into(), IDENTITY.as_bytes().to_vec());
    files.insert("a.qli".into(), IDENTITY.as_bytes().to_vec());
    let left = materialize(&files).unwrap();
    let right = materialize(&files).unwrap();
    let a = FrozenSources::capture(left.path()).unwrap();
    let b = FrozenSources::capture(right.path()).unwrap();
    assert_eq!(a.source_id(), b.source_id());
    assert_eq!(a.files(), b.files());
    fs::write(right.path().join("a.qli"), IDENTITY.replace('\n', "\r\n")).unwrap();
    assert_ne!(
        a.source_id(),
        FrozenSources::capture(right.path()).unwrap().source_id()
    );
    let mut renamed = files.clone();
    let bytes = renamed.remove("a.qli").unwrap();
    renamed.insert("c.qli".into(), bytes);
    assert_ne!(
        digest_files("qleisli.source.v1", &files),
        digest_files("qleisli.source.v1", &renamed)
    );
    assert_ne!(
        digest_files("domain-a", &files),
        digest_files("domain-b", &files)
    );
}

#[test]
fn private_items_and_bundled_modules_do_not_enter_public_index() {
    let mut files = Files::new();
    files.insert(
        "library.qli".into(),
        format!("{IDENTITY}unitary fn hidden(q: Q<Bit>) -> Q<Bit> {{ q }}").into_bytes(),
    );
    let sources = FrozenSources::from_files(files).unwrap();
    let index = adapter::check(&sources).unwrap().module_index().clone();
    assert_eq!(index.as_array().unwrap().len(), 1);
    assert_eq!(index[0]["declarations"].as_array().unwrap().len(), 1);
    assert_eq!(index[0]["name"], "library");
}

#[test]
fn compiler_diagnostics_have_original_coordinates_and_no_temporary_path() {
    let source = "// 日本語 🦀\r\nunitary fn bad(q: Q<Bit>) -> Q<Bit> { let a = q; q }\r\n";
    let mut files = Files::new();
    files.insert("bad.qli".into(), source.as_bytes().to_vec());
    let sources = FrozenSources::from_files(files).unwrap();
    let diagnostic = adapter::check(&sources).err().unwrap();
    assert_eq!(diagnostic.category, "compiler");
    assert_eq!(diagnostic.id, "ownership");
    assert!(!diagnostic.message.contains("qargo-snapshot-"));
    let location = diagnostic.primary.unwrap();
    assert_eq!(location.path, "bad.qli");
    assert_eq!(
        (location.line, location.column),
        coordinates(source, location.start)
    );
    assert!(source.is_char_boundary(location.start));
    assert!(source.is_char_boundary(location.end));
}

#[test]
fn coordinate_convention_matches_crlf_and_scalar_positions() {
    let source = "日🦀\r\nx\ry\nz";
    for (needle, expected) in [
        ("日", (1, 1)),
        ("🦀", (1, 2)),
        ("x", (2, 1)),
        ("y", (3, 1)),
        ("z", (4, 1)),
    ] {
        assert_eq!(coordinates(source, source.find(needle).unwrap()), expected);
    }
}

#[test]
fn capture_rejects_invalid_source_bytes_at_the_normal_checker() {
    let mut files = Files::new();
    files.insert("bad.qli".into(), vec![0xff]);
    let sources = FrozenSources::from_files(files).unwrap();
    let diagnostic = adapter::check(&sources).err().unwrap();
    assert_eq!(diagnostic.category, "compiler");
    assert_eq!(diagnostic.id, "project");
}

#[test]
fn source_capture_applies_byte_limits_and_rejects_path_aliases() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("large.qli"), vec![b' '; (1 << 20) + 1]).unwrap();
    assert_eq!(
        FrozenSources::capture(root.path()).err().unwrap().id,
        "limit"
    );
    let mut files = Files::new();
    files.insert("../escaped.qli".into(), vec![]);
    assert!(FrozenSources::from_files(files).is_err());
    let mut files = Files::new();
    files.insert("wrong.rs".into(), vec![]);
    assert!(FrozenSources::from_files(files).is_err());
}

#[cfg(unix)]
#[test]
fn capture_rejects_source_and_non_source_symlinks() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    fs::write(other.path().join("library.qli"), IDENTITY).unwrap();
    symlink(
        other.path().join("library.qli"),
        root.path().join("linked.qli"),
    )
    .unwrap();
    assert!(FrozenSources::capture(root.path()).is_err());
    fs::remove_file(root.path().join("linked.qli")).unwrap();
    symlink(other.path(), root.path().join("linked_directory")).unwrap();
    assert!(collect_tree(root.path(), None).is_err());
}

#[test]
fn traversal_budget_counts_directories_at_the_entry_boundary() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("module.qli"), IDENTITY).unwrap();
    for index in 0..4095 {
        fs::create_dir(root.path().join(format!("directory-{index}"))).unwrap();
    }
    let captured = FrozenSources::capture(root.path()).unwrap();
    assert_eq!(captured.count(), 1);
    fs::create_dir(root.path().join("one-directory-too-many")).unwrap();
    let diagnostic = FrozenSources::capture(root.path()).err().unwrap();
    assert_eq!(diagnostic.id, "limit");
    assert_eq!(diagnostic.message, "Input contains more than 4096 entries.");
}

#[test]
fn traversal_budget_counts_extension_filtered_files_at_the_entry_boundary() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("module.qli"), IDENTITY).unwrap();
    for index in 0..4095 {
        fs::write(root.path().join(format!("ignored-{index}.txt")), "ignored").unwrap();
    }
    let captured = FrozenSources::capture(root.path()).unwrap();
    assert_eq!(captured.count(), 1);
    fs::write(root.path().join("one-file-too-many.txt"), "ignored").unwrap();
    let diagnostic = FrozenSources::capture(root.path()).err().unwrap();
    assert_eq!(diagnostic.id, "limit");
    assert_eq!(diagnostic.message, "Input contains more than 4096 entries.");
}

#[test]
fn traversal_depth_limit_retains_the_64_directory_boundary() {
    let root = tempfile::tempdir().unwrap();
    let mut directory = root.path().to_path_buf();
    for _ in 0..64 {
        directory.push("nested");
        fs::create_dir(&directory).unwrap();
    }
    fs::write(directory.join("module.qli"), IDENTITY).unwrap();
    assert_eq!(FrozenSources::capture(root.path()).unwrap().count(), 1);
    fs::create_dir(directory.join("too-deep")).unwrap();
    let diagnostic = FrozenSources::capture(root.path()).err().unwrap();
    assert_eq!(diagnostic.id, "limit");
    assert_eq!(diagnostic.message, "Input directory depth exceeds 64.");
}
