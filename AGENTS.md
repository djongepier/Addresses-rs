# Agent Instructions

## Rust version
- Use the Rust stable toolchain and the 2024 edition.
- The project minimum is Rust 1.89 because rusqlite 0.43 requires it (the 2024 edition alone requires Rust 1.85). Keep the manifest's `rust-version` accurate when changing dependencies.
- Keep changes compatible with Windows and Linux.

## Style and complexity
- Follow idiomatic Rust conventions and existing project patterns.
- Keep solutions simple and direct. Avoid unnecessary abstractions and complexity.
- Be mindful of memory usage: borrow data where practical, and avoid unnecessary `clone` or copy operations.
- Add concise comments that explain the intent of each logical section of code. Avoid comments that merely repeat what the code does.

## Errors and GUI behavior
- This is a GUI application. Present actionable, user-friendly messages and sensible dialogs; do not expose raw internal errors as the only feedback.
- Use the included `anyhow` crate for error context and propagation where appropriate. Preserve useful diagnostic details for logging or debugging while showing users clear messages.
- Handle expected errors explicitly; do not silently ignore failures.

## Responsiveness
- Blocking code is acceptable when it does not make the GUI unresponsive.
- If work would noticeably block the GUI, use asynchronous or background execution, choosing the least complex approach that keeps the interface responsive.
- This is a desktop application, not a website; do not introduce web-oriented architecture without a clear need.

## Documentation
- Update the README.md if applicable because of implemented features.