//! Pure 50-role light/dark theme document model and import/validation adapter.
//!
//! Integrated from the locally validated pilot candidate (SHA-256
//! `C6D3968D2B59C1B19934841C62B5ABFC1E8E0664EE5E6BC3CC97A2100270D49D`,
//! 26 contract tests passed at local intake) into the candidate binary's
//! module graph for the session theme slice. That pilot evidence covers
//! the unchanged model/import behavior only; this integrated copy,
//! including the `rgb_color` re-export below, requires its own local
//! verification. See the integration delivery notes for this slice.
//!
//! # What this is
//!
//! A standalone, side-effect-free document boundary for a future theme
//! designer: a typed representation of exactly the 50 upstream Material
//! color roles (each with a `dark` and a `light` color), a strict RGB
//! string rule, and an adapter that imports the donor-shaped JSON document
//! (`colors.<role>.dark.color` / `colors.<role>.light.color`) from an
//! already-parsed [`serde_json::Value`].
//!
//! # What this is not
//!
//! No palette generation, no Matugen execution, no HSL/LAB/HCT math, no
//! contrast/WCAG computation, no Iced/WGPU consumer, no profile or
//! `candidate_state` schema migration, no persistence, no scheduling.
//! Upstream generation (`document`, `document_with`, `from_palette`,
//! `generate_colors` in the donor) requires the `material-colors` crate,
//! which is absent from Wallpaper Switcher's `Cargo.toml`/`Cargo.lock`;
//! that code is deliberately not copied and no substitute is invented here.
//!
//! # Provenance
//!
//! * [`ROLE_KEYS`] is copied verbatim, in order, from
//!   `references/skwd-deck-main/crates/skwd-palette/src/material/palette.rs`
//!   (`pub const ROLE_KEYS: [&str; 50]`, donor SHA-256
//!   `51BB3D0667E4CED4F03AB4C95E27EFC881014FDD1355DB7BAC87601AE06603E3`
//!   as recorded in `contracts/theme-role-keys.json`). It includes
//!   `source_color` as the 50th key. The donor's `UI_KEYS` camelCase names
//!   (`primaryText`, `surfaceText`, ...) are a separate 9-key UI projection
//!   and are deliberately NOT used as role names here.
//! * The donor document shape (`colors.<role>.{dark,light}.color`) follows
//!   the donor's private `variants()` helper and its public `role()`
//!   accessor in the same file.
//! * The RGB string rule is 's own decoder, now owned once by
//!   `crate::rgb_color` (extracted from `candidate_state.rs`) and
//!   re-exported publicly below so this module's original API surface is
//!   unchanged. The donor's
//!   `parse_seed` is intentionally NOT mirrored: it strips *every* leading
//!   `#` via `trim_start_matches('#')`, while  strips at most one.
//!   This module follows : repeated `#` is rejected. That is a
//!   deliberate, stricter validation boundary, not an accident.
//! * The donor's `colors()` accessor silently substitutes `#808080` for a
//!   missing or unparsable role. This adapter deliberately does the
//!   opposite: any missing or invalid required color fails the whole
//!   import with a diagnostic naming the role and variant. No default,
//!   black, or seed color is ever fabricated.
//!
//! # Import contract notes
//!
//! * The input is an already-parsed [`Value`]. How the raw JSON text was
//!   produced (which Matugen version, which CLI flags) is out of scope,
//!   and no claim of support for every Matugen output shape is made.
//! * Duplicate JSON object keys cannot be observed at this boundary: a
//!   parsed `serde_json::Value` has already collapsed them. Detecting or
//!   rejecting duplicates is a raw-text import-boundary concern for a
//!   later slice and is explicitly not solved here.
//! * Unrelated top-level metadata (`mode`, `is_dark_mode`, `base16`, or
//!   anything else) is ignored. Unknown keys inside `colors` are reported
//!   in [`ImportOutcome::ignored_roles`] (sorted); they are never treated
//!   as role data and never round-tripped. Extra fields inside a known
//!   role or variant node — including the donor's derived `default`
//!   entry — are ignored; `default` is not independent state and is never
//!   consulted or emitted by this module.
//! * Import is pure: it takes `&Value` and never mutates its input, on
//!   success or on error. Validation is fail-fast and deterministic: the
//!   first defect in [`ROLE_KEYS`] order (dark before light within a
//!   role) is the one reported.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// Number of theme roles. Fixed by the donor `ROLE_KEYS` list.
pub const ROLE_COUNT: usize = 50;

/// The donor's name for the seed/source role, equal to `ROLE_KEYS[49]`.
/// Mirrors `SOURCE_ROLE` in the donor `palette.rs`.
pub const SOURCE_ROLE: &str = "source_color";

