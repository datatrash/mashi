# MASHI

A 100% no-std compatible Rust implementation of a [PAQ](http://mattmahoney.net/dc) style arithmetic coding, context mixing compressor. Its intended use case is compressing 64k demos, but whatever works for you!

### Quick start
The main crate is `mashi-core` but I've provided a very bare-bones `mashi-cli` so you can test the compressor.

To build and use the CLI:
* `cargo build --release`
* `target/release/mashi-cli compress tests/test.wasm`
* `target/release/mashi-cli decompress tests/test.wasm.mashi`

### Unscientific benchmarks
Compressing `tests/test.wasm` with various compressors:

| Compressor | Size | Command-line |
| - | - | - |
| mashi | 12084 | `mashi-cli compress` |
| zpaq | 13004 | `zpaq a -m5` |
| xz | 13522 | `xz --format=raw --lzma2=preset=9e` |
| zstd | 14536 | `zstd --ultra -22` |
| gzip | 15629 | `gzip --9` |

### License
Since the original PAQ compressors are GPL licensed be aware that Mashi is also GPL licensed.