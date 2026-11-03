# TextFold reader

`reader.js`, `reader.css` and `i18n.js` are unmodified copies from
[chenxiachan/textfold](https://github.com/chenxiachan/textfold), commit
`84427249f932e05e781539c0f3960658ffcbe966` (Apache-2.0).
The attribution and license are shipped in `src-tauri/licenses/TEXTFOLD.txt`.

The upstream reader owns five-level rendering, token identity, FLIP motion,
continuous wheel/pinch/slider zoom, viewport staging, local expansion, and Topic
cards. SAG converts its editor DOM and cached generation response to the native
reader payload in `src/lib/textfold-reader.ts`.

`reader.html` supplies the DOM contract from the upstream template. `sag.js`
supplies data through a same-origin parent/frame boundary and loads the original
reader. `sag.css` hides standalone-app controls already provided by SAG, inherits
SAG's palette/fonts, and preserves Markdown ordered list numbering/code blocks.
It also puts reading instructions behind the `?` button and centers the
cards/map selector above the reading-level selector, keeping the latter's vertical position fixed.
The isolated animation host receives the same palette and fonts as the visible
document; its upstream `:host` defaults otherwise override inherited SAG colors.
The iframe keeps the original window-scroll animation model and CSS isolated
from the editor. No inline executable scripts or relaxed CSP are required.
