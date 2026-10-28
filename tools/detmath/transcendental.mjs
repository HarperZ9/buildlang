// DETMATH transcendentals in JavaScript: exp, log, pow, sin and cos by the same fdlibm
// algorithms as the BuildLang C runtime (compiler/src/codegen/runtime.rs,
// build_det_exp, _log, _pow, _sin and _cos). Every step is an IEEE 754
// double operation or a 32-bit word operation, so these give the same bits as
// BuildLang on any engine. Math.exp, Math.log, Math.pow, Math.sin and Math.cos are not specified to
// the bit and change between engine versions; use these where bits must agree.
//
// Algorithms and constants from fdlibm (http://www.netlib.org/fdlibm):
// ====================================================
// Copyright (C) 1993, 2004 by Sun Microsystems, Inc. All rights reserved.
//
// Developed at SunSoft, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this
// software is freely granted, provided that this notice
// is preserved.
// ====================================================

const dv = new DataView(new ArrayBuffer(8));
const hi = (x) => { dv.setFloat64(0, x); return dv.getInt32(0); };           // int32
const lo = (x) => { dv.setFloat64(0, x); return dv.getUint32(4); };          // uint32
const words = (h, l) => { dv.setUint32(0, h >>> 0); dv.setUint32(4, l >>> 0); return dv.getFloat64(0); };
const setHi = (x, h) => words(h, lo(x));
const setLo = (x, l) => words(hi(x), l);
const copysign = (a, b) => words((hi(a) & 0x7fffffff) | (hi(b) & 0x80000000), lo(a));

function scalbn(x, n) {
  const two54 = 1.80143985094819840000e+16, twom54 = 5.55111512312578270212e-17, huge = 1.0e+300, tiny = 1.0e-300;
  let hx = hi(x);
  const lx = lo(x);
  let k = (hx & 0x7ff00000) >> 20;
  if (k === 0) {
    if ((lx | (hx & 0x7fffffff)) === 0) return x;
    x *= two54;
    hx = hi(x);
    k = ((hx & 0x7ff00000) >> 20) - 54;
    if (n < -50000) return tiny * x;
  }
  if (k === 0x7ff) return x + x;
  k = k + n;
  if (k > 0x7fe) return huge * copysign(huge, x);
  if (k > 0) return setHi(x, (hx & 0x800fffff) | (k << 20));
  if (k <= -54) {
    if (n > 50000) return huge * copysign(huge, x);
    return tiny * copysign(tiny, x);
  }
  k += 54;
  x = setHi(x, (hx & 0x800fffff) | (k << 20));
  return x * twom54;
}

export function detExp(x) {
  const one = 1.0, halF = [0.5, -0.5], huge = 1.0e+300, twom1000 = 9.33263618503218878990e-302,
    oThreshold = 7.09782712893383973096e+02, uThreshold = -7.45133219101941108420e+02,
    ln2HI = [6.93147180369123816490e-01, -6.93147180369123816490e-01],
    ln2LO = [1.90821492927058770002e-10, -1.90821492927058770002e-10],
    invln2 = 1.44269504088896338700e+00,
    P1 = 1.66666666666666019037e-01, P2 = -2.77777777770155933842e-03, P3 = 6.61375632143793436117e-05,
    P4 = -1.65339022054652515390e-06, P5 = 4.13813679705723846039e-08;
  if (x === 1.0) return 2.718281828459045; // V8's one change to fdlibm exp: e itself, correctly rounded
  let y, hiv = 0.0, lov = 0.0, c, t, k = 0;
  let hx = hi(x) >>> 0;
  const xsb = hx >>> 31;
  hx = (hx & 0x7fffffff) >>> 0;
  if (hx >= 0x40862E42) {
    if (hx >= 0x7ff00000) {
      if (((hx & 0xfffff) | lo(x)) !== 0) return x + x;
      return xsb === 0 ? x : 0.0;
    }
    if (x > oThreshold) return huge * huge;
    if (x < uThreshold) return twom1000 * twom1000;
  }
  if (hx > 0x3fd62e42) {
    if (hx < 0x3FF0A2B2) {
      hiv = x - ln2HI[xsb]; lov = ln2LO[xsb]; k = 1 - xsb - xsb;
    } else {
      k = Math.trunc(invln2 * x + halF[xsb]);
      t = k;
      hiv = x - t * ln2HI[0];
      lov = t * ln2LO[0];
    }
    x = hiv - lov;
  } else if (hx < 0x3e300000) {
    if (huge + x > one) return one + x;
  } else {
    k = 0;
  }
  t = x * x;
  c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
  if (k === 0) return one - ((x * c) / (c - 2.0) - x);
  y = one - ((lov - (x * c) / (2.0 - c)) - hiv);
  if (k >= -1021) return setHi(y, hi(y) + (k << 20));
  y = setHi(y, hi(y) + ((k + 1000) << 20));
  return y * twom1000;
}

