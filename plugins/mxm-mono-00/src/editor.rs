//! mxm-mono-00's editor.
//!
//! Built to `docs/briefs/mxm-mono-00.md`. Thirteen paged cards: twelve **`Synth`** cards including
//! sample-and-hold, with routing at each target, and the one **Effects** card, the three effects in
//! processing order; and **`Parameters`**, the collection's testing surface. No Patch page or matrix widget. Volume
//! is on none of them: it is in the app bar beside the level meter (design system §3.1 item 6).
//!
//! # The frame is derived, not chosen
//!
//! Thirteen cards page in signal-chain order, each a `mxm_ui::tree` whose floor and height are
//! computed from it, nothing drawn to learn them. The opening size is the quarter-4K budget hugged
//! (`the_opening_size_is_the_budget_hugged`), with the Voice expander reserved open and every route
//! revealed; `the_synth_view_fits_the_editor` holds the Synth view inside it. Window size and editor
//! zoom remain independent.
//!
//! # It is a panel, not a window
//!
//! [`panel`] takes a `Ui` and draws into it. It does not create a window, run an event loop, or own
//! a swapchain.
//!
//! # Gestures
//!
//! Every edit is bracketed in exactly one place — [`binding::Bound::apply`]. An unclosed gesture
//! leaves a host's automation lane latched, and it breaks the player's step editing outright.

pub mod binding;
pub mod sections;

use std::collections::HashMap;
use std::sync::Arc;

use egui::Ui;
use mxm_mono_00_dsp::voice::Activity;
use mxm_ui::space::{SPACE_3, SPACE_4};
use mxm_ui::theme::Tokens;
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, create_egui_editor};

use crate::params::MxmMono00Params;
use crate::telemetry::Telemetry;

/// The size the editor **opens** at. It is no longer a fixed frame: the window resizes and the
/// cards reflow into rows (§3.4, §4.3).
///
/// **Derived, not chosen**: the quarter-4K budget hugged around every page, which
/// `tests::the_opening_size_is_the_budget_hugged` holds.
const REFERENCE: (u32, u32) = (1694, 987);

/// The wider of one S&H card with the panel gutters and the vertical scrollbar, and the app bar at
/// its last compact step. The bar decides it: `the_app_bar_holds_in_the_minimum_window` measures it.
const MINIMUM: (u32, u32) = (468, 320);

/// The keyboard cursor's card for the app bar's Volume, outside the paging keys 0…13.
const VOLUME_CARD: u64 = 64;

/// How the sections are dealt into the `Parameters` view's three columns.
///
/// Balanced by parameter count rather than by section: the signal path on the left, the
/// envelopes and modulators in the middle, routing parameters and effects on the right.
const PARAMETER_COLUMNS: [&[sections::Section]; 3] = [
    &[
        sections::Section::Oscillator1,
        sections::Section::Oscillator2,
        sections::Section::RingMod,
        sections::Section::Mixer,
        sections::Section::Filter,
    ],
    &[
        sections::Section::Amplifier,
        sections::Section::EnvelopeVcf,
        sections::Section::EnvelopeVca,
        sections::Section::Modulator1,
        sections::Section::Modulator2,
        sections::Section::SampleHold,
    ],
    &[
        sections::Section::Voice,
        sections::Section::Routing,
        sections::Section::Fx,
        sections::Section::Output,
    ],
];

/// Builds the editor. Called from `Plugin::editor`.
pub fn create(params: Arc<MxmMono00Params>, telemetry: Arc<Telemetry>) -> Option<MxmMono00Editor> {
    let state = EguiEditorState::from_size(
        nice_plug::editor::dpi::LogicalSize::new(REFERENCE.0, REFERENCE.1),
        1.0,
    );

    create_egui_editor(
        state,
        nice_plug_egui::RepaintNotifier::new(),
        nice_plug_egui::EguiNiceSettings {
            title: crate::NAME.to_owned(),
            // Native resizing reflows the cards; scaling remains the app bar's independent zoom.
            resize_hint: ResizeHint {
                size_constraints: nice_plug::editor::SizeConstraints::min_logical_size(
                    nice_plug::editor::dpi::LogicalSize::new(MINIMUM.0 as f32, MINIMUM.1 as f32),
                ),
                ..ResizeHint::RESIZABLE
            },
            ..Default::default()
        },
        MxmMono00App::new(params, telemetry),
    )
}

/// The editor type the plugin exposes.
pub type MxmMono00Editor = nice_plug_egui::EguiEditor<MxmMono00App>;

