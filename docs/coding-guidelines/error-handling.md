# Error Handling Guidelines

## Rule: Never use `.unwrap()` in production code

**Why:** `.unwrap()` causes panics that crash the application with no recovery.

## Safe Alternatives

### 1. Option: Use pattern matching

```rust
// ❌ BAD - panics if None
let value = option.unwrap();

// ✅ GOOD - safe with early return
let Some(value) = option else {
    return default_value();
};

// ✅ GOOD - safe with if let
if let Some(value) = option {
    use_value(value);
}

// ✅ GOOD - provide fallback
let value = option.unwrap_or_default();
let value = option.unwrap_or(fallback);
```

### 2. Result: Use `?` operator

```rust
// ❌ BAD - panics on error
let data = load_file(path).unwrap();

// ✅ GOOD - propagate error up
let data = load_file(path)?;

// ✅ GOOD - handle specific error
let data = load_file(path).unwrap_or_else(|e| {
    tracing::warn!("Failed to load {}: {}", path, e);
    default_data()
});
```

### 3. When to use `.expect()`

Only use `.expect()` for programmer errors (bugs), never for runtime errors:

```rust
// ✅ OK - this would be a bug in our code
let page = settings.page
    .as_ref()
    .expect("BUG: page should exist when rendering settings");

// ❌ WRONG - external command can fail
let output = Command::new("git").output()
    .expect("git must be installed"); // User might not have git!
```

## CI Check

Our CI runs `scripts/audit-unwraps.sh` and fails if new unwraps are added.
