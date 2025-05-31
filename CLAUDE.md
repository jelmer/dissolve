# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Dissolve is a Python library that helps users replace calls to deprecated library APIs. It provides a decorator `@replace_me` that can be used to mark functions as deprecated and suggest replacements.

## Commands

### Testing
- `pytest tests/` - Run the test suite
- `tox` - Run tests in isolated environments
- `pytest tests/test_decorator.py::test_replace_me` - Run a specific test

### Building and Packaging
- `python -m build` - Build the package
- `pip install -e .` - Install in development mode

## Architecture

The library has a simple structure:
- **dissolve/decorators.py**: Contains the core `replace_me` decorator implementation that:
  - Takes a replacement expression and optional "since" version
  - Emits DeprecationWarning when decorated functions are called
  - Formats the replacement expression with the actual arguments passed
- **dissolve/__init__.py**: Exports the public API (`replace_me`)
- **tests/test_decorator.py**: Tests for the decorator functionality

The decorator is designed to help library maintainers guide users to new APIs by providing specific replacement suggestions with actual argument values substituted.