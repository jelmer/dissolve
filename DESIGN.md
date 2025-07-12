# Dissolve Design Document

## Overview

Dissolve is a Python library that helps developers migrate from deprecated APIs to their replacements. It provides a comprehensive solution for managing API deprecations through runtime warnings and automated code migration tools.

## Core Purpose

The library addresses the common problem of API deprecation by providing:
- A decorator to mark deprecated functions with suggested replacements
- Command-line tools to automatically migrate codebases
- Validation tools to ensure deprecations can be properly migrated
- Utilities to clean up deprecated decorators after migration

## Architecture

### High-Level Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   CLI Interface │    │  CST Processing │    │ Decorator System│
│   (__main__.py) │    │    Pipeline     │    │ (decorators.py) │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────┐
         │             Migration Engine                    │
         │              (migrate.py)                       │
         └─────────────────────────────────────────────────┘
                                 │
    ┌────────────────────────────┼────────────────────────────┐
    │                            │                            │
┌─────────────┐    ┌─────────────────┐    ┌─────────────────┐
│ Validation  │    │   CST Utils     │    │  Import Utils   │
│ (check.py)  │    │ (libcst-based)  │    │(import_utils.py)│
└─────────────┘    └─────────────────┘    └─────────────────┘
```

### Core Components

#### 1. Decorator System (`decorators.py`)
The `@replace_me` decorator marks deprecated functions and provides runtime warnings:
```python
@replace_me(since="1.0.0", remove_in="2.0.0")
def deprecated_function(x, y):
    return new_function(x, y=y)
```

**Responsibilities:**
- Runtime deprecation warnings
- Metadata storage for migration tools
- AST-based analysis to extract replacement expressions (still uses ast for runtime warnings)

#### 2. CST Processing Pipeline
A collection of modules that parse, analyze, and transform Python code using libcst (Concrete Syntax Tree):

- **Collector** (`collector.py`): Discovers `@replace_me` decorated functions and extracts replacement information using CST visitors
- **Replacer** (`replacer.py`): Transforms function calls to use replacement expressions while preserving formatting
- **CST-based processing**: Leverages libcst for format-preserving transformations
- **Legacy AST Utilities** (`ast_utils.py`): Retained for backward compatibility but no longer actively used

#### 3. Migration Engine (`migrate.py`)
The core migration logic that orchestrates the transformation process:
- Cross-file migration with import resolution using libcst
- Interactive mode for user confirmation with position tracking via CST metadata
- Module resolver system for handling dependencies
- Format-preserving transformations to maintain code style

#### 4. Command-Line Interface (`__main__.py`)
Four main commands:
- `dissolve migrate`: Automatically replace deprecated calls (for library users)
- `dissolve cleanup`: Remove deprecated functions entirely (for library maintainers)
- `dissolve check`: Validate that decorators can be migrated
- `dissolve info`: List all deprecated functions and replacements

#### 5. Validation and Analysis
- **Check** (`check.py`): Validates that `@replace_me` functions can be processed using libcst
- **Context Analyzer** (`context_analyzer.py`): Analyzes local definitions and imports (libcst-based)
- **Import Utils** (`import_utils.py`): Manages import requirements and dependencies using CST visitors

## Key Data Structures

### Core Types (`types.py`)
```python
class Replacement(Protocol):
    """Protocol for replacement information"""
    name: str
    replacement: str

class ReplaceInfo:
    """Contains function name and replacement expression template"""
    name: str
    replacement: str

class ImportRequirement:
    """Represents needed imports for replacements"""
    module: str
    names: list[str]
```

### Error Handling
```python
class ReplacementExtractionError(Exception):
    """Raised when a function body can't be processed"""
    
class ReplacementFailureReason(Enum):
    """Categorizes why extraction failed"""
    COMPLEX_BODY = "complex_body"
    RECURSIVE_CALL = "recursive_call"
    NO_RETURN = "no_return"
```

## Workflows

### 1. Migration Workflow
```
Source Code Input
    ↓
Parse CST (libcst) → Collect @replace_me functions → Extract replacement expressions
    ↓
Find function calls → Match with replacements → Substitute arguments
    ↓
Transform CST nodes → Generate format-preserving code → Output migrated code
```

### 2. Validation Workflow
```
Source Code Input
    ↓
Parse CST → Find @replace_me functions → Validate function bodies
    ↓
Check for complex bodies/recursive calls → Report errors/success
```

### 3. Function Cleanup Workflow (for library maintainers)
```
Source Code Input
    ↓
Parse CST → Find @replace_me functions → Check version constraints
    ↓
Remove matching functions entirely → Output cleaned code
```

## Design Patterns

### CST Visitor Pattern
Extensive use of libcst visitors and transformers:
- `DeprecatedFunctionCollector` (cst.CSTVisitor): Collects deprecated function information
- `FunctionCallReplacer` (cst.CSTTransformer): Transforms function calls while preserving formatting
- `ReplaceRemover` (cst.CSTTransformer): Removes deprecated functions cleanly
- `ContextAnalyzer` (cst.CSTVisitor): Analyzes module context with metadata support

### Strategy Pattern
Different migration strategies:
- `FunctionCallReplacer`: Automatic replacement
- `InteractiveFunctionCallReplacer`: User-confirmed replacement

### Template Method Pattern
Common file processing logic in `_process_files_common()` with shared:
- Validation patterns
- Output formatting
- Error handling

## Advanced Features

### Cross-Module Migration
Resolves imports to find deprecated functions in other modules:
```python
# Can migrate calls to deprecated functions in imported modules
from other_module import deprecated_func
result = deprecated_func(x, y)  # Will be replaced
```

### Version-Aware Removal
Uses semantic versioning to determine when to remove decorators:
```python
@replace_me(since="1.0.0", remove_in="2.0.0")  # Removed when version >= 2.0.0
```

### Interactive Mode
Allows selective migration with user confirmation:
```bash
dissolve migrate --interactive mycode.py
# Prompts: Replace deprecated_func(x, y) with new_func(x, y=y)? [y/N]
```

### Context-Aware Analysis
Understands the difference between local definitions and imports:
- Analyzes local variable scope
- Tracks import statements
- Resolves naming conflicts

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

The test suite covers:
- CST transformation correctness
- CLI interface behavior
- Error handling scenarios
- Edge cases in Python syntax
- Cross-module dependency resolution
- Format preservation validation

## Migration to libcst

The library now uses libcst (Concrete Syntax Tree) instead of Python's built-in ast module for all transformation operations. Key changes:

- **Format Preservation**: libcst preserves comments, whitespace, and original formatting
- **Metadata Support**: Position tracking for interactive mode via `cst.MetadataWrapper`
- **Cleaner Transformations**: Uses `cst.RemovalSentinel.REMOVE` for node removal
- **Hybrid Architecture**: libcst for migrations, ast still used in decorators.py for runtime warnings
- **Optional Dependency**: libcst is only required for migration features (`migrate` extra)
