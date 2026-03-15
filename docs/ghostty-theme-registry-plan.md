# Ghostty-Style Theme Registry Plan

Implementation plan for expanding Codirigent's theme system from a built-in
`dark/light` toggle into a registry-backed theme model with custom theme files,
runtime theme IDs, and terminal palette behavior that can scale toward a
Ghostty-style theme experience.

This document is intentionally written before code changes. It is the working
plan for the branch `feat/ghostty-theme-registry`.

---

## Purpose

Codirigent already has the terminal rendering primitives needed for richer
themes, but the application still behaves like a two-theme product:

- the runtime UI uses `CodirigentTheme`
- the settings UI only exposes `dark` and `light`
- the saved setting is treated like a boolean mode rather than a durable
  registry theme ID
- custom theme loading infrastructure exists separately but is not wired into
  app startup or live theme application

The goal of this task series is to make themes a first-class product feature
instead of a hardcoded toggle.

---

## Problem Statement

The current implementation has four structural gaps:

1. **Two parallel theme models**
   - `crates/codirigent-ui/src/theme.rs` defines the runtime theme actually used
     by UI and terminal rendering.
   - `crates/codirigent-ui/src/theme_config.rs` and
     `crates/codirigent-ui/src/theme_manager.rs` define a serializable theme
     model and registry, but they are not the active runtime path.

2. **Theme selection is hardcoded**
   - Settings only present `dark` and `light`.
   - Theme switching constructs `CodirigentTheme::dark()` or
     `CodirigentTheme::light()` directly.

3. **Saved theme identity is not durable**
   - `appearance.theme` is stored as a `String`, but the settings page rebuild
     currently infers the value from current background lightness instead of
     preserving the active theme ID.

4. **Load/apply path is incomplete**
   - User settings are loaded and cached, but the selected theme is not treated
     as a registry-resolved startup input.

---

## Current Architecture Inventory

### Runtime Theme Path

- `crates/codirigent-ui/src/theme.rs`
  - owns `CodirigentTheme`
  - contains UI colors, terminal colors, typography, spacing
  - contains ANSI 16-color palette and 256-color indexed conversion

- `crates/codirigent-ui/src/terminal_colors.rs`
  - maps terminal named/indexed/spec colors into runtime theme colors

- `crates/codirigent-ui/src/terminal_view.rs`
  - caches terminal bg/fg from `CodirigentTheme`
  - updates terminal runtime when theme changes

- `crates/codirigent-ui/src/workspace/core.rs`
  - stores the active `CodirigentTheme`

### Settings and Persistence Path

- `crates/codirigent-core/src/config.rs`
  - `AppearanceSettings.theme: String`
  - `TerminalSettings` stores font/cursor/line-height preferences

- `crates/codirigent-ui/src/workspace/settings_panels.rs`
  - theme dropdown is currently `["dark", "light"]`
  - directly constructs built-in runtime themes

- `crates/codirigent-ui/src/workspace/impl_settings.rs`
  - settings page rebuild overwrites `appearance.theme` based on background
    lightness
  - settings load path updates cached settings but does not appear to resolve
    and apply an arbitrary theme ID through a registry

### Unused or Underused Theme Registry Path

- `crates/codirigent-ui/src/theme_config.rs`
  - serializable `Theme`
  - `ThemeColors`, `TerminalColors`, typography, spacing

- `crates/codirigent-ui/src/theme_manager.rs`
  - registry for built-in and JSON-loaded themes
  - theme loading from a directory or file
  - active theme switching by ID

---

## Target End State

After this work series:

- Codirigent loads a theme registry on startup.
- The active theme is identified by a durable theme ID.
- `appearance.theme` means "selected theme ID", not "dark mode boolean".
- Settings list all available themes, not just `dark/light`.
- Built-in themes and custom JSON themes use the same application path.
- Runtime theme application updates both UI and terminal state consistently.
- The terminal palette model is structured so it can grow toward a
  Ghostty-style theme schema without another large refactor.

---

## Non-Goals For The First Pass

The first implementation pass should not try to do all theme features at once.
These are explicitly out of scope unless they fall out naturally:

- importing Ghostty theme files verbatim with full syntax compatibility
- automatic OS appearance switching
- a theme editor UI
- remote theme downloads
- dynamic generation of 256-color cube replacements on the first pass

The first pass is about establishing the correct architecture and durable
runtime behavior.

---

## Design Principles

