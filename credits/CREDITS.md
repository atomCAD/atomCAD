# 3rd Party Licenses

All original work in the atomCAD project is distributed under the terms of the Mozilla Public
License, v. 2.0.  See [LICENSE](../LICENSE) for details.

The following files are derived from 3rd party projects and retain their original license, in order
to facilitate the process of upstreaming bug fixes:

## DejaVu Sans Bold

- `rust/assets/DejaVuSans-Bold.ttf` — DejaVu Fonts 2.37, used to generate the atom-label glyph
  atlas (see `rust/examples/gen_font_atlas.rs`).
- `rust/assets/font_atlas.png` — a signed-distance-field atlas rendered from the glyph outlines of
  the above, and therefore covered by the same license.

License: Bitstream Vera Fonts Copyright (Bitstream, Inc.) with Arev fonts additions (Tavmjong Bah);
DejaVu changes are in the public domain. Full text in
[`rust/assets/DejaVuSans-Bold-LICENSE.txt`](../rust/assets/DejaVuSans-Bold-LICENSE.txt).

Upstream: https://github.com/dejavu-fonts/dejavu-fonts