export function detLog(x) {
  const ln2_hi = 6.93147180369123816490e-01, ln2_lo = 1.90821492927058770002e-10, two54 = 1.80143985094819840000e+16,
    Lg1 = 6.666666666666735130e-01, Lg2 = 3.999999999940941908e-01, Lg3 = 2.857142874366239149e-01,
    Lg4 = 2.222219843214978396e-01, Lg5 = 1.818357216161805012e-01, Lg6 = 1.531383769920937332e-01,
    Lg7 = 1.479819860511658591e-01;
  let hfsq, f, s, z, R, w, t1, t2, dk, i, j, k = 0, hx = hi(x);
  const lx = lo(x);
  if (hx < 0x00100000) {
    if (((hx & 0x7fffffff) | lx) === 0) return -two54 / 0.0;
    if (hx < 0) return (x - x) / 0.0;
    k -= 54; x *= two54;
    hx = hi(x);
  }
  if (hx >= 0x7ff00000) return x + x;
  k += (hx >> 20) - 1023;
  hx &= 0x000fffff;
  i = (hx + 0x95f64) & 0x100000;
  x = setHi(x, hx | (i ^ 0x3ff00000));
  k += i >> 20;
  f = x - 1.0;
  if ((0x000fffff & (2 + hx)) < 3) {
    if (f === 0.0) {
      if (k === 0) return 0.0;
      dk = k;
      return dk * ln2_hi + dk * ln2_lo;
    }
    R = f * f * (0.5 - 0.33333333333333333 * f);
    if (k === 0) return f - R;
    dk = k;
    return dk * ln2_hi - ((R - dk * ln2_lo) - f);
  }
  s = f / (2.0 + f);
  dk = k;
  z = s * s;
  i = hx - 0x6147a;
  w = z * z;
  j = 0x6b851 - hx;
  t1 = w * (Lg2 + w * (Lg4 + w * Lg6));
  t2 = z * (Lg1 + w * (Lg3 + w * (Lg5 + w * Lg7)));
  i |= j;
  R = t2 + t1;
  if (i > 0) {
    hfsq = 0.5 * f * f;
    if (k === 0) return f - (hfsq - s * (hfsq + R));
    return dk * ln2_hi - ((hfsq - (s * (hfsq + R) + dk * ln2_lo)) - f);
  }
  if (k === 0) return f - s * (f - R);
  return dk * ln2_hi - ((s * (f - R) - dk * ln2_lo) - f);
}

