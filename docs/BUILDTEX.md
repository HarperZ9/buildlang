# BuildTeX design note

BuildTeX is BuildLang's own document and proof path. A document is a BuildLang program; the layout, line breaking, math, fonts and finishes are BuildLang's own code; a math claim in a document can carry a proof that BuildLang checks. No TeX engine, LaTeX package or Lean toolchain runs at build or read time. LaTeX and Lean sources come in once, through one-way importers, and the BuildLang source is the record afterwards.

This note records what the typeset research established, the design for each part, what this change builds, and what is still design only.

## What the typeset research established

The engine in `typeset/engine` came out of the typeset field work (PR #115). Its results, with the tests that hold them:

| Part | Result | Held by |
|---|---|---|
| Line breaking | Knuth-Plass after tex.web 813 to 890: integer badness and demerits, three passes, emergency stretch, font expansion and margin protrusion. A BuildLang port (`kp/knuth_plass.bld`) chose the same breaks as the Python engine on every paragraph of a real paper. | `tests/test_breaker.py`; `kp/crosscheck.py` |
| Hyphenation | Liang patterns from plain TeX `hyphen.tex` plus the TUGboat exception log; gives TeX's answers at `\lefthyphenmin=2 \righthyphenmin=3`. | `tests/test_engine.py` Hyphenation |
| Inline rules | TeX space factors, url.sty break points, ligatures and kerning from GSUB, GPOS and `kern`. | `engine/hlist.py`, `engine/shape.py` |
| Math | TeXbook Appendix G with OpenType MATH constants: scripts, fractions, radicals, display limits. Not done: `\left`/`\right` sizing, accents, matrices, MathVariants growth. | `tests/test_engine.py` MathBoxes |
| Pages | TeX's page builder with club, widow and keep penalties, ragged or justified bottoms. | `engine/pages.py` |
| Fonts | TrueType parsing and subsetting, CID fonts with ToUnicode so text extracts, CFF passthrough for harvested Latin Modern. | PDF round trip test |
| Output | PDF with XMP and an sRGB OutputIntent for PDF/A-2b (not validated by veraPDF), SVG and HTML with a selectable text layer, all from one layout. | `engine/pdfout.py`, `engine/svgout.py` |
| Finishes | Emboss and deboss as a height map: the aperture rings rise, the name sinks, a lit relief render on a fibre normal map, gold foil from measured n and k when the materials library is present, and separate die, foil and spot plates for a press. | `engine/emboss.py` |

The engine is stdlib Python today. Knuth-Plass is the first part that also exists in BuildLang. Moving the rest into BuildLang is the main open work (see "Native port").

## Input syntax

The source of record is a BuildLang program over `typeset/doc/doc.bld`:

```
mod doc;
use doc::*;

fn main() ~ Console + FileSystem {
    let mut d = doc_new("Avery Example");
    theme(&mut d, "press");
    blk(&mut d, "h1", "Avery Example");
    blk(&mut d, "h2", "Experience");
    blk(&mut d, "li", "Wrote 1,200 regression cases, each paired with a {b:planted fault}.");
    labelled(&mut d, "eq", "eq:energy", "E = mc^2");
    let ok = save(&d, "resume.doc.tsv");
}
```

The program saves `buildlang.doc/v0`, a line-oriented IR (`meta`, `theme`, `block`, `mark`, `rel` records). Inline spans are `{i:..}`, `{b:..}`, `{sc:..}`, `{tt:..}`, `{m:..}` for math, `{ref:label}` and `{cite:key}`; `{{` and `}}` are literal braces. The capability effect `~ Console + FileSystem` declares what the document program may touch, and `buildc check` holds the program to it.

A theme is JSON data with single inheritance (`themes/*.json`): page geometry, fonts by role, spacing, line-break parameters, block styles, art layers and the finish. The author writes structure; the theme decides the look.

## Migrating from .tex

`buildtex/texin.py` reads a LaTeX subset as text and writes the IR, or a BuildLang program that saves the same IR (`texin.to_program`). Covered: the title block, `\section`, `\subsection`, `\paragraph`, itemize and enumerate, `\newpage`, `\label`, `\emph`, `\textit`, `\textbf`, `\textsc`, `\texttt` and `\url` (verbatim, no dash or quote ligatures), `\href`, `\ref`, `\cite`, inline and display math, TeX quotes, dashes, ties and escapes. Preamble commands are skipped. Any other command is collected and reported in the build receipt, never dropped without a record; an unbalanced group is an error.

The import runs once. After it, the `.bld` program is edited, and the `.tex` file is history. The papers on the site need `\subsection` with labels, custom title pages and longtable before they import cleanly; that is the next subset to add.

## Output: PDF and HTML from one layout

`python -m buildtex SOURCE OUT --theme NAME` lays the document out once and writes `OUT.pdf`, `OUT.html` and per-page SVG from the same pages, then reads its own PDF back and writes `OUT.buildtex.json`, the build receipt:

- the source, IR and output SHA-256;
- the facts: page count from the PDF and from the layout, overfull lines, underfull lines, missing glyphs, every font and whether it is embedded, images, blind text runs, and whether every block's words come out of the PDF in source order;
- `failed`, the list of broken conditions, empty for a passing build;
- `external_programs`, every process the build started.

The guard (`buildtex/guard.py`) wraps every way Python starts a process for the length of a build. A TeX or Lean program (pdflatex, xelatex, lualatex, latexmk, tex, bibtex, biber, kpsewhich, lean, lake, elan and the rest of the list) raises `ForbiddenBinary`; anything else is recorded. A `.tsv` or `.tex` build starts nothing. A `.bld` build starts `buildc`, BuildLang's own compiler, to run the document program.

## Finishes as styles

The finish is theme data, so a document changes look without changing source.

| Theme | Look | Use |
|---|---|---|
| `ats` | Black on white, one sans family, no art, no relief, ragged right, no hyphenation | Applicant tracking systems and plain printing |
| `living` | The site look: paper ground, Hanken Grotesk, Accession Display name, generative aperture, halftone band, hairline rule | Screen reading |
| `press` | `living` plus the press finish inside the page | The designed resume and CV |
| `latex-article` | Article class, 11 pt, A4, Latin Modern metrics | Matching an existing LaTeX paper |

The press finish (`engine/press.py`) lights the emboss height map and places it on page one as an image: the aperture rings stand up from the paper with a foil core, and the name sits in a debossed well. `deboss_finish` picks `inked` (the default: the name also prints in ink, as letterpress does, so it keeps full contrast) or `blind` (relief only; the name stays in the text layer through PDF render mode 3, so search, copy and screen readers still get it). `match_paper` keeps flat paper in the relief at the page's paper colour so the relief has no band edge. For a real press run, `--plates` on `engine.render` still writes the separate die, foil and spot plates.

## Proofs without Lean

The aim: a math claim in a document carries a proof, and a build fails when the proof does not check.

**Proof form.** A proof is a BuildLang value in a small, explicit term language: a statement (a formula over declared symbols), and a proof that is a sequence of steps, each citing an axiom, a hypothesis or an earlier theorem and a substitution. This is the Metamath shape: no tactics and no elaboration inside the kernel, so the checker stays small enough to read in one sitting.

**Kernel.** The checker does one thing: for each step, apply the cited rule under the substitution, confirm the result matches the step's stated formula, and confirm the disjoint-variable conditions. A theorem is checked when every step checks and the last step is the statement. Nothing else is trusted: not the importer, not the document program, not the layout.

**What "checked" means in a document.** A block `labelled(&mut d, "thm", "thm:x", ..)` may name a proof object. The build runs the kernel on it and records in the receipt the theorem, the axioms it depends on, the kernel's own SHA-256 and the verdict. The PDF and HTML mark the theorem as checked only when the verdict is a pass, and the receipt names which axioms the claim rests on, so a reader sees the trusted base.

**Importing Lean, one way.** For a useful subset: `def` and `theorem` statements translate to declared symbols and formulas; term proofs and simple tactic proofs (`intro`, `apply`, `exact`, `rw` with named lemmas, `simp only [..]` with a listed set) are replayed into explicit steps. A Lean proof that needs elaboration the importer cannot replay is reported, not translated, and the claim stays unchecked in the document. Lean never runs; the importer reads `.lean` text.

**What exists in BuildLang today.** The compiler checks capability effects and seals accountability receipts (`buildlang-receipt-verification/v1`), and scientific-runtime receipts check a stated invariant by re-running a kernel (`docs/SCIENTIFIC-RECEIPT.md`). Those are checks of programs and measured series, not of proofs. A Metamath-format proof kernel written in BuildLang exists as separate research work; it is not in this repository and is not wired into BuildTeX. The proof form, the document hook and the Lean importer above are design only.

## Native port

Typesetting is planned as BuildLang code end to end. The order follows dependence and risk:

1. Line breaking: done in BuildLang (`kp/knuth_plass.bld`), cross-checked against the Python engine.
2. Hyphenation and the inline list builder: pattern tries and space factors are integer code; port next and cross-check on the same paragraphs.
3. Page builder: integer penalties and dimensions; same cross-check on page breaks.
4. Font parsing, shaping and subsetting: byte-level table code; needs BuildLang slices and pointer arithmetic gaps closed (recorded in `typeset/findings/`).
5. PDF and SVG writers, then the relief render (floating point; checked against the Python output by image difference).

Each step lands with a differential test against the Python engine on real documents, so the port cannot drift silently. The Python engine stays as the reference until every step agrees.

## What this change builds

- `typeset/buildtex/`: the build command, the LaTeX-subset importer, the TeX and Lean guard, and the layout checks.
- `typeset/engine/press.py`: the press finish inside the PDF and HTML; image XObjects and invisible-text runs in the PDF writer; images in SVG and HTML.
- `typeset/themes/press.json` and `ats.json`.
- `typeset/tests/test_buildtex.py`: a fictional resume and CV from `.tex` through both themes, asserting one page for the resume, a multi-page CV whose PDF and layout agree, no overfull line, no missing glyph, every font embedded, reading order, the relief in the PDF, the blind finish's text layer, HTML from the same layout, byte-identical rebuilds, and that no TeX or Lean program starts.

## Limits

- The engine is Python; only line breaking also runs as BuildLang.
- Math needs an OpenType MATH font. The `press` and `ats` themes carry none, so they set math source as plain text and record a note in the receipt; a math-heavy document uses a theme with a MATH font, such as `latex-article` where Cambria Math is installed.
- PDF/A-2b metadata is written but no validator has run; tagged PDF structure (for screen readers beyond the text layer) is not written.
- The relief is a raster at the theme's dpi (150 for `press`); text and rules stay vector.
- The proof checker, the proof hook in documents and the Lean importer are design only.
