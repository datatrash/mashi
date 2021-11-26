# MASHI まし

A 100% no-std compatible Rust implementation of a [PAQ](http://mattmahoney.net/dc) style arithmetic coding, context mixing compressor. Its intended use case is compressing 64k demos, but whatever works for you!

### Quick start
The main crate is `mashi-core` but I've provided a very bare-bones `mashi-cli` so you can test the compressor.

To build and use the CLI:
* `cargo build --release`
* `target/release/mashi-cli compress tests/test.wasm`
* `target/release/mashi-cli decompress tests/test.wasm.mashi`

### Cargo features
* Use the `encoder` feature if you want to include the encoder
* Use the `std` feature to enable std support (e.g. for running the unit tests)

### Unscientific benchmarks
Compressing `tests/test.wasm` with various compressors:

| Compressor | % | Size | Command-line |
| - | - | - | - |
| mashi | 34.4% | 12083 | `mashi-cli compress` |
| zpaq | 37.0% | 13004 | `zpaq a -m5` |
| xz | 38.5% | 13522 | `xz --format=raw --lzma2=preset=9e` |
| zstd | 41.3% | 14536 | `zstd --ultra -22` |
| gzip | 44.5% | 15629 | `gzip --9` |
| original | 100% | 35151 | |

### License
Since the original PAQ compressors are GPL licensed be aware that Mashi is also GPL licensed.

### Credits
* The compression model is based on the PAQ work by [Matt Mahoney](http://mattmahoney.net/dc) et al.
* The arithmetic coder is a port of the one in [Crinkler](https://github.com/runestubbe/Crinkler), done by Mentor and Blueberry.
* Wrapping my head around all of this was aided by the videos of [Jeff Miller](http://jwmi.github.io/) and the compression seminar by [Ferris](https://github.com/yupferris) at Revision 2020.
* Some of my porting efforts were eased by the work done by [aufdj](https://github.com/aufdj).