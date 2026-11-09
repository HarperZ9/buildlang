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
| Math | TeXbook Appendix G with OpenType MATH constants: scripts, fractions, radicals, display limits, accents. Not done: `\left`/`\right` sizing, matrices, MathVariants growth. | `tests/test_engine.py` MathBoxes; `tests/test_math_text.py`, `tests/test_math_vocab.py` |
| Pages | TeX's page builder with club, widow and keep penalties, ragged or justified bottoms; figures and tables float. | `engine/pages.py`, `engine/floats.py`; `tests/test_floats.py` |
| Fonts | TrueType parsing and subsetting, CID fonts with ToUnicode so text extracts, CFF passthrough for harvested Latin Modern. | PDF round trip test |
| Output | PDF with an sRGB OutputIntent, SVG and HTML with a selectable text layer, all from one layout. The PDF has no author, creator or XMP packet and is dated only by `SOURCE_DATE_EPOCH`; `--pdfa` adds the PDF/A-2b packet (not validated by veraPDF). | `engine/pdfout.py`, `engine/svgout.py` |
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

## Papers: figures, tables, equations and references

The `paper` theme sets research papers and whitepapers in the same press family as the designed CV and resumes:
the laid paper, the aperture emblem with its foil core, the dot band, and the title and byline debossed and inked.
The decoration stays on page one above the abstract (`clear_body`) and is drawn as PDF artifact content, so the
text layer is the paper alone. `paper-plain` is the same theme with every embellishment off.

| Block | IR | What it does |
|---|---|---|
| `title`, `subtitle`, `author`, `meta` | `block	title	...` | The title block; `title` and `author` sit in the deboss |
| `abstracthead`, `abstract` | | The abstract, inset from both margins |
| `section`, `subsection` | label optional | Sections are numbered; `{ref:label}` gives the number |
| `eq` | label optional | Display math, numbered `(n)` at the right margin |
| `figure` | fifth field `src=chart.png;width=0.8;alt=...` | A PNG beside the IR, scaled to the width fraction, captioned "Figure n." and kept with its caption |
| `tablecap`, `thead`, `row` | cells split by U+00A6; `cols=0.5,0.25,0.25` and `align=l,r,r` on the first row; `last=1` on the last | A table: caption "Table n.", a ruled header, wrapped cells, a closing rule. `align` sets each column left (`l`), right (`r`) or centred (`c`) and holds for the rest of the table, as `cols` does; numbers go in `r` columns so their last digits line up |
| `bib` | label is the cite key | Numbered references; `{cite:key}` gives `[n]` |

Figures and tables float, as in LaTeX. A float is a figure with its caption, or a table from its caption through
the row marked `last=1`, and it is never broken. When it fits in the space left on the page it is set where it
stands. When it does not, the text after it fills the page and the float goes at the top of the next page, so a
page no longer ends half empty in front of a tall figure. Floats keep their order: one that arrives while others
wait goes behind them. A float is never set before its first `{ref:..}`: one that comes earlier in the source moves
to just after the paragraph that cites it. The theme sets `"floats": false` to turn this off and
`page.float_sep` (12 pt) for the space between a top float and the text. A table caption is `unbreakable`, so a
two-line caption never splits from its table.

In the text layer a float reads where it is set. Floats at the top of a page read after the lines that finish a
paragraph split from the page before, so that paragraph reads whole. The layout lists every float that reads
later than its source position (`Layout.float_moves`), and the reading-order check reads the document in that
order (`buildtex.check.reading_order`): only floats move, so a lost or misplaced paragraph still fails.

In a BuildLang program, `attributed(&mut d, kind, label, text, attrs)` writes the fifth field; attributes are
separated by `;`, as in `cols=0.5,0.25,0.25;align=l,r,r`. The PNG reader
(`engine/pngread.py`) takes 8-bit grey, RGB, grey with alpha and RGBA, and lays alpha over the theme's paper.

Text-layer rules a paper build keeps:

- A label (section number, "Figure 1.", "[1]") is followed by a real space in the text layer, so it extracts as
  "1 Method", never "1Method".
- Math glyphs map to their plain letters in the ToUnicode table (NFKC), so an equation copies as letters and
  digits; `check` drops math commands such as `rac` before it compares words.
- Reading order is checked against the laid-out document, after references and citations are numbered.
- `buildtex.check.same_text(a, b)` compares the words of two PDFs page by page. A paper's embossed build and its
  `paper-plain` build must agree; the papers on the site report that check.
- A math box writes its glyph runs in source order: the base, then its scripts as the source wrote them
  (`e_1^2` extracts as `e12`, `e^2_1` as `e21`), and an operator, then its limits as written
  (`\sum_{j=1}^{m}` extracts as `∑ j=1 m`). The reading-order check counts
  letters and digits only; `×` and `÷` are symbols, so `1.6 \times 10^{-15}` compares as `1 6 10 15`.
  `tests/test_math_text.py` holds these cases.
