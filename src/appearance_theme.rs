//! Pure ownership for the Settings > Theme visual draft.
//!
//! The designer extension retains a complete document, name, loaded identity,
//! separate saved/last-load baselines, hex input, recent colors and HSV state.
//! Hide/departure rotates widget bindings while preserving that visual draft.
//! Application appearance is owned separately by CandidateState's persisted
//! applied snapshot and the existing application publication owner. Loading or
//! editing this session never changes that snapshot or a legacy appearance.
//!
//! Nothing here touches files, tasks, providers or native windows. Internal
//! strict-codec audition fixtures remain available without a raw JSON UI or
//! transfer action. EffectiveThemeColors projects only the existing nine donor
//! roles; all other document cells remain editable and preserved.

use serde_json::Value;

use crate::donor_ui;
use crate::theme_roles::{import_document, ColorVariant, ThemeDocument, ThemeRole};

#[path = "appearance_theme/designer.rs"]
mod designer;

/// A committed session selection: the whole document, never a flattened
/// subset, plus the variant the user applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeSelection {
    document: ThemeDocument,
    variant: ColorVariant,
}

impl ThemeSelection {
    pub fn document(&self) -> &ThemeDocument {
        &self.document
    }

    pub fn variant(&self) -> ColorVariant {
        self.variant
    }
}

/// A validated preview of the current draft. The preview is bound to
/// the draft revision it was auditioned from and to the session token
/// issued at installation, so a stale Apply captured from an earlier
/// preview (for example Preview dark followed by Preview light of the
/// same text) can never promote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePreview {
    document: ThemeDocument,
    variant: ColorVariant,
    draft_revision: u64,
    token: u64,
}

impl ThemePreview {
    pub fn document(&self) -> &ThemeDocument {
        &self.document
    }

    pub fn variant(&self) -> ColorVariant {
        self.variant
    }

    pub fn draft_revision(&self) -> u64 {
        self.draft_revision
    }

    pub fn token(&self) -> u64 {
        self.token
    }
}

/// The nine source-established projected colors of one document
/// variant, as linear `[f32; 3]` channel values in `0.0..=1.0`
/// (channel byte / 255.0, alpha handled by each existing consumer).
/// Field-to-role binding is exactly the upstream UI projection:
/// primary, on_primary, tertiary, surface, on_surface,
/// surface_variant, surface_container, background, outline. No other
/// role is projected, aliased or inferred here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectiveThemeColors {
    pub primary: [f32; 3],
    pub primary_text: [f32; 3],
    pub tertiary: [f32; 3],
    pub surface: [f32; 3],
    pub surface_text: [f32; 3],
    pub surface_variant: [f32; 3],
    pub surface_container: [f32; 3],
    pub background: [f32; 3],
    pub outline: [f32; 3],
}

impl EffectiveThemeColors {
    /// Project a document at an explicit variant by assigning all nine
    /// fields directly from the document's own values. The legacy
    /// derived palette (`Palette::from_rgb`) is never run on document
    /// colors, so nothing is mixed or re-derived.
    pub fn from_document(document: &ThemeDocument, variant: ColorVariant) -> Self {
        let channel = |role: ThemeRole| {
            let bytes = document.role_colors(role).rgb_bytes(variant);
            [
                bytes[0] as f32 / 255.0,
                bytes[1] as f32 / 255.0,
                bytes[2] as f32 / 255.0,
            ]
        };
        Self {
            primary: channel(ThemeRole::Primary),
            primary_text: channel(ThemeRole::OnPrimary),
            tertiary: channel(ThemeRole::Tertiary),
            surface: channel(ThemeRole::Surface),
            surface_text: channel(ThemeRole::OnSurface),
            surface_variant: channel(ThemeRole::SurfaceVariant),
            surface_container: channel(ThemeRole::SurfaceContainer),
            background: channel(ThemeRole::Background),
            outline: channel(ThemeRole::Outline),
        }
    }

    /// Build the production donor palette by assigning all nine
    /// independent fields exactly.
    pub fn palette(&self) -> donor_ui::Palette {
        let color = |rgb: [f32; 3]| iced::Color::from_rgb(rgb[0], rgb[1], rgb[2]);
        donor_ui::Palette {
            primary: color(self.primary),
            primary_text: color(self.primary_text),
            surface: color(self.surface),
            surface_text: color(self.surface_text),
            surface_variant: color(self.surface_variant),
            surface_container: color(self.surface_container),
            background: color(self.background),
            outline: color(self.outline),
            tertiary: color(self.tertiary),
        }
    }
}

/// What an audition attempt did. `Stale` means the message carried a
/// draft revision or session token that is no longer current and was
/// ignored without changing anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditionOutcome {
    Installed,
    Failed,
    Stale,
    /// Role-editor preview refused because unset (pending) hex input
    /// exists. Nothing was mutated except the editor field diagnostic.
    Blocked,
}

/// In-memory Settings > Theme session state. All fields are private;
/// transitions go through the revision/token-guarded methods below so
/// the lifecycle invariants cannot be bypassed by callers.
/// State of the in-session role editor: the second canonical draft
/// mode of [`ThemeSession`]. While an editor exists, the session's
/// draft authority is this typed document, never the raw text.
///
/// `loaded` is the immutable baseline captured when the editor
/// started; it never moves during the editor's lifetime. `buffer` is
/// the exact pending hex text for the selected role/variant; it is
/// *unset* (`pending`) from the moment it is typed until Set color,
/// a role/variant switch, or Reset discards/submits it. Callbacks
/// bind to (`instance`, `selection_generation`, role, variant):
/// `instance` identifies this editor, `selection_generation` rotates
/// whenever the buffer's input stream ends (switch, successful Set,
/// effective Reset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleEditorState {
    instance: u64,
    document: ThemeDocument,
    loaded: ThemeDocument,
    selected_role: ThemeRole,
    selected_variant: ColorVariant,
    selection_generation: u64,
    filter: String,
    buffer: String,
    pending: bool,
    field_diagnostic: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeSession {
    committed: Option<ThemeSelection>,
    preview: Option<ThemePreview>,
    draft_text: String,
    draft_revision: u64,
    session_token: u64,
    diagnostic: Option<String>,
    ignored_roles: Vec<String>,
    editor: Option<RoleEditorState>,
    next_editor_instance: u64,
    next_selection_generation: u64,
    raw_context: u64,
    designer: Option<designer::DesignerState>,
}

impl ThemeSession {
    /// A fresh session: no document, no preview, empty draft. Matches
    /// application start and restart (nothing is restored from disk).
    pub fn new() -> Self {
        Self::default()
    }

    pub fn draft_text(&self) -> &str {
        &self.draft_text
    }

    pub fn draft_revision(&self) -> u64 {
        self.draft_revision
    }

    pub fn session_token(&self) -> u64 {
        self.session_token
    }

    /// The current raw-input context identity. Raw draft callbacks
    /// carry it; it rotates on every mode change and on
    /// Apply/Cancel/Use saved/departure, so a queued raw edit from an
    /// ended context cannot overwrite a newer draft or editor.
    pub fn raw_context(&self) -> u64 {
        self.raw_context
    }

    fn rotate_raw_context(&mut self) {
        self.raw_context = self.raw_context.wrapping_add(1);
    }

    /// True while the role editor is the session's draft mode.
    pub fn editor_active(&self) -> bool {
        self.editor.is_some()
    }

    pub fn editor_instance(&self) -> Option<u64> {
        self.editor.as_ref().map(|editor| editor.instance)
    }

    pub fn editor_selection_generation(&self) -> Option<u64> {
        self.editor
            .as_ref()
            .map(|editor| editor.selection_generation)
    }

    pub fn editor_selected_role(&self) -> Option<ThemeRole> {
        self.editor.as_ref().map(|editor| editor.selected_role)
    }

    pub fn editor_selected_variant(&self) -> Option<ColorVariant> {
        self.editor.as_ref().map(|editor| editor.selected_variant)
    }