/// Exact donor role keys, in donor order. See the module docs for
/// provenance. Index in this array is the role's stable index; see
/// [`ThemeRole::index`].
pub const ROLE_KEYS: [&str; ROLE_COUNT] = [
    "primary",
    "on_primary",
    "tertiary",
    "surface",
    "on_surface",
    "surface_variant",
    "surface_container",
    "background",
    "outline",
    "error",
    "error_container",
    "inverse_on_surface",
    "inverse_primary",
    "inverse_surface",
    "on_background",
    "on_error",
    "on_error_container",
    "on_primary_container",
    "on_primary_fixed",
    "on_primary_fixed_variant",
    "on_secondary",
    "on_secondary_container",
    "on_secondary_fixed",
    "on_secondary_fixed_variant",
    "on_surface_variant",
    "on_tertiary",
    "on_tertiary_container",
    "on_tertiary_fixed",
    "on_tertiary_fixed_variant",
    "outline_variant",
    "primary_container",
    "primary_fixed",
    "primary_fixed_dim",
    "scrim",
    "secondary",
    "secondary_container",
    "secondary_fixed",
    "secondary_fixed_dim",
    "shadow",
    "surface_bright",
    "surface_container_high",
    "surface_container_highest",
    "surface_container_low",
    "surface_container_lowest",
    "surface_dim",
    "surface_tint",
    "tertiary_container",
    "tertiary_fixed",
    "tertiary_fixed_dim",
    "source_color",
];

/// One of the exactly 50 theme roles.
///
/// Variants are declared in [`ROLE_KEYS`] order, so the declaration
/// position is the role's stable index. The serialized and string form of
/// a role is its exact donor key (for example `on_primary`); camelCase UI
/// names such as `primaryText` are not role names and are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ThemeRole {
    Primary,
    OnPrimary,
    Tertiary,
    Surface,
    OnSurface,
    SurfaceVariant,
    SurfaceContainer,
    Background,
    Outline,
    Error,
    ErrorContainer,
    InverseOnSurface,
    InversePrimary,
    InverseSurface,
    OnBackground,
    OnError,
    OnErrorContainer,
    OnPrimaryContainer,
    OnPrimaryFixed,
    OnPrimaryFixedVariant,
    OnSecondary,
    OnSecondaryContainer,
    OnSecondaryFixed,
    OnSecondaryFixedVariant,
    OnSurfaceVariant,
    OnTertiary,
    OnTertiaryContainer,
    OnTertiaryFixed,
    OnTertiaryFixedVariant,
    OutlineVariant,
    PrimaryContainer,
    PrimaryFixed,
    PrimaryFixedDim,
    Scrim,
    Secondary,
    SecondaryContainer,
    SecondaryFixed,
    SecondaryFixedDim,
    Shadow,
    SurfaceBright,
    SurfaceContainerHigh,
    SurfaceContainerHighest,
    SurfaceContainerLow,
    SurfaceContainerLowest,
    SurfaceDim,
    SurfaceTint,
    TertiaryContainer,
    TertiaryFixed,
    TertiaryFixedDim,
    SourceColor,
}

impl ThemeRole {
    /// All roles, in [`ROLE_KEYS`] order: `ALL[i].index() == i`.
    pub const ALL: [ThemeRole; ROLE_COUNT] = [
        ThemeRole::Primary,
        ThemeRole::OnPrimary,
        ThemeRole::Tertiary,
        ThemeRole::Surface,
        ThemeRole::OnSurface,
        ThemeRole::SurfaceVariant,
        ThemeRole::SurfaceContainer,
        ThemeRole::Background,
        ThemeRole::Outline,
        ThemeRole::Error,
        ThemeRole::ErrorContainer,
        ThemeRole::InverseOnSurface,
        ThemeRole::InversePrimary,
        ThemeRole::InverseSurface,
        ThemeRole::OnBackground,
        ThemeRole::OnError,
        ThemeRole::OnErrorContainer,
        ThemeRole::OnPrimaryContainer,
        ThemeRole::OnPrimaryFixed,
        ThemeRole::OnPrimaryFixedVariant,
        ThemeRole::OnSecondary,
        ThemeRole::OnSecondaryContainer,
        ThemeRole::OnSecondaryFixed,
        ThemeRole::OnSecondaryFixedVariant,
        ThemeRole::OnSurfaceVariant,
        ThemeRole::OnTertiary,
        ThemeRole::OnTertiaryContainer,
        ThemeRole::OnTertiaryFixed,
        ThemeRole::OnTertiaryFixedVariant,
        ThemeRole::OutlineVariant,
        ThemeRole::PrimaryContainer,
        ThemeRole::PrimaryFixed,
        ThemeRole::PrimaryFixedDim,
        ThemeRole::Scrim,
        ThemeRole::Secondary,
        ThemeRole::SecondaryContainer,
        ThemeRole::SecondaryFixed,
        ThemeRole::SecondaryFixedDim,
        ThemeRole::Shadow,
        ThemeRole::SurfaceBright,
        ThemeRole::SurfaceContainerHigh,
        ThemeRole::SurfaceContainerHighest,
        ThemeRole::SurfaceContainerLow,
        ThemeRole::SurfaceContainerLowest,
        ThemeRole::SurfaceDim,
        ThemeRole::SurfaceTint,
        ThemeRole::TertiaryContainer,
        ThemeRole::TertiaryFixed,
        ThemeRole::TertiaryFixedDim,
        ThemeRole::SourceColor,
    ];

