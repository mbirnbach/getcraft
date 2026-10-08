# Attribution

Every non-code asset in this repository, with its author, source and license. GetCraft's own
code is MIT OR Apache-2.0 (see [`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)
and [`NOTICE`](NOTICE)). When you add an asset, add a row here in the same change.

## GetCraft's own assets

| Path | Title | Author | Source | License |
|---|---|---|---|---|
| `assets/getcraft-source.png` | GetCraft app icon artwork (engraved octopus on turquoise) | The GetCraft project owner | Original work | All rights reserved; may be used only as part of GetCraft, see [`assets/LICENSE-icon.txt`](assets/LICENSE-icon.txt) |
| `assets/getcraft-1024.png`, `getcraft-256.png`, `getcraft-64.png`, `getcraft.icns`, `getcraft.ico` | GetCraft app icon in the sizes and formats the packages need | The GetCraft project owner | Generated from `assets/getcraft-source.png` by `apps/getcraft/examples/make_icon.rs` | As above, [`assets/LICENSE-icon.txt`](assets/LICENSE-icon.txt) |
| `assets/dmg/background.png`, `assets/dmg/background@2x.png` | macOS DMG background (octopus in waves, "Drag into Applications to install") | The GetCraft project owner | Original work | As above, [`assets/LICENSE-icon.txt`](assets/LICENSE-icon.txt) |
| `docs/images/getcraft-*.png` | GetCraft screenshots | GetCraft contributors (UI) | Captured from GetCraft on macOS | MIT OR Apache-2.0 for the GetCraft UI; the Crafting App icons shown are listed below |
| `catalog.toml` (`kind` and `description` fields) | Short app descriptions | GetCraft contributors; the descriptions of PhotoCraft, VectorCraft, FilmCraft, LightCraft and EffectCraft quote or closely follow the taglines on [getartcraft.com/apps](https://getartcraft.com/apps) | Original text and short quotations used to describe each app | MIT OR Apache-2.0 for the original text |

## Crafting App icons (`assets/icons/`)

Unmodified copies of the 256 × 256 Linux icon (`assets/app-icon/hicolor/256x256/apps/ai.storyteller.<app>.png`)
from each app's repository, bundled so the catalog has icons offline. They are shown in the app
and in this README to identify each app. Each is licensed MIT OR Apache-2.0 by its project, and
GetCraft uses it under the **MIT License**, whose terms are in [`LICENSE-MIT`](LICENSE-MIT). The
copyright notices are:

| Path | App icon | Copyright | Source |
|---|---|---|---|
| `assets/icons/photocraft.png` | PhotoCraft | Copyright (c) 2026 ArtCraft Team and the PhotoCraft contributors | <https://github.com/storytold/photocraft/tree/main/assets/app-icon> |
| `assets/icons/vectorcraft.png` | VectorCraft | Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors | <https://github.com/storytold/vectorcraft/tree/main/assets/app-icon> |
| `assets/icons/filmcraft.png` | FilmCraft | Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors | <https://github.com/storytold/filmcraft/tree/main/assets/app-icon> |
| `assets/icons/lightcraft.png` | LightCraft | Copyright (c) 2026 ArtCraft Team and the LightCraft contributors | <https://github.com/storytold/lightcraft/tree/main/assets/app-icon> |
| `assets/icons/pdfcraft.png` | PdfCraft | Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors | <https://github.com/storytold/pdfcraft/tree/main/assets/app-icon> |
| `assets/icons/effectcraft.png` | EffectCraft | Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors | <https://github.com/storytold/effectcraft/tree/main/assets/app-icon> |
| `assets/icons/designcraft.png` | DesignCraft | Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors | <https://github.com/storytold/designcraft/tree/main/assets/app-icon> |
| `assets/icons/wordcraft.png` | WordCraft | Copyright (c) 2026 ArtCraft Team and the WordCraft contributors | <https://github.com/storytold/wordcraft/tree/main/assets/app-icon> |
| `assets/icons/gridcraft.png` | GridCraft | Copyright (c) 2026 ArtCraft Team and the GridCraft contributors | <https://github.com/storytold/gridcraft/tree/main/assets/app-icon> |
| `assets/icons/deckcraft.png` | DeckCraft | Copyright (c) 2026 ArtCraft Team and the DeckCraft contributors | <https://github.com/storytold/deckcraft/tree/main/assets/app-icon> |
| `assets/icons/soundcraft.png` | SoundCraft | Copyright (c) 2026 ArtCraft Team and the SoundCraft contributors | <https://github.com/storytold/soundcraft/tree/main/assets/app-icon> |
| `assets/icons/cadcraft.png` | CADCraft | Copyright (c) 2026 ArtCraft Team and the CADCraft contributors | <https://github.com/storytold/cadcraft/tree/main/assets/app-icon> |

Apps that GetCraft discovers later get their icon downloaded from the same path in their
repository at runtime; those icons are not stored in this repository.

## Not included

GetCraft does not include or use the ArtCraft name as a brand, or the ArtCraft wordmark or logo.
Those are trademarks of the ArtCraft Team and are not open source (see the brand license in, for
example, [PhotoCraft's `docs/brand/`](https://github.com/storytold/photocraft/tree/main/docs/brand)).
The name appears in GetCraft and its documentation only in plain text, to identify the apps
GetCraft installs and to say that GetCraft is not affiliated with their makers.