export function detPow(x, y) {
  const bp = [1.0, 1.5], dp_h = [0.0, 5.84962487220764160156e-01], dp_l = [0.0, 1.35003920212974897128e-08],
    one = 1.0, two = 2.0, two53 = 9007199254740992.0, huge = 1.0e300, tiny = 1.0e-300,
    L1 = 5.99999999999994648725e-01, L2 = 4.28571428578550184252e-01, L3 = 3.33333329818377432918e-01,
    L4 = 2.72728123808534006489e-01, L5 = 2.30660745775561754067e-01, L6 = 2.06975017800338417784e-01,
    P1 = 1.66666666666666019037e-01, P2 = -2.77777777770155933842e-03, P3 = 6.61375632143793436117e-05,
    P4 = -1.65339022054652515390e-06, P5 = 4.13813679705723846039e-08,
    lg2 = 6.93147180559945286227e-01, lg2_h = 6.93147182464599609375e-01, lg2_l = -1.90465429995776804525e-09,
    ovt = 8.0085662595372944372e-17, cp = 9.61796693925975554329e-01, cp_h = 9.61796700954437255859e-01,
    cp_l = -7.02846165095275826516e-09, ivln2 = 1.44269504088896338700e+00,
    ivln2_h = 1.44269502162933349609e+00, ivln2_l = 1.92596299112661746887e-08;
  let z, ax, z_h, z_l, p_h, p_l, y1, t1, t2, r, s, t, u, v, w, i, j, k, n;
  const hx = hi(x), lx = lo(x), hy = hi(y), ly = lo(y);
  let ix = hx & 0x7fffffff;
  const iy = hy & 0x7fffffff;

  if ((iy | ly) === 0) return one;
  if (ix > 0x7ff00000 || (ix === 0x7ff00000 && lx !== 0) || iy > 0x7ff00000 || (iy === 0x7ff00000 && ly !== 0)) return x + y;

  let yisint = 0;
  if (hx < 0) {
    if (iy >= 0x43400000) yisint = 2;
    else if (iy >= 0x3ff00000) {
      k = (iy >> 20) - 0x3ff;
      if (k > 20) {
        const jj = ly >>> (52 - k);
        if (((jj << (52 - k)) >>> 0) === ly) yisint = 2 - (jj & 1);
      } else if (ly === 0) {
        j = iy >> (20 - k);
        if ((j << (20 - k)) === iy) yisint = 2 - (j & 1);
      }
    }
  }

  if (ly === 0) {
    if (iy === 0x7ff00000) {
      if (((ix - 0x3ff00000) | lx) === 0) return y - y;
      else if (ix >= 0x3ff00000) return hy >= 0 ? y : 0.0;
      else return hy < 0 ? -y : 0.0;
    }
    if (iy === 0x3ff00000) return hy < 0 ? one / x : x;
    if (hy === 0x40000000) return x * x;
    if (hy === 0x3fe00000 && hx >= 0) return Math.sqrt(x);
  }

  ax = Math.abs(x);
  if (lx === 0) {
    if (ix === 0x7ff00000 || ix === 0 || ix === 0x3ff00000) {
      z = ax;
      if (hy < 0) z = one / z;
      if (hx < 0) {
        if (((ix - 0x3ff00000) | yisint) === 0) z = (z - z) / (z - z);
        else if (yisint === 1) z = -z;
      }
      return z;
    }
  }

  n = hx < 0 ? 0 : 1; // fdlibm: (hx>>31)+1
  if ((n | yisint) === 0) return (x - x) / (x - x);

  s = one;
  if ((n | (yisint - 1)) === 0) s = -one;

  if (iy > 0x41e00000) {
    if (iy > 0x43f00000) {
      if (ix <= 0x3fefffff) return hy < 0 ? huge * huge : tiny * tiny;
      if (ix >= 0x3ff00000) return hy > 0 ? huge * huge : tiny * tiny;
    }
    if (ix < 0x3fefffff) return hy < 0 ? s * huge * huge : s * tiny * tiny;
    if (ix > 0x3ff00000) return hy > 0 ? s * huge * huge : s * tiny * tiny;
    t = ax - one;
    w = (t * t) * (0.5 - t * (0.3333333333333333333333 - t * 0.25));
    u = ivln2_h * t;
    v = t * ivln2_l - w * ivln2;
    t1 = setLo(u + v, 0);
    t2 = v - (t1 - u);
  } else {
    let ss, s2, s_h, s_l, t_h, t_l;
    n = 0;
    if (ix < 0x00100000) { ax *= two53; n -= 53; ix = hi(ax); }
    n += (ix >> 20) - 0x3ff;
    j = ix & 0x000fffff;
    ix = j | 0x3ff00000;
    if (j <= 0x3988E) k = 0;
    else if (j < 0xBB67A) k = 1;
    else { k = 0; n += 1; ix -= 0x00100000; }
    ax = setHi(ax, ix);

    u = ax - bp[k];
    v = one / (ax + bp[k]);
    ss = u * v;
    s_h = setLo(ss, 0);
    t_h = words(((ix >> 1) | 0x20000000) + 0x00080000 + (k << 18), 0);
    t_l = ax - (t_h - bp[k]);
    s_l = v * ((u - s_h * t_h) - s_h * t_l);
    s2 = ss * ss;
    r = s2 * s2 * (L1 + s2 * (L2 + s2 * (L3 + s2 * (L4 + s2 * (L5 + s2 * L6)))));
    r += s_l * (s_h + ss);
    s2 = s_h * s_h;
    t_h = setLo(3.0 + s2 + r, 0);
    t_l = r - ((t_h - 3.0) - s2);
    u = s_h * t_h;
    v = s_l * t_h + t_l * ss;
    p_h = setLo(u + v, 0);
    p_l = v - (p_h - u);
    z_h = cp_h * p_h;
    z_l = cp_l * p_h + p_l * cp + dp_l[k];
    t = n;
    t1 = setLo(((z_h + z_l) + dp_h[k]) + t, 0);
    t2 = z_l - (((t1 - t) - dp_h[k]) - z_h);
  }

  y1 = setLo(y, 0);
  p_l = (y - y1) * t1 + y * t2;
  p_h = y1 * t1;
  z = p_l + p_h;
  j = hi(z);
  i = lo(z) | 0;
  if (j >= 0x40900000) {
    if (((j - 0x40900000) | i) !== 0) return s * huge * huge;
    if (p_l + ovt > z - p_h) return s * huge * huge;
  } else if ((j & 0x7fffffff) >= 0x4090cc00) {
    if ((((j - 0xc090cc00) | 0) | i) !== 0) return s * tiny * tiny;
    if (p_l <= z - p_h) return s * tiny * tiny;
  }

  i = j & 0x7fffffff;
  k = (i >> 20) - 0x3ff;
  n = 0;
  if (i > 0x3fe00000) {
    n = (j + (0x00100000 >> (k + 1))) | 0;
    k = ((n & 0x7fffffff) >> 20) - 0x3ff;
    t = words(n & ~(0x000fffff >> k), 0);
    n = ((n & 0x000fffff) | 0x00100000) >> (20 - k);
    if (j < 0) n = -n;
    p_h -= t;
  }
  t = setLo(p_l + p_h, 0);
  u = t * lg2_h;
  v = (p_l - (t - p_h)) * lg2 + t * lg2_l;
  z = u + v;
  w = v - (z - u);
  t = z * z;
  t1 = z - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
  r = (z * t1) / (t1 - two) - (w + z * w);
  z = one - (r - z);
  j = (hi(z) + (n << 20)) | 0;
  if ((j >> 20) <= 0) z = scalbn(z, n);
  else z = setHi(z, hi(z) + (n << 20));
  return s * z;
}