/// The editor's own state: what the plugin does not own and the host does not need.
pub struct MxmMono00App {
    params: Arc<MxmMono00Params>,
    telemetry: Arc<Telemetry>,
    gui_context: Option<GuiContext>,
    view: usize,
    text_entry: HashMap<&'static str, Option<String>>,
    presets: PresetUi,
    /// Where the keyboard is: a card, and a parameter inside it. Transient, like the text
    /// buffers — it is not a parameter and nothing durable reads it.
    nav: mxm_ui::navigation::State,
}

/// The app bar's preset controls and what they need between frames — `mxm-preset`'s, one for
/// every instrument.
pub use mxm_preset::PresetUi;

impl MxmMono00App {
    pub fn new(params: Arc<MxmMono00Params>, telemetry: Arc<Telemetry>) -> Self {
        let params_for_presets = Arc::clone(&params);
        Self {
            params,
            telemetry,
            gui_context: None,
            view: 0,
            text_entry: HashMap::new(),
            presets: PresetUi::new(params_for_presets.as_ref()),
            nav: mxm_ui::navigation::State::default(),
        }
    }
}

impl NiceEguiApp for MxmMono00App {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        mxm_ui::theme::apply(&egui_ctx);
        mxm_ui::typography::apply(&egui_ctx);
        // Light by default, overridable with `MXM_EDITOR_THEME`. The reasoning, and why the
        // default is not `System`, lives on `mxm_ui::theme::preference`.
        egui_ctx.set_theme(mxm_ui::theme::preference());
        self.gui_context = Some(nice_gui_ctx);
        Ok(())
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(gui_context) = self.gui_context.clone() else {
            return;
        };
        panel(
            ui,
            &self.params,
            &self.telemetry,
            &gui_context.param_setter(),
            &mut self.view,
            &mut self.text_entry,
            &mut self.presets,
            &mut self.nav,
        );
    }

    fn editor_closed(&mut self) {
        self.gui_context = None;
    }
}

/// The whole editor, as a panel.
#[allow(clippy::too_many_arguments)]
pub fn panel(
    ui: &mut Ui,
    params: &MxmMono00Params,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    view: &mut usize,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    presets: &mut PresetUi,
    nav: &mut mxm_ui::navigation::State,
) {
    let tokens = &tokens_for(ui);

    // Live displays — the meter and activity glyph — and the developer channel's
    // requests all arrive between input events, so the editor asks for the next frame itself
    // rather than waiting for the pointer to move. Twenty a second is plenty for a level bar.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));

    // The developer channel's requests, if the plugin was started with it (`lib.rs`): a view and
    // the expander's state. Taken once each; nothing else reads them.
    // One question, and both layers suspend on it: the paging renderer's `hold` and the cursor's
    // `inert` both ask whether another surface owns this frame's keyboard.
    let busy = presets.holds_the_keyboard() || text_entry.values().any(Option::is_some);
    mxm_ui::paging::editor::hold(ui.ctx(), busy);
    mxm_ui::paging::editor::developer_request(ui.ctx(), view, telemetry.take_view_request());

    // **The keyboard cursor moves before anything is drawn**, so a navigation arrow is consumed
    // here rather than also walking egui's own focus ring. It reads the registry and the exact
    // card rectangles the previous frame built, and it resolves the developer-view request first,
    // because which surface this frame is deciding who owns its keyboard.
    if *view == mxm_ui::paging::PARAMETERS {
        // This surface has no cards. Stop rather than merely hiding the outline, or its controls
        // lose their legacy bare-arrow editing to an invisible stale musician cursor.
        mxm_ui::navigation::stop(ui.ctx());
    } else {
        mxm_ui::navigation::paged_with_bar(ui.ctx(), nav, busy, &[VOLUME_CARD]);
    }
    if let Some(open) = telemetry.take_browser_request() {
        presets.set_browser_open(open);
    }
    // A theme, by index. Applied and not stored: this channel is how a screenshot run and a test
    // reach a state, and neither should overwrite the choice made in the control.
    if let Some(index) = telemetry.take_theme_request()
        && let Some(preference) = mxm_ui::theme::from_index(index)
    {
        ui.ctx().set_theme(preference);
    }
    if let Some(open) = telemetry.take_disclosure_request() {
        ui.data_mut(|d| d.insert_temp(sections::disclosure_id(sections::ADVANCED), open));
    }

    let peak = telemetry.take_peak();
    let clipped = telemetry.clipped();
    let activity = telemetry.activity();
    mxm_ui::AppBar::new(crate::NAME).show_with(
        ui,
        tokens,
        |ui| mxm_preset::ui::preset_row(ui, tokens, params, setter, presets),
        |ui| {
            activity_indicator(ui, tokens, activity);
            if mxm_ui::shell::level_meter(ui, tokens, peak, clipped) {
                telemetry.clear_clip();
            }
            // Design system §3.1 slot 6: the output control sits beside its meter, not on a card.
            // A bar card, so the keyboard cursor reaches it although the paging report cannot
            // see the bar.
            mxm_ui::navigation::bar_card(ui, VOLUME_CARD, |ui| {
                ui.scope(|ui| {
                    sections::binding_for("volume", params)
                        .slider_inline(ui, tokens, setter, text_entry, 96.0);
                })
                .response
                .rect
            });
            mxm_ui::shell::zoom_control(ui);

            // §3.1 slot 5, and the same place the player keeps it: at the left end of the bar's
            // right-hand group. What the person picks is remembered for every MXM editor, so the
            // next one to open agrees with this one.
            mxm_ui::shell::editor_theme_control(ui);
        },
    );

    mxm_preset::ui::overlays(ui, tokens, params, setter, presets);

    // **The Parameters view has no tab.** It is the complete generated list, and an editor whose
    // own interface reaches every control does not need a second way to the same parameters in
    // front of a musician every day. It stays reachable: the developer channel still requests it
    // by index, which is what the CLI and a host's automation list use it for.
    // Navigation is derived by the paging renderer.

    // §4.2's compact gutter, twelve points: the Synth view is dense by design, and the four
    // points a side buy eight of the height the tallest column needs.
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(tokens.canvas)
                .inner_margin(egui::Margin::same(SPACE_4 as i8)),
        )
        .show(ui, |ui| {
            if *view == mxm_ui::paging::PARAMETERS {
                parameters_view(ui, tokens, params, setter, text_entry);
            } else {
                paged_view(ui, tokens, params, telemetry, setter, text_entry);
            }
        });
}

