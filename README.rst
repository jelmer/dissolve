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

Running the ``dissolve migrate`` command will automatically replace the
deprecated function call with the suggested replacement:

.. code-block:: console

   $ dissolve migrate --write myproject/utils.py
   Modified: myproject/utils.py
   ...
   result = increment(x=3)
   ...

**For library users**: The migration step above is typically all you need.
Your code now uses the new ``increment`` function instead of the deprecated ``inc`` function.

**For library maintainers**: After users have had time to migrate and you're ready
to remove the deprecated function from your library, you can use ``dissolve cleanup``:

.. code-block:: console

   $ dissolve cleanup --all --write myproject/utils.py
   Modified: myproject/utils.py

This removes the ``inc`` function entirely from the library, leaving only the ``increment`` function.

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


dissolve cleanup
================

The ``dissolve cleanup`` command is designed for **library maintainers** to remove
deprecated functions from their codebase after a deprecation period has ended.
This command removes the entire function definition, not just the ``@replace_me`` 
decorator.

**Audience**: This command is primarily for library authors who want to clean up
their APIs after users have had time to migrate away from deprecated functions.

**Important**: This command removes the entire function definition, which will
break any code that still calls these functions. Only use this after:

1. Sufficient time has passed for users to migrate (based on your deprecation policy)
2. You've verified that usage of these functions has dropped to acceptable levels
3. You're prepared to release a new major version (if following semantic versioning)

Usage:

.. code-block:: console

   $ dissolve cleanup [options] path/to/code

Options:

* ``--all``: Remove all functions with ``@replace_me`` decorators regardless of version
* ``--before VERSION``: Remove only functions with decorators older than the specified version
* ``--current-version VERSION``: Remove functions marked with ``remove_in`` <= current version
* ``-w, --write``: Write changes back to files (default: print to stdout)
* ``--check``: Check if files have deprecated functions that can be removed without modifying them (exits with code 1 if changes are needed)

Examples:

Check if deprecated functions can be removed:

.. code-block:: console

   $ dissolve cleanup --check --current-version 2.0.0 mylib/
   mylib/utils.py: needs function cleanup
   mylib/core.py: up to date
   $ echo $?
   1

Remove functions scheduled for removal in version 2.0.0:

.. code-block:: console

   $ dissolve cleanup --current-version 2.0.0 --write mylib/
   Modified: mylib/utils.py
   Unchanged: mylib/core.py

Remove functions deprecated before version 2.0.0:

.. code-block:: console

   $ dissolve cleanup --before 2.0.0 --write mylib/

This will remove functions like those decorated with ``@replace_me(since="1.0.0")`` 
but keep functions with ``@replace_me(since="2.0.0")`` and newer.

**Typical workflow for library maintainers:**

1. Add ``@replace_me(since="X.Y.Z", remove_in="A.B.C")`` to deprecated functions
2. Release version X.Y.Z with deprecation warnings
3. Wait for the planned removal version A.B.C
4. Run ``dissolve cleanup --current-version A.B.C --write`` to remove deprecated functions
5. Release version A.B.C as a new major version


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

Supported objects
=================

The `replace_me` decorator can currently be applied to:

- Functions
- Async functions
- Methods
- Properties

In the future, support for other types of objects may be added:

- Classes (see https://github.com/jelmer/dissolve/issues/33)

Dissolve will automatically determine the appropriate replacement expression
based on the body of the decorated object. In some cases, this is not possible,
such as when the body is a complex expression or when the object is a lambda
function.

Async Function Deprecation
--------------------------

Async functions are fully supported and work just like regular functions:

.. code-block:: python

   from dissolve import replace_me
   import asyncio

   async def new_fetch_data(url, timeout=30):
       # Modern implementation
       return await fetch_with_timeout(url, timeout)

   @replace_me(since="3.0.0")
   async def old_fetch_data(url):
       return await new_fetch_data(url, timeout=30)

When called, this will emit:

.. code-block:: console

   >>> await old_fetch_data("https://api.example.com")
   <stdin>:1: DeprecationWarning: <function old_fetch_data at 0x...> has been deprecated since 3.0.0; use 'await new_fetch_data('https://api.example.com', timeout=30)' instead

The replacement expression correctly preserves the ``await`` keyword for async calls.


Optional Dependency Usage
=========================

If you don't want to add a runtime dependency on dissolve, you can define a
fallback implementation that mimics dissolve's basic deprecation warning
functionality:

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

This fallback implementation provides the same decorator interface as
dissolve's ``replace_me`` decorator. When dissolve is installed, you get full
deprecation warnings with replacement suggestions and migration support. When
it's not installed, you still get basic deprecation warnings that include a
suggestion to use dissolve's migration tool.
