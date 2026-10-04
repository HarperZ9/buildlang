# buildlang brand assets

Current assets, rendered on 4 October 2026 from the shared art direction:

- `docs/art/hero-dark.svg`, `docs/art/hero-light.svg`: the README hero, 1280 x 480, text outlined from Hanken Grotesk and Conso.
- `docs/art/social.png`: the GitHub social preview, 1280 x 640.
- The mark: `docs/brand/mark-16.png`, `docs/brand/mark-32.png` and `docs/brand/mark-16.svg` (favicon sizes),
  `docs/brand/mark-64.png`, `docs/brand/mark-512.png` and `docs/brand/mark-tile.svg` (app and listing icons),
  `docs/brand/mark-light.svg` and `docs/brand/mark-dark.svg` (on a page, no tile).
- The lockups: `docs/brand/lockup-horizontal-light.svg`, `docs/brand/lockup-horizontal-dark.svg`,
  `docs/brand/lockup-stacked-light.svg` and `docs/brand/lockup-stacked-dark.svg`.
- `docs/art/receipts.json`: a `superstack.receipt/1` for every PNG, with the seed, the scene hash and the font hashes.

The seed is the repository name. The same seed gives the same SVG bytes.

The previous README header, `docs/art/buildlang-header.svg`, stays in the repository with its `docs/art/buildlang.art.json` record.

The record below describes the previous brand render and stays as it was written.

## Previous render

The README hero image in this folder was rendered as part of the operator's flagship presentation pass.

## Rendering Receipt

- Source contract: `telos.rendering.research` (the shared flagship hero renderer).
- Product role: the Effects Language compiler (`buildc`) and toolchain, under the operator's Build ecosystem umbrella.
- Tool-specific motif: typed-effect tokens flowing into verified codegen.
- Typography: rendered locally from the operator-owned Kilon and Conso font packages. The public repository carries only the exported artwork, not the purchased font files.
- Accessibility floor: high-contrast foreground text, non-color-only status labels, and static PNG fallback for GitHub and low-capability hosts.
- Provenance boundary: Reddit and community links are treated as non-evidentiary source leads; implementation claims resolve through lawful papers, standards, official repositories, and repeatable local checks.