    /// Stable index of this role: its position in [`ROLE_KEYS`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Exact donor key of this role, for example `"on_primary"`.
    pub const fn name(self) -> &'static str {
        ROLE_KEYS[self.index()]
    }

    /// Look up a role by its exact donor key.
    ///
    /// The match is exact: no trimming, no case folding, no camelCase
    /// aliases. `"primaryText"`, `"Primary"` and `" primary"` all return
    /// `None`.
    pub fn from_name(name: &str) -> Option<ThemeRole> {
        ThemeRole::ALL
            .iter()
            .copied()
            .find(|role| role.name() == name)
    }

    /// Look up a role by its stable index. Returns `None` for any index
    /// outside `0..ROLE_COUNT`.
    pub fn from_index(index: usize) -> Option<ThemeRole> {
        ThemeRole::ALL.get(index).copied()
    }
}

impl fmt::Display for ThemeRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Error returned when a string is not an exact donor role key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRoleName(pub String);

impl fmt::Display for UnknownRoleName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown theme role `{}`", self.0)
    }
}

impl std::error::Error for UnknownRoleName {}

impl FromStr for ThemeRole {
    type Err = UnknownRoleName;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        ThemeRole::from_name(value).ok_or_else(|| UnknownRoleName(value.to_owned()))
    }
}

impl Serialize for ThemeRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for ThemeRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        ThemeRole::from_name(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown theme role `{name}`")))
    }
}

/// Which of a role's two colors is meant: the dark or the light variant.
///
/// The string form is the donor's variant key, `"dark"` or `"light"`.
/// The donor's derived `"default"` entry is not a variant and is
/// rejected by [`ColorVariant::from_name`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorVariant {
    Dark,
    Light,
}

impl ColorVariant {
    /// Both variants, dark first. This is also the validation order used
    /// within a role during import.
    pub const ALL: [ColorVariant; 2] = [ColorVariant::Dark, ColorVariant::Light];

    /// Donor variant key: `"dark"` or `"light"`.
    pub const fn name(self) -> &'static str {
        match self {
            ColorVariant::Dark => "dark",
            ColorVariant::Light => "light",
        }
    }

    /// Look up a variant by its exact key. `"default"`, `"Dark"` and
    /// every other string return `None`.
    pub fn from_name(name: &str) -> Option<ColorVariant> {
        match name {
            "dark" => Some(ColorVariant::Dark),
            "light" => Some(ColorVariant::Light),
            _ => None,
        }
    }

    /// The other variant.
    pub const fn other(self) -> ColorVariant {
        match self {
            ColorVariant::Dark => ColorVariant::Light,
            ColorVariant::Light => ColorVariant::Dark,
        }
    }
}

impl fmt::Display for ColorVariant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Error returned when a string is not `"dark"` or `"light"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownVariantName(pub String);

impl fmt::Display for UnknownVariantName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown color variant `{}`", self.0)
    }
}

impl std::error::Error for UnknownVariantName {}

impl FromStr for ColorVariant {
    type Err = UnknownVariantName;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        ColorVariant::from_name(value).ok_or_else(|| UnknownVariantName(value.to_owned()))
    }
}

impl Serialize for ColorVariant {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for ColorVariant {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        ColorVariant::from_name(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown color variant `{name}`")))
    }
}

/// Parse an RGB color string into its three channel bytes.
///
/// This is a compatibility re-export of the single decoder owner,
/// [`crate::rgb_color::parse_hex_rgb_bytes`]: the input is trimmed of
/// outer whitespace, at most one leading `#` is stripped, and the
/// remainder must be exactly six ASCII hex digits. Eight-digit/alpha
/// forms, repeated `#`, wrong lengths, non-hex ASCII and non-ASCII
/// lookalikes (including six-byte multibyte strings) are all rejected.
pub use crate::rgb_color::parse_hex_rgb_bytes;

/// Validate an RGB color string and normalize it to lowercase `#rrggbb`.
///
/// Accepts exactly the strings [`parse_hex_rgb_bytes`] accepts — outer
/// whitespace and one optional leading `#` are permitted — and returns
/// the canonical form, for example `"  #F06E44 "` becomes `"#f06e44"`
/// and `"f06e44"` becomes `"#f06e44"`. Returns `None` for anything else,
/// including the donor `parse_seed` forms with repeated `#`.
pub fn normalize_hex_color(value: &str) -> Option<String> {
    let [red, green, blue] = parse_hex_rgb_bytes(value)?;
    Some(format!("#{red:02x}{green:02x}{blue:02x}"))
}