    pub fn editor_buffer(&self) -> Option<&str> {
        self.editor.as_ref().map(|editor| editor.buffer.as_str())
    }

    /// True while typed hex text has not been Set/discarded yet.
    pub fn editor_pending(&self) -> bool {
        self.editor.as_ref().is_some_and(|editor| editor.pending)
    }

    pub fn editor_field_diagnostic(&self) -> Option<&str> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.field_diagnostic.as_deref())
    }

    pub fn editor_filter(&self) -> Option<&str> {
        self.editor.as_ref().map(|editor| editor.filter.as_str())
    }

    /// The editor's mutable draft document (never the preview or
    /// committed snapshot).
    pub fn editor_document(&self) -> Option<&ThemeDocument> {
        self.editor.as_ref().map(|editor| &editor.document)
    }

    /// One value from the editor's immutable loaded baseline.
    pub fn editor_loaded_color(&self, role: ThemeRole, variant: ColorVariant) -> Option<&str> {
        self.editor
            .as_ref()
            .map(|editor| editor.loaded.color(role, variant))
    }

    /// Internal document-codec serialization, derived on demand. No user-facing
    /// raw entry or transfer action consumes it.
    pub fn editor_serialized_json(&self) -> Option<String> {
        self.editor.as_ref().map(|editor| {
            serde_json::to_string(&editor.document.to_value())
                .expect("a validated theme document always serializes")
        })
    }

    /// The diagnostic from the last failed audition, if any. Cleared by
    /// editing, by a successful audition, Apply, Cancel and Use saved
    /// appearance.
    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }

    /// Sorted unknown role names reported by the last successful
    /// audition. Empty when the last audition reported none, or after
    /// any edit/failure/cleanup.
    pub fn ignored_roles(&self) -> &[String] {
        &self.ignored_roles
    }

    pub fn committed_selection(&self) -> Option<&ThemeSelection> {
        self.committed.as_ref()
    }

    pub fn preview(&self) -> Option<&ThemePreview> {
        self.preview.as_ref()
    }

    pub fn has_preview(&self) -> bool {
        self.preview.is_some()
    }

    /// True while a preview or a committed document exists. This is the
    /// coexistence predicate: legacy preset/color editing is disabled
    /// and guarded while it holds.
    pub fn has_document_selection(&self) -> bool {
        self.preview.is_some() || self.committed.is_some()
    }

    /// The effective document and variant: the validated current
    /// preview first, then the committed selection.
    pub fn effective_selection(&self) -> Option<(&ThemeDocument, ColorVariant)> {
        if let Some(preview) = &self.preview {
            return Some((&preview.document, preview.variant));
        }
        self.committed
            .as_ref()
            .map(|selection| (&selection.document, selection.variant))
    }

    /// The nine projected colors of the effective selection, if any.
    pub fn effective_colors(&self) -> Option<EffectiveThemeColors> {
        self.effective_selection()
            .map(|(document, variant)| EffectiveThemeColors::from_document(document, variant))
    }

    fn advance_revision(&mut self) {
        self.draft_revision = self.draft_revision.wrapping_add(1);
    }

    fn advance_token(&mut self) {
        self.session_token = self.session_token.wrapping_add(1);
    }

    fn clear_transient(&mut self) {
        self.preview = None;
        self.draft_text.clear();
        self.diagnostic = None;
        self.ignored_roles.clear();
        self.editor = None;
    }

    /// Replace the raw draft text (one delivered edit event).
    ///
    /// The event must carry the session's current raw-input context:
    /// a queued edit from an ended context (mode change, Apply,
    /// Cancel, Use saved, departure) is rejected and changes nothing.
    /// Raw edits are also rejected while the role editor owns the
    /// draft. Within the current context, successive edits are always
    /// accepted in delivery order; staleness is enforced on
    /// Preview/Apply, not by dropping text. An accepted edit advances
    /// the draft revision and session token, discards any preview and
    /// the now-obsolete diagnostic/report, and keeps the committed
    /// selection untouched.
    pub fn edit_draft(&mut self, raw_context: u64, text: String) -> bool {
        if raw_context != self.raw_context || self.editor.is_some() || self.designer.is_some() {
            return false;
        }
        self.draft_text = text;
        self.advance_revision();
        self.advance_token();
        self.preview = None;
        self.diagnostic = None;
        self.ignored_roles.clear();
        true
    }

    /// Audition the current draft at an explicit variant.
    ///
    /// The caller supplies the draft revision and session token the
    /// Preview control was rendered with; a mismatch makes the attempt
    /// [`AuditionOutcome::Stale`] and changes nothing. Otherwise the
    /// draft is parsed as JSON and the whole document is validated
    /// atomically: on success a complete preview (both variants
    /// retained) is installed under a fresh session token and the
    /// ignored-role report is stored; on any syntax or import failure
    /// the previous preview is cleared, the draft is kept for
    /// correction, and the diagnostic names the failure. The committed
    /// selection is never touched by an audition.
    pub fn audition(
        &mut self,
        draft_revision: u64,
        session_token: u64,
        variant: ColorVariant,
    ) -> AuditionOutcome {
        if draft_revision != self.draft_revision || session_token != self.session_token {
            return AuditionOutcome::Stale;
        }
        if self.editor.is_some() {
            // The raw draft is not the draft authority in role mode;
            // role-mode previews go through `editor_preview`.
            return AuditionOutcome::Stale;
        }
        let text = self.draft_text.clone();
        self.audition_text(&text, variant)
    }

    /// Shared atomic validation/install tail for raw auditions and
    /// role-editor previews: parse `text`, validate the whole
    /// document, install or clear the preview exactly as documented
    /// on [`ThemeSession::audition`].
    fn audition_text(&mut self, text: &str, variant: ColorVariant) -> AuditionOutcome {
        let parsed = serde_json::from_str::<Value>(text);
        let outcome = match parsed {
            Ok(value) => import_document(&value).map_err(|error| error.to_string()),
            Err(error) => Err(format!("invalid JSON: {error}")),
        };
        match outcome {
            Ok(imported) => {
                self.advance_token();
                self.preview = Some(ThemePreview {
                    document: imported.document,
                    variant,
                    draft_revision: self.draft_revision,
                    token: self.session_token,
                });
                self.diagnostic = None;
                self.ignored_roles = imported.ignored_roles;
                AuditionOutcome::Installed
            }
            Err(message) => {
                self.preview = None;
                self.diagnostic = Some(message);
                self.ignored_roles.clear();
                self.advance_token();
                AuditionOutcome::Failed
            }
        }
    }

    /// Promote the current validated preview to the committed
    /// selection. Both the draft revision and the installed preview
    /// token must match the values the Apply control was rendered
    /// with; anything else (including a second queued Apply, or an
    /// Apply captured from an earlier preview of the same text at the
    /// other variant) is a harmless no-op returning `false`.
    pub fn apply(&mut self, draft_revision: u64, preview_token: u64) -> bool {
        if self.designer.is_some() {
            return false;
        }
        if self.editor_pending() {
            // A valid-but-unset (or invalid) editor buffer blocks
            // Apply exactly as it blocks Preview.
            return false;
        }
        let Some(preview) = &self.preview else {
            return false;
        };
        if preview.draft_revision != draft_revision || preview.token != preview_token {
            return false;
        }
        let preview = preview.clone();
        self.committed = Some(ThemeSelection {
            document: preview.document,
            variant: preview.variant,
        });
        self.clear_transient();
        self.rotate_raw_context();
        self.advance_revision();
        self.advance_token();
        true
    }

    /// Cancel transient audition state, keeping the committed
    /// selection. Guarded by the current session token so a stale
    /// queued Cancel cannot clear a newer preview.
    pub fn cancel(&mut self, session_token: u64) -> bool {
        if session_token != self.session_token || self.designer.is_some() {
            return false;
        }
        self.cancel_transient();
        self.rotate_raw_context();
        true
    }

    /// The departure/hide form of Cancel: clears preview, draft,
    /// diagnostic and report while preserving the committed selection.
    /// A no-op when the session is already clean, so lifecycle routes
    /// may call it unconditionally when the Theme owner goes away.
    pub fn cancel_transient(&mut self) {
        if self.designer.is_some() {
            return;
        }
        if self.preview.is_none()
            && self.draft_text.is_empty()
            && self.diagnostic.is_none()
            && self.ignored_roles.is_empty()
            && self.editor.is_none()
        {
            return;
        }
        self.clear_transient();
        self.advance_revision();
        self.advance_token();
    }

    /// End widget ownership unconditionally. Visual drafts, including invalid
    /// input, survive while editor/selection/revision/token identities rotate.
    /// The internal codec-only fixture mode retains its historical cleanup;
    /// neither path can change the application's persisted applied snapshot.
    pub fn on_departure(&mut self) {
        if self.designer.is_some() {
            self.invalidate_designer_bindings();
            return;
        }
        self.clear_transient();
        self.rotate_raw_context();
        self.advance_revision();
        self.advance_token();
    }

    /// Use saved appearance: explicitly drop the committed selection
    /// along with all transient state, so consumers fall back to the
    /// current persisted legacy fields. Guarded by the current session
    /// token. Nothing is saved and no document is synthesized.
    pub fn use_saved_appearance(&mut self, session_token: u64) -> bool {
        if session_token != self.session_token || self.designer.is_some() {
            return false;
        }
        self.committed = None;
        self.clear_transient();
        self.rotate_raw_context();
        self.advance_revision();
        self.advance_token();
        true
    }

    /// Start the role editor from a current validated preview, or
    /// from the committed selection when the raw draft is empty. The
    /// caller supplies the draft revision and session token the Edit
    /// roles action was rendered with; a stale binding (captured
    /// before a departure, a document replacement, a mode transfer or
    /// a later validated document) is rejected before any mutation,
    /// so a delayed Start can never open the editor over a context it
    /// was not rendered for. The
    /// source document is captured as both the editable draft and the
    /// immutable loaded baseline; selection starts at Primary in the
    /// source's explicit variant, the buffer at its stored value.
    /// Starting advances revision/token once, clears the import
    /// diagnostic/report, rotates the raw-input context (mode change)
    /// and rebinds a surviving preview to the new revision/token so
    /// it stays applicable. No document value changes. Returns false
    /// when neither source exists (including a nonempty invalid raw
    /// draft over a committed selection: no silent fallback).
    pub fn start_role_editor(&mut self, draft_revision: u64, session_token: u64) -> bool {
        if draft_revision != self.draft_revision
            || session_token != self.session_token
            || self.designer.is_some()
        {
            return false;
        }
        let (document, variant, from_preview) = if let Some(preview) = &self.preview {
            (preview.document.clone(), preview.variant, true)
        } else if self.draft_text.is_empty() {
            match &self.committed {
                Some(selection) => (selection.document.clone(), selection.variant, false),
                None => return false,
            }
        } else {
            return false;
        };
        self.advance_revision();
        self.advance_token();
        if from_preview {
            let (revision, token) = (self.draft_revision, self.session_token);
            let preview = self.preview.as_mut().expect("preview checked above");
            preview.draft_revision = revision;
            preview.token = token;
        }
        self.next_editor_instance = self.next_editor_instance.wrapping_add(1);
        self.next_selection_generation = self.next_selection_generation.wrapping_add(1);
        let buffer = document.color(ThemeRole::Primary, variant).to_string();
        self.editor = Some(RoleEditorState {
            instance: self.next_editor_instance,
            document: document.clone(),
            loaded: document,
            selected_role: ThemeRole::Primary,
            selected_variant: variant,
            selection_generation: self.next_selection_generation,
            filter: String::new(),
            buffer,
            pending: false,
            field_diagnostic: None,
        });
        self.diagnostic = None;
        self.ignored_roles.clear();
        self.rotate_raw_context();
        true
    }

    /// Validate an editor callback's full binding (instance +
    /// selection generation + typed role + variant). Returns the
    /// editor mutably when the binding is current.
    fn editor_for(
        &mut self,
        instance: u64,
        generation: u64,
        role: ThemeRole,
        variant: ColorVariant,
    ) -> Option<&mut RoleEditorState> {
        let editor = self.editor.as_mut()?;
        if editor.instance != instance
            || editor.selection_generation != generation
            || editor.selected_role != role
            || editor.selected_variant != variant
        {
            return None;
        }
        Some(editor)
    }

    fn rotate_selection_generation(&mut self) {
        self.next_selection_generation = self.next_selection_generation.wrapping_add(1);
        let generation = self.next_selection_generation;
        if let Some(editor) = &mut self.editor {
            editor.selection_generation = generation;
        }
    }

    /// One delivered hex-text event for the selected role/variant.
    /// A different value is stored exactly, marked unset, diagnosed
    /// when it is not a complete valid color, and clears the preview
    /// while advancing revision/token — without touching the editor
    /// document. The same value is a no-op. Stale bindings change
    /// nothing at all.
    pub fn editor_type_text(
        &mut self,
        instance: u64,
        generation: u64,
        role: ThemeRole,
        variant: ColorVariant,
        text: String,
    ) -> bool {
        let Some(editor) = self.editor_for(instance, generation, role, variant) else {
            return false;
        };
        if text == editor.buffer {
            return true;
        }
        editor.buffer = text;
        editor.pending = true;
        editor.field_diagnostic = if crate::theme_roles::normalize_hex_color(&editor.buffer)
            .is_some()
        {
            None
        } else {
            Some("Enter six hexadecimal digits (RRGGBB), with an optional leading #.".to_string())
        };
        self.preview = None;
        self.diagnostic = None;
        self.ignored_roles.clear();
        self.advance_revision();
        self.advance_token();
        true
    }

    /// Set color / Enter: submit the pending buffer for the selected
    /// role/variant through the shared RGB normalization. A valid
    /// value replaces exactly that one cell, normalizes the buffer,
    /// clears the field diagnostic, advances revision/token once and
    /// rotates the selection generation, ending the pre-Set input
    /// stream. An invalid value keeps text and diagnostic and changes
    /// no document or counter. An unsettled-free buffer (nothing
    /// pending, or a pending value that normalizes to the stored
    /// color) clears pending and normalizes the buffer text; the
    /// equivalent-color case still rotates the selection generation,
    /// so the pre-Set input stream ends even though the document,
    /// draft revision and session token do not change.
    pub fn editor_set_color(
        &mut self,
        instance: u64,
        generation: u64,
        role: ThemeRole,
        variant: ColorVariant,
    ) -> bool {
        let Some(editor) = self.editor_for(instance, generation, role, variant) else {
            return false;
        };
        if !editor.pending {
            return true;
        }
        let Some(normalized) = crate::theme_roles::normalize_hex_color(&editor.buffer) else {
            editor.field_diagnostic = Some(
                "Enter six hexadecimal digits (RRGGBB), with an optional leading #.".to_string(),
            );
            return true;
        };
        if normalized == editor.document.color(role, variant) {
            editor.buffer = normalized;
            editor.pending = false;
            editor.field_diagnostic = None;
            self.rotate_selection_generation();
            return true;
        }
        editor.document = editor.document.replaced(role, variant, &normalized);
        editor.buffer = normalized;
        editor.pending = false;
        editor.field_diagnostic = None;
        self.rotate_selection_generation();
        self.preview = None;
        self.advance_revision();
        self.advance_token();
        true
    }

    /// Select a different role: discard any unset buffer, initialize
    /// the buffer from the newly selected stored value and rotate
    /// the selection generation. Document, revision and token are
    /// unchanged; selecting the current role is a no-op.
    pub fn editor_select_role(&mut self, instance: u64, role: ThemeRole) -> bool {
        let Some(editor) = &mut self.editor else {
            return false;
        };
        if editor.instance != instance {
            return false;
        }
        if editor.selected_role == role {
            return true;
        }
        editor.selected_role = role;
        editor.buffer = editor
            .document
            .color(role, editor.selected_variant)
            .to_string();
        editor.pending = false;
        editor.field_diagnostic = None;
        self.rotate_selection_generation();
        self.sync_designer_hsv();
        true
    }

    /// Select the other editing variant: same unset-buffer discard
    /// and generation rotation as a role switch. This never changes
    /// the global preview variant or any document value.
    pub fn editor_select_variant(&mut self, instance: u64, variant: ColorVariant) -> bool {
        let Some(editor) = &mut self.editor else {
            return false;
        };
        if editor.instance != instance {
            return false;
        }
        if editor.selected_variant == variant {
            return true;
        }
        editor.selected_variant = variant;
        editor.buffer = editor
            .document
            .color(editor.selected_role, variant)
            .to_string();
        editor.pending = false;
        editor.field_diagnostic = None;
        self.rotate_selection_generation();
        self.sync_designer_hsv();
        true
    }

    /// Filter input: navigation only. No document, counter, selection
    /// or generation change.
    pub fn editor_set_filter(&mut self, instance: u64, text: String) -> bool {
        let Some(editor) = &mut self.editor else {
            return false;
        };
        if editor.instance != instance {
            return false;
        }
        editor.filter = text;
        true
    }

    /// Reset color to loaded value: restore exactly the selected
    /// role/variant from the immutable loaded baseline and discard
    /// its pending text. An effective reset advances revision/token
    /// once, clears the preview and rotates the selection generation.
    /// Pending text or a field diagnostic makes the reset effective
    /// even when the buffer already reads as the baseline value:
    /// retyping the loaded text without Set leaves the cell unsettled,
    /// and only an effective reset ends that input stream and
    /// unblocks Preview. A reset is a no-op only when the stored
    /// value and the buffer both equal the baseline AND nothing is
    /// pending and no field diagnostic is showing.
    pub fn editor_reset_color(
        &mut self,
        instance: u64,
        generation: u64,
        role: ThemeRole,
        variant: ColorVariant,
    ) -> bool {
        let Some(editor) = self.editor_for(instance, generation, role, variant) else {
            return false;
        };
        let baseline = editor.loaded.color(role, variant).to_string();
        let stored = editor.document.color(role, variant).to_string();
        if stored == baseline
            && editor.buffer == baseline
            && !editor.pending
            && editor.field_diagnostic.is_none()
        {
            return true;
        }
        if stored != baseline {
            editor.document = editor.document.replaced(role, variant, &baseline);
        }
        editor.buffer = baseline;
        editor.pending = false;
        editor.field_diagnostic = None;
        self.rotate_selection_generation();
        self.preview = None;
        self.advance_revision();
        self.advance_token();
        self.sync_designer_hsv();
        true
    }

    /// Preview from role mode: bound to the current draft revision
    /// and session token like a raw audition. Unset (pending) hex
    /// input blocks the preview with a field diagnostic and no other
    /// mutation. Otherwise the editor's serialized full document is
    /// validated atomically through the same import/install path as a
    /// raw audition and installed at the requested variant; the
    /// editor and its document are untouched by previewing.
    pub fn editor_preview(
        &mut self,
        draft_revision: u64,
        session_token: u64,
        variant: ColorVariant,
    ) -> AuditionOutcome {
        if draft_revision != self.draft_revision || session_token != self.session_token {
            return AuditionOutcome::Stale;
        }
        let Some(editor) = &self.editor else {
            return AuditionOutcome::Stale;
        };
        if editor.pending {
            self.editor
                .as_mut()
                .expect("editor checked above")
                .field_diagnostic = Some(
                "Set the pending color before previewing, or switch role or variant or reset                  to discard it."
                    .to_string(),
            );
            return AuditionOutcome::Blocked;
        }
        let text = self
            .editor_serialized_json()
            .expect("editor exists, so it serializes");
        self.audition_text(&text, variant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme_roles::{import_document, ThemeRole, ROLE_KEYS};

    /// The 50 donor keys, transcribed independently in this test module
    /// (same list as the pilot harness oracle, not read back out of the
    /// session implementation).
    const EXPECTED_KEYS: [&str; 50] = [
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

    /// Sentinel component values from the verification plan's example
    /// formula: dark index i uses decimal (10+i, 70+i, 130+i), light
    /// uses (160+i, 210-i, 60+i), each rendered as two-digit hex. All
    /// 100 strings are distinguishable.
    fn sentinel_dark(index: usize) -> String {
        format!("#{:02x}{:02x}{:02x}", 10 + index, 70 + index, 130 + index)
    }

    fn sentinel_light(index: usize) -> String {
        format!("#{:02x}{:02x}{:02x}", 160 + index, 210 - index, 60 + index)
    }

    fn sentinel_rgb(index: usize, variant: ColorVariant) -> [f32; 3] {
        let (r, g, b) = match variant {
            ColorVariant::Dark => (10 + index, 70 + index, 130 + index),
            ColorVariant::Light => (160 + index, 210 - index, 60 + index),
        };
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
    }

    /// Type raw text through the session's current raw context, the
    /// way the production view does.
    fn type_raw(session: &mut ThemeSession, text: String) -> bool {
        let context = session.raw_context();
        session.edit_draft(context, text)
    }

    fn sentinel_text() -> String {
        let mut colors = serde_json::Map::new();
        for (index, key) in EXPECTED_KEYS.iter().enumerate() {
            let mut role_node = serde_json::Map::new();
            role_node.insert(
                "dark".to_string(),
                serde_json::json!({ "color": sentinel_dark(index) }),
            );
            role_node.insert(
                "light".to_string(),
                serde_json::json!({ "color": sentinel_light(index) }),
            );
            colors.insert((*key).to_string(), Value::Object(role_node));
        }
        let mut root = serde_json::Map::new();
        root.insert("colors".to_string(), Value::Object(colors));
        Value::Object(root).to_string()
    }

    fn key_index(key: &str) -> usize {
        EXPECTED_KEYS.iter().position(|k| *k == key).unwrap()
    }

    /// Drive a session to a successful audition of the sentinel text.
    fn auditioned(variant: ColorVariant) -> ThemeSession {
        let mut session = ThemeSession::new();
        type_raw(&mut session, sentinel_text());
        let outcome = session.audition(session.draft_revision(), session.session_token(), variant);
        assert_eq!(outcome, AuditionOutcome::Installed);
        session
    }

    /// Drive a session to a committed sentinel selection.
    fn committed(variant: ColorVariant) -> ThemeSession {
        let mut session = auditioned(variant);
        let preview = session.preview().unwrap();
        assert!(session.apply(preview.draft_revision(), preview.token()));
        session
    }

    #[test]
    fn theme_session_starts_empty_with_no_effective_selection() {
        let session = ThemeSession::new();
        assert_eq!(session.draft_text(), "");
        assert!(!session.has_preview());
        assert!(!session.has_document_selection());
        assert!(session.committed_selection().is_none());
        assert!(session.effective_selection().is_none());
        assert!(session.effective_colors().is_none());
        assert!(session.diagnostic().is_none());
        assert!(session.ignored_roles().is_empty());
    }

    #[test]
    fn theme_session_audition_installs_preview_retaining_all_100_strings() {
        assert_eq!(ROLE_KEYS, EXPECTED_KEYS);
        let session = auditioned(ColorVariant::Dark);
        let preview = session.preview().unwrap();
        assert_eq!(preview.variant(), ColorVariant::Dark);
        for (index, key) in EXPECTED_KEYS.iter().enumerate() {
            let role = ThemeRole::from_name(key).unwrap();
            assert_eq!(
                preview.document().color(role, ColorVariant::Dark),
                sentinel_dark(index),
                "{key} dark"
            );
            assert_eq!(
                preview.document().color(role, ColorVariant::Light),
                sentinel_light(index),
                "{key} light (unselected variant must be retained)"
            );
        }
        // The effective selection is the preview, whole document included.
        let (document, variant) = session.effective_selection().unwrap();
        assert_eq!(variant, ColorVariant::Dark);
        assert_eq!(document, preview.document());
    }

    #[test]
    fn theme_session_projection_assigns_exactly_the_nine_mapped_roles() {
        for variant in [ColorVariant::Dark, ColorVariant::Light] {
            let session = auditioned(variant);
            let colors = session.effective_colors().unwrap();
            let expect = |key: &str| sentinel_rgb(key_index(key), variant);
            assert_eq!(colors.primary, expect("primary"), "{variant:?} primary");
            assert_eq!(colors.primary_text, expect("on_primary"), "{variant:?}");
            assert_eq!(colors.tertiary, expect("tertiary"), "{variant:?}");
            assert_eq!(colors.surface, expect("surface"), "{variant:?}");
            assert_eq!(colors.surface_text, expect("on_surface"), "{variant:?}");
            assert_eq!(
                colors.surface_variant,
                expect("surface_variant"),
                "{variant:?}"
            );
            assert_eq!(
                colors.surface_container,
                expect("surface_container"),
                "{variant:?}"
            );
            assert_eq!(colors.background, expect("background"), "{variant:?}");
            assert_eq!(colors.outline, expect("outline"), "{variant:?}");
            // Deliberately unequal pairs must not be swapped or derived:
            // background != on_primary projection source, and the palette
            // built from the projection carries the same nine values.
            let palette = colors.palette();
            assert_eq!(
                crate::donor_ui::rgb_components(palette.background),
                expect("background")
            );
            assert_eq!(
                crate::donor_ui::rgb_components(palette.primary_text),
                expect("on_primary")
            );
            assert_ne!(expect("background"), expect("on_primary"));
            // Unmapped roles stay out of the projection: mutating none of
            // them could change these fields, and the document still
            // holds them verbatim for both variants.
            let document = session.effective_selection().unwrap().0;
            for key in ["error", "scrim", "shadow", "source_color", "secondary"] {
                let role = ThemeRole::from_name(key).unwrap();
                assert_eq!(
                    document.color(role, variant),
                    match variant {
                        ColorVariant::Dark => sentinel_dark(key_index(key)),
                        ColorVariant::Light => sentinel_light(key_index(key)),
                    },
                    "{key} retained"
                );
            }
        }
    }

    #[test]
    fn theme_session_edit_invalidates_preview_and_keeps_committed() {
        let mut session = committed(ColorVariant::Dark);
        type_raw(&mut session, sentinel_text());
        assert_eq!(
            session.audition(
                session.draft_revision(),
                session.session_token(),
                ColorVariant::Light,
            ),
            AuditionOutcome::Installed
        );
        assert!(session.has_preview());
        let revision_before = session.draft_revision();
        let token_before = session.session_token();

        type_raw(&mut session, "{\"colors\":".to_string());
        assert!(!session.has_preview(), "edit must discard the preview");
        assert!(session.diagnostic().is_none());
        assert!(session.ignored_roles().is_empty());
        assert!(session.draft_revision() > revision_before);
        assert!(session.session_token() > token_before);
        // Committed selection is still effective immediately after edit.
        let (document, variant) = session.effective_selection().unwrap();
        assert_eq!(variant, ColorVariant::Dark);
        assert_eq!(
            document.color(ThemeRole::Primary, ColorVariant::Dark),
            sentinel_dark(0)
        );
    }

    #[test]
    fn theme_session_failed_audition_clears_preview_keeps_committed_and_draft() {
        let mut session = committed(ColorVariant::Light);
        // JSON syntax failure.
        type_raw(&mut session, "{not json".to_string());
        let outcome = session.audition(
            session.draft_revision(),
            session.session_token(),
            ColorVariant::Dark,
        );
        assert_eq!(outcome, AuditionOutcome::Failed);
        assert!(!session.has_preview());
        assert_eq!(
            session.draft_text(),
            "{not json",
            "draft kept for correction"
        );
        assert!(session.diagnostic().is_some());
        assert_eq!(
            session.committed_selection().unwrap().variant(),
            ColorVariant::Light,
            "committed selection survives a failed audition"
        );

        // Import failure: one role's color corrupted. The diagnostic
        // names the role and variant, and no preview is retained.
        let mut value: Value = serde_json::from_str(&sentinel_text()).unwrap();
        value["colors"]["tertiary_container"]["dark"]["color"] =
            Value::String("#zzzzzz".to_string());
        type_raw(&mut session, value.to_string());
        let outcome = session.audition(
            session.draft_revision(),
            session.session_token(),
            ColorVariant::Dark,
        );
        assert_eq!(outcome, AuditionOutcome::Failed);
        let diagnostic = session.diagnostic().unwrap();
        assert!(diagnostic.contains("tertiary_container"), "{diagnostic}");
        assert!(diagnostic.contains("dark"), "{diagnostic}");
        assert!(!session.has_preview());
        assert!(session.committed_selection().is_some());
    }

    #[test]
    fn theme_session_every_single_role_defect_fails_the_audition_atomically() {
        for key in EXPECTED_KEYS {
            for variant_name in ["dark", "light"] {
                let mut session = ThemeSession::new();
                let mut value: Value = serde_json::from_str(&sentinel_text()).unwrap();
                value["colors"][key][variant_name]["color"] =
                    Value::String("not-a-color".to_string());
                type_raw(&mut session, value.to_string());
                let outcome = session.audition(
                    session.draft_revision(),
                    session.session_token(),
                    ColorVariant::Dark,
                );
                assert_eq!(outcome, AuditionOutcome::Failed, "{key}.{variant_name}");
                assert!(!session.has_preview(), "{key}.{variant_name}");
                assert!(
                    session.diagnostic().unwrap().contains(key),
                    "{key}.{variant_name} diagnostic must name the role"
                );
            }
        }
    }

    #[test]
    fn theme_session_stale_audition_and_stale_dark_apply_are_rejected() {
        let mut session = ThemeSession::new();
        type_raw(&mut session, sentinel_text());
        // A Preview carrying an old revision/token changes nothing.
        let outcome = session.audition(0, 0, ColorVariant::Dark);
        assert_eq!(outcome, AuditionOutcome::Stale);
        assert!(!session.has_preview());

        // Preview dark, then Preview light of the *same unchanged text*:
        // the light audition installs under a fresh token, and the Apply
        // captured from the dark preview must not commit.
        let revision = session.draft_revision();
        let dark_token = session.session_token();
        assert_eq!(
            session.audition(revision, dark_token, ColorVariant::Dark),
            AuditionOutcome::Installed
        );
        let dark_preview_token = session.preview().unwrap().token();
        let current_token = session.session_token();
        assert_eq!(
            session.audition(revision, current_token, ColorVariant::Light),
            AuditionOutcome::Installed,
            "same-text variant change re-auditions at the same revision"
        );
        let light_preview_token = session.preview().unwrap().token();
        assert_ne!(dark_preview_token, light_preview_token);
        assert!(
            !session.apply(revision, dark_preview_token),
            "stale dark Apply must not commit the light preview"
        );
        assert!(session.committed_selection().is_none());
        assert!(
            session.apply(revision, light_preview_token),
            "the current preview's Apply commits"
        );
        assert_eq!(
            session.committed_selection().unwrap().variant(),
            ColorVariant::Light
        );
    }

    #[test]
    fn theme_session_apply_promotes_whole_document_and_second_apply_is_noop() {
        let mut session = auditioned(ColorVariant::Dark);
        let preview = session.preview().unwrap();
        let (revision, token) = (preview.draft_revision(), preview.token());
        assert!(session.apply(revision, token));
        assert!(!session.has_preview());
        assert_eq!(session.draft_text(), "", "draft cleared after Apply");
        assert!(session.diagnostic().is_none());
        let selection = session.committed_selection().unwrap();
        assert_eq!(selection.variant(), ColorVariant::Dark);
        // Whole-document retention through Apply and serialization.
        let round_tripped = import_document(&selection.document().to_value()).unwrap();
        assert_eq!(&round_tripped.document, selection.document());
        assert_eq!(
            selection
                .document()
                .color(ThemeRole::SourceColor, ColorVariant::Light),
            sentinel_light(49)
        );
        // A second queued Apply with the same captured values is harmless.
        assert!(!session.apply(revision, token));
        assert!(session.committed_selection().is_some());
    }

    #[test]
    fn theme_session_apply_without_preview_is_a_noop() {
        let mut session = ThemeSession::new();
        assert!(!session.apply(0, 0));
        type_raw(&mut session, sentinel_text());
        assert!(
            !session.apply(session.draft_revision(), session.session_token()),
            "a draft that was never auditioned cannot be applied"
        );
        assert!(session.committed_selection().is_none());
    }

    #[test]
    fn theme_session_cancel_keeps_committed_and_stale_cancel_cannot_clear_newer_preview() {
        let mut session = committed(ColorVariant::Dark);
        type_raw(&mut session, sentinel_text());
        assert_eq!(
            session.audition(
                session.draft_revision(),
                session.session_token(),
                ColorVariant::Light,
            ),
            AuditionOutcome::Installed
        );
        let stale_token = session.session_token().wrapping_sub(1);
        assert!(
            !session.cancel(stale_token),
            "a stale Cancel must not clear the newer preview"
        );
        assert!(session.has_preview());
        assert!(session.cancel(session.session_token()));
        assert!(!session.has_preview());
        assert_eq!(session.draft_text(), "");
        assert_eq!(
            session.committed_selection().unwrap().variant(),
            ColorVariant::Dark,
            "Cancel keeps the committed selection"
        );
        // Cancel on an already-clean session is a no-op that still
        // succeeds at the current token and changes nothing.
        let revision = session.draft_revision();
        assert!(session.cancel(session.session_token()));
        assert_eq!(session.draft_revision(), revision);
    }

    #[test]
    fn theme_session_cancel_transient_preserves_committed_for_departure() {
        let mut session = committed(ColorVariant::Light);
        type_raw(&mut session, sentinel_text());
        assert_eq!(
            session.audition(
                session.draft_revision(),
                session.session_token(),
                ColorVariant::Dark,
            ),
            AuditionOutcome::Installed
        );
        session.cancel_transient();
        assert!(!session.has_preview());
        assert_eq!(session.draft_text(), "");
        assert_eq!(
            session.committed_selection().unwrap().variant(),
            ColorVariant::Light
        );
        // Repeated departure cleanup is harmless.
        session.cancel_transient();
        assert!(session.committed_selection().is_some());
    }

    #[test]
    fn theme_session_departure_invalidates_token_even_when_clean() {
        let mut session = committed(ColorVariant::Dark);
        // Clean committed state: no preview, draft, diagnostic or
        // ignored-role report is present.
        assert!(!session.has_preview());
        assert_eq!(session.draft_text(), "");
        assert!(session.diagnostic().is_none());
        assert!(session.ignored_roles().is_empty());
        let stale_token = session.session_token();
        session.on_departure();
        assert_ne!(session.session_token(), stale_token);
        // The committed selection survives, and a Use saved appearance
        // captured before the departure is stale and rejected.
        assert!(session.committed_selection().is_some());
        assert!(!session.use_saved_appearance(stale_token));
        assert!(session.committed_selection().is_some());
        // A callback carrying the current token still works.
        let current_token = session.session_token();
        assert!(session.use_saved_appearance(current_token));
        assert!(session.committed_selection().is_none());
    }

    #[test]
    fn theme_session_departure_clears_transient_and_preserves_committed() {
        let mut session = committed(ColorVariant::Light);
        type_raw(&mut session, sentinel_text());
        assert_eq!(
            session.audition(
                session.draft_revision(),
                session.session_token(),
                ColorVariant::Dark,
            ),
            AuditionOutcome::Installed
        );
        assert!(session.has_preview());
        let stale_token = session.session_token();
        session.on_departure();
        assert!(!session.has_preview());
        assert_eq!(session.draft_text(), "");
        assert_ne!(session.session_token(), stale_token);
        assert_eq!(
            session.committed_selection().unwrap().variant(),
            ColorVariant::Light
        );
    }

    #[test]
    fn theme_session_use_saved_appearance_clears_everything_and_requires_current_token() {
        let mut session = committed(ColorVariant::Dark);
        let stale_token = session.session_token().wrapping_sub(1);
        assert!(!session.use_saved_appearance(stale_token));
        assert!(session.has_document_selection());
        assert!(session.use_saved_appearance(session.session_token()));
        assert!(!session.has_document_selection());
        assert!(session.effective_selection().is_none());
        assert!(session.effective_colors().is_none());
        assert_eq!(session.draft_text(), "");
    }

    #[test]
    fn theme_session_ignored_roles_are_reported_sorted_after_audition() {
        let mut value: Value = serde_json::from_str(&sentinel_text()).unwrap();
        value["colors"]["zzz_extra"] = serde_json::json!({
            "dark": { "color": "#010203" },
            "light": { "color": "#040506" },
        });
        value["colors"]["aaa_extra"] = serde_json::json!({
            "dark": { "color": "#010203" },
            "light": { "color": "#040506" },
        });
        let mut session = ThemeSession::new();
        type_raw(&mut session, value.to_string());
        assert_eq!(
            session.audition(
                session.draft_revision(),
                session.session_token(),
                ColorVariant::Dark,
            ),
            AuditionOutcome::Installed
        );
        assert_eq!(
            session.ignored_roles(),
            ["aaa_extra".to_string(), "zzz_extra".to_string()]
        );
        // The retained document contains exactly the 50 known roles.
        assert_eq!(
            session.preview().unwrap().document().to_value()["colors"]
                .as_object()
                .unwrap()
                .len(),
            50
        );
    }

    #[test]
    fn theme_session_duplicate_raw_keys_collapse_without_detection_claims() {
        // serde_json collapses duplicate object keys before the importer
        // sees the value; the session inherits that documented behavior
        // (last value wins) and does not pretend to detect duplicates.
        let text = sentinel_text();
        let duplicated = text.replacen(
            &format!("\"primary\":{{\"dark\":{{\"color\":\"{}\"}}", sentinel_dark(0)),
            &format!(
                "\"primary\":{{\"dark\":{{\"color\":\"#000001\"}},\"light\":{{\"color\":\"#000002\"}}}},\"primary\":{{\"dark\":{{\"color\":\"{}\"}}",
                sentinel_dark(0)
            ),
            1,
        );
        assert_ne!(duplicated, text, "fixture must actually duplicate the key");
        let mut session = ThemeSession::new();
        type_raw(&mut session, duplicated);
        assert_eq!(
            session.audition(
                session.draft_revision(),
                session.session_token(),
                ColorVariant::Dark,
            ),
            AuditionOutcome::Installed
        );
        assert_eq!(
            session
                .preview()
                .unwrap()
                .document()
                .color(ThemeRole::Primary, ColorVariant::Dark),
            sentinel_dark(0),
            "the later duplicate wins under serde_json parsing"
        );
    }

    /// Start an editor on an auditioned session and return the
    /// current callback binding.
    fn editor_binding(session: &ThemeSession) -> (u64, u64, ThemeRole, ColorVariant) {
        (
            session.editor_instance().unwrap(),
            session.editor_selection_generation().unwrap(),
            session.editor_selected_role().unwrap(),
            session.editor_selected_variant().unwrap(),
        )
    }

    #[test]
    fn theme_role_editor_starts_from_preview_and_rebinds_it() {
        let mut session = auditioned(ColorVariant::Dark);
        let document = session.preview().unwrap().document().clone();
        assert!(session.start_role_editor(session.draft_revision(), session.session_token()));
        assert!(session.editor_active());
        assert_eq!(session.editor_document(), Some(&document));
        assert_eq!(
            session.editor_loaded_color(ThemeRole::Primary, ColorVariant::Dark),
            Some(document.color(ThemeRole::Primary, ColorVariant::Dark))
        );
        let (.., role, variant) = editor_binding(&session);
        assert_eq!(role, ThemeRole::Primary);
        assert_eq!(variant, ColorVariant::Dark);
        // The preview survived, rebound to the new revision/token:
        // Apply with its current binding still works.
        let preview = session.preview().unwrap();
        let (revision, token) = (preview.draft_revision(), preview.token());
        assert!(session.apply(revision, token));
        assert_eq!(session.committed_selection().unwrap().document(), &document);
    }

    #[test]
    fn theme_role_editor_starts_from_clean_committed_only() {
        let mut session = committed(ColorVariant::Light);
        assert!(session.start_role_editor(session.draft_revision(), session.session_token()));
        let (.., role, variant) = editor_binding(&session);
        assert_eq!(role, ThemeRole::Primary);
        assert_eq!(variant, ColorVariant::Light);
        // A nonempty (invalid) raw draft blocks the committed start:
        // no silent fallback to committed data.
        let mut session = committed(ColorVariant::Dark);
        type_raw(&mut session, "{broken".to_string());
        assert!(!session.start_role_editor(session.draft_revision(), session.session_token()));
        assert!(!session.editor_active());
        // And a session with no document at all cannot start.
        let mut empty = ThemeSession::new();
        assert!(!empty.start_role_editor(empty.draft_revision(), empty.session_token()));
    }

    #[test]
    fn theme_role_editor_type_and_set_changes_exactly_one_cell() {
        let mut session = auditioned(ColorVariant::Dark);
        let original = session.preview().unwrap().document().clone();
        session.start_role_editor(session.draft_revision(), session.session_token());
        let (instance, generation, role, variant) = editor_binding(&session);
        assert!(session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#010203".to_string()
        ));
        // Typing alone never touches the document, and clears preview.
        assert!(session.editor_pending());
        assert_eq!(session.editor_document(), Some(&original));
        assert!(!session.has_preview());
        assert!(session.editor_set_color(instance, generation, role, variant));
        let expected = original.replaced(role, variant, "#010203");
        assert_eq!(session.editor_document(), Some(&expected));
        assert_eq!(session.editor_buffer(), Some("#010203"));
        assert!(!session.editor_pending());
        // The pre-Set input stream ended with the Set: the old
        // generation is stale now.
        assert!(!session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#040506".to_string()
        ));
        assert_eq!(session.editor_document(), Some(&expected));
    }

    #[test]
    fn theme_role_editor_queued_keystrokes_share_one_stream() {
        let mut session = auditioned(ColorVariant::Dark);
        session.start_role_editor(session.draft_revision(), session.session_token());
        let (instance, generation, role, variant) = editor_binding(&session);
        // Several text events captured from the same rendered field
        // are accepted in order even though each advances the session
        // revision/token.
        for text in ["#0", "#01", "#0102", "#010203"] {
            assert!(session.editor_type_text(
                instance,
                generation,
                role,
                variant,
                text.to_string()
            ));
        }
        assert_eq!(session.editor_buffer(), Some("#010203"));
        assert!(session.editor_set_color(instance, generation, role, variant));
        assert_eq!(
            session.editor_document().unwrap().color(role, variant),
            "#010203"
        );
    }

    #[test]
    fn theme_role_editor_switch_away_and_back_rejects_old_stream() {
        let mut session = auditioned(ColorVariant::Dark);
        session.start_role_editor(session.draft_revision(), session.session_token());
        let (instance, generation, role, variant) = editor_binding(&session);
        assert!(session.editor_select_role(instance, ThemeRole::Tertiary));
        assert!(session.editor_select_role(instance, role));
        // The original stream binding is stale after away-and-back.
        assert!(!session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#010203".to_string()
        ));
        assert!(!session.editor_set_color(instance, generation, role, variant));
        assert!(!session.editor_pending());
        // Wrong instance is rejected too, with no state change.
        let counters = (session.draft_revision(), session.session_token());
        assert!(!session.editor_select_role(instance + 99, ThemeRole::Error));
        assert_eq!(
            (session.draft_revision(), session.session_token()),
            counters
        );
    }

    #[test]
    fn theme_role_editor_unset_input_blocks_preview_and_apply() {
        let mut session = auditioned(ColorVariant::Dark);
        session.start_role_editor(session.draft_revision(), session.session_token());
        let (instance, generation, role, variant) = editor_binding(&session);
        // Valid but unset text blocks editor Preview.
        assert!(session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#010203".to_string()
        ));
        let outcome = session.editor_preview(
            session.draft_revision(),
            session.session_token(),
            ColorVariant::Dark,
        );
        assert_eq!(outcome, AuditionOutcome::Blocked);
        assert!(!session.has_preview());
        // Invalid text blocks it too, with a field diagnostic.
        let (instance, generation, role, variant) = editor_binding(&session);
        assert!(session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#zzzzzz".to_string()
        ));
        assert!(session.editor_field_diagnostic().is_some());
        let outcome = session.editor_preview(
            session.draft_revision(),
            session.session_token(),
            ColorVariant::Dark,
        );
        assert_eq!(outcome, AuditionOutcome::Blocked);
        // Set with the invalid value changes no document or counter.
        let counters = (session.draft_revision(), session.session_token());
        let document = session.editor_document().unwrap().clone();
        assert!(session.editor_set_color(instance, generation, role, variant));
        assert_eq!(
            (session.draft_revision(), session.session_token()),
            counters
        );
        assert_eq!(session.editor_document(), Some(&document));
    }

    #[test]
    fn theme_role_editor_reset_restores_only_the_selected_cell() {
        let mut session = auditioned(ColorVariant::Dark);
        session.start_role_editor(session.draft_revision(), session.session_token());
        let (instance, generation, role, variant) = editor_binding(&session);
        // No-op reset on a fresh editor: no counter churn.
        let counters = (session.draft_revision(), session.session_token());
        assert!(session.editor_reset_color(instance, generation, role, variant));
        assert_eq!(
            (session.draft_revision(), session.session_token()),
            counters
        );
        // Change the cell, then reset it back to the loaded baseline.
        assert!(session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#010203".to_string()
        ));
        let (instance, generation, ..) = editor_binding(&session);
        assert!(session.editor_set_color(instance, generation, role, variant));
        assert_eq!(
            session.editor_document().unwrap().color(role, variant),
            "#010203"
        );
        let (instance, generation, ..) = editor_binding(&session);
        let counters = (session.draft_revision(), session.session_token());
        assert!(session.editor_reset_color(instance, generation, role, variant));
        assert_ne!(
            (session.draft_revision(), session.session_token()),
            counters
        );
        let baseline = session
            .editor_loaded_color(role, variant)
            .unwrap()
            .to_string();
        assert_eq!(
            session.editor_document().unwrap().color(role, variant),
            baseline
        );
        assert_eq!(session.editor_buffer(), Some(baseline.as_str()));
    }

    #[test]
    fn theme_role_editor_reset_clears_pending_retyped_baseline() {
        let mut session = committed(ColorVariant::Dark);
        assert!(session.start_role_editor(session.draft_revision(), session.session_token()));
        let (instance, generation, role, variant) = editor_binding(&session);
        let baseline = session
            .editor_loaded_color(role, variant)
            .unwrap()
            .to_string();
        let document_before = session.editor_document().unwrap().clone();
        // Type a different value, then retype the exact loaded text
        // without Set: the buffer reads as the baseline again, but
        // the cell is still pending, so Preview is blocked.
        assert!(session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#010203".to_string()
        ));
        assert!(session.editor_type_text(instance, generation, role, variant, baseline.clone()));
        assert!(session.editor_pending());
        let blocked =
            session.editor_preview(session.draft_revision(), session.session_token(), variant);
        assert_eq!(blocked, AuditionOutcome::Blocked);
        // Reset must end this input stream: pending and the field
        // diagnostic cleared, generation rotated, revision/token
        // advanced, and no document cell changed.
        let (instance, generation, ..) = editor_binding(&session);
        let counters = (session.draft_revision(), session.session_token());
        assert!(session.editor_reset_color(instance, generation, role, variant));
        assert!(!session.editor_pending());
        assert_eq!(session.editor_buffer(), Some(baseline.as_str()));
        assert_eq!(session.editor_document(), Some(&document_before));
        assert_ne!(
            (session.draft_revision(), session.session_token()),
            counters
        );
        let (_, new_generation, ..) = editor_binding(&session);
        assert_ne!(new_generation, generation);
        // Queued pre-reset input is rejected under the old generation.
        assert!(!session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#040506".to_string()
        ));
        assert!(!session.editor_pending());
        // Preview is available again.
        let outcome =
            session.editor_preview(session.draft_revision(), session.session_token(), variant);
        assert_eq!(outcome, AuditionOutcome::Installed);
        // A subsequent clean reset churns no counter or generation.
        let (instance, generation, ..) = editor_binding(&session);
        let counters = (session.draft_revision(), session.session_token());
        assert!(session.editor_reset_color(instance, generation, role, variant));
        assert_eq!(
            (session.draft_revision(), session.session_token()),
            counters
        );
        let (_, same_generation, ..) = editor_binding(&session);
        assert_eq!(same_generation, generation);
    }

    #[test]
    fn theme_role_editor_equivalent_set_rotates_generation() {
        let mut session = committed(ColorVariant::Dark);
        assert!(session.start_role_editor(session.draft_revision(), session.session_token()));
        let (instance, generation, role, variant) = editor_binding(&session);
        assert_eq!(
            session.editor_document().unwrap().color(role, variant),
            "#0a4682"
        );
        // Type an equivalent form of the stored color: uppercase,
        // no leading #, outer whitespace.
        assert!(session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            " 0A4682 ".to_string()
        ));
        assert!(session.editor_pending());
        let counters_after_typing = (session.draft_revision(), session.session_token());
        // Set succeeds and ends the input stream even though the
        // color bytes do not change: pending cleared, buffer
        // normalized, generation rotated; document and the
        // revision/token counters are where typing left them.
        assert!(session.editor_set_color(instance, generation, role, variant));
        assert!(!session.editor_pending());
        assert_eq!(session.editor_buffer(), Some("#0a4682"));
        assert_eq!(
            session.editor_document().unwrap().color(role, variant),
            "#0a4682"
        );
        assert_eq!(
            (session.draft_revision(), session.session_token()),
            counters_after_typing
        );
        let (_, new_generation, ..) = editor_binding(&session);
        assert_ne!(new_generation, generation);
        // A queued pre-Set text event cannot reinstate pending text.
        assert!(!session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#010203".to_string()
        ));
        assert!(!session.editor_pending());
        assert_eq!(session.editor_buffer(), Some("#0a4682"));
        // A fresh current binding still accepts legitimate typing.
        let (instance, generation, ..) = editor_binding(&session);
        assert!(session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#010203".to_string()
        ));
        assert!(session.editor_pending());
        // And a Set with nothing pending is a true no-op.
        let mut clean = committed(ColorVariant::Dark);
        assert!(clean.start_role_editor(clean.draft_revision(), clean.session_token()));
        let (instance, generation, role, variant) = editor_binding(&clean);
        let counters = (clean.draft_revision(), clean.session_token());
        assert!(clean.editor_set_color(instance, generation, role, variant));
        assert_eq!((clean.draft_revision(), clean.session_token()), counters);
        let (_, same_generation, ..) = editor_binding(&clean);
        assert_eq!(same_generation, generation);
    }

    #[test]
    fn theme_role_editor_start_rejects_stale_context() {
        // A Start captured before a departure is stale afterwards
        // and mutates nothing; a fresh binding starts from the
        // preserved committed document.
        let mut session = committed(ColorVariant::Dark);
        let stale = (session.draft_revision(), session.session_token());
        session.on_departure();
        let counters = (session.draft_revision(), session.session_token());
        assert!(!session.start_role_editor(stale.0, stale.1));
        assert!(!session.editor_active());
        assert_eq!(
            (session.draft_revision(), session.session_token()),
            counters
        );
        assert!(session.start_role_editor(session.draft_revision(), session.session_token()));
        assert!(session.editor_active());
        // A Start captured before a document replacement is stale;
        // the fresh Start binds the replacement document.
        let mut session = committed(ColorVariant::Dark);
        let stale = (session.draft_revision(), session.session_token());
        let replacement = sentinel_text().replace("#0a4682", "#0b4682");
        assert_ne!(replacement, sentinel_text());
        type_raw(&mut session, replacement);
        let outcome = session.audition(
            session.draft_revision(),
            session.session_token(),
            ColorVariant::Dark,
        );
        assert_eq!(outcome, AuditionOutcome::Installed);
        let preview = session.preview().unwrap();
        assert!(session.apply(preview.draft_revision(), preview.token()));
        assert!(!session.start_role_editor(stale.0, stale.1));
        assert!(!session.editor_active());
        assert!(session.start_role_editor(session.draft_revision(), session.session_token()));
        assert_eq!(
            session
                .editor_document()
                .unwrap()
                .color(ThemeRole::Primary, ColorVariant::Dark),
            "#0b4682"
        );
    }

    #[test]
    fn theme_role_editor_variant_switch_discards_unset_without_side_effects() {
        let mut session = committed(ColorVariant::Dark);
        let committed_document = session.committed_selection().unwrap().document().clone();
        session.start_role_editor(session.draft_revision(), session.session_token());
        let (instance, generation, role, _) = editor_binding(&session);
        assert!(session.editor_type_text(
            instance,
            generation,
            role,
            ColorVariant::Dark,
            "#010203".to_string()
        ));
        assert!(session.editor_pending());
        assert!(session.editor_select_variant(instance, ColorVariant::Light));
        assert!(!session.editor_pending());
        assert_eq!(
            session.editor_buffer(),
            Some(committed_document.color(role, ColorVariant::Light))
        );
        // The document and the committed selection are untouched, and
        // no global preview appeared or changed variant.
        assert_eq!(session.editor_document(), Some(&committed_document));
        assert!(!session.has_preview());
        assert_eq!(
            session.committed_selection().unwrap().variant(),
            ColorVariant::Dark
        );
    }

    #[test]
    fn theme_role_editor_codec_serializes_edits_without_a_raw_transfer_path() {
        let mut session = ThemeSession::new();
        let document = import_document(&serde_json::from_str(&sentinel_text()).unwrap())
            .unwrap()
            .document;
        session.load_designer(document, ColorVariant::Dark, "Codec fixture".into(), None);
        let (instance, generation, role, variant) = editor_binding(&session);
        assert!(session.editor_type_text(instance, generation, role, variant, "#010203".into()));
        assert!(session.editor_set_color(instance, generation, role, variant));
        let document = session.editor_document().unwrap().clone();
        let serialized = serde_json::to_value(&document).unwrap();
        let imported = import_document(&serialized).unwrap();
        assert_eq!(imported.document, document);
        assert_eq!(document.color(role, variant), "#010203");
        assert!(session.editor_active());
        assert!(session.draft_text().is_empty());
    }

    #[test]
    fn theme_role_editor_departure_clears_editor_and_stale_contexts() {
        let mut session = auditioned(ColorVariant::Dark);
        session.start_role_editor(session.draft_revision(), session.session_token());
        let (instance, generation, role, variant) = editor_binding(&session);
        let raw_context = session.raw_context();
        session.on_departure();
        assert!(!session.editor_active());
        // Every pre-departure binding is dead after re-entry.
        assert!(!session.editor_type_text(
            instance,
            generation,
            role,
            variant,
            "#010203".to_string()
        ));
        assert!(!session.edit_draft(raw_context, sentinel_text()));
        assert!(type_raw(&mut session, sentinel_text()));
    }
}
