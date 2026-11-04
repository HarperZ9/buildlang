$c ( ) -> wff |- $.
$v p q $.
wp $f wff p $.
wq $f wff q $.
wi $a wff ( p -> q ) $.
${
  min $e |- p $.
  maj $e |- ( p -> q ) $.
  mp $a |- q $.
$}
${
  h1 $e |- p $.
  h2 $e |- ( p -> q ) $.
  th1 $p |- q $= wp wq h1 h2 mp $.
$}
th2 $p wff ( p -> p ) $= wp wp wi $.
