// Writes tests/json-float/corpus.txt: one double a line as its 16 hex digits,
// a space, and the text JSON.stringify writes for it. The corpus covers zeros,
// NaN and the infinities, subnormals, the 1e-7 and 1e21 notation switches,
// every power of ten in range, integers near 2^53, and seeded random bit
// patterns and decimals. Run: node tools/json_float_corpus.mjs > tests/json-float/corpus.txt
const dv = new DataView(new ArrayBuffer(8));
const hex = (x) => { dv.setFloat64(0, x); return dv.getBigUint64(0).toString(16).padStart(16, '0'); };
let seed = 20261023;
const rnd = () => { seed = (Math.imul(seed, 1103515245) + 12345) >>> 0; return seed; };
const vals = [0, -0, NaN, Infinity, -Infinity, 1, -1, 0.1, 0.2, 0.1 + 0.2, 0.3, 1 / 3, 2 / 3, 4.35,
  100, 1790, Math.PI, Math.E, 5e-324, -5e-324, 2.2250738585072014e-308, 2.225073858507201e-308,
  1.7976931348623157e308, -1.7976931348623157e308, 2 ** 53, 2 ** 53 + 2, 2 ** 53 - 1, -(2 ** 53),
  1e21, 1e20, 999999999999999900000, 9.999999999999999e20, 123456789012345680000, 1e-6, 1e-7,
  1.5e-7, 9.999999999999999e-7, 0.000001, 0.0000012345, 1e300, 1.2345e-300, 0.5, 0.25, 0.125];
for (let e = -323; e <= 308; e++) vals.push(Number('1e' + e), Number('-2.5e' + e));
for (let i = 0; i < 800; i++) { dv.setUint32(0, rnd()); dv.setUint32(4, rnd()); vals.push(dv.getFloat64(0)); }
for (let i = 0; i < 400; i++) vals.push((rnd() % 200000 - 100000) / 64, (rnd() % 100000) * 1e-9, (rnd() % 1000000) / 1000);
process.stdout.write(vals.map((v) => `${hex(v)} ${JSON.stringify(v)}`).join('\n') + '\n');
