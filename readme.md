# MASHI まし

Mashi is a Javascript+WASM compressor for use in 64k intros.

The compression engine is based on Squishy by Ferris/Logicoma. Specific WASM context models have been added to improve the compression ratio of WASM payloads.

However, Mashi can also be used for JS-only productions.

### Using Mashi for your demoscene productions

If you just have an `intro.js`:

`mashi pack intro.js out.html`

if you have an `intro.js` which should receive the bytes in `intro.wasm` as an argument:

`mashi pack intro.js --wasm intro.wasm out.html`

The resulting `out.html` can be opened in a browser (with local file access enabled) and will automatically invoke
your Javascript code, passing in the decompressed WASM binary as a parameter. You can then instantiate the WASM module
from your code and off you go.

Please note that if you're opening this file locally, your browser probably will block the loader and you will need to start your browser with a special flag. For Chrome you will need to pass in the `--allow-file-access-from-files` command line flag, or you can host the resulting `out.html` in a simple web server instead.

### In the wild

* [Raltron by Fnuque](https://www.pouet.net/prod.php?which=106875), winner of the Evoke 2026 64k intro competition

### Status

This is a very early release, so beware. Having said that, all files in the official WebAssembly Test Suite can be
successfully compressed and decompressed without Mashi eating any bytes.

Browsers other than Chrome and FireFox are untested, currently. Feel free to test and report back!

The decompression stub is written in pure WASM and takes about 2.5k.

### How to hack

1. Install a nightly toolchain, since the build process requires access to unstable features.
1. Install the `binaryen` toolset (specifically `wasm-opt`) and `bun`.
1. Do your worst!
1. Run all unit and integration tests (enable Cargo feature `manual-tests`) to check for any regressions
1. Profit / bask in glory

### Credits

* The compression model is based on the Squishy compressor by [Ferris](https://github.com/yupferris).
* Other work (WASM context modelling, tooling, etc) by [Sagacity](https://github.com/sagacity).
