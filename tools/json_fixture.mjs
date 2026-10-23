// Writes tests/json/docs.txt (one JSON document a line, valid and invalid) and
// tests/json/docs.expected: for a valid document, JSON.stringify of the parsed
// value and the same with every object's keys sorted; for an invalid one, the
// word "error" twice. Run: node tools/json_fixture.mjs
import { writeFileSync } from 'node:fs';
const sortKeys = (v) => Array.isArray(v) ? v.map(sortKeys)
  : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, sortKeys(v[k])])) : v;
// esc("u0041") is a JSON unicode escape: a backslash, then the given text.
const esc = (...units) => '"' + units.map((u) => String.fromCharCode(92) + u).join('') + '"';
const docs = [
  'null', 'true', 'false', '0', '-0', '1', '-1', '0.1', '1e21', '1E-7', '-2.5e+3', '123456789012345678901234567890',
  '5e-324', '1.7976931348623157e308', '0.30000000000000004', '"plain"', '""', String.raw`"tab\there"`,
  String.raw`"quote \" and backslash \\ and slash \/"`, String.raw`"\b\f\n\r\t"`, esc("u0041", "u00e9", "u4e2d"),
  esc("ud83d", "ude00"), String.raw`"\u0001\u001f"`, '"café 中文"', '[]', '{}', '[[],[[]],{}]',
  '  [ 1 , 2 , 3 ]  ', '\t{"a" :\t1 }',
  '{"b":1,"a":2,"c":{"z":[1,{"y":2,"x":3}],"m":null}}',
  '{"id":"riposte","frames":[0,6,12],"dmg":12.5,"tags":["fast","low"]}',
  '{"t":"cbegin","v":1,"seed":7,"arena":"yard","fighters":[{"id":1,"team":0,"kind":"lamb","x":0.5,"z":-2,"face":0}]}',
  '{"":0}', '[-0.0, 0.0, 100, 1e2]', '{"a":1,"b":2,"a":3}', '[1e400, -1e400]',
  // invalid
  '[1,2,]', '{"a":1,}', '[01]', '{a:1}', "'x'", '"unterminated', '[1 2]', 'tru', 'nul', '1.', '.5', '-',
  String.raw`"bad \x escape"`, '"ctrl \u0001 raw"', '[1]x', '{"a"}', '{"a":}', '+1',
];
const lines = [];
for (const d of docs) {
  try { const v = JSON.parse(d); lines.push(JSON.stringify(v), JSON.stringify(sortKeys(v))); }
  catch { lines.push('error', 'error'); }
}
writeFileSync('tests/json/docs.txt', docs.join('\n') + '\n');
writeFileSync('tests/json/docs.expected', lines.join('\n') + '\n');
