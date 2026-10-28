// Writes tests/detmath/trig.expected: the JS twin's sin and cos for the inputs
// trig.bld prints, as bit patterns ("nan" for NaN). The 1e7..1e308 inputs are
// built with the twin's pow, as BuildLang builds them, so the inputs match.
// Run: node tools/detmath_trig_expected.mjs > tests/detmath/trig.expected
import { detPow, detSin, detCos } from './detmath/transcendental.mjs';

const dv = new DataView(new ArrayBuffer(8));
const hex = (v) => { if (Number.isNaN(v)) return 'nan'; dv.setFloat64(0, v); return dv.getBigUint64(0).toString(16).padStart(16, '0'); };
const lines = [];
const row = (i, x) => { lines.push(`${i} sin ${hex(detSin(x))}`, `${i} cos ${hex(detCos(x))}`); };
let s = 99991;
for (let i = 0; i < 400; i++) {
  s = (s * 48271) % 2147483647;
  const u = s / 2147483647;
  s = (s * 48271) % 2147483647;
  const sign = s / 2147483647 < 0.5 ? -1 : 1;
  const k = i % 4;
  const x = k === 0 ? sign * u * 0.8 : k === 1 ? sign * u * 100 : k === 2 ? sign * u * 1000000 : sign * detPow(10, 7 + u * 301);
  row(i, x);
}
const pi = 3.141592653589793;
row(400, 0); row(401, -0); row(402, 5e-324); row(403, 0.0000000000000001); row(404, pi / 4); row(405, pi / 2);
row(406, pi); row(407, 1000 * pi / 2); row(408, 1e22); row(409, 894075887953458.8); row(410, 1.7976931348623157e308);
row(411, -1e300); row(412, Infinity); row(413, NaN);
process.stdout.write(lines.join('\n') + '\n');
