#!/usr/bin/python3
"""Setup script for dissolve with optional Rust binary installation."""

import os
import subprocess
import sys
from pathlib import Path

from setuptools import setup
from setuptools.command.build import build
from setuptools.command.develop import develop
from setuptools.command.install import install


class BuildRustExtension:
    """Helper class to build the Rust binary."""
    
    def build_rust_binary(self):
        """Build the Rust binary using cargo."""
        # Check if we're installing with the 'tool' extra
        # This is a bit hacky but works for most cases
        installing_tool = any(
            'tool' in arg for arg in sys.argv 
            if '[' in arg or 'tool' in arg
        )
        
        # Also check environment variable for more explicit control
        if os.environ.get('DISSOLVE_BUILD_RUST', '').lower() in ('1', 'true', 'yes'):
            installing_tool = True
            
        if not installing_tool:
            return
            
        print("Building Rust binary for dissolve...")
        
        # Check if cargo is available
        try:
            subprocess.run(['cargo', '--version'], check=True, capture_output=True)
        except (subprocess.CalledProcessError, FileNotFoundError):
            raise RuntimeError(
                "cargo not found. The 'tool' extra requires Rust to be installed.\n"
                "Please install Rust from https://rustup.rs/ or install without the 'tool' extra."
            )
            
        # Build the Rust binary
        try:
            subprocess.run(
                ['cargo', 'build', '--release', '--bin', 'dissolve'],
                check=True,
                cwd=Path(__file__).parent
            )
            print("Rust binary built successfully!")
        except subprocess.CalledProcessError as e:
            raise RuntimeError(
                f"Failed to build Rust binary: {e}\n"
                "The 'tool' extra requires the Rust binary to be built successfully.\n"
                "Please fix the build errors or install without the 'tool' extra."
            )


class BuildCommand(build, BuildRustExtension):
    """Custom build command that builds the Rust binary."""
    
    def run(self):
        self.build_rust_binary()
        super().run()


class DevelopCommand(develop, BuildRustExtension):
    """Custom develop command that builds the Rust binary."""
    
    def run(self):
        self.build_rust_binary()
        super().run()


class InstallCommand(install, BuildRustExtension):
    """Custom install command that installs the Rust binary."""
    
    def run(self):
        super().run()
        
        # Check if tool extra was requested
        installing_tool = any(
            'tool' in arg for arg in sys.argv 
            if '[' in arg or 'tool' in arg
        ) or os.environ.get('DISSOLVE_BUILD_RUST', '').lower() in ('1', 'true', 'yes')
        
        if not installing_tool:
            return
        
        # Install the Rust binary if it was built
        rust_binary = Path(__file__).parent / 'target' / 'release' / 'dissolve'
        if not rust_binary.exists():
            raise RuntimeError(
                "Rust binary not found after build. "
                "The 'tool' extra requires the Rust binary to be built successfully."
            )
            
        # Find the scripts directory
        if self.install_scripts:
            scripts_dir = self.install_scripts
        else:
            # Fallback to finding it from the installation paths
            scripts_dir = os.path.join(self.install_base, 'bin')
            
        if not os.path.exists(scripts_dir):
            os.makedirs(scripts_dir)
            
        # Copy the binary to the scripts directory
        import shutil
        dest = os.path.join(scripts_dir, 'dissolve')
        print(f"Installing Rust binary to {dest}")
        try:
            shutil.copy2(rust_binary, dest)
            # Make it executable
            os.chmod(dest, 0o755)
        except Exception as e:
            raise RuntimeError(
                f"Failed to install Rust binary: {e}\n"
                "The 'tool' extra requires the Rust binary to be installed successfully."
            )


# Configure setup to use our custom commands
setup(
    cmdclass={
        'build': BuildCommand,
        'develop': DevelopCommand,
        'install': InstallCommand,
    },
)