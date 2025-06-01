# Contributing to Dissolve

Thank you for your interest in contributing to Dissolve! This document provides guidelines for contributing to the project.

## Development Setup

1. Clone the repository and install dependencies:
   ```bash
   pip install -e .[dev]
   ```

2. Install pre-commit hooks (optional but recommended):
   ```bash
   pre-commit install
   ```

## Code Quality

We maintain high code quality standards using several tools:

### Linting and Formatting
- **Ruff**: Used for linting and code formatting
  ```bash
  ruff check .
  ruff format .
  ```

### Type Checking
- **MyPy**: Used for static type checking
  ```bash
  mypy dissolve/
  ```

### Testing
- **Pytest**: Used for running tests
  ```bash
  pytest
  ```

**All new code should include comprehensive unit tests.** Tests should cover:
- Normal operation and expected behavior
- Edge cases and error conditions
- Different input combinations and scenarios

Place tests in the `tests/` directory following the naming convention `test_<module_name>.py`.

## Before Submitting a Pull Request

Please ensure your code passes all quality checks:

```bash
ruff check .
ruff format .
mypy dissolve/
pytest
```

## Design Guidelines

Please read [DESIGN.md](DESIGN.md) to understand the project's architecture and design principles before making significant changes.

## Submitting Changes

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Ensure all tests pass and code quality checks succeed
5. Submit a pull request with a clear description of your changes

Thank you for contributing!