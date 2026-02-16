(async function(mashiBytes) {
    let d = document;
    d.body.innerHTML = "<progress id=p>";
    let mashi = await WebAssembly.instantiate(mashiBytes);

    let step = () => {
        let progress = mashi.instance.exports.d();
        d.getElementById("p").value = progress;
        if (progress) {
            // more to do
            setTimeout(step, 0);
        } else {
            let mem = mashi.instance.exports.memory.buffer;
            let view = new DataView(mem);
            let pos = 0x700008;
            let js_size = view.getUint32(pos, true);
            let wasm_size = view.getUint32(pos + 4 , true);
            let js = new TextDecoder().decode(new Uint8Array(mem, 0, js_size));
            let wasm = new Uint8Array(mem, js_size, wasm_size);
            new Function(js)(wasm);
        }
    };
    step();
})(arguments[0]);