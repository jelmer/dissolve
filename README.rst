dissolve
========

The dissolve library helps users replaces calls to deprecated library APIs.

Example
=======

E.g. if you had a function "inc" that has been renamed to "increment" in version 0.1.0
of your library:

.. code-block:: python

   from dissolve import replace_me

   @replace_me("increment({x})", since="0.1.0")
   def inc(x):
      return x + 1


Running this code will yield a warning:

.. code-block:: console

   ...
   >>> inc(x=3)
   <stdin>:1: DeprecationWarning: <function inc at 0x7feaf5ead5a0> has been deprecated since 0.1.0; use 'increment(4)' instead
   4
