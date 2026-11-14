//! Source-defined language/script projection; app state owns the selected catalog.
//! Resources are supplied skwd-wall translations; see SKWD-WALL-NOTICE.md.
use fluent::{concurrent::FluentBundle, FluentArgs, FluentResource};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

include!(concat!(env!("OUT_DIR"), "/embedded_locales.rs"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Script {
    Latin,
    Simplified,
    Japanese,
    Arabic,
    Bengali,
    Devanagari,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Language {
    pub(super) tag: &'static str,
    pub(super) title_key: &'static str,
    pub(super) script: Script,
    pub(super) rtl: bool,
}

pub(super) const LANGUAGES: [Language; 14] = [
    Language {
        tag: "en-US",
        title_key: "settings-language-english",
        script: Script::Latin,
        rtl: false,
    },
    Language {
        tag: "sv-SE",
        title_key: "settings-language-swedish",
        script: Script::Latin,
        rtl: false,
    },
    Language {
        tag: "es-ES",
        title_key: "settings-language-spanish",
        script: Script::Latin,
        rtl: false,
    },
    Language {
        tag: "pt-BR",
        title_key: "settings-language-portuguese",
        script: Script::Latin,
        rtl: false,
    },
    Language {
        tag: "ru-RU",
        title_key: "settings-language-russian",
        script: Script::Latin,
        rtl: false,
    },
    Language {
        tag: "zh-CN",
        title_key: "settings-language-chinese",
        script: Script::Simplified,
        rtl: false,
    },
    Language {
        tag: "ja-JP",
        title_key: "settings-language-japanese",
        script: Script::Japanese,
        rtl: false,
    },
    Language {
        tag: "ar-SA",
        title_key: "settings-language-arabic",
        script: Script::Arabic,
        rtl: true,
    },
    Language {
        tag: "fr-FR",
        title_key: "settings-language-french",
        script: Script::Latin,
        rtl: false,
    },
    Language {
        tag: "bn-BD",
        title_key: "settings-language-bengali",
        script: Script::Bengali,
        rtl: false,
    },
    Language {
        tag: "ur-PK",
        title_key: "settings-language-urdu",
        script: Script::Arabic,
        rtl: true,
    },
    Language {
        tag: "hi-IN",
        title_key: "settings-language-hindi",
        script: Script::Devanagari,
        rtl: false,
    },
    Language {
        tag: "fa-IR",
        title_key: "settings-language-persian",
        script: Script::Arabic,
        rtl: true,
    },
    Language {
        tag: "tr-TR",
        title_key: "settings-language-turkish",
        script: Script::Latin,
        rtl: false,
    },
];

pub(super) fn normalized_tag(raw: &str) -> Option<&'static str> {
    let locale = raw
        .trim()
        .split(['.', '@'])
        .next()?
        .replace('_', "-")
        .to_ascii_lowercase();
    if matches!(locale.as_str(), "c" | "posix") {
        return Some("en-US");
    }
    let mut parts = locale.split('-');
    let prefix = parts.next()?;
    if prefix == "zh" {
        let parts: Vec<_> = parts.collect();
        if parts
            .iter()
            .any(|part| matches!(*part, "hant" | "tw" | "hk" | "mo"))
            && !parts.contains(&"hans")
        {
            return None;
        }
    }
    LANGUAGES
        .iter()
        .filter(|language| matches!(language.tag, "en-US" | "zh-CN"))
        .find(|language| {
            language
                .tag
                .split('-')
                .next()
                .unwrap()
                .eq_ignore_ascii_case(prefix)
        })
        .map(|language| language.tag)
}

pub(super) struct Catalog {
    pub(super) language: &'static Language,
    bundle: FluentBundle<FluentResource>,
}

impl Catalog {
    pub(super) fn new(requested: &str, system_locale: &str) -> Result<Self, String> {
        let tag = if requested == "auto" || requested.trim().is_empty() {
            normalized_tag(system_locale).unwrap_or("en-US")
        } else {
            normalized_tag(requested).unwrap_or("en-US")
        };
        let language = LANGUAGES
            .iter()
            .find(|language| language.tag == tag)
            .unwrap();
        let mut bundle = FluentBundle::new_concurrent(vec![tag.parse().expect("fixed locale tag")]);
        bundle.set_use_isolating(false);
        for locale in ["en-US", tag] {
            if tag == "en-US" && bundle.has_message("settings-tab-picker") {
                continue;
            }
            let resources = LOCALE_RESOURCES
                .iter()
                .find(|(key, _)| *key == locale)
                .ok_or_else(|| format!("Missing embedded locale: {locale}"))?
                .1;
            for source in resources {
                let resource = FluentResource::try_new((*source).to_owned())
                    .map_err(|(_, errors)| format!("Invalid {locale} resource: {errors:?}"))?;
                if locale == "en-US" {
                    bundle
                        .add_resource(resource)
                        .map_err(|errors| format!("Invalid fallback resource: {errors:?}"))?;
                } else {
                    bundle.add_resource_overriding(resource);
                }
            }
        }
        Ok(Self { language, bundle })
    }

    #[cfg(test)]
    pub(super) fn with_embedded_rtl_for_test() -> Self {
        let mut catalog = Self::default();
        catalog.language = LANGUAGES
            .iter()
            .find(|language| language.tag == "ar-SA")
            .unwrap();
        catalog.bundle.set_use_isolating(false);
        let (_, sources) = LOCALE_RESOURCES
            .iter()
            .find(|(tag, _)| *tag == "ar-SA")
            .unwrap();
        for source in *sources {
            catalog
                .bundle
                .add_resource_overriding(FluentResource::try_new((*source).to_owned()).unwrap());
        }
        catalog
    }

    pub(super) fn text(&self, key: &str) -> String {
        self.format(key, None)
            .expect("verified embedded translation key")
    }

    pub(super) fn text_static(&self, key: &'static str) -> &'static str {
        // Source Card titles borrow static text. Intern each fixed locale/key
        // once; selected language remains owned by the calling app catalog.
        type Cache = HashMap<(&'static str, &'static str), &'static str>;
        static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
        let mut cache = CACHE
            .get_or_init(Mutex::default)
            .lock()
            .expect("locale text cache");
        cache
            .entry((self.language.tag, key))
            .or_insert_with(|| Box::leak(self.text(key).into_boxed_str()))
    }

    pub(super) fn format(
        &self,
        key: &str,
        args: Option<&FluentArgs<'_>>,
    ) -> Result<String, String> {
        let pattern = self
            .bundle
            .get_message(key)
            .and_then(|message| message.value())
            .ok_or_else(|| format!("Missing translation: {key}"))?;
        let mut errors = Vec::new();
        let text = self
            .bundle
            .format_pattern(pattern, args, &mut errors)
            .into_owned();
        if errors.is_empty() {
            Ok(text)
        } else {
            Err(format!("Could not format {key}: {errors:?}"))
        }
    }

    pub(super) fn font_bytes(&self) -> &'static [u8] {
        match self.language.script {
            Script::Latin => include_bytes!("../../assets/fonts/RobotoCondensed-Bold-UI.ttf"),
            Script::Simplified => include_bytes!("../../assets/fonts/RobotoCondensed-Bold-SC.ttf"),
            Script::Japanese => include_bytes!("../../assets/fonts/RobotoCondensed-Bold-JP.ttf"),
            Script::Arabic => include_bytes!("../../assets/fonts/RobotoCondensed-Bold-AR.ttf"),
            Script::Bengali => include_bytes!("../../assets/fonts/RobotoCondensed-Bold-BN.ttf"),
            Script::Devanagari => include_bytes!("../../assets/fonts/RobotoCondensed-Bold-HI.ttf"),
        }
    }
}

impl Default for Catalog {
    fn default() -> Self {
        Self::new("en-US", "en-US").expect("verified embedded default locale")
    }
}

/// Source environment precedence, with Windows UI languages when POSIX locale
/// variables are absent. This caches observation only; Catalog owns selection.
pub(super) fn automatic_tag() -> &'static str {
    static AUTOMATIC: OnceLock<&'static str> = OnceLock::new();
    AUTOMATIC.get_or_init(|| {
        let names = [
            "SKWD_WALL_LOCALE",
            "LC_ALL",
            "LC_MESSAGES",
            "LANG",
            "LANGUAGE",
        ];
        let values = names.map(|name| std::env::var(name).ok());
        resolve_automatic(
            values.each_ref().map(|value| value.as_deref()),
            &windows_ui_languages(),
        )
    })
}

fn resolve_automatic(values: [Option<&str>; 5], windows: &[String]) -> &'static str {
    let values = values.map(|value| value.map(str::trim).filter(|value| !value.is_empty()));
    if let Some(explicit) = values[0] {
        return normalized_tag(explicit).unwrap_or("en-US");
    }
    let system = values[1..4].iter().copied().flatten().next();
    if system.is_some_and(|locale| {
        matches!(
            locale
                .split(['.', '@'])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "c" | "posix"
        )
    }) {
        return "en-US";
    }
    if let Some(preference) = values[4].and_then(|list| list.split(':').find_map(normalized_tag)) {
        return preference;
    }
    match system {
        Some(system) => normalized_tag(system).unwrap_or("en-US"),
        None => windows
            .iter()
            .find_map(|locale| normalized_tag(locale))
            .unwrap_or("en-US"),
    }
}