// sin and cos: fdlibm's reduction by pi/2 (Cody-Waite below 2^19 pi/2, Payne-Hanek above) and its kernels.
function ksin(x, y, iy) {
  const half = 0.5, S1 = -1.66666666666666324348e-01, S2 = 8.33333333332248946124e-03,
    S3 = -1.98412698298579493134e-04, S4 = 2.75573137070700676789e-06,
    S5 = -2.50507602534068634195e-08, S6 = 1.58969099521155010221e-10;
  const ix = hi(x) & 0x7fffffff;
  if (ix < 0x3e400000 && Math.trunc(x) === 0) return x;
  const z = x * x, v = z * x, r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
  if (iy === 0) return x + v * (S1 + z * r);
  return x - ((z * (half * y - v * r) - y) - v * S1);
}

function kcos(x, y) {
  const one = 1.0, C1 = 4.16666666666666019037e-02, C2 = -1.38888888888741095749e-03,
    C3 = 2.48015872894767294178e-05, C4 = -2.75573143513906633035e-07,
    C5 = 2.08757232129817482790e-09, C6 = -1.13596475577881948265e-11;
  const ix = hi(x) & 0x7fffffff;
  if (ix < 0x3e400000 && Math.trunc(x) === 0) return one;
  const z = x * x, r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
  if (ix < 0x3FD33333) return one - (0.5 * z - (z * r - x * y));
  const qx = ix > 0x3fe90000 ? 0.28125 : words(ix - 0x00200000, 0);
  const hz = 0.5 * z - qx, a = one - qx;
  return a - (hz - (z * r - x * y));
}

const IPIO2 = [
  0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, 0x439041, 0xFE5163, 0xABDEBB, 0xC561B7,
  0x246E3A, 0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129, 0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C,
  0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C, 0x845F8B, 0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11,
  0x8B5A0A, 0x6D1F6D, 0x367ECF, 0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, 0xF17B3D, 0x0739F7,
  0x8A5292, 0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3, 0x91615E,
  0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA, 0x73A8C9, 0x60E27B, 0xC08C6B,
];
const PIO2 = [1.57079625129699707031e+00, 7.54978941586159635335e-08, 5.39030252995776476554e-15,
  3.28200341580791294123e-22, 1.27065575308067607349e-29, 1.22933308981111328932e-36,
  2.73370053816464559624e-44, 2.16741683877804819444e-51];

