#!/bin/bash
# scripts/audit-unwraps.sh
# Finds production unwrap() calls (excluding tests, docs, examples)

echo "🔍 Scanning for production unwrap() calls..."
echo ""

# Find unwraps in production code only
grep -rn "\.unwrap()" crates/*/src --include="*.rs" \
  | grep -v "tests/" \
  | grep -v "///" \
  | grep -v "// Example" \
  | grep -v "#\[test\]" \
  | grep -v "#\[cfg(test)\]" \
  > /tmp/unwraps.txt

TOTAL=$(cat /tmp/unwraps.txt | wc -l)

echo "Found $TOTAL production unwrap() calls:"
echo ""
cat /tmp/unwraps.txt
echo ""
echo "Priority files to fix:"
cat /tmp/unwraps.txt | cut -d: -f1 | sort | uniq -c | sort -rn | head -10
