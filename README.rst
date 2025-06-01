dissolve
========

The dissolve library helps users replace calls to deprecated library APIs by
automatically substituting the deprecated function call with the body of the
deprecated function.

Example
=======

E.g. if you had a function "inc" that has been renamed to "increment" in
version 0.1.0 of your library:

.. code-block:: python

   from dissolve import replace_me

   def increment(x):
      return x + 1

   @replace_me(since="0.1.0")
   def inc(x):
      return increment(x)


Running this code will yield a warning:

.. code-block:: console

   ...
   >>> inc(x=3)
   <stdin>:1: DeprecationWarning: <function inc at 0x7feaf5ead5a0> has been deprecated since 0.1.0; use 'increment(x)' instead
   4

Running the ``dissolve migrate`` command will automatically replace the deprecated function call with the suggested replacement:

.. code-block:: console

   $ dissolve migrate myproject/utils.py
   Migrated: myproject/utils.py
   ...
   result = increment(x=3)
   ...

dissolve migrate
================

The ``dissolve migrate`` command can automatically update your codebase to
replace deprecated function calls with their suggested replacements.

Usage:

.. code-block:: console

   $ dissolve migrate path/to/code

This will:

1. Search for Python files in the specified path
2. Find calls to functions decorated with ``@replace_me``
3. Replace them with the suggested replacement expression
4. Show a diff of the changes

Options:

* ``-w, --write``: Write changes back to files instead of printing to stdout
* ``--check``: Check if files need migration without modifying them (exits with code 1 if changes are needed)

Examples:

Preview changes:

.. code-block:: console

   $ dissolve migrate myproject/utils.py
   # Migrated: myproject/utils.py
   ...
   result = 5 + 1
   ...

Check if migration is needed:

.. code-block:: console

   $ dissolve migrate --check myproject/
   myproject/utils.py: needs migration
   myproject/core.py: up to date
   $ echo $?
   1

Apply changes:

.. code-block:: console

   $ dissolve migrate --write myproject/
   Modified: myproject/utils.py
   Unchanged: myproject/core.py

The command respects the replacement expressions defined in the ``@replace_me``
decorator and substitutes actual argument values.


dissolve remove
===============

The ``dissolve remove`` command can remove ``@replace_me`` decorators from your
codebase. This is useful when you want to clean up old deprecation markers.

Usage:

.. code-block:: console

   $ dissolve remove [options] path/to/code

Options:

* ``--all``: Remove all ``@replace_me`` decorators regardless of version
* ``--before VERSION``: Remove only decorators with a version older than the specified version
* ``-w, --write``: Write changes back to files (default: print to stdout)
* ``--check``: Check if files have removable decorators without modifying them (exits with code 1 if changes are needed)

Examples:

Check if decorators can be removed:

.. code-block:: console

   $ dissolve remove --check --all myproject/
   myproject/utils.py: has removable decorators
   myproject/core.py: no removable decorators
   $ echo $?
   1

Remove all decorators:

.. code-block:: console

   $ dissolve remove --all --write myproject/
   Modified: myproject/utils.py
   Unchanged: myproject/core.py

Remove decorators before version 2.0.0:

.. code-block:: console

   $ dissolve remove --before 2.0.0 --write myproject/

This will remove decorators like ``@replace_me(since="1.0.0")`` but keep
``@replace_me(since="2.0.0")`` and newer.


dissolve check
==============

The ``dissolve check`` command verifies that all ``@replace_me`` decorated
functions in your codebase can be successfully processed by the ``dissolve
migrate`` command. This is useful for ensuring your deprecation decorators are
properly formatted.

Usage:

.. code-block:: console

   $ dissolve check path/to/code

This will:

1. Search for Python files with ``@replace_me`` decorated functions
2. Verify that each decorated function has a valid replacement expression
3. Report any functions that cannot be processed by migrate

Examples:

Check all files in a directory:

.. code-block:: console

   $ dissolve check myproject/
   myproject/utils.py: 3 @replace_me function(s) can be replaced
   myproject/core.py: 1 @replace_me function(s) can be replaced

When errors are found:

.. code-block:: console

   $ dissolve check myproject/broken.py
   myproject/broken.py: ERRORS found
     Function 'old_func' cannot be processed by migrate

The command exits with code 1 if any errors are found, making it useful in CI
pipelines to ensure all deprecations are properly formatted.


Optional Dependency Usage
=========================

If you don't want to add a runtime dependency on dissolve, you can define a fallback implementation that mimics dissolve's basic deprecation warning functionality:

.. code-block:: python

   try:
       from dissolve import replace_me
   except ModuleNotFoundError:
       import warnings
       
       def replace_me(since=None, remove_in=None):
           def decorator(func):
               def wrapper(*args, **kwargs):
                   msg = f"{func.__name__} has been deprecated"
                   if since:
                       msg += f" since {since}"
                   if remove_in:
                       msg += f" and will be removed in {remove_in}"
                   msg += ". Consider running 'dissolve migrate' to automatically update your code."
                   warnings.warn(msg, DeprecationWarning, stacklevel=2)
                   return func(*args, **kwargs)
               return wrapper
           return decorator

This fallback implementation provides the same decorator interface as dissolve's ``replace_me`` decorator. When dissolve is installed, you get full deprecation warnings with replacement suggestions and migration support. When it's not installed, you still get basic deprecation warnings that include a suggestion to use dissolve's migration tool.
