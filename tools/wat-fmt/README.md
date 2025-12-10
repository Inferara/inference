<div align="center">
  <h1><code>wat-fmt</code></h1>

  <p>
    <strong>A pretty formatter for <a href="https://webassembly.github.io/spec/core/text/index.html">WebAssembly Text Format</a>. It is fast, flexible, and supports <a href="https://www.inferara.com/en/papers/specifying-algorithms-using-non-deterministic-computations/">Inference non-deterministic instructions</a>.</strong>
  </p>

  <p>
    <a href="https://crates.io/crates/wat-fmt"><img src="https://img.shields.io/crates/v/wat-fmt.svg?style=flat-square" alt="Crates.io version" /></a>
    <a href="https://crates.io/crates/wat-fmt"><img src="https://img.shields.io/crates/d/wat-fmt.svg?style=flat-square" alt="Download" /></a>
  </p>

</div>

## Build

`wat-fmt` is a one file crate with `#[no_std]` support. It can be built for different targets. The standard `cargo build` command will build the crate for the host target.

### Build for WASM

Since `wat-fmt` is a `no_std` crate, it can be built for WASM. This is useful for running the formatter in the browser or in a WebAssembly runtime.

Prerequisites:

```bash
cargo install wasm-pack
```
Alternatively, you can use another tool to build WASM  binaries.

Uncomment the crate type in the `Cargo.toml` file:

```toml
[lib]
crate-type = ["cdylib"]
```

To build the crate for WASM, run the following command with the `wasm` feature:

```bash
wasm-pack build --target web --features wasm
```

## Examples

Source: `(module (func $add (param $a i32) (param $b i32) (result i32) (local $c i32) i32.uzumaki local.set $c local.get $a local.get $c i32.add) (export "add" (func $add) ) )`

Formatted:
```wat
(module
  (func $add (param $a i32) (param $b i32) (result i32)
    (local $c i32)
    i32.uzumaki
    local.set $c
    local.get $a
    local.get $c
    i32.add
  )
  (export "add" (func $add) )
)
```

### WebAssembly example

index.html:
```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>WAT Formatter</title>
    <script type="module">
        import {handleButtonClick} from './main.js';
    </script>
</head>
<body>
    <h1>WAT Formatter by <a href="https://inferara.com/">Inferara</a></h1>
    <textarea id="input" rows="10" cols="30">(module (func $add (param $a i32) (param $b i32) (result i32) (local $c i32) i32.uzumaki local.set $c local.get $a local.get $c i32.add) (export "add" (func $add) ) )</textarea>
    <button onclick="handleButtonClick()">Format Input</button>
    <h2>Output:</h2>
    <pre id="output"></pre>
    <script type="module" src="./main.js"></script>
</body>
</html>
```

main.js
```javascript
import init, { format } from '../pkg/wat_fmt.js';

let wasmInitialized = false;

export async function initWasm() {
    await init();
    wasmInitialized = true;
}

export async function handleButtonClick() {
    if (!wasmInitialized) {
        console.error("WebAssembly module is not initialized.");
        return;
    }

    const input = document.getElementById('input').value;

    try {
        const result = format(input);
        document.getElementById('output').textContent = result;
    } catch (error) {
        console.error("Error calling format function:", error);
    }
}

window.onload = initWasm;
window.handleButtonClick = handleButtonClick;
```


## License

MIT
