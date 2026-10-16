# Third-party material

| What | Where | License |
|---|---|---|
| Tabler Icons font (Noctalia's build, `noctalia-tabler.ttf`) | `native/assets/tabler-icons.ttf` | MIT: icons © Paweł Kuna (https://tabler.io/icons), font build © noctalia-dev ([LICENSE.noctalia](native/assets/LICENSE.noctalia)) |
| Emoji list with names and keywords (from Noctalia 5.2.1) | `native/assets/emoji.json` | MIT, © noctalia-dev ([LICENSE.noctalia](native/assets/LICENSE.noctalia)) |
| `hyprland-toplevel-export-v1` protocol | `native/mist-ui/protocols/` | BSD-3-Clause, © Hyprland contributors |
| Rust crates | `native/Cargo.lock`, `scripts/mistd/Cargo.lock` | their own licenses (MIT / Apache-2.0 / BSD / Zlib / MPL-2.0) |

Not included and not redistributable:

- **SF Pro** (Apple). Mist uses it when the user installed it; otherwise Inter or Noto Sans.
- **macOS system sounds.** `sounds/*.wav` in the author's working tree are Apple's and must not be published
  (see [docs/PUBLISHING.md](docs/PUBLISHING.md)); without them Mist plays the freedesktop sound theme.
