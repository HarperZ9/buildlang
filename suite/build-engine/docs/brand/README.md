# build-engine brand assets

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

The record below describes the previous brand render and stays as it was written.

## Previous render

- `build-engine-mark.svg` — square logomark (candlestick history + dashed forecast
  line converging on a prediction point). Use for avatars, favicons, and small
  placements.
- `build-engine-hero.svg` — wide banner used at the top of the project README.

Palette: the marks use the Project Telos indigo (`#4636e8`) for the forecast/signal
line, with profit/loss accents (`#38d39f` / `#ff4d6d`) on a near-black indigo ground
(`#160c28`/`#0a0816`), matching the rest of the build-* family. Assets are SVG so
they scale cleanly; export to PNG only if a raster is required.