1. **One runtime source of truth**
   - The app should resolve every selected theme into one runtime
     `CodirigentTheme`.

2. **Theme ID is stable**
   - Any active theme must have a durable identifier that round-trips through
     settings and app restart.

3. **Custom themes should not be a side path**
   - Built-in and file-loaded themes should use the same selection and apply
     flow.

4. **Terminal fidelity matters**
   - ANSI palette, foreground/background, cursor, and selection colors must all
     switch with the active theme.

5. **Incremental delivery**
   - The work should land as small, reviewable tasks following
     `docs/task-verification-workflow.md`.

---

## Implementation Constraints

These constraints apply to every task in this branch:

1. **Do not load theme files on the UI thread**
   - theme discovery, directory scans, and file reads must happen on a
     background executor
   - the UI thread may receive resolved theme data and apply it, but must not
     block on filesystem traversal or JSON file IO

2. **Keep files at manageable length**
   - do not keep expanding already-large files with unrelated theme logic
   - when a change starts to push a file into "grab bag" territory, extract a
     focused helper/module instead
   - prefer small, reviewable modules over one large integration file

3. **Prefer reusable components over duplicated wiring**
   - shared theme resolution, fallback, conversion, and apply behavior should be
     centralized
   - avoid copy-pasting theme selection logic across startup, settings, and
     terminal update paths

4. **Avoid magic numbers unless they are inherent to the domain**
   - filesystem polling delays, cache TTLs, directory limits, and fallback
     constants must be named
   - if a number is part of a terminal standard or palette definition, document
     why it is fixed

5. **Separate IO, state, and presentation concerns**
   - file loading belongs in a theme loading/service layer
   - theme registry state belongs in app/workspace state
   - settings UI should only render options and trigger actions

---

## Proposed Implementation Shape

### 1. Introduce a Registry-to-Runtime Conversion Layer

Create a conversion path from the serializable registry theme model into
`CodirigentTheme`.

Options:

- add `impl TryFrom<theme_config::Theme> for CodirigentTheme`
- or add `Theme::to_runtime_theme() -> Result<CodirigentTheme>`

Expected result:

- the registry model becomes useful without replacing the runtime renderer
- theme parsing and theme application stop being separate systems

### 2. Make Theme Selection Registry-Driven

Replace direct `dark/light` branching with:

1. resolve selected theme ID from settings
2. look it up in the theme registry
3. convert it into `CodirigentTheme`
4. apply it to workspace and terminals

Fallback behavior:

- if the theme ID is missing or invalid, fall back to built-in `dark`
- log the failure with enough detail to diagnose bad custom themes

### 3. Preserve Theme IDs In Settings

Remove the current behavior that reconstructs `appearance.theme` by inspecting
background lightness.

Instead:

- track the current active theme ID in workspace settings state
- persist and rebuild the settings page using that actual ID

### 4. Load Custom Themes From A Well-Defined Directory

Decide and document the custom theme directory. Likely candidate:

- `%APPDATA%/codirigent/themes/` on Windows
- `~/.config/codirigent/themes/` on Linux/macOS

The initial implementation should:

- load built-in themes first
- then overlay custom themes from disk
- allow custom themes to coexist with built-ins under unique IDs
- perform file discovery and JSON loading off the UI thread

### 5. Keep Runtime Theme Mutations Compatible

Today the code mutates parts of the active runtime theme after applying a base
theme, for example:

- grid gap
- UI font size
- terminal font size
- terminal font family
- terminal line height

The new registry-driven apply path must preserve those user overrides rather
than resetting them when a theme changes.

### 6. Prepare For Ghostty-Style Theme Growth

The first pass does not need full Ghostty config syntax, but the schema should
be able to expand toward these terminal concepts cleanly:

- background
- foreground
- cursor color
- cursor text color
- selection background
- selection foreground
- ANSI 16 palette
- optional split between light and dark variants

If a schema change is needed, prefer a backward-compatible addition over a
throwaway one-off field.

---

## Task Series

This branch should be executed as a small task series, not one large patch.

### Task 1. Document and Wire The Runtime Registry Backbone

Deliverables:

- conversion path from serializable theme model to runtime `CodirigentTheme`
- built-in themes exposed through the registry path
- unit tests for conversion and fallback behavior

Done when:

- a theme ID can produce a runtime theme without `if theme == "light"`

### Task 2. Apply Saved Theme IDs During Settings Load / Startup

Deliverables:

- startup or settings load path resolves `appearance.theme`
- invalid IDs fall back safely
- active theme ID is retained in workspace state

