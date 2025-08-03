#!/bin/bash

# Add std::path::Path import to test files that don't have it
for file in src/tests/*.rs; do
    if grep -q "migrate_file" "$file" && ! grep -q "use std::path::Path" "$file"; then
        # Find the last use statement and add after it
        sed -i '/^use std::collections::HashMap;$/a use std::path::Path;' "$file"
        # If that didn't work, try after TypeIntrospectionMethod
        sed -i '/^use crate::{.*TypeIntrospectionMethod.*};$/a use std::path::Path;' "$file"
        # If still not added, add after the last use statement
        if ! grep -q "use std::path::Path" "$file"; then
            awk '/^use / { last_use = NR } 
                 { lines[NR] = $0 } 
                 END { 
                     for (i = 1; i <= NR; i++) {
                         print lines[i]
                         if (i == last_use) print "use std::path::Path;"
                     }
                 }' "$file" > "$file.tmp" && mv "$file.tmp" "$file"
        fi
    fi
done

# Fix migrate_file calls
find src/tests -name "*.rs" -type f -exec sed -i 's/"test\.py"\.to_string()/Path::new("test.py")/g' {} \;
find src/tests -name "*.rs" -type f -exec sed -i 's/test_ctx\.file_path/Path::new(\&test_ctx.file_path)/g' {} \;

# Fix check_file calls - this function also needs Path parameter
find src/migrate_ruff.rs -type f -exec sed -i 's/file_path: String/file_path: \&Path/g' {} \;

echo "Fixed test paths"