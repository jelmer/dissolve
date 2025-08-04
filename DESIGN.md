# Dissolve Design Document

## Overview

Dissolve is a hybrid Python/Rust tool that helps developers migrate from deprecated APIs to their replacements. The core migration and analysis functionality has been rewritten in Rust for improved performance, while the `@replace_me` decorator remains in Python for runtime deprecation warnings.

## Core Purpose

The tool addresses the common problem of API deprecation by providing:
- A Python decorator (`@replace_me`) to mark deprecated functions with runtime warnings
- A Rust-based CLI tool for fast code analysis and transformation
- Automated migration of deprecated function calls to their replacements
- Validation tools to ensure deprecations can be properly migrated
- Version-aware cleanup utilities for library maintainers

## Architecture

### Rust/Python Split

The project maintains a clear separation between Rust and Python components:

**Rust Components** (Performance-critical operations):
- CLI binary and command parsing
- AST parsing using Ruff parser
- Code transformation and migration logic
- Type introspection integration (Pyright/MyPy)
- File scanning and pattern matching
- Deprecated function collection and analysis

**Python Components** (Runtime functionality):
- `@replace_me` decorator for runtime warnings
- Python AST analysis for decorator metadata extraction
- Integration with Python's warning system

### Key Design Decisions

1. **Ruff Parser**: Uses Ruff's Python parser for fast, accurate AST parsing
2. **Type Introspection**: Integrates with Pyright LSP and MyPy daemon for type-aware replacements
3. **Format Preservation**: Maintains original code formatting through careful AST manipulation
4. **No Configuration**: Works out-of-the-box without configuration files
5. **Parallel Processing**: Leverages Rust's concurrency for large codebases

### Core Components (Rust Implementation)

#### 1. CLI Binary (`src/bin/main.rs`)
The main entry point providing four commands:
- `migrate`: Replace deprecated function calls with their replacements
- `cleanup`: Remove deprecated functions based on version constraints
- `check`: Validate that deprecated functions can be migrated
- `info`: List all deprecated functions and their replacements

#### 2. Migration Engine (`src/migrate_ruff.rs`)
Orchestrates the complete migration process:
- Parses Python source using Ruff parser
- Collects deprecated functions from current file and dependencies
- Applies type-aware transformations
- Supports both automatic and interactive modes
- Preserves code formatting through AST manipulation

#### 3. Function Collection (`src/core/ruff_collector.rs`)
Discovers and analyzes `@replace_me` decorated functions:
- Extracts replacement expressions from function bodies
- Handles various Python constructs (functions, methods, properties)
- Collects parameter information and metadata
- Tracks inheritance relationships for method resolution

#### 4. AST Transformation (`src/ruff_parser_improved.rs`)
Performs the actual code transformations:
- Identifies deprecated function calls
- Maps arguments to replacement expressions
- Handles complex cases like method calls, chained calls
- Integrates with type introspection for accurate replacements
- Preserves original code structure and formatting

#### 5. Type Introspection (`src/type_introspection_context.rs`)
Provides type information for accurate replacements:
- **Pyright Integration** (`src/pyright_lsp.rs`): LSP-based type checking
- **MyPy Integration** (`src/mypy_lsp.rs`): Daemon-based type analysis
- Falls back gracefully when type checkers unavailable
- Caches type information for performance

#### 6. Python Decorator (Python Component)
The `@replace_me` decorator remains in Python:
```python
@replace_me(since="1.0.0", remove_in="2.0.0")
def deprecated_function(x, y):
    return new_function(x, y=y)
```
- Provides runtime deprecation warnings
- Uses Python's AST for metadata extraction
- Integrates with Python's warning system

## Key Data Structures

### Core Types (Rust)

```rust
// src/core/types.rs
pub struct ReplaceInfo {
    pub old_name: String,
    pub replacement_expr: String,
    pub replacement_ast: Option<Box<ruff_python_ast::Expr>>,
    pub construct_type: ConstructType,
    pub parameters: Vec<ParameterInfo>,
    pub return_type: Option<String>,
    pub since: Option<String>,
    pub remove_in: Option<String>,
    pub message: Option<String>,
}

pub enum ConstructType {
    Function,
    Property,
    ClassMethod,
    StaticMethod,
    AsyncFunction,
    Class,
    ClassAttribute,
    ModuleAttribute,
}

pub struct ParameterInfo {
    pub name: String,
    pub has_default: bool,
    pub default_value: Option<String>,
    pub is_vararg: bool,   // *args
    pub is_kwarg: bool,    // **kwargs
    pub is_kwonly: bool,   // keyword-only
}
```

### Collection Results

```rust
pub struct CollectionResult {
    pub replacements: HashMap<String, ReplaceInfo>,
    pub unreplaceable: HashMap<String, UnreplaceableConstruct>,
    pub inheritance_map: HashMap<String, Vec<String>>,
}

pub struct UnreplaceableConstruct {
    pub construct_type: ConstructType,
    pub reason: ReplacementFailureReason,
    pub message: String,
}
```