/// A single color string that failed validation, with the variant it was
/// supplied for. Returned by [`RoleColors::try_new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidColor {
    /// The variant the invalid string was supplied for.
    pub variant: ColorVariant,
    /// The offending string, exactly as supplied.
    pub raw: String,
}

impl fmt::Display for InvalidColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid {} color {:?}: expected exactly six ASCII hex digits \
             with an optional single leading `#`",
            self.variant, self.raw
        )
    }
}

impl std::error::Error for InvalidColor {}

/// The dark and light colors of one role.
///
/// Invariant: both stored strings are normalized `#rrggbb` (lowercase,
/// exactly one leading `#`). The only constructors are
/// [`RoleColors::try_new`] and the import adapter, both of which
/// validate, so the invariant holds for every value of this type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RoleColors {
    dark: String,
    light: String,
}

impl RoleColors {
    /// Build a role's colors from raw strings, validating and
    /// normalizing both. Dark is validated first. No default is
    /// substituted on failure; the offending variant and string are
    /// reported in [`InvalidColor`].
    pub fn try_new(dark: &str, light: &str) -> Result<RoleColors, InvalidColor> {
        let dark = normalize_hex_color(dark).ok_or_else(|| InvalidColor {
            variant: ColorVariant::Dark,
            raw: dark.to_owned(),
        })?;
        let light = normalize_hex_color(light).ok_or_else(|| InvalidColor {
            variant: ColorVariant::Light,
            raw: light.to_owned(),
        })?;
        Ok(RoleColors { dark, light })
    }

    /// The normalized dark color, as `#rrggbb`.
    pub fn dark(&self) -> &str {
        &self.dark
    }

    /// The normalized light color, as `#rrggbb`.
    pub fn light(&self) -> &str {
        &self.light
    }

    /// The normalized color for an explicit variant. This never consults
    /// any `default` entry; the choice is exactly the caller's.
    pub fn get(&self, variant: ColorVariant) -> &str {
        match variant {
            ColorVariant::Dark => &self.dark,
            ColorVariant::Light => &self.light,
        }
    }

    /// The channel bytes of the normalized color for a variant. Cannot
    /// fail: stored colors satisfy the normalization invariant.
    pub fn rgb_bytes(&self, variant: ColorVariant) -> [u8; 3] {
        parse_hex_rgb_bytes(self.get(variant))
            .expect("RoleColors invariant: stored colors are normalized #rrggbb")
    }
}

/// A complete theme document: both variants of every one of the 50
/// roles, stored in [`ROLE_KEYS`] order.
///
/// This is a standalone candidate document model. It is not 's
/// `AppearanceSettings`, it has no schema version, and serializing it
/// does not migrate or publish any profile.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThemeDocument {
    colors: [RoleColors; ROLE_COUNT],
}

impl ThemeDocument {
    /// Assemble a document from already-validated per-role colors, in
    /// [`ROLE_KEYS`] order.
    pub fn new(colors: [RoleColors; ROLE_COUNT]) -> ThemeDocument {
        ThemeDocument { colors }
    }

    /// The dark and light colors of one role.
    pub fn role_colors(&self, role: ThemeRole) -> &RoleColors {
        &self.colors[role.index()]
    }

    /// The normalized `#rrggbb` color of one role in one explicit
    /// variant.
    pub fn color(&self, role: ThemeRole, variant: ColorVariant) -> &str {
        self.colors[role.index()].get(variant)
    }

    /// A copy of this document with exactly one role/variant color
    /// replaced. `normalized` must already be a normalized `#rrggbb`
    /// string (see [`normalize_hex_color`]); it is re-validated through
    /// [`RoleColors::try_new`], so an unnormalized value panics rather
    /// than corrupting the document invariant. The other 99 values are
    /// preserved exactly, and this document is unchanged.
    pub fn replaced(
        &self,
        role: ThemeRole,
        variant: ColorVariant,
        normalized: &str,
    ) -> ThemeDocument {
        let current = self.role_colors(role);
        let pair = match variant {
            ColorVariant::Dark => RoleColors::try_new(normalized, current.light()),
            ColorVariant::Light => RoleColors::try_new(current.dark(), normalized),
        }
        .expect("replaced() requires a normalized #rrggbb value");
        let mut colors = self.colors.clone();
        colors[role.index()] = pair;
        ThemeDocument { colors }
    }

    /// All 50 normalized colors for one variant, in [`ROLE_KEYS`] order.
    pub fn colors_for(&self, variant: ColorVariant) -> [&str; ROLE_COUNT] {
        std::array::from_fn(|index| self.colors[index].get(variant))
    }

