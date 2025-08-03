// Copyright (C) 2024 Jelmer Vernooij <jelmer@samba.org>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::migrate_ruff::migrate_file;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::{ConstructType, RuffDeprecatedFunctionCollector, TypeIntrospectionMethod};
use std::collections::HashMap;
use std::path::Path;

#[test]
fn test_wrapper_class_collector() {
    let source = r#"
from dissolve import replace_me

class UserManager:
    def __init__(self, database_url, cache_size=100):
        self.db = database_url
        self.cache = cache_size

@replace_me(since="2.0.0")
class UserService:
    def __init__(self, database_url, cache_size=50):
        self._manager = UserManager(database_url, cache_size * 2)
    
    def get_user(self, user_id):
        return self._manager.get_user(user_id)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    // Should detect the UserService class
    assert!(result.replacements.contains_key("test_module.UserService"));
    let replacement = &result.replacements["test_module.UserService"];
    assert_eq!(replacement.construct_type, ConstructType::Class);
    assert_eq!(
        replacement.replacement_expr,
        "UserManager({database_url}, {cache_size} * 2)"
    );
}

#[test]
fn test_wrapper_class_migration() {
    let source = r#"
from dissolve import replace_me

class UserManager:
    def __init__(self, database_url, cache_size=100):
        self.db = database_url
        self.cache = cache_size

@replace_me(since="2.0.0")
class UserService:
    def __init__(self, database_url, cache_size=50):
        self._manager = UserManager(database_url, cache_size * 2)
    
    def get_user(self, user_id):
        return self._manager.get_user(user_id)

# Test instantiations
service = UserService("postgres://localhost")
admin_service = UserService("mysql://admin", cache_size=100)
services = [UserService(url) for url in ["db1", "db2"]]
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    println!("Replacements: {:?}", result.replacements);

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // Should replace class instantiations with the wrapper target
    // For the first call with no explicit cache_size, it should omit the parameter entirely
    // since UserManager's default (100) equals UserService's default (50) * 2
    assert!(migrated.contains(r#"service = UserManager("postgres://localhost")"#));
    // For the explicit cache_size, it should substitute the value
    assert!(migrated.contains(r#"admin_service = UserManager("mysql://admin", 100 * 2)"#));
    // For the comprehension with no explicit cache_size, it should omit the parameter
    assert!(migrated.contains(r#"services = [UserManager(url) for url in ["db1", "db2"]]"#));

    // Should not replace the class definition itself
    assert!(migrated.contains("@replace_me(since=\"2.0.0\")"));
    assert!(migrated.contains("class UserService:"));
}

#[test]
fn test_wrapper_class_with_kwargs() {
    let source = r#"
from dissolve import replace_me

class Database:
    def __init__(self, url, timeout=30):
        self.url = url
        self.timeout = timeout

@replace_me(since="1.5.0")
class LegacyDB:
    def __init__(self, url, timeout=10):
        self._db = Database(url, timeout + 20)

# Test with keyword args
db = LegacyDB("postgres://localhost", timeout=15)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should replace with correct timeout calculation
    assert!(migrated.contains(r#"db = Database("postgres://localhost", 15 + 20)"#));
}

#[test]
fn test_class_with_no_init_replacement() {
    let source = r#"
from dissolve import replace_me

@replace_me()
class OldClass:
    def method(self):
        return "old"

# This should not be migrated since there's no clear replacement pattern
obj = OldClass()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    // Class should be detected but without a clear replacement
    assert!(
        result.replacements.contains_key("test_module.OldClass")
            || result.unreplaceable.contains_key("test_module.OldClass")
    );
}

#[test]
fn test_wrapper_class_in_comprehensions() {
    let source = r#"
from dissolve import replace_me

class NewAPI:
    def __init__(self, name):
        self.name = name

@replace_me()
class OldAPI:
    def __init__(self, name):
        self._api = NewAPI(name.upper())

# Test in various comprehensions
apis = [OldAPI(name) for name in ["test", "prod"]]
api_dict = {name: OldAPI(name) for name in ["a", "b"]}
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should replace in comprehensions
    assert!(migrated.contains(r#"apis = [NewAPI(name.upper()) for name in ["test", "prod"]]"#));
    assert!(migrated.contains(r#"api_dict = {name: NewAPI(name.upper()) for name in ["a", "b"]}"#));
}
