# rsprune

A fast Rust-based tool for finding unused TypeScript/JavaScript exports. Drop-in replacement for [ts-unused-exports](https://github.com/pzavolinsky/ts-unused-exports) with significantly better performance.

## Features

- Reads your `tsconfig.json` (including JSONC with comments and trailing commas)
- Resolves `paths` aliases and `baseUrl` via `oxc_resolver`
- Detects named, default, re-exports, type exports, enums, interfaces
- Tracks dynamic `import()` expressions
- Respects `// ts-unused-exports:disable-next-line` suppression comments
- Parallel file walking and parsing via rayon
- Exits with code `1` when unused exports are found (CI-friendly)

## Usage

```bash
rsprune [OPTIONS] [tsconfig]
```

### Options

| Flag | Description |
|------|-------------|
| `--ignore-files <regex>` | Skip files matching a regex pattern (e.g. `--ignore-files '\.spec\.'`) |
| `--exclude-paths-from-report <path>` | Omit paths from output (e.g. `--exclude-paths-from-report src/test`) |
| `--timing` | Print per-phase timing breakdown to stderr |

### Examples

```bash
# Run against tsconfig.json in the current directory
rsprune

# Custom tsconfig location
rsprune path/to/tsconfig.json

# Ignore test files
rsprune --ignore-files '\.spec\.' --ignore-files '\.test\.'

# Use in CI (exits 1 if unused exports found)
rsprune && echo "Clean!"
```

### Output

```
5 modules with unused exports
src/utils/helpers.ts[12,0]: unusedFn
src/components/Button.tsx[34,7]: ButtonProps
```

Format: `file[line,col]: exportName` (0-indexed lines).

## Installation

```bash
cargo install --path .
```

## Performance

Benchmarked on a ~6300 file TypeScript codebase:

| Tool | Time |
|------|------|
| ts-unused-exports | ~8s |
| rsprune | ~250ms |

## Suppressing warnings

Add a comment on the line before the export:

```ts
// ts-unused-exports:disable-next-line
export const internalHelper = () => {};
```

## Development

```bash
cargo build --release
cargo test
```
