use assert_cmd::Command;
use std::fs;

#[test]
fn test_migrate_prefers_local_package_over_installed() {
    // The local `json` package shadows the standard library one, as a tree
    // being migrated shadows an installed copy of itself.
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("json");
    fs::create_dir(&package).unwrap();
    fs::write(package.join("__init__.py"), "").unwrap();
    fs::write(
        package.join("decoder.py"),
        r#"from dissolve import replace_me


@replace_me()
def old_decode(s):
    return new_decode(s)


def new_decode(s):
    return s
"#,
    )
    .unwrap();
    fs::write(
        package.join("user.py"),
        "from json.decoder import old_decode\n\nold_decode(\"x\")\n",
    )
    .unwrap();

    let output = Command::cargo_bin("dissolve")
        .unwrap()
        .current_dir(dir.path())
        .args(["migrate", "json/user.py"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        "# migration: json/user.py\nfrom json.decoder import old_decode\n\nnew_decode(\"x\")\n",
        String::from_utf8(output.stdout).unwrap()
    );
}
