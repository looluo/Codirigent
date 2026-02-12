# Clone Optimization Guidelines

## When Clone is Acceptable ✅

### 1. Arc<T> Clones (Cheap)
```rust
let shared = Arc::new(data);
let cloned = shared.clone(); // OK: Just ref count increment
```

### 2. Copy Types (Free)
```rust
let id = SessionId(42);
let copy = id; // OK: Copy, not clone
```

### 3. Necessary Ownership
```rust
// Multiple closures need owned data
let path1 = path.clone();
let path2 = path.clone();
cx.listener(move |_, _, _| { use path1; });
cx.listener(move |_, _, _| { use path2; });
```

## When Clone is Problematic ❌

### 1. String in Hot Paths
```rust
// BAD: String clone allocates heap memory
for _ in 0..1000 {
    let task_id = TaskId(id.0.clone()); // Heap allocation!
}

// GOOD: Use Arc<str> instead
pub struct TaskId(Arc<str>); // Clone is ref count only
```

### 2. Repeated Clones in Same Scope
```rust
// BAD: Multiple clones when one suffices
fn process(data: &Data) {
    helper1(data.clone());
    helper2(data.clone());
    helper3(data.clone());
}

// GOOD: Clone once and share
fn process(data: &Data) {
    let owned = data.clone();
    helper1(&owned);
    helper2(&owned);
    helper3(&owned);
}
```

### 3. Cloning Large Collections
```rust
// BAD: Clone entire Vec when slice would work
fn process(items: &Vec<Item>) -> Vec<Item> {
    items.clone() // Deep copy!
}

// GOOD: Return references or use Arc<[T]>
fn process(items: &[Item]) -> &[Item] {
    items
}
```

## Optimization Strategies

### Strategy 1: Use Arc for Shared Ownership
- **When**: Multiple owners, frequent cloning
- **Types**: String → Arc<str>, Vec<T> → Arc<[T]>

### Strategy 2: Use Cow for Conditional Ownership
- **When**: Sometimes borrow, sometimes own
- **Example**: String truncation in UI

### Strategy 3: Use References When Possible
- **When**: Single owner, passing data down
- **Prefer**: `&T` over `T.clone()`

### Strategy 4: Derive Copy for Small Types
- **When**: Type is ≤16 bytes and trivially copyable
- **Types**: IDs (u64), flags, small enums

## Measuring Impact

```bash
# Run benchmarks before/after
cargo bench --bench clone_bench

# Profile hot paths
cargo flamegraph --bin codirigent

# Check allocations
cargo test -- --nocapture | grep "alloc"
```

## Project-Specific Patterns

### TaskId (Arc<str>)
```rust
// GOOD: Cheap cloning
let id1 = TaskId::from("task-001");
let id2 = id1.clone(); // Just ref count increment

// BAD: Direct Arc construction
let id = TaskId(Arc::from("task-001")); // Prefer From trait
```

### Event Emission
```rust
// OK: TaskId clones in events are cheap with Arc<str>
self.emit(PipelineEvent::Started {
    task_id: task_id.clone(), // Cheap Arc clone
    session_id,
});
```

### UI String Truncation
```rust
use std::borrow::Cow;

// Avoid allocation for short strings
let label: Cow<str> = if text.len() > 12 {
    Cow::Owned(format!("{}...", &text[..12]))
} else {
    Cow::Borrowed(text.as_str())
};
```

## Anti-Patterns to Avoid

### 1. Clone in Loop Without Consideration
```rust
// RISKY: Measure impact first
for _ in 0..1_000_000 {
    expensive_operation(data.clone());
}
```

### 2. Cloning to Satisfy Borrow Checker
```rust
// LAZY: Often indicates design issue
fn process(data: Data) {
    let cloned = data.clone(); // Why?
    helper(&cloned);
}
```

### 3. Unnecessary Cow Complexity
```rust
// OVERKILL: For rarely-cloned data
let value: Cow<str> = if condition {
    Cow::Owned(compute())
} else {
    Cow::Borrowed("default")
};
// If this runs once per second, just use String
```

## Review Checklist

Before committing clone optimizations:

- [ ] Measured performance impact with benchmarks
- [ ] Verified hot paths with profiling
- [ ] Considered API ergonomics vs performance trade-off
- [ ] Added tests demonstrating optimization correctness
- [ ] Documented why clones are necessary or acceptable
- [ ] Ensured no breaking API changes for public types