## Workflows

### 1. Migration Workflow (Rust)
```
Python Source File
    ↓
Ruff Parser → AST Generation → Collect @replace_me functions
    ↓
Dependency Analysis → Collect functions from imported modules
    ↓
AST Visitor → Find deprecated calls → Type introspection (if needed)
    ↓
Argument Mapping → Generate replacement AST → Apply transformations
    ↓
Code Generation → Format preservation → Output migrated code
```

### 2. Type-Aware Resolution
```
Function Call Found
    ↓
Check if method call → Query type checker (Pyright/MyPy)
    ↓
Resolve actual type → Find matching replacement
    ↓
Apply type-specific transformation
```

### 3. Interactive Mode
```
Find replacement opportunity
    ↓
Calculate line/column position → Show context to user
    ↓
Prompt for confirmation → Apply if approved
    ↓
Continue to next occurrence
```

## Technology Stack

### Rust Dependencies

1. **AST Parsing**
   - `ruff_python_parser`: Fast Python parser from the Ruff project
   - `ruff_python_ast`: AST node definitions
   - `ruff_python_codegen`: Code generation from AST
   - `ruff_text_size`: Text position tracking

2. **CLI and I/O**
   - `clap`: Command-line argument parsing with derive macros
   - `glob`: File pattern matching
   - `anyhow`/`thiserror`: Error handling

3. **Type Checking Integration**
   - `pyo3`: Python interop for MyPy integration
   - Custom LSP clients for Pyright/MyPy
   - `serde`/`serde_json`: LSP message serialization

4. **Utilities**
   - `regex`: Pattern matching for file scanning
   - `once_cell`: Lazy static initialization
   - `tracing`: Structured logging

### Python Components

- Standard library only for the decorator module
- No external dependencies required for runtime functionality

## Advanced Features

### Type-Aware Method Resolution
Uses type checkers to resolve method calls correctly:
```python
# Detects that obj is of type Foo and finds the right replacement
obj = get_foo()
result = obj.deprecated_method()  # Correctly replaced based on type
```

### Inheritance Tracking
Tracks class hierarchies to handle inherited deprecated methods:
```python
class Parent:
    @replace_me(...)
    def old_method(self): ...

class Child(Parent):
    pass

Child().old_method()  # Correctly identifies and replaces
```

### Cross-Module Dependency Analysis
- Recursively analyzes imported modules (configurable depth)
- Builds a complete map of available replacements
- Handles various import styles (from, import as, etc.)

### Parallel Processing
- File discovery and initial scanning done in parallel
- Type checking queries can be batched
- Large codebases processed efficiently

## Error Handling Philosophy

### Graceful Degradation
- Falls back to original code when replacement fails
- Preserves functionality even when migration isn't possible
- Provides detailed error messages for debugging

### Comprehensive Validation
- Checks for complex function bodies that can't be automatically migrated
- Detects recursive calls that would cause infinite loops
- Validates that replacement expressions are syntactically correct

### Preview-First Design
- Default behavior shows changes without applying them
- Requires explicit `--write` flag to modify files
- Provides diff output for review

## CLI Design Philosophy

### Correctness
- Ensures transformations are correct and safe
  (only apply replacements when types match, don't simply )

### Safety First
- Preview mode by default
- Explicit write operations
- Comprehensive validation before changes

### Developer Experience
- Rich help text and examples
- Multiple output formats (diff, summary, detailed)
- Interactive mode for complex migrations
- Batch processing for large codebases

### Integration Friendly
- Exit codes for CI/CD integration
- Machine-readable output options
- Configurable behavior through command-line flags

## Testing Strategy

The Rust implementation includes comprehensive test coverage:
- Unit tests for each component
- Integration tests for complete workflows
- Regression tests for edge cases and bug fixes
- Real-world scenario tests (e.g., Dulwich migration)
- Format preservation validation
- Type checker integration tests

## Python/Rust Boundary

### What Stays in Python

1. **The `@replace_me` decorator** must remain in Python because:
   - It runs at import time in user code
   - It needs to emit Python warnings
   - It must be importable by Python projects
   - It uses Python's AST for runtime analysis

2. **Runtime functionality**:
   - Warning emission
   - Decorator parameter validation
   - Integration with Python's warning filters

### What Moved to Rust

1. **All CLI commands and file processing**
2. **AST parsing and transformation** using Ruff parser
3. **Type checking integration** via LSP/daemon
4. **Performance-critical operations** like file scanning
5. **Cross-module dependency analysis**

### Integration Points

- The Rust tool can read decorator metadata from Python files
- Type checkers are invoked via LSP or daemon protocols
- PyO3 is used for Python interop when needed

## Performance Improvements

The Rust migration provides significant performance benefits:

- **File Scanning**: ~10x faster with parallel processing
- **AST Parsing**: Ruff parser is much faster than Python's ast
- **Memory Usage**: Lower memory footprint
- **Type Checking**: Efficient caching and batching
- **Large Codebases**: Scales better with parallel processing