// Payne-Hanek: x[] are 24-bit chunks of |x| scaled by 2^-e0 (fdlibm __kernel_rem_pio2, prec 2)
function kremPio2(x, y, e0, nx) {
  const two24 = 1.67772160000000000000e+07, twon24 = 5.96046447753906250000e-08;
  const iq = new Array(20).fill(0), f = new Array(20).fill(0), fq = new Array(20).fill(0), q = new Array(20).fill(0);
  const jk = 4, jp = jk, jx = nx - 1;
  let jv = Math.trunc((e0 - 3) / 24);
  if (jv < 0) jv = 0;
  let q0 = e0 - 24 * (jv + 1), i, j, k, z, fw, n, ih, carry;
  j = jv - jx;
  for (i = 0; i <= jx + jk; i++, j++) f[i] = j < 0 ? 0.0 : IPIO2[j];
  for (i = 0; i <= jk; i++) {
    for (j = 0, fw = 0.0; j <= jx; j++) fw += x[j] * f[jx + i - j];
    q[i] = fw;
  }
  let jz = jk;
  for (;;) {
    for (i = 0, j = jz, z = q[jz]; j > 0; i++, j--) {
      fw = Math.trunc(twon24 * z);
      iq[i] = Math.trunc(z - two24 * fw);
      z = q[j - 1] + fw;
    }
    z = scalbn(z, q0);
    z -= 8.0 * Math.floor(z * 0.125);
    n = Math.trunc(z);
    z -= n;
    ih = 0;
    if (q0 > 0) {
      i = iq[jz - 1] >> (24 - q0); n += i;
      iq[jz - 1] -= i << (24 - q0);
      ih = iq[jz - 1] >> (23 - q0);
    } else if (q0 === 0) {
      ih = iq[jz - 1] >> 23;
    } else if (z >= 0.5) {
      ih = 2;
    }
    if (ih > 0) {
      n += 1; carry = 0;
      for (i = 0; i < jz; i++) {
        j = iq[i];
        if (carry === 0) {
          if (j !== 0) { carry = 1; iq[i] = 0x1000000 - j; }
        } else {
          iq[i] = 0xffffff - j;
        }
      }
      if (q0 === 1) iq[jz - 1] &= 0x7fffff;
      else if (q0 === 2) iq[jz - 1] &= 0x3fffff;
      if (ih === 2) {
        z = 1.0 - z;
        if (carry !== 0) z -= scalbn(1.0, q0);
      }
    }
    if (z === 0.0) {
      j = 0;
      for (i = jz - 1; i >= jk; i--) j |= iq[i];
      if (j === 0) {
        for (k = 1; iq[jk - k] === 0; k++) { /* count the terms needed */ }
        for (i = jz + 1; i <= jz + k; i++) {
          f[jx + i] = IPIO2[jv + i];
          for (j = 0, fw = 0.0; j <= jx; j++) fw += x[j] * f[jx + i - j];
          q[i] = fw;
        }
        jz += k;
        continue;
      }
    }
    break;
  }
  if (z === 0.0) {
    jz -= 1; q0 -= 24;
    while (iq[jz] === 0) { jz--; q0 -= 24; }
  } else {
    z = scalbn(z, -q0);
    if (z >= two24) {
      fw = Math.trunc(twon24 * z);
      iq[jz] = Math.trunc(z - two24 * fw);
      jz += 1; q0 += 24;
      iq[jz] = fw;
    } else {
      iq[jz] = Math.trunc(z);
    }
  }
  fw = scalbn(1.0, q0);
  for (i = jz; i >= 0; i--) { q[i] = fw * iq[i]; fw *= twon24; }
  for (i = jz; i >= 0; i--) {
    for (fw = 0.0, k = 0; k <= jp && k <= jz - i; k++) fw += PIO2[k] * q[i + k];
    fq[jz - i] = fw;
  }
  fw = 0.0;
  for (i = jz; i >= 0; i--) fw += fq[i];
  y[0] = ih === 0 ? fw : -fw;
  fw = fq[0] - fw;
  for (i = 1; i <= jz; i++) fw += fq[i];
  y[1] = ih === 0 ? fw : -fw;
  return n & 7;
}

