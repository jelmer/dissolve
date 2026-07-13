// Tests for the ty-based type introspection backend

use crate::ty_introspect::TyTypeIntrospector;
use ruff_text_size::{TextRange, TextSize};
use std::io::Write;

/// Write `source` into a temp project and return the type ty infers at `range`.
fn infer_at(source: &str, range: TextRange) -> Option<String> {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("example.py");
    let mut file = std::fs::File::create(&path).expect("create source file");
    file.write_all(source.as_bytes()).expect("write source");
    file.sync_all().expect("flush source");

    let introspector = TyTypeIntrospector::new(Some(dir.path().to_str().unwrap()))
        .expect("introspector should initialize");
    introspector
        .query_type(&path, range)
        .expect("query should succeed")
}

/// Infer the type at the last occurrence of `needle`.
///
/// Uses the last occurrence so a receiver can be selected by name without
/// matching its own assignment earlier in the file.
fn infer(source: &str, needle: &str) -> Option<String> {
    let offset = source.rfind(needle).expect("needle should occur in source");
    let range = TextRange::at(
        TextSize::try_from(offset).unwrap(),
        TextSize::try_from(needle.len()).unwrap(),
    );
    infer_at(source, range)
}

#[test]
fn test_infers_local_class_instance() {
    let source = r#"
class Repo:
    def get_worktree(self):
        return self

repo = Repo()
repo.get_worktree()
"#;
    assert_eq!(Some("example.Repo".to_string()), infer(source, "repo"));
}

#[test]
fn test_infers_stdlib_class_instance() {
    let source = r#"
import pathlib

p = pathlib.Path(".")
p.exists()
"#;
    assert_eq!(Some("pathlib.Path".to_string()), infer(source, "p"));
}

#[test]
fn test_infers_annotated_parameter() {
    let source = r#"
class Repo:
    pass

def process(repo: Repo):
    repo.commit()
"#;
    assert_eq!(Some("example.Repo".to_string()), infer(source, "repo"));
}

#[test]
fn test_no_class_name_for_unannotated_parameter() {
    // An unannotated parameter is Unknown, not an instance of any class.
    let source = r#"
def f(x):
    return x
"#;
    assert_eq!(None, infer(source, "x"));
}

/// Source exercising a return type ty has to infer rather than read off an annotation.
const UNANNOTATED_RETURN: &str = r#"
class Resource:
    def close(self):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *args):
        pass

def open_resource():
    return Resource()

with open_resource() as r:
    r.close()
"#;

/// Range covering the `r` receiver in `r.close()`.
fn with_binding_range() -> TextRange {
    let offset = UNANNOTATED_RETURN.rfind("r.close").unwrap();
    TextRange::at(TextSize::try_from(offset).unwrap(), TextSize::from(1))
}

#[test]
fn test_unannotated_return_is_not_inferred() {
    // ty 0.0.59 does not infer un-annotated return types, so `open_resource()`
    // is Unknown and the `with` binding has no class name. Pyright infers this.
    // Annotating `-> Resource` makes ty resolve it.
    assert_eq!(None, infer_at(UNANNOTATED_RETURN, with_binding_range()));
}

#[test]
#[ignore = "ty does not yet infer un-annotated return types"]
fn test_infers_with_statement_binding() {
    assert_eq!(
        Some("example.Resource".to_string()),
        infer_at(UNANNOTATED_RETURN, with_binding_range())
    );
}

#[test]
fn test_infers_with_statement_binding_when_annotated() {
    // The same code resolves once the return type is annotated.
    let source = r#"
class Resource:
    def close(self):
        pass

    def __enter__(self) -> "Resource":
        return self

    def __exit__(self, *args):
        pass

def open_resource() -> Resource:
    return Resource()

with open_resource() as r:
    r.close()
"#;
    let offset = source.rfind("r.close").unwrap();
    let range = TextRange::at(TextSize::try_from(offset).unwrap(), TextSize::from(1));
    assert_eq!(
        Some("example.Resource".to_string()),
        infer_at(source, range)
    );
}