#[cfg(windows)]
fn windows_ui_languages() -> Vec<String> {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut count = 0;
    let mut size = 0;
    // The first call reports the MULTI_SZ size in UTF-16 code units.
    if unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut size,
        )
    } == 0
        || !(2..=65536).contains(&size)
    {
        return Vec::new();
    }
    let mut buffer = vec![0u16; size as usize];
    if unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            buffer.as_mut_ptr(),
            &mut size,
        )
    } == 0
    {
        return Vec::new();
    }
    buffer
        .split(|unit| *unit == 0)
        .filter(|value| !value.is_empty())
        .filter_map(|value| String::from_utf16(value).ok())
        .collect()
}

#[cfg(not(windows))]
fn windows_ui_languages() -> Vec<String> {
    Vec::new()
}

/// Preserve file-backed/system and Nerd faces. Owned bytes bypass Iced's
/// pointer deduplication when returning to a removed embedded UI face.
pub(super) fn reload_ui_font(previous: &Catalog, next: &Catalog) -> Result<bool, String> {
    publish_locale_change(previous.language.script, next, || Ok(()))
}

/// Acquire the fallible font lock first; publish the preference before changing
/// faces. A failed save leaves both the committed catalog and font unchanged.
pub(super) fn publish_locale_change(
    previous: Script,
    next: &Catalog,
    publish: impl FnOnce() -> Result<(), String>,
) -> Result<bool, String> {
    if previous == next.language.script {
        publish()?;
        return Ok(false);
    }
    use iced::advanced::graphics::text::{cosmic_text::fontdb::Source, font_system};
    let mut system = font_system()
        .write()
        .map_err(|_| "Font database is unavailable.".to_owned())?;
    publish()?;
    let database = system.raw().db_mut();
    let ids: Vec<_> = database
        .faces()
        .filter(|face| {
            matches!(&face.source, Source::Binary(_))
                && face
                    .families
                    .iter()
                    .any(|(family, _)| family == "Roboto Condensed")
        })
        .map(|face| face.id)
        .collect();
    for id in ids {
        database.remove_face(id);
    }
    system.load_font(std::borrow::Cow::Owned(next.font_bytes().to_vec()));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_locale_keeps_source_override_c_and_preference_order_with_windows_fallback() {
        let windows = vec!["ja-JP".into(), "en-US".into()];
        assert_eq!(resolve_automatic([None; 5], &windows), "en-US");
        assert_eq!(
            resolve_automatic([Some("ar_SA"), Some("C"), None, None, None], &windows),
            "en-US"
        );
        assert_eq!(
            resolve_automatic([Some("unknown"), Some("ja_JP"), None, None, None], &windows),
            "en-US"
        );
        assert_eq!(
            resolve_automatic([None, Some("C.UTF-8"), None, None, Some("fr:ja")], &windows),
            "en-US"
        );
        assert_eq!(
            resolve_automatic(
                [None, None, Some("de_DE"), None, Some("unknown:fr_FR:ja")],
                &windows
            ),
            "en-US"
        );
        assert_eq!(
            resolve_automatic([None, None, None, Some("zh_Hant_TW"), None], &[]),
            "en-US"
        );
        assert_eq!(
            resolve_automatic([None, None, None, Some("zh_Hans_TW"), None], &[]),
            "zh-CN"
        );
    }

    #[test]
    fn all_source_locales_format_actual_labels_and_select_exact_script_assets() {
        for language in LANGUAGES {
            let catalog = Catalog::new(language.tag, "en-US").unwrap();
            assert_eq!(
                catalog.language.tag,
                normalized_tag(language.tag).unwrap_or("en-US")
            );
            assert_eq!(catalog.language.rtl, false);
            for key in [
                "settings-tab-picker",
                "settings-section-general",
                "settings-language-system",
                language.title_key,
            ] {
                assert!(!catalog.format(key, None).unwrap().trim().is_empty());
            }
            assert!(catalog.font_bytes().len() > 400_000);
        }
        assert_eq!(
            Catalog::new("auto", "ja_JP.UTF-8").unwrap().language.tag,
            "en-US"
        );
        assert_eq!(Catalog::new("auto", "zh-TW").unwrap().language.tag, "en-US");
        assert_eq!(
            Catalog::new("invalid", "en-US").unwrap().language.tag,
            "en-US"
        );
        assert_eq!(normalized_tag("zh-Hans-TW"), Some("zh-CN"));
    }

    #[test]
    fn parallel_catalogs_keep_static_source_titles_bound_to_their_explicit_locale() {
        let key = "settings-tab-picker";
        let english = std::thread::spawn(move || {
            let catalog = Catalog::new("en-US", "ja-JP").unwrap();
            let first = catalog.text_static(key);
            for _ in 0..16 {
                assert!(std::ptr::eq(first, catalog.text_static(key)));
                assert_eq!(first, catalog.text(key));
            }
            first
        });
        let japanese = std::thread::spawn(move || {
            let catalog = Catalog::new("zh-CN", "en-US").unwrap();
            let first = catalog.text_static(key);
            for _ in 0..16 {
                assert!(std::ptr::eq(first, catalog.text_static(key)));
                assert_eq!(first, catalog.text(key));
            }
            first
        });
        assert_ne!(english.join().unwrap(), japanese.join().unwrap());
    }
}
