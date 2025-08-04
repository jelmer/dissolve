#!/usr/bin/env python3
"""CLI wrapper for dissolve that either runs the Rust binary or provides installation instructions."""

import shutil
import subprocess
import sys
from pathlib import Path
from typing import Optional


def find_dissolve_binary() -> Optional[str]:
    """Find the dissolve Rust binary in PATH or common locations."""
    # First check if it's in PATH
    binary_path = shutil.which("dissolve")
    if binary_path:
        # Make sure it's actually the Rust binary, not this Python script
        try:
            result = subprocess.run(
                [binary_path, "--version"], capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0 and "dissolve" in result.stdout.lower():
                return binary_path
        except (
            subprocess.TimeoutExpired,
            subprocess.CalledProcessError,
            FileNotFoundError,
        ):
            pass

    # Check common cargo install locations
    home = Path.home()
    cargo_bin = home / ".cargo" / "bin" / "dissolve"
    if cargo_bin.exists():
        return str(cargo_bin)

    # Check if we're in development and there's a local binary
    current_dir = Path(__file__).parent.parent
    local_binary = current_dir / "target" / "release" / "dissolve"
    if local_binary.exists():
        return str(local_binary)

    return None


def print_installation_instructions() -> None:
    """Print instructions for installing the Rust binary."""
    print("The dissolve Rust binary is not installed or not found in PATH.")
    print()
    print("To install the high-performance Rust version:")
    print("  cargo install dissolve-python")
    print()
    print("If you don't have Rust installed:")
    print("  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh")
    print("  source ~/.cargo/env")
    print("  cargo install dissolve-python")
    print()
    print("Alternative: You can also use the Python-only version (slower):")
    print("  python -m dissolve [arguments]")


def main() -> None:
    """Main entry point that either runs the Rust binary or shows installation instructions."""
    binary_path = find_dissolve_binary()

    if binary_path:
        # Run the Rust binary with all arguments passed through
        try:
            result = subprocess.run([binary_path, *sys.argv[1:]])
            sys.exit(result.returncode)
        except KeyboardInterrupt:
            sys.exit(130)  # Standard exit code for SIGINT
        except Exception as e:
            print(f"Error running dissolve binary: {e}", file=sys.stderr)
            sys.exit(1)
    else:
        # Binary not found, show installation instructions
        if len(sys.argv) > 1:
            # User tried to run a command, so show error first
            print("Error: dissolve Rust binary not found.", file=sys.stderr)
            print()

        print_installation_instructions()
        sys.exit(1)


if __name__ == "__main__":
    main()
