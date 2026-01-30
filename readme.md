# MASHI まし

Mashi is a 100% no-std compatible WASM compressor that has best-in-class compression performance thanks to context
modelling that is specifically tuned for WASM binaries.

The main purpose of Mashi is to enable and promote WASM usage in size limited demoscene productions.

### Using Mashi for your demoscene productions

The most straightforward way of using Mashi is to use it to create a single `index.html` from a Javascript file and a
WASM file.

The resulting `index.html` can be opened in a browser (with local file access enabled) and will automatically invoke
your Javascript code, passing in the decompressed WASM binary as a parameter. You can then instantiate the WASM module
from your code and off you go.

### Embedding Mashi in your own toolchain

You can use the `mashi-core` crate to manually (de)compress WASM files using the provided `compress` and `decompress`
methods.

### Unscientific benchmarks

Compressing `mashi-core/tests/test.wasm` with various compressors:

| Compressor | %     | Size  | Command-line                        |
|------------|-------|-------|-------------------------------------|
| mashi      | 27.9% | 9813  | TBD                                 |
| zpaq       | 37.0% | 13004 | `zpaq a -m5`                        |
| xz         | 38.5% | 13522 | `xz --format=raw --lzma2=preset=9e` |
| zstd       | 41.3% | 14536 | `zstd --ultra -22`                  |
| gzip       | 44.5% | 15629 | `gzip --9`                          |
| original   | 100%  | 35151 |                                     |

### Status

This is a very early release, so beware. Having said that, all files in the official WebAssembly Test Suite can be
successfully compressed and decompressed without Mashi eating any bytes.

Also, the decompressor stub is currently still quite large (nearly 6k) which needs work.

### How to hack

1. Install a nightly toolchain, since the build process requires access to unstable features.
2. Do your worst in `mashi-core`
3. Optional: Run the test suite in `mashi-test-suite` to check for any regressions
4. To build the decompressor stub used by the CLI, run `cargo xtask build-prerequisites` which will also report on the
   size the decompressor will end up being after Zopfli compression used by the `index.html` loader.
5. Build `mashi-cli`

### Credits

* The compression model is based on the Squishy compressor by [Ferris](http://github.com/yupferris).
* Other work (WASM context modelling, tooling, etc) by [Sagacity](https://github.com/sagacity).