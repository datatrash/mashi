(async function(mashiBytes) {
    console.log("hai");
    console.log(mashiBytes);
    let mashi = await WebAssembly.instantiate(mashiBytes, {});
    mashi.instance.exports.d();
    let mem = mashi.instance.exports.memory.buffer;
    let view = new DataView(mem);
    let js_size = view.getUint32(8, true);
    let wasm_size = view.getUint32(12, true);
    let js = new TextDecoder().decode(new Uint8Array(mem, 1048576, js_size));
    let wasm = new Uint8Array(mem, 1048576 + js_size, wasm_size);
    new Function(js)(wasm);
})(arguments[0]);