/// Every paging item, each floor computed from its card's tree in `ui`'s fonts, and each card
/// exactly as wide as that floor: its ceiling is its floor, so everything packs tight. Nothing here
/// is typed.
pub fn page_items(ui: &Ui, params: &MxmMono00Params) -> Vec<mxm_ui::paging::Item<'static>> {
    use mxm_ui::{
        flow::Card,
        paging::{Category as C, Item, Key},
    };
    let floor = |index: usize| {
        mxm_ui::tree::card_floor(
            ui,
            sections::card_title(index),
            &sections::card(ui, index, params),
        )
    };
    let mut items: Vec<_> = sections::SYNTH
        .iter()
        .zip(sections::SYNTH_KEYS)
        .enumerate()
        .map(|(index, (s, key))| Item {
            key: Key(key),
            card: Card::new(s.title(), floor(index)).capped(floor(index)),
            category: match key {
                0 => C::Performance,
                1 | 2 | 3 | 8 | 10 => C::Modulators,
                4 | 5 | 14 => C::Generators,
                _ => C::Tone,
            },
            kind: s.title(),
        })
        .collect();
    let effects = floor(sections::SYNTH.len());
    items.push(Item {
        key: Key(11),
        card: Card::new(sections::EFFECTS, effects).capped(effects),
        category: C::Effects,
        kind: sections::EFFECTS,
    });
    items
}

fn paged_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmMono00Params,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    use mxm_ui::paging::Key;
    let items = page_items(ui, params);
    let text_editing = entries.values().any(Option::is_some);
    let mut live = sections::Live {
        params,
        setter,
        text_entry: entries,
        tempo: telemetry.tempo.get(),
    };
    mxm_ui::paging::editor::show(
        ui,
        tokens,
        &items,
        &[&[Key(1), Key(2)], &[Key(4), Key(5), Key(14)]],
        text_editing,
        &mut |ui, index| sections::card(ui, index, params),
        &mut |ui, _, leaf, rect| sections::paint(ui, tokens, leaf, rect, &mut live),
    );
}

/// The paging items as the editor computes them, from a context set up as an editor's is — three
/// passes in, so the weighted font cuts are bound — for tests, which have no editor `Ui` to hand.
#[cfg(test)]
pub(crate) fn test_items() -> Vec<mxm_ui::paging::Item<'static>> {
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    let params = MxmMono00Params::default();
    let mut items = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            items = page_items(ui, &params);
        });
        output.textures_delta.clear();
    }
    items
}

/// The cards' floors in paging order, as [`test_items`] computes them.
#[cfg(test)]
pub(crate) fn test_floors() -> Vec<f32> {
    test_items().iter().map(|item| item.card.floor).collect()
}

