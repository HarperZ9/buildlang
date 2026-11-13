// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
//
// Loader for modules built with `buildc FILE.bld --target wasm`, for pages
// and for Node. No dependencies.
//
// Calling convention (the memory layout is the ABI):
//   - integers: i32 for 32-bit and smaller, BigInt for 64-bit (i64, usize)
//   - a string is an i32 handle to an 8-byte header {ptr u32, len u32};
//     strings are immutable, so a handle may be shared freely
//   - a Vec is an i32 handle to a 12-byte header {ptr u32, len u32, cap u32};
//     copying a handle shares the vector, as in the C backend
//   - the module exports `memory`, `__alloc(size) -> ptr` and
//     `__str_new(ptr, len) -> handle`, plus every function it defines
// Memory can grow during any call, so views are taken fresh each time.

'use strict';

const encoder = new TextEncoder();
const decoder = new TextDecoder();

async function load(source, options = {}) {
  const write = options.write || ((s) => console.log(s));
  let instance = null;
  const bytesView = () => new Uint8Array(instance.exports.memory.buffer);
  const words = () => new DataView(instance.exports.memory.buffer);
  const env = {
    print_i64: (v) => write(v.toString()),
    print_u64: (v) => write(BigInt.asUintN(64, v).toString()),
    print_f64: (v) => write(String(v)),
    print_bool: (v) => write(v ? 'true' : 'false'),
    print_str: (p, l) => write(decoder.decode(bytesView().subarray(p, p + l))),
  };
  const result = await WebAssembly.instantiate(source, { env });
  instance = result.instance || result;
  const x = instance.exports;

  const copyIn = (bytes) => {
    const p = x.__alloc(bytes.length);
    bytesView().set(bytes, p);
    return p;
  };
  const header = (h, n) => {
    const w = words();
    return Array.from({ length: n }, (_, i) => w.getUint32(h + 4 * i, true));
  };

  return {
    exports: x,
    /** A string handle holding `s` (UTF-8). */
    str(s) {
      const b = encoder.encode(s);
      return x.__str_new(copyIn(b), b.length);
    },
    /** The text of a string handle. */
    readStr(h) {
      const [p, l] = header(h, 2);
      return decoder.decode(bytesView().slice(p, p + l));
    },
    /** A Vec<u8> handle holding a copy of `bytes`. */
    bytes(bytes) {
      const p = copyIn(bytes);
      const h = x.__alloc(12);
      const w = words();
      w.setUint32(h, p, true);
      w.setUint32(h + 4, bytes.length, true);
      w.setUint32(h + 8, bytes.length, true);
      return h;
    },
    /** A copy of the bytes of a Vec<u8> handle. */
    readBytes(h) {
      const [p, l] = header(h, 2);
      return bytesView().slice(p, p + l);
    },
    /** A Vec handle of i32 handles (for Vec<Vec<u8>> and Vec<str>). */
    handles(list) {
      const raw = new Uint8Array(new Uint32Array(list).buffer);
      return this.bytesWithLength(raw, list.length);
    },
    bytesWithLength(raw, len) {
      const h = this.bytes(raw);
      const w = words();
      w.setUint32(h + 4, len, true);
      w.setUint32(h + 8, len, true);
      return h;
    },
    /** Run the module's `main`. */
    main() {
      return x.main();
    },
  };
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = { load };
} else {
  globalThis.BuildLangWasm = { load };
}
