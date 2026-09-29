# rsprune ✂️

**Blazing fast unused export finder for TypeScript and JavaScript** — a Rust-powered drop-in replacement for [ts-unused-exports](https://github.com/pzavolinsky/ts-unused-exports), ~30x faster.

## ⚡ Performance

Benchmarked on a ~6,300 file TypeScript codebase:

| Tool | Time |
|------|------|
| ts-unused-exports | ~8s |
| **rsprune** | **~250ms** |

## ✨ Features

- 🔍 Detects named, default, re-exports, type exports, enums, interfaces, and dynamic `import()`
- 🧩 Optionally follows `import.meta.webpackContext()` and `require.context()` (`--bundler-contexts`)
- 🧪 Optionally reports exports that only tests use (`--test-files`)
- 🗺️ Resolves `paths` aliases and `baseUrl` via `oxc_resolver`
- 📄 Reads `tsconfig.json` including JSONC (comments + trailing commas)
- 🔇 Respects `// ts-unused-exports:disable-next-line` suppression comments
- ⚙️ Parallel file walking and parsing via rayon
- 🚦 Exits with code `1` when unused exports are found (CI-friendly)

## 📦 Installation

```bash
# via npm
npm install -g rsprune

# via cargo
cargo install --path .
```

## 🚀 Usage

```bash
rsprune [OPTIONS] [tsconfig]
```

Just run `rsprune` from your project root — it finds `tsconfig.json` automatically.

### Options

| Flag | Description |
|------|-------------|
| `--ignore-files <regex>` | Skip files matching a regex (e.g. `--ignore-files '\.spec\.'`) |
| `--exclude-paths-from-report <path>` | Omit paths from output (e.g. `--exclude-paths-from-report src/test`) |
| `--timing` | Print per-phase timing breakdown to stderr |
| `--bundler-contexts` | Treat files loaded by `import.meta.webpackContext()` or `require.context()` as fully used |
| `--test-files <REGEX>` | Treat matching files as tests and also report exports that only they use (repeatable) |

### Examples

```bash
# Run in current directory
rsprune

# Custom tsconfig location
rsprune path/to/tsconfig.json

# Ignore test and spec files
rsprune --ignore-files '\.spec\.' --ignore-files '\.test\.'

# Count files loaded through webpack/rspack contexts as used
rsprune --bundler-contexts

# Also report exports that only spec files use
rsprune --test-files '\.spec\.'

# Use in CI — exits 1 if unused exports found
rsprune && echo "✅ Clean!"
```

### Output

```
3 modules with unused exports
src/utils/helpers.ts[12,0]: unusedFn
src/components/Button.tsx[34,7]: ButtonProps
src/lib/utils.ts[8,0]: formatDate
```

Format: `file[line,col]: exportName` (1-indexed lines, 0-indexed columns).

With `--test-files`, a second section lists exports used by tests but by no other file:

```
0 modules with unused exports
1 modules with exports only used by tests
src/utils/helpers.ts[20,13]: resetCache
```

An export isn't reported when its local binding is referenced elsewhere in its own module, or when another
export of the same binding (e.g. `export default Foo` alongside `export const Foo`) is used outside tests.

## 🔇 Suppressing warnings

Add a comment on the line before the export:

```ts
// rsprune:disable-next-line
export const internalHelper = () => {};
```

The `ts-unused-exports:disable-next-line` comment is also supported for compatibility with existing codebases.

## 🛠️ Development

```bash
cargo build --release
cargo test
```

## 📄 License

MIT
