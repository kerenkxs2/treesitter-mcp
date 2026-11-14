# Agent Guidelines for treesitter-mcp

## Build & Test Commands
- **Build**: `task build` (normal `target/release`), or pass `output_dir=target/alpha-cpp` to stage independently of connected clients.
- **Test all**: `task test` (existing upstream regressions and C/C++ contract tests).
- **Focused C/C++ tests**: `task test:cpp`.
- **Check**: `task check` (formatting, runtime Clippy with zero warnings, Go vet and the maintained-source size gate).
- **Auto-format**: `task format`.
- **Source size**: `task source:line-check` checks maintained files in the current change against the 500-line ceiling; preserve untouched third-party scope.
- **MCP protocol smoke**: `task smoke`, optionally `-- --voxel-root /path/to/godot_voxel` for real C++ source. The smoke server is isolated; deployment/reload of connected clients is a separate operation.
- Task parameters are explicit inputs. Do not add environment-variable modes or replace the Taskfile with shell automation. Local maintenance requires Rust, Task and Go.

## Portable tooling and local host facts

Serena and Tree-sitter must provide functional coverage for at least TypeScript, Rust, Go, GDScript and Lua. Verify the actual language operations; configuration entries and successful builds alone do not prove support.

Machine-specific facts belong in an ignored local `.env`: installed external executable paths, user home or checkout locations, state directories, host identity and local endpoints. Keep these facts out of source constants, hardcoded host allowlists, versioned Taskfile paths and assistant rules tied to named machines. Project-internal paths derive from the absolute project root; shared protocols, dependency versions/checksums and environment-independent behavior remain versioned.

Read local facts through the canonical typed configuration boundary, with clear errors for missing or invalid required values. Do not source arbitrary shell code or maintain competing local configuration readers. Environment files hold stable facts or secrets, never hidden feature toggles, bypasses, alternate pipelines or execution modes. Preserve existing credentials and their documented ownership; changing a local fact does not grant agent access.

Keep `AGENTS.md`, `CLAUDE.md` and `GEMINI.md` synchronized in the same change. Shared skills describe this portable contract; host installations and generated runtime metadata are validated separately.

## Code Style & Conventions
- **TDD Approach**: Follow RED/GREEN/BLUE pattern - write tests first, implement to pass, refactor
- **Testing**: Write always test for business logic and requirements and if these are missing ask for them
- **Error Handling**: Use `eyre::Result<T>` for all fallible operations, propagate with `?` operator
- **Imports**: Group by category with blank lines: std → external crates → internal modules (e.g., `crate::mcp`, `crate::parser`)
- **Naming**: snake_case for functions/variables, PascalCase for types, SCREAMING_SNAKE_CASE for constants
- **Logging**: Use `log::info!`, `log::debug!`, `log::error!` macros with descriptive messages
- **Documentation**: Use `///` for public items and `//!` for module-level docs with clear purpose statements
- **Serde**: Use `#[serde(skip_serializing_if = "...")]` to omit empty/optional fields in JSON output
- **JSON-RPC**: MCP protocol uses JSON-RPC 2.0 over stdio (stdin/stdout), keep messages compact so as little tokens are used as possible(remove whitespace)
- **Tree-sitter**: Node types are language-specific, use S-expression queries, handle malformed code gracefully

## treesitter-mcp Tools
Use treesitter-mcp to understand code structure before making changes:
- **Exploring codebase?** → `code_map` on the directory
- **Before editing a known symbol?** → `minimal_edit_context` for the target and its relevant dependencies
- **Refactoring or renaming?** → `find_usages` to check impact
- **After making changes?** → `parse_diff` to verify what changed at symbol level
- **Before running tests?** → `affected_by_diff` to see what might break
- **Got a line number?** → `symbol_at_line` to identify the symbol and scope
- **Need quick file overview?** → `view_code` with `detail="signatures"`

Prefer treesitter-mcp over grep for structural queries (finding functions, classes, usages).

**Keep the instruction files in step.** `AGENTS.md` is the source of `CLAUDE.md`, `GEMINI.md` and any other assistant instruction file beside it. They carry the same content; only a section headed as specific to one assistant, placed just before this notice, may differ. Changing any of them means reviewing and updating the others in the same change.
