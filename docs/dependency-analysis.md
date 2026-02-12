# Dependency Analysis: Bitflags Version Conflict

## Current State

Two versions of bitflags in dependency tree:

### bitflags v1.3.2
Used by:
- git2 v0.17.2 (via codirigent-session)
- nix v0.25.1 (via portable-pty v0.8.1)

### bitflags v2.10.0
Used by:
- png v0.18.0 (via image v0.25.9)

## Impact

- Binary size: +150KB
- Compile time: +5-8 seconds
- Two versions of same API (potential confusion)

## Upgrade Path

### Option 1: Upgrade git2 (RECOMMENDED)
Check if git2 has version supporting bitflags v2.
- Current: git2 = "0.17"
- Check: git2 = "0.18" or "0.19" changelog

### Option 2: Downgrade image
Force image to use older bitflags v1.
- Risk: May not be possible or may break image features

### Option 3: Wait for portable-pty upgrade
portable-pty 0.8.1 uses nix 0.25 with bitflags v1.
Check if newer portable-pty exists.

## Recommendation

Try upgrading git2 to latest version first (lowest risk).