/// The brief's §8 activity indicator: one glyph, three states, beside the meter.
///
/// This is the first instrument in the collection that sounds with no key down, and a person must
/// be able to see that it is the patch doing it and not the host.
fn activity_indicator(ui: &mut Ui, tokens: &Tokens, activity: Activity) {
    let (glyph, colour, hover) = match activity {
        Activity::Live => (
            "\u{25CF}",
            tokens.accent,
            "Live: the patch is sounding on its own.",
        ),
        Activity::Tailing => (
            "\u{25D0}",
            tokens.text_secondary,
            "Tailing: an envelope or an effect is still settling.",
        ),
        Activity::Inert => ("\u{25CB}", tokens.text_secondary, "Inert: silent."),
    };
    ui.label(egui::RichText::new(glyph).color(colour).size(14.0))
        .on_hover_text(hover);
}

/// Every parameter as a slider, grouped into the brief's sections as cards.
///
/// **Editable, not a readout**: this is where a parameter gets driven when the panel is not the
/// thing under test. The list scrolls independently of the Synth view's card reflow.
fn parameters_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmMono00Params,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
) {
    mxm_ui::shell::scroll_list(ui).show(ui, |ui| {
        ui.columns(PARAMETER_COLUMNS.len(), |uis| {
            for (column, sections) in uis.iter_mut().zip(PARAMETER_COLUMNS) {
                for section in sections {
                    mxm_ui::ModuleCard::new(section.title()).show(column, tokens, |ui| {
                        for id in section.parameters() {
                            // The list is the canonical names: its FX card holds three modules.
                            sections::binding_for(id, params)
                                .unlabelled()
                                .slider(ui, tokens, setter, text_entry);
                        }
                    });
                    column.add_space(SPACE_3);
                }
            }
        });
    });
}

/// The collection's tokens, with this instrument's identity accent (brief §7): copper.
fn tokens_for(ui: &Ui) -> Tokens {
    let dark = ui.visuals().dark_mode;
    let base = if dark { mxm_ui::DARK } else { mxm_ui::LIGHT };
    base.with_identity(mxm_ui::theme::COPPER, dark)
}

#[cfg(test)]
mod tests {
    // `all_rects` finds cards by key == index, which the Ring mod card's key 14 breaks, so this
    // panel looks its cards up through `SYNTH_KEYS` instead and leaves that one helper unused.
    #![allow(dead_code)]
    use mxm_plugin_test::{opening_size, paging_checks};

