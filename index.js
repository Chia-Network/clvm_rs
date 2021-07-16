import * as wasm from "clvm";
import { hexlify } from "binascii";
import { Buffer } from "buffer";
import * as clvm_tools from "clvm_tools";
import {Elm, ports} from "./src/Main.elm";

const app = Elm.Main.init({node: document.getElementById("main")});
const evalRE = /EvalErr\((.*)\)/;

function disassemble(buf) {
    let resultText = '';
    clvm_tools.setPrintFunction((t) => {resultText += t;});
    const tstring = buf.toString('hex');
    clvm_tools.opd(['',tstring]);
    return resultText;
}

app.ports.requestRunClvm.subscribe((msg) => {
    const prog = msg[1];
    const args = msg[2];
    try {
        // Since clvm clashes with the clvm_tools import, we use clvm_tools
        // as an intermediary.
        const argsString = `(1 . ${args})`;
        const assembledArgs = clvm_tools.assemble(argsString);
        const SExp = assembledArgs.constructor;
        const argsEncoded = Buffer.from(assembledArgs.toString(), 'hex');
        const evaluatedArgs = wasm.run_clvm(argsEncoded, new Uint8Array([0x80]));
        const t = wasm.run_clvm(Buffer.from(prog, 'hex'), evaluatedArgs);
        const tbuffer = Buffer.from(t);
        const stringResult = tbuffer.toString('utf-8');
        const matchResult = stringResult.match(evalRE);
        if (matchResult) {
            const jsonData = `[${matchResult[1]}]`;
            throw JSON.parse(jsonData);
        }
        const tstring = tbuffer.toString('hex');
        const reassembledOutput = disassemble(tbuffer);
        app.ports.respondRunClvm.send([msg[0], "", [tstring, reassembledOutput]]);
    } catch (e) {
        app.ports.respondRunClvm.send([msg[0], e.toString(), ["",""]]);
    }
});

app.ports.requestCompileClvm.subscribe((msg) => {
    try {
        const assembled = clvm_tools.assemble(msg[1]);
        const serialized = assembled.toString();
        app.ports.respondCompileClvm.send([msg[0], "", serialized]);
    } catch(e) {
        app.ports.respondCompileClvm.send([msg[0], e.toString(), ""]);
    }
});