const NPIO2_HW = [
  0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB, 0x402C463A,
  0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB, 0x403AB41B, 0x403C463A,
  0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C, 0x4043A28C, 0x40446B9C, 0x404534AC,
  0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
];

function remPio2(x, y) {
  const half = 0.5, two24 = 1.67772160000000000000e+07, invpio2 = 6.36619772367581382433e-01,
    pio2_1 = 1.57079632673412561417e+00, pio2_1t = 6.07710050650619224932e-11,
    pio2_2 = 6.07710050630396597660e-11, pio2_2t = 2.02226624879595063154e-21,
    pio2_3 = 2.02226624871116645580e-21, pio2_3t = 8.47842766036889956997e-32;
  let z, w, t, r, fn, i, j, n;
  const hx = hi(x), ix = hx & 0x7fffffff;
  if (ix <= 0x3fe921fb) { y[0] = x; y[1] = 0; return 0; }
  if (ix < 0x4002d97c) {
    if (hx > 0) {
      z = x - pio2_1;
      if (ix !== 0x3ff921fb) { y[0] = z - pio2_1t; y[1] = (z - y[0]) - pio2_1t; }
      else { z -= pio2_2; y[0] = z - pio2_2t; y[1] = (z - y[0]) - pio2_2t; }
      return 1;
    }
    z = x + pio2_1;
    if (ix !== 0x3ff921fb) { y[0] = z + pio2_1t; y[1] = (z - y[0]) + pio2_1t; }
    else { z += pio2_2; y[0] = z + pio2_2t; y[1] = (z - y[0]) + pio2_2t; }
    return -1;
  }
  if (ix <= 0x413921fb) {
    t = Math.abs(x);
    n = Math.trunc(t * invpio2 + half);
    fn = n;
    r = t - fn * pio2_1;
    w = fn * pio2_1t;
    if (n < 32 && ix !== NPIO2_HW[n - 1]) {
      y[0] = r - w;
    } else {
      j = ix >> 20;
      y[0] = r - w;
      i = j - ((hi(y[0]) >> 20) & 0x7ff);
      if (i > 16) {
        t = r; w = fn * pio2_2; r = t - w; w = fn * pio2_2t - ((t - r) - w); y[0] = r - w;
        i = j - ((hi(y[0]) >> 20) & 0x7ff);
        if (i > 49) { t = r; w = fn * pio2_3; r = t - w; w = fn * pio2_3t - ((t - r) - w); y[0] = r - w; }
      }
    }
    y[1] = (r - y[0]) - w;
    if (hx < 0) { y[0] = -y[0]; y[1] = -y[1]; return -n; }
    return n;
  }
  if (ix >= 0x7ff00000) { y[0] = y[1] = x - x; return 0; }
  const e0 = (ix >> 20) - 1046;
  z = words(ix - (e0 << 20), lo(x)); // e0 can be -3
  const tx = [0, 0, 0];
  for (i = 0; i < 2; i++) { tx[i] = Math.trunc(z); z = (z - tx[i]) * two24; }
  tx[2] = z;
  let nx = 3;
  while (tx[nx - 1] === 0.0) nx--;
  n = kremPio2(tx, y, e0, nx);
  if (hx < 0) { y[0] = -y[0]; y[1] = -y[1]; return -n; }
  return n;
}

export function detSin(x) {
  const ix = hi(x) & 0x7fffffff, y = [0, 0];
  if (ix <= 0x3fe921fb) return ksin(x, 0.0, 0);
  if (ix >= 0x7ff00000) return x - x;
  switch (remPio2(x, y) & 3) {
    case 0: return ksin(y[0], y[1], 1);
    case 1: return kcos(y[0], y[1]);
    case 2: return -ksin(y[0], y[1], 1);
    default: return -kcos(y[0], y[1]);
  }
}

export function detCos(x) {
  const ix = hi(x) & 0x7fffffff, y = [0, 0];
  if (ix <= 0x3fe921fb) return kcos(x, 0.0);
  if (ix >= 0x7ff00000) return x - x;
  switch (remPio2(x, y) & 3) {
    case 0: return kcos(y[0], y[1]);
    case 1: return -ksin(y[0], y[1], 1);
    case 2: return -kcos(y[0], y[1]);
    default: return ksin(y[0], y[1], 1);
  }
}
