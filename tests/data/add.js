(async function(wasmBytes) {
    console.log("Hello!");
    console.log(wasmBytes);
    let wasm = await WebAssembly.instantiate(wasmBytes, {});
    console.log("adding via wasm: " + wasm.instance.exports.add(5, 16));
})(arguments[0]);