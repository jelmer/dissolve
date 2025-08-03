# Contributing to Dissolve

Thank you for your interest in contributing to Dissolve! This document provides guidelines for contributing to the project.

## 🛠️ Development Setup

### Prerequisites
- **Rust toolchain** (1.70+): Install via [rustup.rs](https://rustup.rs/)
- **Python** (3.8+): For the decorator package and testing
- **Git**: For version control

### Initial Setup
1. **Clone and build**:
   ```bash
   git clone https://github.com/jelmer/dissolve
   cd dissolve
   
   # Build Rust components
   cargo build
   
   # Install Python package in development mode
   pip install -e .
   ```

2. **Install development dependencies**:
   ```bash
   # Python tools
   pip install -e .[dev]
   
   # Rust tools (if not already installed)
   rustup component add clippy rustfmt
   ```

3. **Optional: Pre-commit hooks**:
   ```bash
   pre-commit install
   ```

## 🧪 Testing & Code Quality

We maintain high code quality standards using several tools:

### Rust Development
```bash
# Run all tests
cargo test

# Run with specific thread limit (recommended for CI)
RUST_TEST_THREADS=4 cargo test

# Check code style
cargo clippy

# Format code
cargo fmt

# Run clippy with fixes
cargo clippy --fix
```

### Python Development
```bash
# Format Python code
ruff format .

# Check Python code style
ruff check .

# Fix Python issues
ruff check --fix .

# Type checking
mypy dissolve/

# Run Python tests
PYTHONPATH=. pytest dissolve/tests/
```

### 🚨 Important: Test Parallelism
The test suite creates many **Pyright LSP instances** which can be resource-intensive:

- **Default**: Tests auto-limit to 4 threads to prevent timeouts
- **CI/Limited Resources**: Use `RUST_TEST_THREADS=2 cargo test` 
- **Powerful Machines**: You can try `RUST_TEST_THREADS=8 cargo test` but may hit timeouts

### Test Guidelines
**All new code must include comprehensive tests** covering:
- ✅ Normal operation and expected behavior
- ✅ Edge cases and error conditions  
- ✅ Different input combinations
- ✅ Integration with existing components

**Test Organization**:
- **Rust tests**: Place in `src/tests/test_<feature>.rs`
- **Python tests**: Place in `dissolve/tests/test_<module>.py`
- **Integration tests**: Use existing `src/tests/test_*_comprehensive.rs` patterns

## 📋 Contribution Workflow

### Before You Start
1. **Read [DESIGN.md](DESIGN.md)** to understand the architecture and design principles
2. **Check existing issues** on [GitHub](https://github.com/jelmer/dissolve/issues)
3. **Fork the repository** and create a feature branch

### Development Process
1. **Write code** following the established patterns
2. **Add tests** that cover your changes comprehensively
3. **Run the full test suite**:
   ```bash
   # Rust components
   cargo test
   cargo clippy
   cargo fmt --check
   
   # Python components  
   ruff check .
   ruff format --check .
   mypy dissolve/
   ```
4. **Update documentation** if needed
5. **Submit a pull request** with a clear description of your changes

Thank you for contributing to Dissolve! 🙏
