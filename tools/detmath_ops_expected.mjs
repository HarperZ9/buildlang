// Writes tests/detmath/ops.expected: the JS values of the operations ops.bld
// prints, as bit patterns. Run: node tools/detmath_ops_expected.mjs > tests/detmath/ops.expected
const dv = new DataView(new ArrayBuffer(8));
const hex = (v) => { dv.setFloat64(0, v); return dv.getBigUint64(0).toString(16).padStart(16, '0'); };
const isNaN1 = (v) => (Number.isNaN(v) ? 1 : 0);
const rows = [
  ['sqrt(2)', Math.sqrt(2)], ['sqrt(3)', Math.sqrt(3)], ['sqrt(1e-300)', Math.sqrt(1e-300)],
  ['floor(-0.5)', Math.floor(-0.5)], ['floor(2.999999999999999)', Math.floor(2.999999999999999)],
  ['floor(-1e300)', Math.floor(-1e300)], ['abs(-0)', Math.abs(-0)], ['abs(-2.75)', Math.abs(-2.75)],
  ['min(0,-0)', Math.min(0, -0)], ['min(-0,0)', Math.min(-0, 0)], ['max(0,-0)', Math.max(0, -0)],
  ['max(-0,0)', Math.max(-0, 0)], ['min(1,NaN) is NaN', isNaN1(Math.min(1, NaN))],
  ['max(NaN,1) is NaN', isNaN1(Math.max(NaN, 1))], ['min(2.5,-1.25)', Math.min(2.5, -1.25)],
  ['max(2.5,-1.25)', Math.max(2.5, -1.25)],
  ['max(0.1,0.30000000000000004)', Math.max(0.1, 0.30000000000000004)],
  ['1/3', 1 / 3], ['0.1+0.2', 0.1 + 0.2],
];
process.stdout.write(rows.map(([k, v]) => `${k} ${hex(v)}`).join('\n') + '\n');
