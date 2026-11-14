//! Named visual editing uses the existing role document and binding owner.
//! Persistence and departure decisions belong to the application controller.
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct DesignerState {
    name: String,
    loaded_id: Option<u64>,
    saved_id: Option<u64>,
    saved_name: String,
    saved_document: ThemeDocument,
    recent: Vec<String>,
    hsv: (f32, f32, f32),
}

impl ThemeSession {
    /// Called only after the caller has resolved dirty departure, or on first
    /// opening. Loading a document never changes the applied appearance.
    pub fn load_designer(
        &mut self,
        document: ThemeDocument,
        variant: ColorVariant,
        name: String,
        loaded_id: Option<u64>,
    ) {
        self.clear_transient();
        self.next_editor_instance = self.next_editor_instance.wrapping_add(1);
        self.next_selection_generation = self.next_selection_generation.wrapping_add(1);
        self.editor = Some(RoleEditorState {
            instance: self.next_editor_instance,
            buffer: document.color(ThemeRole::Primary, variant).into(),
            document: document.clone(),
            loaded: document.clone(),
            selected_role: ThemeRole::Primary,
            selected_variant: variant,
            selection_generation: self.next_selection_generation,
            filter: String::new(),
            pending: false,
            field_diagnostic: None,
        });
        let recent = self
            .designer
            .take()
            .map(|state| state.recent)
            .unwrap_or_default();
        self.designer = Some(DesignerState {
            saved_name: name.clone(),
            name,
            loaded_id,
            saved_id: loaded_id,
            hsv: crate::donor_folio::source_ui::color::hex_to_hsv(
                document.color(ThemeRole::Primary, variant),
            )
            .expect("validated document RGB"),
            saved_document: document,
            recent,
        });
        self.rotate_raw_context();
        self.advance_revision();
        self.advance_token();
    }

