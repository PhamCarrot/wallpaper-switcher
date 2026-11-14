//! Typed local editor records. These never invoke commands, files, or providers.
use super::{catalog::parse_resolution, keys, ActionId};

pub use crate::candidate_state::pending::{Command, Integration, SemanticModel};
pub use crate::candidate_state::preferences::{Orientation, ResolutionPreset};
#[derive(Debug, Default)]
pub struct Collections {
    pub resolutions: Vec<ResolutionPreset>,
    pub commands: Vec<Command>,
    pub integrations: Vec<Integration>,
    pub semantic_models: Vec<SemanticModel>,
}

impl Collections {
    pub fn len(&self, root: &str) -> usize {
        match root {
            keys::filter_bar::RESOLUTION_PRESETS => self.resolutions.len(),
            keys::post_processing::LIST => self.commands.len(),
            keys::integrations::LIST => self.integrations.len(),
            keys::semantic::MODELS => self.semantic_models.len(),
            _ => 0,
        }
    }
    pub fn action(
        &mut self,
        action: ActionId,
        catalog: &crate::candidate_app::localization::Catalog,
    ) -> bool {
        match action {
            ActionId::CreateResolutionPresetBand(band) => {
                let (label, from, to) = match band {
                    "fhd" => (
                        "settings-filter-resolution-band-fhd",
                        (1920, 1080),
                        Some((2559, 1439)),
                    ),
                    "qhd" => (
                        "settings-filter-resolution-band-qhd",
                        (2560, 1440),
                        Some((3839, 2159)),
                    ),
                    "4k" => (
                        "settings-filter-resolution-band-4k",
                        (3840, 2160),
                        Some((5119, 2879)),
                    ),
                    "5k" => (
                        "settings-filter-resolution-band-5k",
                        (5120, 2880),
                        Some((7679, 4319)),
                    ),
                    "8k" => ("settings-filter-resolution-band-8k", (7680, 4320), None),
                    "custom" => ("settings-filter-resolution-band-custom", (1920, 1080), None),
                    _ => return false,
                };
                self.resolutions.push(ResolutionPreset {
                    retained: Default::default(),
                    label: catalog.text(label),
                    orientation: Orientation::Wide,
                    from,
                    to,
                });
            }
            ActionId::AddResolutionPreset => {
                return self.action(ActionId::CreateResolutionPresetBand("custom"), catalog)
            }
            ActionId::RemoveResolutionPreset(index) => return remove(&mut self.resolutions, index),
            ActionId::AddPostCommand => self.commands.push(Command {
                kind: "all".into(),
                command: String::new(),
                ..Default::default()
            }),
            ActionId::RemovePostCommand(index) => return remove(&mut self.commands, index),
            ActionId::AddIntegration => self.integrations.push(Integration {
                name: String::new(),
                template: String::new(),
                output: String::new(),
                ..Default::default()
            }),
            ActionId::RemoveIntegration(index) => return remove(&mut self.integrations, index),
            ActionId::AddSemanticModel => self.semantic_models.push(SemanticModel {
                name: String::new(),
                manifest: String::new(),
                ..Default::default()
            }),
            ActionId::RemoveSemanticModel(index) => {
                return remove(&mut self.semantic_models, index)
            }
            _ => return false,
        }
        true
    }
    pub fn text(&self, path: &str) -> Option<String> {
        if let Some((index, field)) = member(path, keys::filter_bar::RESOLUTION_PRESETS) {
            let item = self.resolutions.get(index)?;
            return Some(match field {
                "label" => item.label.clone(),
                "orientation" => match item.orientation {
                    Orientation::Wide => "wide",
                    Orientation::Tall => "tall",
                }
                .into(),
                "from" => format!("{}x{}", item.from.0, item.from.1),
                "to" => item.to.map(|(w, h)| format!("{w}x{h}")).unwrap_or_default(),
                _ => return None,
            });
        }
        if let Some((index, field)) = member(path, keys::post_processing::LIST) {
            let item = self.commands.get(index)?;
            return Some(match field {
                "type" => item.kind.clone(),
                "command" => item.command.clone(),
                _ => return None,
            });
        }
        if let Some((index, field)) = member(path, keys::integrations::LIST) {
            let item = self.integrations.get(index)?;
            return Some(match field {
                "name" => item.name.clone(),
                "template" => item.template.clone(),
                "output" => item.output.clone(),
                "reload" => item.reload.clone().unwrap_or_default(),
                _ => return None,
            });
        }
        if let Some((index, field)) = member(path, keys::semantic::MODELS) {
            let item = self.semantic_models.get(index)?;
            return Some(match field {
                "name" => item.name.clone(),
                "manifest" => item.manifest.clone(),
                _ => return None,
            });
        }
        None
    }
    pub fn edit(&mut self, path: &str, value: &str) -> Result<(), String> {
        if let Some((index, field)) = member(path, keys::filter_bar::RESOLUTION_PRESETS) {
            let item = self
                .resolutions
                .get_mut(index)
                .ok_or("Resolution record no longer exists.")?;
            let mut next = item.clone();
            match field {
                "label" => next.label = value.into(),
                "orientation" => {
                    next.orientation = match value {
                        "wide" => Orientation::Wide,
                        "tall" => Orientation::Tall,
                        _ => return Err("Choose wide or tall.".into()),
                    }
                }
                "from" => {
                    next.from = parse_resolution(value).ok_or("Enter positive width × height.")?
                }
                "to" => {
                    next.to = if value.trim().is_empty() {
                        None
                    } else {
                        Some(parse_resolution(value).ok_or("Enter positive width × height.")?)
                    }
                }
                _ => return Err("Unknown resolution field.".into()),
            }
            if !next.valid() {
                return Err(
                    "Upper resolution must not be smaller than the lower resolution.".into(),
                );
            }
            *item = next;
            return Ok(());
        }
        if let Some((index, field)) = member(path, keys::post_processing::LIST) {
            let item = self
                .commands
                .get_mut(index)
                .ok_or("Command record no longer exists.")?;
            match field {
                "type" if ["all", "static", "video", "we"].contains(&value) => {
                    item.kind = value.into()
                }
                "command" => item.command = value.into(),
                _ => return Err("Unknown command field or kind.".into()),
            }
            return Ok(());
        }
        if let Some((index, field)) = member(path, keys::integrations::LIST) {
            let item = self
                .integrations
                .get_mut(index)
                .ok_or("Integration no longer exists.")?;
            match field {
                "name" => item.name = value.into(),
                "template" => item.template = value.into(),
                "output" => item.output = value.into(),
                "reload" => item.reload = Some(value.into()),
                "enabled" => {
                    item.enabled = Some(value.parse().map_err(|_| "Enter a boolean value.")?)
                }
                "livePreview" => {
                    item.live_preview = Some(value.parse().map_err(|_| "Enter a boolean value.")?)
                }
                _ => return Err("Unknown integration field.".into()),
            }
            return Ok(());
        }
        if let Some((index, field)) = member(path, keys::semantic::MODELS) {
            let item = self
                .semantic_models
                .get_mut(index)
                .ok_or("Model record no longer exists.")?;
            match field {
                "name" => item.name = value.into(),
                "manifest" => item.manifest = value.into(),
                _ => return Err("Unknown model field.".into()),
            }
            return Ok(());
        }
        Err("Unknown local collection field.".into())
    }
}
fn remove<T>(items: &mut Vec<T>, index: u16) -> bool {
    if usize::from(index) >= items.len() {
        return false;
    }
    items.remove(usize::from(index));
    true
}
fn member<'a>(path: &'a str, root: &str) -> Option<(usize, &'a str)> {
    let rest = path.strip_prefix(root)?.strip_prefix('.')?;
    let (index, field) = rest.split_once('.')?;
    Some((index.parse().ok()?, field))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_bands_and_invalid_pair_edits_preserve_last_valid_record() {
        let catalog = crate::candidate_app::localization::Catalog::default();
        let mut lists = Collections::default();
        assert!(lists.action(ActionId::CreateResolutionPresetBand("fhd"), &catalog));
        let before = lists.resolutions[0].clone();
        let root = keys::filter_bar::RESOLUTION_PRESETS;
        assert!(lists.edit(&format!("{root}.0.from"), "0×1080").is_err());
        assert!(lists.edit(&format!("{root}.0.to"), "1280x720").is_err());
        assert_eq!(lists.resolutions[0], before);
        assert!(lists.edit(&format!("{root}.0.to"), "3840X2160").is_ok());
        assert_eq!(lists.resolutions[0].to, Some((3840, 2160)));
        assert!(lists.edit(&format!("{root}.0.orientation"), "tall").is_ok());
        assert!(lists.edit(&format!("{root}.0.to"), "").is_ok());
        assert_eq!(lists.resolutions[0].to, None);
        assert!(!lists.action(ActionId::CreateResolutionPresetBand("invented"), &catalog));
        assert!(!lists.action(ActionId::RemoveResolutionPreset(2), &catalog));
        assert_eq!(lists.resolutions.len(), 1);
    }
    #[test]
    fn local_command_model_and_integration_forms_never_accept_execution_actions() {
        let catalog = crate::candidate_app::localization::Catalog::default();
        let mut lists = Collections::default();
        assert!(lists.action(ActionId::AddPostCommand, &catalog));
        assert!(lists.action(ActionId::AddIntegration, &catalog));
        assert!(lists.action(ActionId::AddSemanticModel, &catalog));
        assert_eq!(lists.text("postProcessing.0.type").as_deref(), Some("all"));
        assert!(lists
            .edit("postProcessing.0.command", "echo literal local text")
            .is_ok());
        assert!(lists.edit("postProcessing.0.type", "unknown").is_err());
        assert!(lists
            .edit("integrations.0.output", "C:/owned/example.conf")
            .is_ok());
        assert!(lists
            .edit("semantic.models.0.manifest", "C:/owned/manifest.json")
            .is_ok());
        assert!(!lists.action(ActionId::ImportSemanticModel, &catalog));
        assert!(!lists.action(ActionId::OptimizeImages, &catalog));
        assert!(lists.edit("postProcessing.0.execute", "true").is_err());
        assert!(lists.action(ActionId::RemovePostCommand(0), &catalog));
        assert!(lists.edit("postProcessing.0.command", "stale").is_err());
    }
}
