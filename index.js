import * as wasm from "clvm";


// https://stackoverflow.com/questions/14603205/how-to-convert-hex-string-into-a-bytes-array-and-a-bytes-array-in-the-hex-strin/36050152
function parseHexString(str) { 
    var result = [];
    while (str.length >= 2) { 
        result.push(parseInt(str.substring(0, 2), 16));

        str = str.substring(2, str.length);
    }

    return new Uint8Array(result);
}

function createHexString(arr) {
    var result = "";
    var z;

    for (var i = 0; i < arr.length; i++) {
        var str = arr[i].toString(16);

        z = 2 - str.length + 1;
        str = Array(z).join("0") + str;

        result += str;
    }

    return result;
}


function run_program() {
    let prog = parseHexString(document.getElementById("program").innerHTML);
    let args = parseHexString(document.getElementById("args").innerHTML);
    let t = wasm.run_clvm(prog, args);
    document.getElementById("output").innerHTML = createHexString(t);
}

run_program();
