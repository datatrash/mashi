(async function(mashiBytes) {
    document.body.innerHTML = "<progress id=p>";
    let mashi = await WebAssembly.instantiate(mashiBytes);

    let step = () => {
        let progress = mashi.instance.exports.d();
        document.getElementById("p").value = progress;
        if (progress) {
            // more to do
            setTimeout(step, 0);
        } else {
            // Grab the sizes of js/wasm from memory and decode everything from 0x100000 onwards
            let mem = mashi.instance.exports.memory.buffer;
            let view = new DataView(mem);
            let js_size = view.getUint32(8, true);
            let wasm_size = view.getUint32(12, true);
            let js = new TextDecoder().decode(new Uint8Array(mem, 0x100000, js_size));
            let wasm = new Uint8Array(mem, 0x100000 + js_size, wasm_size);
            new Function(js)(wasm);
        }
    };
    step();
})(arguments[0]);