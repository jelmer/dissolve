"""Test that the @replace_me decorator has no external dependencies."""

import ast
import os
import sys
from pathlib import Path


def test_decorator_module_only_uses_stdlib():
    """Ensure decorators.py and its dependencies only import from the Python standard library."""
    # Find the decorators.py file
    decorators_path = Path(__file__).parent.parent / "dissolve" / "decorators.py"

    assert decorators_path.exists(), f"decorators.py not found at {decorators_path}"

    # We need to check decorators.py and any local modules it imports
    modules_to_check = [decorators_path]
    checked_modules = set()

    # Get standard library modules dynamically
    def is_stdlib_module(module_name):
        """Check if a module is part of the Python standard library."""
        import importlib.util

        spec = importlib.util.find_spec(module_name)
        if spec is None:
            return False

        # Check if it's a built-in module
        if spec.origin is None:
            return True

        # Check if the module is in the standard library path
        if spec.origin:
            stdlib_path = os.path.dirname(os.__file__)
            return spec.origin.startswith(stdlib_path)

        return False

    while modules_to_check:
        module_path = modules_to_check.pop()
        if module_path in checked_modules:
            continue
        checked_modules.add(module_path)

        # Parse the file
        with open(module_path) as f:
            tree = ast.parse(f.read())

        # Collect all imports
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    module_name = alias.name.split(".")[0]
                    if not is_stdlib_module(module_name):
                        raise AssertionError(
                            f"{module_path.name} imports non-stdlib module: {module_name}. "
                            f"The @replace_me decorator must only depend on the Python standard library."
                        )
            elif isinstance(node, ast.ImportFrom):
                if node.module is not None:
                    continue
                for alias in node.names:
                    module_name = alias.name.split(".")[0]
                    if not is_stdlib_module(module_name):
                        raise AssertionError(
                            f"{module_path.name} imports non-stdlib module: {module_name}. "
                            f"The @replace_me decorator must only depend on the Python standard library."
                        )


def test_decorator_can_be_imported_standalone():
    """Test that we can import just the decorator without any dependencies."""
    # Save the current sys.modules
    original_modules = sys.modules.copy()

    try:
        # Remove dissolve modules except decorators
        to_remove = [
            key
            for key in sys.modules.keys()
            if key.startswith("dissolve") and key != "dissolve.decorators"
        ]
        for key in to_remove:
            del sys.modules[key]

        # Try to import just the decorator
        from dissolve.decorators import replace_me

        # Test that we can use it
        @replace_me()
        def old_func(x):
            return x + 1

        # Should work without warnings in test mode
        result = old_func(5)
        assert result == 6

    finally:
        # Restore sys.modules
        sys.modules.clear()
        sys.modules.update(original_modules)


def test_decorator_ast_usage():
    """Verify that decorators.py uses only ast module, not libcst."""
    decorators_path = Path(__file__).parent.parent / "dissolve" / "decorators.py"

    with open(decorators_path) as f:
        content = f.read()

    # Check that libcst is not imported or used
    assert "libcst" not in content, "decorators.py must not use libcst"
    assert "cst." not in content, "decorators.py must not use libcst"

    # Verify ast is used for parsing
    assert "import ast" in content or "from ast import" in content, (
        "decorators.py should use the ast module from stdlib"
    )
