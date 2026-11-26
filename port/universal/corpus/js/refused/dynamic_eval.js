function bad(x) {
    let y = x + 1;
    let z = eval("y * 2");
    let p = new Proxy({}, {});
    return z;
}