    pub fn designer_active(&self) -> bool {
        self.designer.is_some()
    }
    pub fn designer_name(&self) -> &str {
        self.designer
            .as_ref()
            .map(|state| state.name.as_str())
            .unwrap_or_default()
    }
    pub fn designer_loaded_id(&self) -> Option<u64> {
        self.designer.as_ref().and_then(|state| state.loaded_id)
    }
    pub fn designer_dirty(&self) -> bool {
        match (&self.designer, &self.editor) {
            (Some(state), Some(editor)) => {
                editor.pending
                    || state.name != state.saved_name
                    || editor.document != state.saved_document
            }
            _ => false,
        }
    }
    pub fn designer_recent(&self) -> &[String] {
        self.designer
            .as_ref()
            .map(|state| state.recent.as_slice())
            .unwrap_or_default()
    }
    pub fn designer_set_name(&mut self, instance: u64, name: String) -> bool {
        if self.editor_instance() != Some(instance) {
            return false;
        }
        let Some(state) = &mut self.designer else {
            return false;
        };
        if state.name != name {
            if name.trim() != state.saved_name {
                state.loaded_id = None;
            }
            state.name = name;
            self.advance_revision();
            self.advance_token();
        }
        true
    }
    /// Save success moves the dirty baseline, never the last-load color baseline.
    pub fn designer_saved(&mut self, id: u64, name: String) {
        if let (Some(state), Some(editor)) = (&mut self.designer, &self.editor) {
            state.name = name.clone();
            state.saved_name = name;
            state.saved_document = editor.document.clone();
            state.loaded_id = Some(id);
            state.saved_id = Some(id);
        }
        self.advance_token();
    }
    pub fn designer_forget_id(&mut self, id: u64) {
        if let Some(state) = &mut self.designer {
            if state.loaded_id == Some(id) {
                state.loaded_id = None;
            }
            if state.saved_id == Some(id) {
                state.saved_id = None;
            }
        }
        self.advance_token();
    }
    pub fn designer_cancel_confirmation(&mut self) {
        self.advance_token();
    }
    /// Hide/reopen retains the exact draft, including invalid text. Both the
    /// editor identity and selection identity rotate to reject queued widgets.
    pub(super) fn invalidate_designer_bindings(&mut self) {
        self.next_editor_instance = self.next_editor_instance.wrapping_add(1);
        if let Some(editor) = &mut self.editor {
            editor.instance = self.next_editor_instance;
        }
        self.rotate_selection_generation();
        self.rotate_raw_context();
        self.preview = None;
        self.advance_revision();
        self.advance_token();
    }
    /// Reset all to the latest successful save/load; Reset color intentionally
    /// uses the separate last-load baseline in RoleEditorState instead.
    pub fn designer_reset(&mut self, instance: u64) -> bool {
        if self.editor_instance() != Some(instance) {
            return false;
        }
        let Some(state) = &mut self.designer else {
            return false;
        };
        state.name = state.saved_name.clone();
        state.loaded_id = state.saved_id;
        let Some(editor) = &mut self.editor else {
            return false;
        };
        editor.document = state.saved_document.clone();
        editor.loaded = state.saved_document.clone();
        editor.buffer = editor
            .document
            .color(editor.selected_role, editor.selected_variant)
            .into();
        editor.pending = false;
        editor.field_diagnostic = None;
        self.sync_designer_hsv();
        self.invalidate_designer_bindings();
        true
    }
    /// Direct wheel/recent edits replace just one validated cell. The input
    /// stream stays bound throughout a drag; selection changes still stale it.
    pub fn designer_color(
        &mut self,
        instance: u64,
        generation: u64,
        role: ThemeRole,
        variant: ColorVariant,
        color: &str,
    ) -> bool {
        if self.designer.is_none() {
            return false;
        }
        let Some(normalized) = crate::theme_roles::normalize_hex_color(color) else {
            return false;
        };
        let Some(editor) = self.editor_for(instance, generation, role, variant) else {
            return false;
        };
        let changed = editor.document.color(role, variant) != normalized || editor.pending;
        editor.document = editor.document.replaced(role, variant, &normalized);
        editor.buffer = normalized;
        editor.pending = false;
        editor.field_diagnostic = None;
        self.sync_designer_hsv();
        if changed {
            self.preview = None;
            self.advance_revision();
            self.advance_token();
        }
        true
    }