    /// **The editor opens at the quarter-4K budget, hugged** — the owner's rule, 2026-09-09. The budget
    /// is the most room an editor may ask for, so laying the panel out there shows as many modules as
    /// it ever will; taking the slack away is the whole of the size.
    #[test]
    fn the_opening_size_is_the_budget_hugged() {
        let params = MxmMono00Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        opening_size::is_the_budget_hugged(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &REVEAL,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    /// **The app bar holds in the narrowest window**: its `…` menu whole and nothing drawn over
    /// anything else, from `MINIMUM` up (`opening_size::bar_holds_from_the_minimum`).
    #[test]
    fn the_app_bar_holds_in_the_minimum_window() {
        let params = MxmMono00Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        opening_size::bar_holds_from_the_minimum(
            egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    use mxm_plugin_test::keyboard_checks;

    /// What this editor keeps behind a disclosure, opened so the reachability check sees it.
    /// The Voice card's Advanced expander holds master tune and the bend range.
    const REVEAL: fn(&egui::Context) = |ctx| {
        ctx.data_mut(|d| d.insert_temp(sections::disclosure_id(sections::ADVANCED), true));
    };

    /// The rollout's own failure mode: a control whose `navigation::at` scope was forgotten paints
    /// exactly as before and is simply unreachable from the keyboard. Nothing else would say so.
    /// Volume is in the inventory like every other parameter; it registers from the app bar's
    /// bar card, which is drawn on every page, and the cursor lands there first.
    #[test]
    fn the_keyboard_cursor_reaches_and_operates_every_parameter() {
        let params = MxmMono00Params::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        // **Every route present, because an absent one draws nothing at all.** That is the routing
        // interface's own rule — a target with nothing wired draws no group — so at Init nine rows
        // exist and 291 do not. Revealing them all is the only way this check covers the
        // parameters the conversion added.
        {
            use nice_plug::params::InternalParamMut as _;
            for (_, group) in params.routes.all() {
                for presence in group.presence_params() {
                    // Safety: a test owns these parameters outright; nothing else holds one.
                    unsafe { presence._internal_set_plain_value(true) };
                }
            }
        }
        let mut ids: Vec<&str> = sections::all_parameters(&params)
            .iter()
            .map(|bound| bound.id)
            .collect();
        ids.extend(
            crate::routes::offered_route_ids().flat_map(|(amount, present)| [amount, present]),
        );
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        keyboard_checks::the_cursor_reaches_and_operates(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &test_items(),
            keyboard_checks::Coverage::Exactly(&ids),
            &REVEAL,
            &host,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    #[test]
    fn every_dynamic_page_fits_and_every_card_is_reachable() {
        let params = MxmMono00Params::default();
        let telemetry = Telemetry::default();
        telemetry.request_disclosure(true);
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0;
        let mut entries = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        paging_checks::verify(
            &test_items(),
            &[
                egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
                egui::vec2(1880.0, 1040.0),
                egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            ],
            |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut entries,
                    &mut presets,
                    &mut nav,
                )
            },
        );
    }
    use super::*;
    use nice_plug::params::internals::ParamPtr;
    use nice_plug::prelude::{PluginApi, PluginState};

    struct NoHost;

    impl nice_plug::context::gui::GuiContextInner for NoHost {
        // A test double has no host to ask for a restart (nice-plug 0.4).
        fn request_restart(&self) {}
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    /// Lays the whole editor out at a given size — with everything that expands **expanded**
    /// when `expanded` says so, which is the state the frame has to fit — and hands back the
    /// context and the height it actually needed.
    fn lay_out(view: usize, width: f32, height: f32, expanded: bool) -> (egui::Context, f32) {
        let ctx = egui::Context::default();
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.set_theme(egui::ThemePreference::Light);
        ctx.data_mut(|d| d.insert_temp(sections::disclosure_id(sections::ADVANCED), expanded));

        let params = MxmMono00Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        telemetry.request_view(view as u8);
        let mut view = view;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        };

        let mut used = 0.0;
        for _ in 0..3 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
            output.textures_delta.clear();
            used = ctx.globally_used_rect().height();
        }
        (ctx, used)
    }

    /// What the editor drew at a given size.
    fn measure(view: usize, width: f32, height: f32, expanded: bool) -> f32 {
        lay_out(view, width, height, expanded).1
    }

    /// The height a fixed view needs: where its levelled cards end, plus the panel's margin.
    /// Not `globally_used_rect`: the central panel fills the window whatever is in it, so that
    /// only exceeds the window once something has already been cut off, and a window taller
    /// than its content reads as exactly full.
    /// Synth is measured from its rows; FX from its levelled columns. Add the panel margin.
    fn view_height(view: usize, expanded: bool) -> f32 {
        if view == 0 {
            return placed(REFERENCE.0 as f32, expanded)
                .iter()
                .map(egui::Rect::bottom)
                .fold(0.0_f32, f32::max)
                + SPACE_4;
        }
        let (ctx, _) = lay_out(view, REFERENCE.0 as f32, REFERENCE.1 as f32, expanded);
        mxm_ui::paging::editor::report(&ctx)
            .unwrap()
            .visible
            .iter()
            .map(|(_, r)| r.bottom())
            .fold(0.0_f32, f32::max)
            + SPACE_4
    }

    /// Where every card landed on the `Synth` view, at a given width.
    fn placed(width: f32, expanded: bool) -> Vec<egui::Rect> {
        let (ctx, _) = lay_out(0, width, 20000.0, expanded);
        // By each card's permanent key, not its position: the ring modulator's card is key 14.
        let report = mxm_ui::paging::editor::report(&ctx).expect("paging report");
        sections::SYNTH_KEYS
            .iter()
            .map(|&key| {
                report
                    .visible
                    .iter()
                    .find(|(k, _)| k.0 == key)
                    .unwrap_or_else(|| panic!("card {key} absent"))
                    .1
            })
            .collect()
    }

    /// Cards whose vertical extents overlap are on one row, read off the geometry.
    fn rows(placed: &[egui::Rect]) -> Vec<Vec<egui::Rect>> {
        let mut sorted = placed.to_vec();
        sorted.sort_by(|a, b| {
            a.top()
                .partial_cmp(&b.top())
                .unwrap()
                .then(a.left().partial_cmp(&b.left()).unwrap())
        });
        let mut rows: Vec<Vec<egui::Rect>> = Vec::new();
        for rect in sorted {
            match rows.last_mut() {
                Some(row) if row.iter().any(|r| r.bottom() > rect.top() + 1.0) => row.push(rect),
                _ => rows.push(vec![rect]),
            }
        }
        rows
    }

    /// Every row of cards shares a bottom edge, on every page.
    #[test]
    fn the_cards_end_level_on_every_view() {
        for expanded in [false, true] {
            for width in [
                MINIMUM.0 as f32,
                400.0,
                700.0,
                1000.0,
                1200.0,
                1600.0,
                REFERENCE.0 as f32,
            ] {
                for row in rows(&placed(width, expanded)) {
                    if row.len() < 2 {
                        continue;
                    }
                    let low = row
                        .iter()
                        .map(egui::Rect::bottom)
                        .fold(f32::INFINITY, f32::min);
                    let high = row
                        .iter()
                        .map(egui::Rect::bottom)
                        .fold(f32::NEG_INFINITY, f32::max);
                    assert!(
                        high - low < 1.0,
                        "Synth, expanded {expanded} at {width}: a row of {} cards ends {:.1} ragged",
                        row.len(),
                        high - low
                    );
                }
            }
        }

        // Effects now share the same row renderer and per-page proof, not fixed columns.
        every_dynamic_page_fits_and_every_card_is_reachable();
    }

    /// Nothing is clipped, overlapping, or any width but its floor.
    #[test]
    fn the_synth_view_reflows_without_clipping_or_overlap() {
        for width in [
            MINIMUM.0 as f32,
            400.0,
            700.0,
            1000.0,
            1200.0,
            1600.0,
            REFERENCE.0 as f32,
        ] {
            let placed = placed(width, true);
            let floors = test_floors();
            for (index, rect) in placed.iter().enumerate() {
                assert!(
                    rect.left() >= -0.5 && rect.right() <= width + 0.5,
                    "{} runs from {:.1} to {:.1} at {width}",
                    sections::SYNTH[index].title(),
                    rect.left(),
                    rect.right()
                );
                assert!(
                    (rect.width() - floors[index]).abs() <= 0.5,
                    "{} is {:.1} wide, not its floor {:.1}",
                    sections::SYNTH[index].title(),
                    rect.width(),
                    floors[index]
                );
            }
            for (i, a) in placed.iter().enumerate() {
                for (j, b) in placed.iter().enumerate().skip(i + 1) {
                    let overlap = a.intersect(*b);
                    assert!(
                        overlap.width() <= 0.5 || overlap.height() <= 0.5,
                        "{} overlaps {} at {width}",
                        sections::SYNTH[i].title(),
                        sections::SYNTH[j].title()
                    );
                }
            }
        }
    }

    /// **The scar every editor here carries, turned into a number.** A height guessed before the
    /// panel existed, and controls cut off the bottom of a window that cannot be resized. Pinned
    /// from above and from below, so nobody adds height instead of tightening.
    #[test]
    fn the_synth_view_fits_the_editor() {
        // **Measured with the Voice card's expander open** — the owner's rule: room for anything
        // that expands, so opening it never pushes a control off a window that cannot grow.
        let closed = view_height(0, false);
        let used = view_height(0, true);
        eprintln!("the Synth view needs {closed} points closed and {used} with the expander open");
        // Fit is a property of each derived page, not the total unpaged height.
        every_dynamic_page_fits_and_every_card_is_reachable();
        assert!(
            closed <= used,
            "opening the expander made the view shorter ({closed} closed, {used} open)"
        );
        // The Voice card may not be the tallest in its row, so opening its expander can change the
        // view's height not at all; that it opened shows in **its own card** growing.
        let (before, after) = (
            placed(REFERENCE.0 as f32, false),
            placed(REFERENCE.0 as f32, true),
        );
        assert!(
            before
                .iter()
                .zip(&after)
                .all(|(closed, open)| (open.height() - closed.height()).abs() < 0.75),
            "disclosure space is reserved; opening it must not move card borders"
        );
    }

    /// The Effects page — the developer channel's category 5 — fits the same frame.
    #[test]
    fn the_effects_page_fits_the_editor() {
        let used = view_height(5, true);
        eprintln!("the Effects page needs {used} points");
        assert!(
            used <= REFERENCE.1 as f32,
            "the Effects page needs {used} points of height in a {} point window",
            REFERENCE.1
        );
        assert!(used > 300.0, "the Effects page drew almost nothing: {used}");
    }

    /// `Parameters` is allowed to be taller than the frame: its list scrolls.
    #[test]
    fn the_parameters_view_lays_out_without_panicking() {
        assert!(
            measure(
                mxm_ui::paging::PARAMETERS,
                REFERENCE.0 as f32,
                REFERENCE.1 as f32,
                true
            ) > 0.0
        );
    }

    /// **Volume is painted once, in the app bar** (design system §3.1 item 6), and not on the
    /// Amplifier card that used to end with it. Read off the keyboard cursor's registry, which is
    /// built by painting, on a canvas tall enough to draw every card.
    #[test]
    fn volume_is_drawn_once_in_the_app_bar() {
        let (ctx, _) = lay_out(0, REFERENCE.0 as f32, 20000.0, true);
        let spots = mxm_ui::navigation::spots(&ctx);
        let volume: Vec<_> = spots.iter().filter(|spot| spot.key == "volume").collect();
        assert_eq!(volume.len(), 1, "Volume is painted {} times", volume.len());
        assert_eq!(volume[0].card, VOLUME_CARD, "Volume is the app bar's");
        // The Amplifier card was painted too, so Volume's absence from it is a finding rather
        // than a card that never drew.
        assert!(
            spots
                .iter()
                .any(|spot| spot.key == "tone" && spot.card != VOLUME_CARD),
            "the Amplifier card was not painted"
        );
        assert!(
            volume[0].rect.bottom()
                <= spots
                    .iter()
                    .filter(|spot| spot.card != VOLUME_CARD)
                    .map(|spot| spot.rect.top())
                    .fold(f32::INFINITY, f32::min),
            "Volume sits above every card, in the bar"
        );
    }

    /// The developer channel's addresses are the paging categories and Parameters, with no Patch
    /// page; a fresh editor opens on the first musician page.
    #[test]
    fn the_views_are_the_briefs_and_synth_opens() {
        assert_eq!(
            mxm_ui::paging::Category::from_request(5),
            Some(mxm_ui::paging::Category::Effects)
        );
        assert_eq!(mxm_ui::paging::PARAMETERS, 127);
        let params = Arc::new(MxmMono00Params::default());
        let app = MxmMono00App::new(params, Telemetry::shared());
        assert_eq!(app.view, 0, "a fresh editor opens on Synth");
    }

    /// Every card, in every state that changes what it holds, passes the layout tree's checks
    /// (plans/plan-layout-tree.md §4.3, `tree_checks::card`): its floor holds its content with
    /// nothing painted outside the card, the content floor is exact, the height its tree states is
    /// the height it draws, and every leaf stays in the room it was given.
    ///
    /// The states are this editor's structural-state matrix: the init patch; every route revealed
    /// at full negative depth, where a reading carries its sign and every digit — the widest text a
    /// row can show; the Voice card's Advanced expander open; tempo sync on with a tempo and without
    /// one, the delay caption's other two texts (the second its longest); and the layout lab's
    /// routing-as-controls flag, which draws no stack at all. Every floor is the one the editor
    /// computes at Init, so a state that widened a card past it would fail the first check.
    #[test]
    fn every_card_passes_the_tree_checks_in_every_state() {
        use nice_plug::params::InternalParamMut as _;
        let floors = test_floors();
        for state in [
            "init",
            "every route revealed",
            "Advanced open",
            "tempo sync on, no tempo",
            "tempo sync on, synced",
            "routing shown as controls",
        ] {
            let params = MxmMono00Params::default();
            let telemetry = Telemetry::default();
            if state == "every route revealed" {
                for (_, group) in params.routes.all() {
                    for presence in group.presence_params() {
                        // Safety: a test owns these parameters outright; nothing else holds one.
                        unsafe { presence._internal_set_plain_value(true) };
                    }
                }
                let amounts: Vec<&str> = crate::routes::ROUTE_IDS
                    .iter()
                    .flat_map(|target| target.iter().map(|(amount, _)| *amount))
                    .collect();
                for (id, parameter, _) in params.param_map() {
                    if amounts.contains(&id.as_str()) {
                        // Safety: as above.
                        unsafe { parameter._internal_set_normalized_value(0.0) };
                    }
                }
            }
            // Every sync on — the delay's, the sample clock's and both LFOs' — with a tempo and
            // without, so each synced control's division and free readings are both checked.
            if state.starts_with("tempo sync on") {
                // Safety: as above.
                unsafe {
                    params.tempo_sync._internal_set_plain_value(true);
                    params.sh_sync._internal_set_plain_value(true);
                    params.lfo1_sync._internal_set_plain_value(true);
                    params.lfo2_sync._internal_set_plain_value(true);
                }
                if state.ends_with("synced") {
                    telemetry.tempo.publish(Some(120.0));
                }
            }
            let open = state == "Advanced open";
            let lab = state == "routing shown as controls";
            let setup = move |ctx: &egui::Context| {
                ctx.data_mut(|d| {
                    d.insert_temp(sections::disclosure_id(sections::ADVANCED), open);
                    if lab {
                        d.insert_temp(sections::routing_shown_as_controls(), true);
                    }
                });
            };
            let host = NoHost;
            let setter = ParamSetter::new(&host);
            for (index, floor) in floors.iter().enumerate() {
                let mut text_entry = HashMap::new();
                let mut live = sections::Live {
                    params: &params,
                    setter: &setter,
                    text_entry: &mut text_entry,
                    tempo: telemetry.tempo.get(),
                };
                tree_checks::card(
                    &setup,
                    state,
                    sections::card_title(index),
                    *floor,
                    &|ui| sections::card(ui, index, &params),
                    &mut |ui, leaf, rect| {
                        sections::paint(ui, &mxm_ui::LIGHT, leaf, rect, &mut live);
                    },
                );
            }
        }
    }

    /// **Syncing moves nothing** (`plans/plan-editor-standard.md` E1): a synced control reads its
    /// division where it read its value, and its column already holds the widest of both
    /// (`binding::synced_widest`), so the cards that hold one are the same size with sync on and off.
    #[test]
    fn the_synced_cards_keep_their_size_as_sync_turns_on() {
        use nice_plug::params::InternalParamMut as _;
        let cards = [
            sections::SYNTH.len(),
            sections::SYNTH
                .iter()
                .position(|s| *s == sections::Section::Modulator1)
                .unwrap(),
            sections::SYNTH
                .iter()
                .position(|s| *s == sections::Section::SampleHold)
                .unwrap(),
        ];
        let sizes = |sync: bool| -> Vec<(f32, f32)> {
            let params = MxmMono00Params::default();
            // Safety: a test owns these parameters outright; nothing else holds one.
            unsafe {
                params.tempo_sync._internal_set_plain_value(sync);
                params.sh_sync._internal_set_plain_value(sync);
                params.lfo1_sync._internal_set_plain_value(sync);
            }
            let ctx = egui::Context::default();
            mxm_ui::typography::apply(&ctx);
            mxm_ui::theme::apply(&ctx);
            let mut sizes = Vec::new();
            for _ in 0..2 {
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    let body = mxm_ui::shell::body_ui(ui);
                    sizes = cards
                        .iter()
                        .map(|&index| {
                            let card = sections::card(&body, index, &params);
                            let width = card.min_width(&body);
                            (width, card.reserved_height(&body, width))
                        })
                        .collect();
                });
                output.textures_delta.clear();
            }
            sizes
        };
        assert_eq!(sizes(false), sizes(true), "a synced card changed size");
    }

    /// Every section is dealt into exactly one column, so nothing is drawn twice or not at all.
    #[test]
    fn every_section_is_in_exactly_one_column() {
        let dealt: Vec<sections::Section> = PARAMETER_COLUMNS
            .iter()
            .flat_map(|column| column.iter().copied())
            .collect();
        for section in sections::SECTIONS {
            let count = dealt.iter().filter(|s| *s == section).count();
            assert_eq!(count, 1, "{} is in {count} columns", section.title());
        }
        assert_eq!(dealt.len(), sections::SECTIONS.len());
    }

    use mxm_plugin_test::tree_checks;

    /// Every page at the opening size, light and dark, for the owner's review of the layout-tree
    /// conversion (plans/plan-layout-tree.md §4.3): `target/layout-tree/mxm-mono-00/<tag>/`, where
    /// `MXM_PICTURES` names the tag.
    ///
    /// `MXM_PICTURES=after cargo test -p mxm-mono-00 --lib tree_pictures -- --ignored`
    #[test]
    #[ignore = "renders through wgpu; run by hand"]
    fn tree_pictures() {
        let tag = std::env::var("MXM_PICTURES").unwrap_or_else(|_| "after".to_owned());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/layout-tree/mxm-mono-00")
            .join(tag);
        let params = MxmMono00Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        tree_checks::pictures(
            &|_| {},
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &dir,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    /// Card names come from the collection's shared vocabulary; only the numbering is new.
    #[test]
    fn card_names_come_from_the_collections_vocabulary() {
        const KNOWN: &[&str] = &[
            "LFO",
            "Oscillator",
            // The ring modulator as a generator card of its own, beside the oscillators — the
            // first in the collection, and the name the next one takes.
            "Ring mod",
            "Mixer",
            "Filter",
            "Amplifier",
            "Envelope",
            "Voice",
            "Sample & Hold",
            "Routing",
            "FX",
            // The drum machine's and classic verb's output cards; here it heads Volume in the
            // Parameters list, because Volume itself is in the app bar.
            "Output",
        ];
        for title in sections::SECTIONS.iter().map(|s| s.title()) {
            let stem = title.split([' ', '(']).next().unwrap_or(title);
            assert!(
                KNOWN.iter().any(|known| title == *known || stem == *known),
                "{title} is a card name the collection does not use anywhere else"
            );
        }
    }
}
