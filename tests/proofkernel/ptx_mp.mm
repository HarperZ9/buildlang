$( Message passing (MP) under a simplified PTX memory model.

   Thread 0:  st x = 1  ;  fence  ;  st y = 1
   Thread 1:  ld y (reads 1)  ;  fence  ;  ld x

   The outcome to forbid is the read of x returning the initial 0. With the
   fences, the causality axiom orders the write of x before the read of x,
   so the read must see that write or a coherence-later one. Without them
   no axiom in this database orders the two events.

   Simplified from the PTX memory consistency model: one scope, every access
   strong, fences as release and acquire patterns, and visibility stated as
   "reads that write or a coherence-later one". $)

$c |- ev po isfence fenced rf cause sees $.
$c Wx Fa Wy Ry Fb Rx $.
$v a b c d f $.
ea $f ev a $.
eb $f ev b $.
ec $f ev c $.
ed $f ev d $.
ef $f ev f $.

$( The six events of the litmus test. $)
tWx $a ev Wx $.
tFa $a ev Fa $.
tWy $a ev Wy $.
tRy $a ev Ry $.
tFb $a ev Fb $.
tRx $a ev Rx $.

$( A fence between two events in program order orders them. $)
${
  fen.1 $e |- po a f $.
  fen.2 $e |- isfence f $.
  fen.3 $e |- po f b $.
  ax-fence $a |- fenced a b $.
$}

$( Base causality order: a release pattern, an observation through
   reads-from, and an acquire pattern. $)
${
  cau.1 $e |- fenced a b $.
  cau.2 $e |- rf b c $.
  cau.3 $e |- fenced c d $.
  ax-causality $a |- cause a d $.
$}

$( The causality axiom: a read that a write precedes in causality order
   reads that write or a coherence-later one. $)
${
  vis.1 $e |- cause a d $.
  ax-visible $a |- sees d a $.
$}

$( The fenced program. The read of x sees the write of x. $)
${
  mpf.1 $e |- po Wx Fa $.
  mpf.2 $e |- isfence Fa $.
  mpf.3 $e |- po Fa Wy $.
  mpf.4 $e |- rf Wy Ry $.
  mpf.5 $e |- po Ry Fb $.
  mpf.6 $e |- isfence Fb $.
  mpf.7 $e |- po Fb Rx $.
  mp-fenced $p |- sees Rx Wx $=
    tWx tRx tWx tWy tRy tRx tWx tWy tFa mpf.1 mpf.2 mpf.3 ax-fence mpf.4
    tRy tRx tFb mpf.5 mpf.6 mpf.7 ax-fence ax-causality ax-visible $.
$}
