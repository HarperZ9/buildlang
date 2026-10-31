# typeset

BuildLang as a document language. An author writes structure in a BuildLang program (blocks, inline spans, math, references, measured annotations); a theme, kept as data, decides the look; our own layout engine sets it to PDF, SVG and HTML from one layout.

## Parts

| Path | What it does |
|---|---|
| `doc/doc.bld` | The document layer: `Doc`, `blk`, `labelled`, inline `em`/`strong`/`sc`/`tt`/`math`/`ref_to`/`cite`, `theme`, and the typed annotation layer (`Device`, `Layer`, `mark`, `mark_phrase`, `relate`). A program saves `buildlang.doc/v0` IR. |
| `kp/knuth_plass.bld` | Knuth-Plass line breaking written in BuildLang, integer badness and demerits as in tex.web. `kp/crosscheck.py` runs it against the Python engine on real paragraphs. |
| `engine/` | Stdlib-only Python engine. `sfnt`/`cmap`/`glyf`: OpenType parsing, GPOS and kern-table kerning, GSUB ligatures, outlines. `subset`: TrueType subsetting. `hyph`: Liang. `breaker`: Knuth-Plass after tex.web 813-890, with font expansion and margin protrusion. `hlist`: TeX space factors, url.sty breaks, microtypography. `mathfont`/`mathparse`/`mathlayout`: TeXbook Appendix G with OpenType MATH constants. `pages`: TeX's page builder with club, widow and keep penalties. `pdfout`/`pdfgfx`/`icc`: PDF writer (CID fonts, ToUnicode, reading-order text, XMP and sRGB OutputIntent for PDF/A-2b). `svgout`: SVG and HTML with a selectable text layer. `art`: generative aperture, halftone, rules. `emboss`/`raster`: height map, lit relief, die, foil and spot plates. `annotate`: the measured-annotation overlay. `pdfread`: round-trip reader for our own PDFs. |
| `themes/` | `parity` and `parity-cv` (the site's ReportLab settings), `latex-article` (article class, 11pt, A4), `living` (the site look), `lens` (living with annotation margins). |
| `data/hyph/` | `hyph_en_US.dic`, BSD-style, derived from plain TeX `hyphen.tex` and the TUGboat exception log (licence in the README beside it). |
| `findings/` | Repro of a BuildLang miscompile found while building this. |

## Run it

```
cd typeset
python -m unittest discover -s tests
python -m engine.render your-document.doc.tsv out/doc --theme living
python -m engine.render your-document.doc.tsv out/doc --annotations
```

`buildc run` on a program that uses `doc/doc.bld` writes the `.doc.tsv` the renderer reads.

Fonts: Hanken Grotesk and Accession Display (OFL 1.1) ship in `data/fonts`. `TYPESET_FONTS` adds folders to the search path, so a theme can name another font you hold. The `living` theme sets utility lines in Hanken.

The annotation layer runs only with `--annotations`; without it, output is byte-identical (a test checks this).

## Limits

- The `latex-article` theme needs Latin Modern metric files in `data/lm`, which are not in this repo, so it fails until you supply them. The importers and comparison scripts stayed out too.
- Latin Modern is not installed here, so the LaTeX comparison uses metrics and CFF subsets harvested from the shipped LaTeX PDFs; kerning pairs come from those PDFs and cover only pairs that occur in them.
- SVG and HTML draw TrueType outlines; CFF fonts (Latin Modern) fall back to browser text, so the HTML of the LaTeX port is a reading view, not a proof.
- Math covers scripts, fractions, radicals and display limits. Delimiters sized by `\left`/`\right`, accents, matrices and MathVariants glyph growth are not done.
- PDF/A-2b: the metadata, embedded fonts and OutputIntent are written; no validator (veraPDF) was run.
