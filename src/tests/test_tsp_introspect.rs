// Tests for the Type Server Protocol backend.
//
// These need a type server; they are skipped when pyright-typeserver is not
// installed (npm install -g pyright-typeserver).

use crate::tsp_introspect::{TspClient, DEFAULT_COMMAND};
use ruff_text_size::{TextRange, TextSize};
use std::process::{Command, Stdio};

fn have_type_server() -> bool {
    let available = Command::new(DEFAULT_COMMAND[0])
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok();
    if !available {
        eprintln!("skipping: {} not installed", DEFAULT_COMMAND[0]);
    }
    available
}

/// Write `files` into a temp project and return the type at `range` in the first.
fn infer_at_in(files: &[(&str, &str)], range: TextRange) -> Option<String> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create package dir");
        std::fs::write(&path, source).expect("write source");
    }
    let mut client =
        TspClient::new(Some(dir.path().to_str().unwrap())).expect("type server should start");
    client
        .query_type(&dir.path().join(files[0].0), module_of(files[0].0), range)
        .expect("query should succeed")
}

/// The module name a test file is migrated as.
fn module_of(file_name: &str) -> &str {
    file_name.strip_suffix(".py").expect("test files are .py")
}

fn infer_at(source: &str, range: TextRange) -> Option<String> {
    infer_at_in(&[("example.py", source)], range)
}

fn range_of(offset: usize, needle: &str) -> TextRange {
    TextRange::at(
        TextSize::try_from(offset).unwrap(),
        TextSize::try_from(needle.len()).unwrap(),
    )
}

/// Infer the type at the last occurrence of `needle`.
fn infer(source: &str, needle: &str) -> Option<String> {
    let offset = source.rfind(needle).expect("needle should occur in source");
    infer_at(source, range_of(offset, needle))
}

#[test]
fn test_infers_local_class_instance() {
    if !have_type_server() {
        return;
    }
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
    if !have_type_server() {
        return;
    }
    let source = r#"
import pathlib

p = pathlib.Path(".")
p.exists()
"#;
    assert_eq!(Some("pathlib.Path".to_string()), infer(source, "p"));
}

#[test]
fn test_infers_annotated_parameter() {
    if !have_type_server() {
        return;
    }
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
    if !have_type_server() {
        return;
    }
    let source = r#"
def f(x):
    return x
"#;
    assert_eq!(None, infer(source, "x"));
}

#[test]
fn test_no_class_name_for_union() {
    if !have_type_server() {
        return;
    }
    let source = r#"
class Repo:
    def close(self):
        pass

def f(repo: Repo | None):
    repo.close()
"#;
    assert_eq!(None, infer(source, "repo"));
}

#[test]
fn test_infers_nested_class() {
    if !have_type_server() {
        return;
    }
    let source = r#"
class Outer:
    class Inner:
        def m(self):
            pass

i = Outer.Inner()
i.m()
"#;
    assert_eq!(Some("example.Outer.Inner".to_string()), infer(source, "i"));
}

#[test]
fn test_infers_decorated_class() {
    if !have_type_server() {
        return;
    }
    let source = r#"
import dataclasses

@dataclasses.dataclass
class Point:
    x: int

p = Point(1)
p.x
"#;
    assert_eq!(Some("example.Point".to_string()), infer(source, "p"));
}

#[test]
fn test_infers_class_object() {
    // The receiver of a classmethod call is the class itself.
    if !have_type_server() {
        return;
    }
    let source = r#"
class Repo:
    @classmethod
    def open(cls):
        return cls()

Repo.open()
"#;
    let offset = source.rfind("Repo.open").unwrap();
    assert_eq!(
        Some("example.Repo".to_string()),
        infer_at(source, range_of(offset, "Repo"))
    );
}

#[test]
fn test_infers_class_from_package_module() {
    if !have_type_server() {
        return;
    }
    let main = r#"
from pkg.sub import Repo

r = Repo()
r.close()
"#;
    let sub = r#"
class Repo:
    def close(self):
        pass
"#;
    let offset = main.rfind("r.close").unwrap();
    assert_eq!(
        Some("pkg.sub.Repo".to_string()),
        infer_at_in(
            &[
                ("main.py", main),
                ("pkg/__init__.py", ""),
                ("pkg/sub.py", sub),
            ],
            range_of(offset, "r"),
        )
    );
}

#[test]
fn test_infers_unannotated_factory_return() {
    if !have_type_server() {
        return;
    }
    let source = r#"
class Resource:
    def close(self):
        pass

def open_resource():
    return Resource()

x = open_resource()
x.close()
"#;
    let offset = source.find("open_resource()\n").unwrap();
    assert_eq!(
        Some("example.Resource".to_string()),
        infer_at(source, range_of(offset, "open_resource()"))
    );
}

#[test]
fn test_with_binding_when_enter_returns_other_class() {
    if !have_type_server() {
        return;
    }
    let source = r#"
class Handle:
    def close(self):
        pass

class Opener:
    def __enter__(self):
        return Handle()

    def __exit__(self, *args):
        pass

def make_opener():
    return Opener()

with make_opener() as h:
    h.close()
"#;
    let offset = source.rfind("h.close").unwrap();
    // Must be Handle, never Opener.
    assert_eq!(
        Some("example.Handle".to_string()),
        infer_at(source, range_of(offset, "h"))
    );
}

#[test]
fn test_sees_updated_content() {
    if !have_type_server() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("example.py");
    let before = "class A: pass\nclass B: pass\nx = A()\nx\n";
    std::fs::write(&path, before).expect("write source");

    let mut client =
        TspClient::new(Some(dir.path().to_str().unwrap())).expect("type server should start");
    let range = |source: &str| range_of(source.rfind('x').unwrap(), "x");
    assert_eq!(
        Some("example.A".to_string()),
        client.query_type(&path, "example", range(before)).unwrap()
    );

    let after = "class A: pass\nclass B: pass\nx = B()\nx\n";
    client.set_file_content(&path, after).unwrap();
    assert_eq!(
        Some("example.B".to_string()),
        client.query_type(&path, "example", range(after)).unwrap()
    );
}

#[test]
fn test_names_local_classes_after_callers_module() {
    // The file's location says `example`, but it is being migrated as `pkg.real`.
    if !have_type_server() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("example.py");
    let source = "class Repo:\n    pass\n\nr = Repo()\nr\n";
    std::fs::write(&path, source).expect("write source");

    let mut client =
        TspClient::new(Some(dir.path().to_str().unwrap())).expect("type server should start");
    let range = range_of(source.rfind('r').unwrap(), "r");
    assert_eq!(
        Some("pkg.real.Repo".to_string()),
        client.query_type(&path, "pkg.real", range).unwrap()
    );
}