    /// Iterate `(role, colors)` pairs in [`ROLE_KEYS`] order.
    pub fn iter(&self) -> impl Iterator<Item = (ThemeRole, &RoleColors)> + '_ {
        ThemeRole::ALL.iter().copied().zip(self.colors.iter())
    }

    /// Serialize to the donor-shaped JSON document:
    /// `{"colors": {"<role>": {"dark": {"color": "#rrggbb"},
    /// "light": {"color": "#rrggbb"}}, ...}}` with stable human-readable
    /// role keys and both variants preserved.
    ///
    /// Deliberate omissions: no `default` entry is emitted (it is
    /// derived donor state, not independent data), and no `mode`,
    /// `is_dark_mode` or `base16` metadata is invented. Key order inside
    /// the returned [`Value`] is `serde_json`'s map order; role order is
    /// carried by [`ThemeRole`] / [`ROLE_KEYS`], not by JSON object
    /// order.
    pub fn to_value(&self) -> Value {
        let mut colors = Map::new();
        for role in ThemeRole::ALL {
            let role_colors = &self.colors[role.index()];
            let mut dark_node = Map::new();
            dark_node.insert("color".to_string(), Value::String(role_colors.dark.clone()));
            let mut light_node = Map::new();
            light_node.insert(
                "color".to_string(),
                Value::String(role_colors.light.clone()),
            );
            let mut role_node = Map::new();
            role_node.insert("dark".to_string(), Value::Object(dark_node));
            role_node.insert("light".to_string(), Value::Object(light_node));
            colors.insert(role.name().to_string(), Value::Object(role_node));
        }
        let mut root = Map::new();
        root.insert("colors".to_string(), Value::Object(colors));
        Value::Object(root)
    }
}

impl Serialize for ThemeDocument {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ThemeDocument {
    /// Deserialize the same donor-shaped document [`ThemeDocument::to_value`]
    /// produces, through the same validation as [`import_document`], with
    /// one deliberate difference: because this is the candidate's own
    /// document format, an unknown role inside `colors` is an error here
    /// rather than a reported ignored name, so a serialized document can
    /// never silently lose entries on a round trip.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let outcome = import_document(&value)
            .map_err(|error| serde::de::Error::custom(format!("{error}")))?;
        if let Some(first) = outcome.ignored_roles.first() {
            return Err(serde::de::Error::custom(format!(
                "unknown theme role `{first}` in serialized theme document"
            )));
        }
        Ok(outcome.document)
    }
}

/// Why importing a donor-shaped document failed.
///
/// Every variant identifies the role and, where applicable, the variant
/// responsible, so a caller can point at the exact defective entry. No
/// variant is ever produced *after* substituting a fallback color: an
/// `Err` means no document exists for that input at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeImportError {
    /// The root value is not a JSON object.
    RootNotObject,
    /// The root object has no `colors` key.
    MissingColors,
    /// `colors` is present but is not a JSON object.
    ColorsNotObject,
    /// A required role key is absent from `colors`.
    MissingRole {
        /// The absent role.
        role: ThemeRole,
    },
    /// A role's value in `colors` is not a JSON object.
    RoleNotObject {
        /// The malformed role.
        role: ThemeRole,
    },
    /// A role object has no entry for a required variant.
    MissingVariant {
        /// The role missing a variant.
        role: ThemeRole,
        /// The absent variant.
        variant: ColorVariant,
    },
    /// A variant entry is not a JSON object, so it has no `color`.
    VariantNotObject {
        /// The role whose variant is malformed.
        role: ThemeRole,
        /// The malformed variant.
        variant: ColorVariant,
    },
    /// A variant object has no `color` key.
    MissingColor {
        /// The role missing a color string.
        role: ThemeRole,
        /// The variant missing a color string.
        variant: ColorVariant,
    },
    /// `color` is present but is not a JSON string.
    ColorNotString {
        /// The role whose color has the wrong type.
        role: ThemeRole,
        /// The variant whose color has the wrong type.
        variant: ColorVariant,
    },
    /// `color` is a string but fails the RGB rule.
    InvalidColor {
        /// The role whose color is invalid.
        role: ThemeRole,
        /// The variant whose color is invalid.
        variant: ColorVariant,
        /// The offending string, exactly as it appeared in the input.
        raw: String,
    },
}