Done when:

- restarting the app with a non-default theme keeps the same theme selected

### Task 3. Make The Settings Theme Picker Dynamic

Deliverables:

- settings theme dropdown is populated from the registry
- selection applies by theme ID
- settings rebuild preserves the active theme ID

Done when:

- custom or built-in registry themes are selectable from settings without
  hardcoded branching

### Task 4. Load Custom Theme Files From Disk

Deliverables:

- custom theme directory resolution
- file loading on startup
- invalid file handling with non-fatal logging
- tests for loading valid and invalid theme files

Done when:

- dropping a valid theme JSON file into the theme directory makes it selectable

### Task 5. Expand Terminal Theme Fidelity Where Needed

Deliverables:

- review the serializable theme schema against runtime terminal needs
- add missing fields only if required for correct runtime parity
- verify terminal fg/bg/cursor/selection/ANSI palette switch correctly

Done when:

- terminal behavior remains visually consistent after switching among themes

---

## Risks And Review Focus

### Risk 1. Theme Drift Between Models

If `theme_config::Theme` cannot fully represent runtime needs, conversion logic
may silently drop behavior.

Review focus:

- terminal fields
- status colors
- typography/spacings that are currently mutated at runtime

### Risk 2. Settings Page Regressions

The current settings page rebuild flow reconstructs display state from runtime
theme values. That can easily wipe out the selected theme ID.

Review focus:

- open settings after switching themes
- close and reopen settings
- restart app and reopen settings

### Risk 3. Startup Ordering

If theme loading happens after UI creation or after terminal views are
constructed, the app may flash the wrong theme or only partially update.

Review focus:

- initial workspace creation
- settings background load path
- terminal creation after theme application
- background theme loading handoff back to UI state application

### Risk 4. Overwriting User Overrides

Applying a new base theme must not discard user font size, terminal font
preferences, or grid gap choices.

Review focus:

- theme switch after changing font sizes
- theme switch after changing terminal line height
- theme switch after changing terminal font family

---

## Verification Strategy

This task series follows `docs/task-verification-workflow.md`.

Per task, after implementation:

```bash
cargo clean
cargo build --all-features
cargo test --all --all-targets --all-features
cargo test -p codirigent-ui --lib --features gpui-full
cargo clippy --all --all-targets --all-features -- -D warnings
cargo fmt --all --check
bash scripts/audit-unwraps.sh
```

Required review pass after verification:

- inspect the diff for dead theme paths and duplicate logic
- review fallback behavior for invalid theme IDs and broken JSON files
- review startup ordering and settings rebuild behavior
- review terminal palette behavior, not just UI chrome colors
- confirm file IO and theme discovery do not happen on the UI thread
- confirm new constants are named and justified
- confirm touched files remain at maintainable size

---

## Suggested File Touch Order

To keep the series reviewable, prefer this order:

1. `crates/codirigent-ui/src/theme.rs`
2. `crates/codirigent-ui/src/theme_config.rs`
3. `crates/codirigent-ui/src/theme_manager.rs`
4. `crates/codirigent-ui/src/workspace/impl_settings.rs`
5. `crates/codirigent-ui/src/workspace/settings_panels.rs`
6. any startup/bootstrap files that need registry initialization
7. tests
8. follow-up docs updates if behavior changes materially

This order keeps model changes ahead of UI wiring.

---

## Open Questions Before Implementation

1. Where should the registry live at runtime?
   - central app state
   - workspace state
   - settings state

2. Should built-in themes remain defined in `theme.rs`, or should they be
   generated from `theme_config.rs` and then converted into runtime themes?

3. Do we want the first pass to add richer terminal fields to
   `theme_config.rs`, or keep schema changes minimal and only fill the missing
   runtime wiring?

4. Should custom theme discovery be automatic on every startup, or only when
   the settings panel opens?

Recommended answers for the first pass:

- keep the registry in app/workspace state
- preserve `theme.rs` as the runtime authority initially
- add only the schema fields required for parity
- load custom themes on startup so the selected theme is valid before settings
  open

---

## Completion Standard

This plan is complete only when all of the following are true:

- theme selection is registry-based
- `appearance.theme` stores and preserves a real theme ID
- startup and settings load paths apply the saved theme
- custom themes can be loaded from disk
- terminal colors switch consistently with the active theme
- each task is verified and reviewed per `docs/task-verification-workflow.md`

Until then, the branch is still in progress.