- The PDF names an author (`/Author`, and `dc:creator` with `--pdfa`) and a subject only when the document sets
  `meta pdf_author` and `meta subject`. Without them the PDF carries the title and producer alone, as before.

Math in `paper` is set in Noto Sans Math (SIL OFL 1.1, `data/fonts/NotoSansMath-Regular.ttf`), so the theme
builds on any machine.

The math vocabulary (`engine/mathparse.py`):

| Kind | Commands |
|---|---|
| Greek | every lower-case letter from `\alpha` to `\omega` in math italic, with `\varepsilon`, `\vartheta`, `\varphi`, `\varrho`, `\varpi`, `\varkappa` and `\ell`; the capitals `\Gamma`, `\Delta`, `\Theta`, `\Lambda`, `\Xi`, `\Pi`, `\Sigma`, `\Upsilon`, `\Phi`, `\Psi`, `\Omega`, upright as in TeX |
| Text | `\text{..}`, `\textrm`, `\mbox` (upright, spaces kept); `\mathrm`, `\operatorname` (upright, spaces dropped); `\mathbf`, `\boldsymbol` (mathematical bold) |
| Accents | `\bar` and `\overline` (a rule over the base), `\hat`, `\widehat`, `\tilde`, `\widetilde`, `\dot`, `\ddot`, `\vec` (a combining mark centred over the base; the text layer reads base then mark) |
| Operators | `\log`, `\ln`, `\exp`, `\sin`, `\cos`, `\tan`, `\sinh`, `\cosh`, `\tanh`, the arc and other plain.tex names; upright with operator spacing. `\max`, `\min`, `\sup`, `\inf`, `\lim`, `\det`, `\gcd`, `\Pr` take limits in display |
| Structure | `\frac`, `\sqrt`, `^`, `_`, `\left` and `\right` (delimiters at natural size), spaces `\,` `\:` `\;` `\!` `\quad` `\qquad` |
| Symbols | relations, binary operators, arrows, set symbols, `\infty`, `\partial`, `\nabla`, `\hbar`, dots, angle brackets, `\{` `\}` |

A command outside this list is an error. The parser sets nothing for it, the build records it under
`facts.math_errors` in the receipt and lists it in `failed`, so raw source never reaches the page. A missing
argument or an unbalanced group is an error the same way. `tests/test_math_vocab.py` holds the vocabulary.

## Migrating from .tex

`buildtex/texin.py` reads a LaTeX subset as text and writes the IR, or a BuildLang program that saves the same IR (`texin.to_program`). Covered: the title block, `\section`, `\subsection`, `\paragraph`, itemize and enumerate, `\newpage`, `\label`, `\emph`, `\textit`, `\textbf`, `\textsc`, `\texttt` and `\url` (verbatim, no dash or quote ligatures), `\href`, `\ref`, `\cite`, inline and display math, TeX quotes, dashes, ties and escapes. Preamble commands are skipped. Any other command is collected and reported in the build receipt, never dropped without a record; an unbalanced group is an error.

The import runs once. After it, the `.bld` program is edited, and the `.tex` file is history. The papers on the site need `\subsection` with labels, custom title pages and longtable before they import cleanly; that is the next subset to add.

## Output: PDF and HTML from one layout

`python -m buildtex SOURCE OUT --theme NAME` lays the document out once and writes `OUT.pdf`, `OUT.html` and per-page SVG from the same pages, then reads its own PDF back and writes `OUT.buildtex.json`, the build receipt:

- the source, IR and output SHA-256;
- the facts: page count from the PDF and from the layout, overfull lines (each with `where`: the block and line, or for a line in a table cell the block, table row and column, as in `block 12, table row 3, column 2`; the header row is `header`), underfull lines, missing glyphs, every font and whether it is embedded, images, blind text runs, and whether every block's words come out of the PDF in source order;
- `failed`, the list of broken conditions, empty for a passing build;
- `external_programs`, every process the build started.

The guard (`buildtex/guard.py`) wraps every way Python starts a process for the length of a build. A TeX or Lean program (pdflatex, xelatex, lualatex, latexmk, tex, bibtex, biber, kpsewhich, lean, lake, elan and the rest of the list) raises `ForbiddenBinary`; anything else is recorded. A `.tsv` or `.tex` build starts nothing. A `.bld` build starts `buildc`, BuildLang's own compiler, to run the document program.

## Finishes as styles

The finish is theme data, so a document changes look without changing source.

| Theme | Look | Use |
|---|---|---|
| `ats` | Black on white, one sans family, no art, no relief, ragged right, no hyphenation | Applicant tracking systems and plain printing |
| `living` | The site look: paper ground, Hanken Grotesk, Accession Display name, generative aperture, halftone band, hairline rule | Screen reading |
| `press` | `living` plus the press finish inside the page | Letters, covers and art pieces |
| `press-resume` | `press` held to resume standards: 10 pt body, round bullets, headings larger than body, the relief cut off above the first section heading (`clear_body`), the name inked | The designed resumes and CV, shipped beside a plain edition they must parse identically to |
| `latex-article` | Article class, 11 pt, A4, Latin Modern metrics | Matching an existing LaTeX paper |