impl fmt::Display for ThemeImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeImportError::RootNotObject => {
                formatter.write_str("theme document root must be a JSON object")
            }
            ThemeImportError::MissingColors => {
                formatter.write_str("theme document is missing the `colors` object")
            }
            ThemeImportError::ColorsNotObject => {
                formatter.write_str("`colors` must be a JSON object keyed by role name")
            }
            ThemeImportError::MissingRole { role } => {
                write!(
                    formatter,
                    "missing required role `{}` in `colors`",
                    role.name()
                )
            }
            ThemeImportError::RoleNotObject { role } => {
                write!(
                    formatter,
                    "role `{}` must be an object with `dark` and `light` variants",
                    role.name()
                )
            }
            ThemeImportError::MissingVariant { role, variant } => {
                write!(
                    formatter,
                    "role `{}` is missing the `{}` variant",
                    role.name(),
                    variant.name()
                )
            }
            ThemeImportError::VariantNotObject { role, variant } => {
                write!(
                    formatter,
                    "role `{}` variant `{}` must be an object with a `color` string",
                    role.name(),
                    variant.name()
                )
            }
            ThemeImportError::MissingColor { role, variant } => {
                write!(
                    formatter,
                    "role `{}` variant `{}` is missing its `color` string",
                    role.name(),
                    variant.name()
                )
            }
            ThemeImportError::ColorNotString { role, variant } => {
                write!(
                    formatter,
                    "role `{}` variant `{}` `color` must be a string",
                    role.name(),
                    variant.name()
                )
            }
            ThemeImportError::InvalidColor { role, variant, raw } => {
                write!(
                    formatter,
                    "role `{}` variant `{}` has invalid color {raw:?}: expected exactly \
                     six ASCII hex digits with an optional single leading `#`",
                    role.name(),
                    variant.name()
                )
            }
        }
    }
}

impl std::error::Error for ThemeImportError {}

/// The result of a successful import: the validated document plus the
/// names of any unknown `colors` entries that were ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    /// The validated document. Every required role and variant was
    /// present and valid; nothing was defaulted.
    pub document: ThemeDocument,
    /// Unknown keys found inside `colors`, sorted. These were reported,
    /// not imported, and they do not appear in
    /// [`ThemeDocument::to_value`]. Empty when the input contained
    /// exactly the 50 known roles.
    pub ignored_roles: Vec<String>,
}

fn read_variant_color(
    role_node: &Map<String, Value>,
    role: ThemeRole,
    variant: ColorVariant,
) -> Result<String, ThemeImportError> {
    let variant_value = role_node
        .get(variant.name())
        .ok_or(ThemeImportError::MissingVariant { role, variant })?;
    let variant_node = variant_value
        .as_object()
        .ok_or(ThemeImportError::VariantNotObject { role, variant })?;
    let color_value = variant_node
        .get("color")
        .ok_or(ThemeImportError::MissingColor { role, variant })?;
    let raw = color_value
        .as_str()
        .ok_or(ThemeImportError::ColorNotString { role, variant })?;
    normalize_hex_color(raw).ok_or_else(|| ThemeImportError::InvalidColor {
        role,
        variant,
        raw: raw.to_owned(),
    })
}

fn read_role_colors(role_value: &Value, role: ThemeRole) -> Result<RoleColors, ThemeImportError> {
    let role_node = role_value
        .as_object()
        .ok_or(ThemeImportError::RoleNotObject { role })?;
    let dark = read_variant_color(role_node, role, ColorVariant::Dark)?;
    let light = read_variant_color(role_node, role, ColorVariant::Light)?;
    Ok(RoleColors { dark, light })
}

/// Import a donor-shaped theme document from an already-parsed JSON
/// value.
///
/// Reads `colors.<role>.dark.color` and `colors.<role>.light.color` for
/// every one of the 50 roles in [`ROLE_KEYS`]. Unrelated top-level keys
/// are ignored; unknown keys inside `colors` are collected into
/// [`ImportOutcome::ignored_roles`]. The donor's per-role `default`
/// entry is ignored, never consulted.
///
/// The function is pure and fail-fast: the input is never mutated, and
/// the first defect in [`ROLE_KEYS`] order (dark before light within a
/// role) is returned as a [`ThemeImportError`] naming the role and
/// variant. A successful result means every required color existed and
/// was valid — no value is ever filled in.
pub fn import_document(value: &Value) -> Result<ImportOutcome, ThemeImportError> {
    let root = value.as_object().ok_or(ThemeImportError::RootNotObject)?;
    let colors_value = root.get("colors").ok_or(ThemeImportError::MissingColors)?;
    let colors = colors_value
        .as_object()
        .ok_or(ThemeImportError::ColorsNotObject)?;

    let mut ignored_roles: Vec<String> = colors
        .keys()
        .filter(|key| ThemeRole::from_name(key).is_none())
        .cloned()
        .collect();
    ignored_roles.sort();

    let mut entries: Vec<RoleColors> = Vec::with_capacity(ROLE_COUNT);
    for role in ThemeRole::ALL {
        let role_value = colors
            .get(role.name())
            .ok_or(ThemeImportError::MissingRole { role })?;
        entries.push(read_role_colors(role_value, role)?);
    }
    let colors: [RoleColors; ROLE_COUNT] = entries
        .try_into()
        .expect("one validated entry was pushed per role in ROLE_KEYS");

    Ok(ImportOutcome {
        document: ThemeDocument::new(colors),
        ignored_roles,
    })
}

