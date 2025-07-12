"""Test edge cases for formatting preservation."""

from dissolve.migrate import migrate_source


class TestFormattingEdgeCases:
    """Test edge cases and potential issues with formatting preservation."""

    def test_multiple_replacements_same_line(self):
        """Test multiple replacements on the same line."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Multiple calls on one line
result = old_func(1) + old_func(2) + old_func(3)  # Should all be replaced
"""
        result = migrate_source(source.strip())

        # All three calls should be replaced
        assert (
            result.count("new_func") == 4
        )  # 3 in the result line + 1 in function body
        assert "new_func(1 + 1) + new_func(2 + 1) + new_func(3 + 1)" in result

        # Comment should be preserved
        assert "# Should all be replaced" in result

    def test_replacement_in_string_literals(self):
        """Test that replacements don't happen inside string literals."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Should replace this
result = old_func(10)

# Should NOT replace these (they're in strings)
doc = "Call old_func(10) to get result"
example = 'old_func(20)'
multiline = '''
Example usage:
    old_func(30)
'''
"""
        result = migrate_source(source.strip())

        # Only the actual call should be replaced
        assert "result = new_func(10 + 1)" in result

        # String contents should NOT be modified
        assert '"Call old_func(10) to get result"' in result
        assert "'old_func(20)'" in result
        assert "old_func(30)" in result  # In multiline string

    def test_replacement_with_same_line_comment(self):
        """Test replacement when there's a comment on the same line."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

result = old_func(10)  # This is important! TODO: check this
"""
        result = migrate_source(source.strip())

        # Replacement should happen
        assert "new_func(10 + 1)" in result

        # Comment should be preserved on the same line
        assert "# This is important! TODO: check this" in result

    def test_complex_multiline_replacement(self):
        """Test complex multiline calls with various formatting."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(a, b, c, d):
    return new_func(a + b, c, d)

# Complex multiline call
result = old_func(
    10,  # first
    20,  # second
    
    # Third argument with blank line above
    30,
    
    # Fourth argument
    40
)  # End of call
"""
        result = migrate_source(source.strip())

        # Replacement should happen (though formatting may be lost)
        assert "new_func(10 + 20, 30, 40)" in result

        # Comments should be preserved
        assert "# Complex multiline call" in result
        assert "# End of call" in result

    def test_replacement_in_nested_calls(self):
        """Test replacement in deeply nested calls."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Nested calls
result = str(len(str(old_func(old_func(10)))))
"""
        result = migrate_source(source.strip())

        # Both nested calls should be replaced
        assert result.count("new_func") == 3  # 2 in result + 1 in function
        assert "new_func(new_func(10 + 1) + 1)" in result

    def test_preserves_unusual_spacing(self):
        """Test preservation of unusual but valid spacing."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Unusual spacing
result1 = old_func( 10 )  # Extra spaces inside parens
result2 = old_func(
                    20  # Deeply indented
                  )
result3=old_func(30)  # No spaces around =
"""
        result = migrate_source(source.strip())

        # All replacements should happen
        assert result.count("new_func") >= 4

        # Comments on same line as code should be preserved
        assert "# Extra spaces inside parens" in result
        assert "# No spaces around =" in result

        # Note: Comments inside multiline calls are lost during AST unparsing
        # This is a known limitation of the current approach

    def test_unicode_identifiers(self):
        """Test preservation with unicode identifiers."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Unicode identifiers (Python 3 allows these)
résultat = old_func(10)  # French
結果 = old_func(20)  # Japanese
αποτέλεσμα = old_func(30)  # Greek
"""
        result = migrate_source(source.strip())

        # All replacements should happen
        assert result.count("new_func") >= 4

        # Unicode identifiers should be preserved
        assert "résultat" in result
        assert "結果" in result
        assert "αποτέλεσμα" in result

    def test_empty_file_handling(self):
        """Test handling of empty or minimal files."""
        # Empty file
        assert migrate_source("") == ""

        # Only comments
        source = "# Just a comment"
        assert migrate_source(source) == source

        # Only imports, no replacements
        source = "from dissolve import replace_me"
        assert migrate_source(source) == source

    def test_syntax_error_handling(self):
        """Test that syntax errors don't crash the migration."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# This line has valid syntax
result = old_func(10)

# Note: We can't test actual syntax errors because ast.parse would fail
# But we ensure the tool handles edge cases gracefully
"""
        result = migrate_source(source.strip())
        assert "new_func(10 + 1)" in result

    def test_very_long_lines(self):
        """Test preservation of very long lines."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Very long line
result = old_func(10) + old_func(20) + old_func(30) + old_func(40) + old_func(50) + old_func(60) + old_func(70) + old_func(80) + old_func(90) + old_func(100)  # noqa: E501
"""
        result = migrate_source(source.strip())

        # All replacements should happen
        assert result.count("new_func") >= 11

        # The noqa comment should be preserved
        assert "# noqa: E501" in result

    def test_replacement_near_string_boundaries(self):
        """Test replacements near string boundaries."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Calls near strings
result1 = old_func(10) + "text"
result2 = "text" + str(old_func(20))
result3 = f"Value: {old_func(30)}"
result4 = "before" + old_func(40) + "after"
"""
        result = migrate_source(source.strip())

        # All replacements should happen
        assert "new_func(10 + 1)" in result
        assert "new_func(20 + 1)" in result
        assert "new_func(30 + 1)" in result
        assert "new_func(40 + 1)" in result

    def test_class_context_preservation(self):
        """Test preservation of class context and indentation."""
        source = """
from dissolve import replace_me

class OuterClass:
    '''Outer class docstring'''
    
    class InnerClass:
        '''Inner class docstring'''
        
        @replace_me()
        def old_method(self, x):
            '''Method docstring'''
            # Method comment
            return self.new_method(x * 2)
    
    def use_inner(self):
        '''Use the inner class'''
        inner = self.InnerClass()
        # Call the old method
        return inner.old_method(10)  # Should be replaced
"""
        result = migrate_source(source.strip())

        # Replacement in method call should happen
        # The call inner.old_method(10) should become inner.new_method(10 * 2)
        assert "inner.new_method(10 * 2)" in result

        # All docstrings should be preserved
        assert "'''Outer class docstring'''" in result
        assert "'''Inner class docstring'''" in result
        assert "'''Method docstring'''" in result
        assert "'''Use the inner class'''" in result

        # Comments should be preserved
        assert "# Method comment" in result
        assert "# Call the old method" in result
        assert "# Should be replaced" in result

    def test_generator_and_yield_preservation(self):
        """Test preservation with generators and yield statements."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

def generator():
    '''Generator function'''
    # Yield results
    yield old_func(1)  # First
    yield old_func(2)  # Second
    
    # Yield from
    yield from [old_func(3), old_func(4)]
"""
        result = migrate_source(source.strip())

        # All replacements should happen
        assert result.count("new_func") >= 5

        # Structure should be preserved
        assert "yield new_func(1 + 1)" in result
        assert "yield new_func(2 + 1)" in result
        assert "# First" in result
        assert "# Second" in result
        assert "yield from" in result

    def test_async_await_preservation(self):
        """Test preservation with async/await syntax."""
        source = """
from dissolve import replace_me

@replace_me()
async def old_async_func(x):
    return await new_async_func(x + 1)

async def use_async():
    '''Async function using old API'''
    # Single await
    result1 = await old_async_func(10)  # Comment 1
    
    # Multiple awaits
    result2 = await old_async_func(20) + await old_async_func(30)  # Comment 2
    
    # In async with
    async with some_context():
        result3 = await old_async_func(40)
"""
        result = migrate_source(source.strip())

        # All replacements should happen
        assert result.count("new_async_func") >= 5

        # Comments and structure should be preserved
        assert "'''Async function using old API'''" in result
        assert "# Single await" in result
        assert "# Comment 1" in result
        assert "# Multiple awaits" in result
        assert "# Comment 2" in result
        assert "async with some_context():" in result
