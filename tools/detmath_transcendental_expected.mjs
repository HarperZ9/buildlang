// Writes tests/detmath/transcendental.expected: the JS twin's exp, log and pow
// for the inputs transcendental.bld prints, as bit patterns ("nan" for NaN).
// Run: node tools/detmath_transcendental_expected.mjs > tests/detmath/transcendental.expected
import { detExp, detLog, detPow } from './detmath/transcendental.mjs';

const dv = new DataView(new ArrayBuffer(8));
const hex = (v) => { if (Number.isNaN(v)) return 'nan'; dv.setFloat64(0, v); return dv.getBigUint64(0).toString(16).padStart(16, '0'); };
const lines = [];
const row = (i, x, y, e) => {
  lines.push(`${i} pow ${hex(detPow(x, y))}`, `${i} exp ${hex(detExp(e))}`, `${i} log ${hex(detLog(x))}`);
};
let s = 12345;
for (let i = 0; i < 400; i++) {
  s = (s * 48271) % 2147483647;
  const x = s / 2147483647 * 4 + 0.000000001;
  s = (s * 48271) % 2147483647;
  const y = s / 2147483647 * 16 - 8;
  row(i, x, y, y * 4);
}
const tiny = 5e-324;
row(400, 0, -1, 0); row(401, -0, 3, -0); row(402, 1, Infinity, 1); row(403, -1, -Infinity, -1);
row(404, -2, 3, 709.7); row(405, -2, 0.5, -745.2); row(406, 0.9999999, 1075, 700); row(407, 0.9999999, -1076, -700);
row(408, 2, -1074.5, 2); row(409, 10, 308, 0.5); row(410, 3, 40, 1); row(411, tiny, 0.5, tiny);
row(412, 1e-310, 2.4, 1e-310); row(413, Infinity, -0.5, Infinity); row(414, NaN, 0, NaN);
row(415, 0.7529411764705882, 1 / 2.2, 0.0000000001);
process.stdout.write(lines.join('\n') + '\n');
