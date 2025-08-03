# Dissolve

A powerful library and CLI tool for automatically migrating deprecated Python code by replacing function calls with their updated implementations.

## 🎯 What Does Dissolve Do?

Dissolve helps you migrate from deprecated Python APIs by:
- **Automatically replacing** deprecated function calls with their modern equivalents
- **Supporting magic methods** (str, repr, len, bool, int, float, bytes, hash)
- **Preserving code formatting** and comments during migration
- **Providing type-aware replacements** using static analysis

## 📦 Installation

### Basic Installation (Python decorator only)
```bash
pip install dissolve
```

### Full Installation (CLI tool + migration features)
```bash
# Install Rust toolchain first
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build and install dissolve CLI
cargo install --path .
```

## 🚀 Quick Start

### 1. Mark Deprecated Functions

```python
from dissolve import replace_me

@replace_me(since="1.0.0", remove_in="2.0.0")
def old_checkout(repo, branch, force=False):
    """Deprecated: Use checkout() instead."""
    return checkout(repo, branch, force=force)

def checkout(repo, branch, force=False):
    """New implementation."""
    # ... modern implementation
```

### 2. Run Migration

```bash
# Migrate a single file
dissolve migrate path/to/file.py

# Migrate entire project
dissolve migrate src/

# Check what would be migrated (dry run)
dissolve check src/
```

### 3. Magic Method Support

Dissolve automatically handles Python magic methods:

```python
# Before migration
length = len(my_object)
text = str(my_object)

# After migration (if __len__/__str__ are deprecated)
length = my_object.size()
text = my_object.to_string()
```

## 🏗️ Architecture

### Core Components

```
dissolve/
├── src/
│   ├── core/                    # Core replacement collection
│   │   ├── ruff_collector.rs   # AST-based function discovery
│   │   └── types.rs            # Core data structures
│   ├── ruff_parser_improved.rs # Advanced replacement engine
│   ├── migrate_ruff.rs         # Migration orchestration
│   ├── pyright_lsp.rs          # Type introspection via Pyright
│   └── bin/main.rs             # CLI interface
└── dissolve/                   # Python package
    ├── __init__.py            # @replace_me decorator
    └── decorators.py          # Deprecation helpers
```

### Key Features

- **AST-Based Analysis**: Uses Ruff parser for accurate Python code analysis
- **Type Introspection**: Pyright LSP integration for intelligent type-aware replacements
- **Magic Method Detection**: Automatic migration of `str()`, `len()`, etc. calls
- **Formatting Preservation**: Maintains code style and comments
- **Comprehensive Testing**: 240+ tests covering edge cases and real-world scenarios

## 📋 Commands

| Command | Description |
|---------|-------------|
| `dissolve migrate <path>` | Apply migrations to Python files |
| `dissolve check <path>` | Show what would be migrated (dry run) |
| `dissolve remove <path>` | Remove deprecated functions after migration |
| `dissolve info <path>` | Show deprecation information |

## 🔧 Configuration

Dissolve uses intelligent defaults but can be configured via command-line options:

```bash
# Use specific type introspection method
dissolve migrate --type-method pyright src/

# Set timeout for type checking
dissolve migrate --timeout 30 src/

# Interactive mode for complex migrations
dissolve migrate --interactive src/
```

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run specific test categories
cargo test test_magic_methods
cargo test test_collection_comprehensive

# Test with coverage
cargo test --features coverage
```

## 📈 Performance

- **Builtin Optimization**: Caches Python builtins for faster processing
- **Parallel Processing**: Handles multiple files concurrently
- **Memory Efficient**: Streams large files without loading entirely into memory

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature-name`
3. Make your changes and add tests
4. Ensure tests pass: `cargo test`
5. Submit a pull request

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

## 📄 License

Licensed under the Apache License, Version 2.0. See [COPYING](COPYING) for details.

## 🔗 Related Projects

- [Ruff](https://github.com/astral-sh/ruff) - Python AST parsing
- [Pyright](https://github.com/microsoft/pyright) - Type checking integration