/// One navigation group of the role editor, in donor display order.
///
/// Group membership and order follow the donor theme designer
/// (`frontend/theme/roles.rs` descriptors in group order, displayed
/// by `frontend/theme_designer/view.rs`); labels are the donor
/// English strings from `locales/en-US/theme-designer.ftl`, bound by
/// the role-editor handoff inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoleGroup {
    WallInterface,
    PrimaryAndSecondary,
    Tertiary,
    SurfacesAndOutlines,
    Errors,
    SourceAndEffects,
}

impl RoleGroup {
    /// All six groups in display order.
    pub const ALL: [RoleGroup; 6] = [
        RoleGroup::WallInterface,
        RoleGroup::PrimaryAndSecondary,
        RoleGroup::Tertiary,
        RoleGroup::SurfacesAndOutlines,
        RoleGroup::Errors,
        RoleGroup::SourceAndEffects,
    ];

    /// The donor group label.
    pub const fn label(self) -> &'static str {
        match self {
            RoleGroup::WallInterface => "Wall interface",
            RoleGroup::PrimaryAndSecondary => "Primary and secondary",
            RoleGroup::Tertiary => "Tertiary",
            RoleGroup::SurfacesAndOutlines => "Surfaces and outlines",
            RoleGroup::Errors => "Errors",
            RoleGroup::SourceAndEffects => "Source and effects",
        }
    }
}

/// Static navigation metadata for one role: its editor group, the
/// donor English label and description, and whether  currently
/// renders this role through a consumer (only the nine mapped roles
/// do). Descriptions are the donor's, except `source_color`: the
/// donor text instructs palette derivation, which  defers, so
/// its description states that instead of promising generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleDescriptor {
    pub role: ThemeRole,
    pub group: RoleGroup,
    pub label: &'static str,
    pub description: &'static str,
    pub has_consumer: bool,
}

