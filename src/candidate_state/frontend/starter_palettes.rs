//! Bundled documents extracted from the pinned beta.26 palette expansion.
//! See LICENSES/THEME-ADAPTATION.md for the inputs and conversion.
use super::{ColorVariant, Deserialize, ThemeDocument};
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct StarterPalette {
    pub id: String,
    pub name: String,
    pub variant: ColorVariant,
    pub document: ThemeDocument,
}

static STARTERS: LazyLock<Vec<StarterPalette>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../assets/themes/starter-palettes.json"))
        .expect("bundled complete beta.26 starter palette documents")
});

pub fn starter_palettes() -> &'static [StarterPalette] {
    &STARTERS
}
