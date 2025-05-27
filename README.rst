dissolve
========

The dissolve library helps users replaces calls to deprecated library APIs.

Example
=======

E.g. if you had a function "inc" that has been renamed to "increment" in
version 0.1.0 of your library:

.. code-block:: python

   from dissolve import replace_me

   @replace_me(since="0.1.0")
   def inc(x):
      return x + 1


Running this code will yield a warning:

.. code-block:: console

   ...
   >>> inc(x=3)
   <stdin>:1: DeprecationWarning: <function inc at 0x7feaf5ead5a0> has been deprecated since 0.1.0; use 'x + 1' instead
   4


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