/// All 50 descriptors in donor display order: groups in
/// [`RoleGroup::ALL`] order, descriptor (index) order within each
/// group. This is the role editor's navigation order; it differs
/// from raw role-index order.
pub const ROLE_DESCRIPTORS: [RoleDescriptor; ROLE_COUNT] = [
    RoleDescriptor {
        role: ThemeRole::Primary,
        group: RoleGroup::WallInterface,
        label: "Accent",
        description: "Selection borders, active chips, sliders, and highlights",
        has_consumer: true,
    },
    RoleDescriptor {
        role: ThemeRole::OnPrimary,
        group: RoleGroup::WallInterface,
        label: "Accent text",
        description: "Text shown on accent-coloured chips and buttons",
        has_consumer: true,
    },
    RoleDescriptor {
        role: ThemeRole::Tertiary,
        group: RoleGroup::WallInterface,
        label: "Second accent",
        description: "Tag chips, secondary highlights, and the swatch strip",
        has_consumer: true,
    },
    RoleDescriptor {
        role: ThemeRole::Surface,
        group: RoleGroup::WallInterface,
        label: "Panel",
        description: "Settings and overlay backgrounds",
        has_consumer: true,
    },
    RoleDescriptor {
        role: ThemeRole::OnSurface,
        group: RoleGroup::WallInterface,
        label: "Panel text",
        description: "Body text on panels and cards",
        has_consumer: true,
    },
    RoleDescriptor {
        role: ThemeRole::SurfaceVariant,
        group: RoleGroup::WallInterface,
        label: "Panel alt",
        description: "Card backs, the filter bar, and subtle fills",
        has_consumer: true,
    },
    RoleDescriptor {
        role: ThemeRole::SurfaceContainer,
        group: RoleGroup::WallInterface,
        label: "Panel raised",
        description: "Cards, rows, and raised panels",
        has_consumer: true,
    },
    RoleDescriptor {
        role: ThemeRole::Background,
        group: RoleGroup::WallInterface,
        label: "Backdrop",
        description: "The background behind every panel",
        has_consumer: true,
    },
    RoleDescriptor {
        role: ThemeRole::Outline,
        group: RoleGroup::WallInterface,
        label: "Outline",
        description: "Thin borders and dividers",
        has_consumer: true,
    },
    RoleDescriptor {
        role: ThemeRole::InversePrimary,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Inverse primary",
        description: "Accent on inverse surfaces",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnPrimaryContainer,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Primary container text",
        description: "Text on the primary container",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnPrimaryFixed,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Fixed primary text",
        description: "Text on the fixed primary colour",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnPrimaryFixedVariant,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Fixed primary text alt",
        description: "Secondary text on the fixed primary colour",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnSecondary,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Secondary text",
        description: "Text on secondary controls",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnSecondaryContainer,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Secondary container text",
        description: "Text on the secondary container",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnSecondaryFixed,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Fixed secondary text",
        description: "Text on the fixed secondary colour",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnSecondaryFixedVariant,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Fixed secondary text alt",
        description: "Secondary text on the fixed secondary colour",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::PrimaryContainer,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Primary container",
        description: "Background for primary content",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::PrimaryFixed,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Fixed primary",
        description: "Primary accent normally shared by dark and light themes",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::PrimaryFixedDim,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Fixed primary dim",
        description: "Dimmer version of the fixed primary accent",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::Secondary,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Secondary",
        description: "Supporting accent used by integrations",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SecondaryContainer,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Secondary container",
        description: "Background for secondary content",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SecondaryFixed,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Fixed secondary",
        description: "Secondary accent normally shared by dark and light themes",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SecondaryFixedDim,
        group: RoleGroup::PrimaryAndSecondary,
        label: "Fixed secondary dim",
        description: "Dimmer version of the fixed secondary accent",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnTertiary,
        group: RoleGroup::Tertiary,
        label: "Tertiary text",
        description: "Text on tertiary controls",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnTertiaryContainer,
        group: RoleGroup::Tertiary,
        label: "Tertiary container text",
        description: "Text on the tertiary container",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnTertiaryFixed,
        group: RoleGroup::Tertiary,
        label: "Fixed tertiary text",
        description: "Text on the fixed tertiary colour",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnTertiaryFixedVariant,
        group: RoleGroup::Tertiary,
        label: "Fixed tertiary text alt",
        description: "Secondary text on the fixed tertiary colour",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::TertiaryContainer,
        group: RoleGroup::Tertiary,
        label: "Tertiary container",
        description: "Background for tertiary content",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::TertiaryFixed,
        group: RoleGroup::Tertiary,
        label: "Fixed tertiary",
        description: "Tertiary accent normally shared by dark and light themes",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::TertiaryFixedDim,
        group: RoleGroup::Tertiary,
        label: "Fixed tertiary dim",
        description: "Dimmer version of the fixed tertiary accent",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::InverseOnSurface,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Inverse surface text",
        description: "Text on inverse surfaces",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::InverseSurface,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Inverse surface",
        description: "Surfaces with the opposite lightness",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnBackground,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Background text",
        description: "Text directly on the background",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnSurfaceVariant,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Surface variant text",
        description: "Secondary text on surfaces",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OutlineVariant,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Outline variant",
        description: "Subtle borders and dividers",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SurfaceBright,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Bright surface",
        description: "Brightest general surface",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SurfaceContainerHigh,
        group: RoleGroup::SurfacesAndOutlines,
        label: "High container",
        description: "Surface for prominent containers",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SurfaceContainerHighest,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Highest container",
        description: "Surface for the most prominent containers",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SurfaceContainerLow,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Low container",
        description: "Surface for recessed containers",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SurfaceContainerLowest,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Lowest container",
        description: "Surface for the most recessed containers",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SurfaceDim,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Dim surface",
        description: "Dimmest general surface",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SurfaceTint,
        group: RoleGroup::SurfacesAndOutlines,
        label: "Surface tint",
        description: "Accent tint used for elevated surfaces",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::Error,
        group: RoleGroup::Errors,
        label: "Error",
        description: "Error indicators and destructive actions",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::ErrorContainer,
        group: RoleGroup::Errors,
        label: "Error container",
        description: "Background behind error messages",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnError,
        group: RoleGroup::Errors,
        label: "Error text",
        description: "Text on error-coloured controls",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::OnErrorContainer,
        group: RoleGroup::Errors,
        label: "Error container text",
        description: "Text inside error messages",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::Scrim,
        group: RoleGroup::SourceAndEffects,
        label: "Scrim",
        description: "Colour behind modal overlays",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::Shadow,
        group: RoleGroup::SourceAndEffects,
        label: "Shadow",
        description: "Colour used for shadows",
        has_consumer: false,
    },
    RoleDescriptor {
        role: ThemeRole::SourceColor,
        group: RoleGroup::SourceAndEffects,
        label: "Source colour",
        description: "Source colour stored in the document. Palette generation from a source colour is deferred in this editor.",
        has_consumer: false,
    },
];

/// The descriptor for one role.
pub fn role_descriptor(role: ThemeRole) -> &'static RoleDescriptor {
    ROLE_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.role == role)
        .expect("every role has a descriptor")
}

/// Descriptors matching a filter, in display order. Donor filter
/// semantics: the needle is trimmed and lowercased; a role matches
/// when its label (lowercased) contains the needle or its exact
/// role key contains the needle. An empty needle matches all roles.
pub fn filtered_role_descriptors(filter: &str) -> Vec<&'static RoleDescriptor> {
    let needle = filter.trim().to_lowercase();
    ROLE_DESCRIPTORS
        .iter()
        .filter(|descriptor| {
            needle.is_empty()
                || descriptor.label.to_lowercase().contains(&needle)
                || descriptor.role.name().contains(&needle)
        })
        .collect()
}