    pub(super) fn sync_designer_hsv(&mut self) {
        if let (Some(state), Some(editor)) = (&mut self.designer, &self.editor) {
            state.hsv = crate::donor_folio::source_ui::color::hex_to_hsv(
                editor
                    .document
                    .color(editor.selected_role, editor.selected_variant),
            )
            .expect("validated document RGB");
        }
    }
    pub fn designer_hsv(&self) -> (f32, f32, f32) {
        self.designer
            .as_ref()
            .map(|state| state.hsv)
            .unwrap_or_default()
    }
    pub fn designer_wheel(
        &mut self,
        instance: u64,
        generation: u64,
        role: ThemeRole,
        variant: ColorVariant,
        action: crate::donor_folio::source_ui::wheel::WheelAction,
    ) -> bool {
        use crate::donor_folio::source_ui::{color::hsv_to_hex, wheel::WheelAction};
        if self.designer.is_none()
            || self
                .editor_for(instance, generation, role, variant)
                .is_none()
        {
            return false;
        }
        let (h, s, v) = self.designer_hsv();
        let hsv = match action {
            WheelAction::Hue(h) if h.is_finite() => (
                h.rem_euclid(360.0),
                if s == 0.0 { 0.5 } else { s },
                if v == 0.0 { 0.5 } else { v },
            ),
            WheelAction::SV(s, v) if s.is_finite() && v.is_finite() => {
                (h, s.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
            }
            WheelAction::DragEnd => {
                return self.designer_remember_color(instance, generation, role, variant)
            }
            _ => return false,
        };
        if !self.designer_color(
            instance,
            generation,
            role,
            variant,
            &hsv_to_hex(hsv.0, hsv.1, hsv.2),
        ) {
            return false;
        }
        self.designer.as_mut().expect("checked above").hsv = hsv;
        true
    }
    pub fn designer_set_hex(
        &mut self,
        instance: u64,
        generation: u64,
        role: ThemeRole,
        variant: ColorVariant,
    ) -> bool {
        if self.designer.is_none() || !self.editor_set_color(instance, generation, role, variant) {
            return false;
        }
        if self.editor_pending() {
            return false;
        }
        self.sync_designer_hsv();
        self.designer_remember_color(
            instance,
            self.editor_selection_generation().expect("editor exists"),
            role,
            variant,
        )
    }
    pub fn designer_remember_color(
        &mut self,
        instance: u64,
        generation: u64,
        role: ThemeRole,
        variant: ColorVariant,
    ) -> bool {
        let Some(editor) = self.editor_for(instance, generation, role, variant) else {
            return false;
        };
        if editor.pending {
            return false;
        }
        let color = editor.document.color(role, variant).to_string();
        let Some(state) = &mut self.designer else {
            return false;
        };
        state.recent.retain(|entry| entry != &color);
        state.recent.push(color);
        if state.recent.len() > 8 {
            state.recent.remove(0);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> ThemeSession {
        let mut session = ThemeSession::new();
        session.load_designer(
            crate::candidate_state::frontend::nord_document(),
            ColorVariant::Dark,
            "Nord draft".into(),
            None,
        );
        session
    }
    fn binding(session: &ThemeSession) -> (u64, u64, ThemeRole, ColorVariant) {
        (
            session.editor_instance().unwrap(),
            session.editor_selection_generation().unwrap(),
            session.editor_selected_role().unwrap(),
            session.editor_selected_variant().unwrap(),
        )
    }
    #[test]
    fn p5b_hide_retains_invalid_text_and_document_but_rejects_old_widgets() {
        let mut session = session();
        let before = session.editor_document().unwrap().clone();
        let (i, g, r, v) = binding(&session);
        session.editor_type_text(i, g, r, v, "#bad input".into());
        assert!(session.designer_dirty());
        let revision = session.draft_revision();
        session.on_departure();
        assert_eq!(session.editor_buffer(), Some("#bad input"));
        assert_eq!(session.editor_document(), Some(&before));
        assert!(session.editor_pending());
        assert!(session.draft_revision() > revision);
        assert!(!session.editor_type_text(i, g, r, v, "#ffffff".into()));
        assert!(!session.designer_set_name(i, "stale".into()));
    }
    #[test]
    fn p5b_save_and_reset_color_have_distinct_baselines() {
        let mut session = session();
        let (i, g, r, v) = binding(&session);
        let loaded = session.editor_loaded_color(r, v).unwrap().to_string();
        session.designer_color(i, g, r, v, "#123456");
        session.designer_saved(7, "Saved".into());
        assert!(!session.designer_dirty());
        assert_eq!(session.editor_loaded_color(r, v), Some(loaded.as_str()));
        session.editor_reset_color(i, g, r, v);
        assert_eq!(session.editor_document().unwrap().color(r, v), loaded);
        assert!(session.designer_dirty());
        session.designer_reset(i);
        assert_eq!(session.editor_document().unwrap().color(r, v), "#123456");
        assert!(!session.designer_dirty());
    }
    #[test]
    fn p5b_all100_cells_recent_bound_and_stale_selection() {
        let mut session = session();
        let mut expected = session.editor_document().unwrap().clone();
        for (index, key) in crate::theme_roles::ROLE_KEYS.iter().enumerate() {
            let role = ThemeRole::from_name(key).unwrap();
            for variant in [ColorVariant::Dark, ColorVariant::Light] {
                let i = session.editor_instance().unwrap();
                session.editor_select_role(i, role);
                session.editor_select_variant(i, variant);
                let (i, g, r, v) = binding(&session);
                let color = format!(
                    "#{:06x}",
                    index * 2 + usize::from(variant == ColorVariant::Light)
                );
                assert!(session.designer_color(i, g, r, v, &color));
                assert!(session.designer_remember_color(i, g, r, v));
                expected = expected.replaced(r, v, &color);
                assert_eq!(session.editor_document(), Some(&expected));
            }
        }
        assert_eq!(session.designer_recent().len(), 8);
        let expected_recent: Vec<_> = (92..100).map(|value| format!("#{value:06x}")).collect();
        assert_eq!(session.designer_recent(), expected_recent);
        let (i, g, r, v) = binding(&session);
        assert!(session.designer_color(i, g, r, v, "#000061"));
        assert!(session.designer_remember_color(i, g, r, v));
        let mut expected_recent = expected_recent;
        expected_recent.retain(|color| color != "#000061");
        expected_recent.push("#000061".into());
        assert_eq!(session.designer_recent(), expected_recent);
        expected = expected.replaced(r, v, "#000061");
        let (i, g, r, v) = binding(&session);
        session.editor_select_variant(i, ColorVariant::Dark);
        assert!(!session.designer_color(i, g, r, v, "#abcdef"));
        assert_eq!(session.editor_document(), Some(&expected));
    }

    #[test]
    fn p5b_name_only_edits_are_dirty_and_legacy_actions_cannot_drop_the_draft() {
        let mut session = session();
        let document = session.editor_document().unwrap().clone();
        let (i, _, _, _) = binding(&session);
        let revision = session.draft_revision();
        let token = session.session_token();
        assert!(session.designer_set_name(i, "Changed name".into()));
        assert!(session.designer_dirty());
        assert_eq!(session.editor_document(), Some(&document));
        assert!(session.draft_revision() > revision);
        assert!(session.session_token() > token);
        let current = session.clone();
        let revision = session.draft_revision();
        let token = session.session_token();
        assert!(!session.apply(revision, token));
        assert!(!session.cancel(token));
        assert!(!session.use_saved_appearance(token));
        assert!(!session.start_role_editor(revision, token));
        assert!(!session.edit_draft(session.raw_context(), "queued raw edit".into()));
        session.cancel_transient();
        assert_eq!(session, current);
        session.on_departure();
        assert_eq!(session.designer_name(), "Changed name");
        assert!(session.designer_dirty());
        session.designer_saved(4, "Changed name".into());
        assert!(!session.designer_dirty());
        let i = session.editor_instance().unwrap();
        assert!(session.designer_set_name(i, "Another name".into()));
        assert!(session.designer_reset(i));
        assert_eq!(session.designer_name(), "Changed name");
        assert!(!session.designer_dirty());
    }

    #[test]
    fn p5c_wheel_keeps_hue_through_black_and_rejects_stale_or_nonfinite_input() {
        use crate::donor_folio::source_ui::wheel::WheelAction;
        let mut session = session();
        let (i, g, r, v) = binding(&session);
        assert!(session.designer_wheel(i, g, r, v, WheelAction::Hue(480.0)));
        assert_eq!(session.designer_hsv().0, 120.0);
        assert!(session.designer_wheel(i, g, r, v, WheelAction::SV(0.0, 0.0)));
        assert_eq!(session.editor_document().unwrap().color(r, v), "#000000");
        assert!(session.designer_wheel(i, g, r, v, WheelAction::SV(1.0, 1.0)));
        assert_eq!(session.editor_document().unwrap().color(r, v), "#00ff00");
        assert!(session.designer_wheel(i, g, r, v, WheelAction::DragEnd));
        assert_eq!(session.designer_recent(), ["#00ff00"]);
        let before = session.clone();
        assert!(!session.designer_wheel(i, g, r, v, WheelAction::Hue(f32::NAN)));
        assert!(!session.designer_wheel(i, g, r, v, WheelAction::SV(f32::INFINITY, 0.0)));
        assert_eq!(session, before);
        session.editor_select_role(i, ThemeRole::Surface);
        let before = session.clone();
        assert!(!session.designer_wheel(i, g, r, v, WheelAction::Hue(90.0)));
        assert_eq!(session, before);
    }
}
