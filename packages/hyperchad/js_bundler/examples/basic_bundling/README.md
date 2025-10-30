# Basic JavaScript Bundling Example

## Summary

This example demonstrates how to use the hyperchad_js_bundler package to bundle multiple JavaScript files into a single optimized output file using either SWC or esbuild.

## What This Example Demonstrates

- Using the unified `bundle()` function to bundle JavaScript files
- Bundling multiple JavaScript modules with ES6 imports
- Automatic bundler selection (SWC or esbuild) based on feature flags
- Creating and reading bundled output files
- Verifying bundling results and output file size

## Prerequisites

- Basic understanding of JavaScript modules and bundling concepts
- Familiarity with Rust file I/O operations
- For esbuild: Node.js and npm/pnpm/bun must be installed
- For SWC: No external dependencies required (pure Rust implementation)

## Running the Example

Execute the example from the repository root:

```bash
cargo run --manifest-path packages/hyperchad/js_bundler/examples/basic_bundling/Cargo.toml
```

To run with only SWC bundler (no Node.js required):

```bash
cargo run --manifest-path packages/hyperchad/js_bundler/examples/basic_bundling/Cargo.toml --no-default-features --features swc
```

To run with only esbuild bundler (requires Node.js):

```bash
cargo run --manifest-path packages/hyperchad/js_bundler/examples/basic_bundling/Cargo.toml --no-default-features --features esbuild
```

## Expected Output

When you run the example, you should see output similar to:

```
=== HyperChad JavaScript Bundler Example ===

Input file: /path/to/js_source/index.js
Output file: /path/to/dist/bundle.js

Source files to bundle:
  - index.js
  - math.js
  - utils.js

Starting bundling process...
Using bundler: SWC (Rust-based bundler with minification)
load: loading file /path/to/js_source/index.js
load: loading file /path/to/js_source/utils.js
load: loading file /path/to/js_source/math.js
Bundled as 1 bundles
Created /path/to/dist/bundle.js (1KiB)

✓ Bundling successful!
Output file size: 1 KB

First 5 lines of bundled output:
  1: function greet(name){return `Hello from ${name} bundler!`}
  2: function calculate(a,b){return add(a,b)*multiply(2,1)}
  3: function add(x,y){return x+y}
  4: function multiply(x,y){return x*y}
  5: console.log(greet("HyperChad"));
  ... (more lines)

=== Example Complete ===
```

The bundler will create a `dist/bundle.js` file containing the combined and minified JavaScript code.

## Code Walkthrough

### 1. Setting Up Paths

The example first establishes the input and output paths:

```rust
let example_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
let source_dir = example_dir.join("js_source");
let output_dir = example_dir.join("dist");

let input_file = source_dir.join("index.js");
let output_file = output_dir.join("bundle.js");
```

### 2. Verifying Input Files

Before bundling, the example checks that the source file exists:

```rust
if !input_file.exists() {
    eprintln!("Error: Input file does not exist: {}", input_file.display());
    return Err("Input file not found".into());
}
```

### 3. Performing the Bundling

The core bundling operation uses the unified `bundle()` function:

```rust
fn bundle_javascript(input: &Path, output: &Path) {
    println!("Using bundler: {}", get_bundler_name());
    hyperchad_js_bundler::bundle(input, output);
}
```

The `bundle()` function automatically:

- Selects SWC if the `swc` feature is enabled (preferred)
- Falls back to esbuild if only the `esbuild` feature is enabled
- Resolves module dependencies from import statements
- Combines all modules into a single output file
- Minifies the output (SWC) or bundles with minification (esbuild)

### 4. Displaying Results

After bundling, the example displays information about the output:

```rust
let metadata = fs::metadata(&output_file)?;
let size_kb = metadata.len() / 1024;
println!("✓ Bundling successful!");
println!("Output file size: {} KB", size_kb);
```

### 5. Source Files Structure

The example includes three JavaScript files that demonstrate module imports:

**`index.js`** (entry point):

```javascript
import { greet } from './utils.js';
import { calculate } from './math.js';

function main() {
    console.log(greet('HyperChad'));
    console.log(`Calculation result: ${calculate(10, 5)}`);
}
```

The bundler resolves these imports and combines all files into one.

## Key Concepts

### Unified Bundler Interface

The `hyperchad_js_bundler::bundle()` function provides a single API that works with multiple bundlers. You don't need to choose which bundler to use at runtime - it's determined at compile time based on feature flags.

### Feature-Based Selection

The package uses Cargo features to enable different bundlers:

- **`swc` feature**: Enables the SWC bundler (pure Rust, no external dependencies)
- **`esbuild` feature**: Enables esbuild bundler (requires Node.js and npm)
- **Default features**: Both bundlers are enabled, with SWC taking priority

### Module Resolution

Both bundlers automatically resolve ES6 module imports:

- Follow `import` statements to find dependencies
- Resolve relative paths (e.g., `./utils.js`)
- Bundle all required modules into a single file
- Eliminate unused code (tree shaking)

### Minification

The bundlers optimize the output:

- **SWC**: Provides comprehensive minification with compress and mangle options
- **esbuild**: Uses `--minify` flag for fast minification
- Both reduce file size and improve load times

## Testing the Example

### Verify the Output File

After running the example, check that the output file was created:

```bash
ls -lh packages/hyperchad/js_bundler/examples/basic_bundling/dist/bundle.js
```

### Inspect the Bundled Code

View the bundled output to see how modules were combined:

```bash
cat packages/hyperchad/js_bundler/examples/basic_bundling/dist/bundle.js
```

You should see all three JavaScript files combined into one, with minified code.

### Compare Bundlers

Run the example with different bundlers to compare:

```bash
# Using SWC (default)
cargo run --manifest-path packages/hyperchad/js_bundler/examples/basic_bundling/Cargo.toml

# Using esbuild only
cargo run --manifest-path packages/hyperchad/js_bundler/examples/basic_bundling/Cargo.toml --no-default-features --features esbuild
```

Compare the output files to see differences in minification strategies and file sizes.

## Troubleshooting

### "Input file not found" Error

**Problem**: The example can't find the JavaScript source files.

**Solution**: Ensure you're running the example from the repository root, and that the `js_source` directory exists with the JavaScript files.

### esbuild Not Found

**Problem**: When using the esbuild feature, you see errors about esbuild not being found.

**Solution**: Ensure Node.js and npm (or pnpm/bun) are installed. The example will automatically run `npm install` to install esbuild, but this requires Node.js to be available in your PATH.

### Compilation Errors

**Problem**: The example fails to compile.

**Solution**: Make sure you have the required features enabled. The default features include both `swc` and `esbuild`, but you can enable them individually with `--features swc` or `--features esbuild`.

### Empty Output File

**Problem**: The bundle.js file is created but is empty or incomplete.

**Solution**: Check the console output for bundler errors. Ensure the JavaScript source files have valid syntax and use standard ES6 module syntax.

## Related Examples

This is currently the only example for hyperchad_js_bundler. Future examples may include:

- Advanced bundling with custom configurations
- TypeScript bundling examples
- Integration with build pipelines
- Multi-entry point bundling

---

Generated with Claude Code