The press finish (`engine/press.py`) lights the emboss height map and places it on page one as an image: the aperture rings stand up from the paper with a foil core, and the name sits in a debossed well. `deboss_finish` picks `inked` (the default: the name also prints in ink, as letterpress does, so it keeps full contrast) or `blind` (relief only; the name stays in the text layer through PDF render mode 3, so search, copy and screen readers still get it). `match_paper` keeps flat paper in the relief at the page's paper colour so the relief has no band edge. For a real press run, `--plates` on `engine.render` still writes the separate die, foil and spot plates.

## Proofs without Lean

The aim: a math claim in a document carries a proof, and a build fails when the proof does not check.

**Proof form.** A proof is a BuildLang value in a small, explicit term language: a statement (a formula over declared symbols), and a proof that is a sequence of steps, each citing an axiom, a hypothesis or an earlier theorem and a substitution. This is the Metamath shape: no tactics and no elaboration inside the kernel, so the checker stays small enough to read in one sitting.

**Kernel.** The checker does one thing: for each step, apply the cited rule under the substitution, confirm the result matches the step's stated formula, and confirm the disjoint-variable conditions. A theorem is checked when every step checks and the last step is the statement. Nothing else is trusted: not the importer, not the document program, not the layout.

**What "checked" means in a document.** A block `labelled(&mut d, "thm", "thm:x", ..)` may name a proof object. The build runs the kernel on it and records in the receipt the theorem, the axioms it depends on, the kernel's own SHA-256 and the verdict. The PDF and HTML mark the theorem as checked only when the verdict is a pass, and the receipt names which axioms the claim rests on, so a reader sees the trusted base.

**Importing Lean, one way.** For a useful subset: `def` and `theorem` statements translate to declared symbols and formulas; term proofs and simple tactic proofs (`intro`, `apply`, `exact`, `rw` with named lemmas, `simp only [..]` with a listed set) are replayed into explicit steps. A Lean proof that needs elaboration the importer cannot replay is reported, not translated, and the claim stays unchecked in the document. Lean never runs; the importer reads `.lean` text.

**What exists in BuildLang today.** The compiler checks capability effects and seals accountability receipts (`buildlang-receipt-verification/v1`), and scientific-runtime receipts check a stated invariant by re-running a kernel (`docs/SCIENTIFIC-RECEIPT.md`). Those are checks of programs and measured series, not of proofs. The checker candidate is `tools/proofkernel` (mmk), a Metamath verifier written in BuildLang, proposed in HarperZ9/buildlang#117. BuildTeX does not call it; wiring it in waits until #117 merges. The proof form, the document hook and the Lean importer above are design only.

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

## Publish metadata

A BuildTeX PDF carries the title and the producer and nothing that names a person or a machine. There is no `/Author`, no `/Creator` and no XMP packet, so Flywheel's `scripts/check_publish_metadata.py` reports nothing on it. The date is `SOURCE_DATE_EPOCH` when that is set and absent otherwise; the writer never reads the clock, so a rebuild gives the same bytes. `layout.json` lists output files by name, never by directory. The receipt records the choice as `pdf_metadata`: `pdfa` and `creation_date`.

```
SOURCE_DATE_EPOCH=$(git log -1 --format=%ct) python -m buildtex resume.tex out/resume
```

`--pdfa` adds the XMP packet that PDF/A-2b needs, with the title, the producer and the date. The check reports any XMP packet, so a file meant to pass it is built without `--pdfa`. `tests/test_publish_metadata.py` holds these rules.

## Limits

- The engine is Python; only line breaking also runs as BuildLang.
- Figures are PNG only; tables do not break inside a row; a right-aligned column lines up last digits, not decimal points.
- Floats go only where they stand or at the top of a later page: there are no bottom floats and no float-only
  pages, and floats at the top of a page may fill it. A table taller than the page does not float.
- Math needs an OpenType MATH font. The `paper` theme ships one (Noto Sans Math); the `press` and `ats` themes carry none, so they set math source as plain text and record a note in the receipt; a math-heavy document uses a theme with a MATH font, such as `latex-article` where Cambria Math is installed.
- PDF/A-2b metadata is written only with `--pdfa`, and no validator has run. A publish-metadata check reports that packet even though it holds no author. Decoration is marked as `/Artifact` content so extraction and assistive technology skip it, but no structure tree (tagged PDF) is written.
- The relief is a raster at the theme's dpi (150 for `press`); text and rules stay vector.
- The proof hook in documents and the Lean importer are design only. The checker candidate is mmk in #117, not yet merged or wired in.
