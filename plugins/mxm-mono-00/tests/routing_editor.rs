//! Exercise the shipped panel, not the layout lab or the routing helper in isolation.
//! A passive caption has the same words, but cannot satisfy these click/gesture assertions.
//!
//! **What these test changed with the conversion.** Before it, every input was a fifteen-value
//! dropdown and the sweep was 225 target/source choices. There is no dropdown now: a target has one
//! slot per source, so what a player does is *add* a source to a target and *remove* it again, and
//! the sweep is the 433 offered pairs those two gestures reach — the patch bay's, and since Rev 3 the
//! panel's own wiring too, so no card anywhere chooses where its output goes. Each is one parameter write — that is the
//! whole of what makes a removal undoable and what makes the player read it as a step lock rather
//! than a preset load — and it is what the gesture assertions here pin.

// A *(target, source)* pair **is** an index pair: the two loops below walk the routing grid, and
// both indices are used for more than reaching one array — `ROUTE_IDS`, the two name tables and the
// parameter lookups all key on them.
#![allow(clippy::needless_range_loop)]

use std::collections::HashMap;
use std::sync::Mutex;

use egui::{Rect, ThemePreference, vec2};
use kittest::Queryable;
use mxm_modulation::standard::Offer;
use mxm_mono_00::editor::{
    self, PresetUi,
    sections::{self, Section},
};
use mxm_mono_00::params::MxmMono00Params;
use mxm_mono_00::routes::ROUTE_IDS;
use mxm_mono_00::telemetry::Telemetry;
use mxm_mono_00_dsp::routing::{
    SOURCE_NAMES, SOURCES, TARGET_NAMES, TARGETS, offer, source, target,
};
use nice_plug::prelude::*;

#[derive(Default)]
struct ApplyingHost(Mutex<Vec<(String, &'static str)>>);

impl ApplyingHost {
    fn record(&self, param: ParamPtr, action: &'static str) {
        self.0
            .lock()
            .unwrap()
            .push((unsafe { param.name() }.to_owned(), action));
    }

    fn take(&self) -> Vec<(String, &'static str)> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
        self.record(param, "begin");
    }
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, value: f32) {
        self.record(param, "set");
        unsafe {
            param._internal_set_normalized_value(value);
        }
    }
    unsafe fn raw_end_set_parameter(&self, param: ParamPtr) {
        self.record(param, "end");
    }
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _: PluginState) {}
}

/// The card that owns a target's stack.
fn owner(target: usize) -> Section {
    *sections::SECTIONS
        .iter()
        .find(|s| s.targets().contains(&target))
        .unwrap_or_else(|| panic!("{} is on no card", TARGET_NAMES[target]))
}

fn target_key(section: Section) -> mxm_ui::paging::Key {
    // A card's key is its permanent id, not its position: see `sections::SYNTH_KEYS`.
    mxm_ui::paging::Key(
        sections::SYNTH
            .iter()
            .position(|s| *s == section)
            .map_or(11, |i| sections::SYNTH_KEYS[i]),
    )
}
fn request_target(ctx: &egui::Context, section: Section) {
    mxm_ui::paging::editor::request_card(ctx, target_key(section));
}
fn target_rect(ctx: &egui::Context, section: Section) -> Rect {
    mxm_ui::paging::editor::report(ctx)
        .unwrap()
        .visible
        .into_iter()
        .find(|(key, _)| *key == target_key(section))
        .expect("requested target card is drawn")
        .1
}

/// The `‹ modulate ›` menu's accessible name: **the painted name**, then the resting option.
///
/// A row's slider and its removal announce the canonical name — *"Oscillator 2 pitch from LFO 1"*
/// — while the menu's caption is what the card paints, which drops the prefix the card title
/// already carries. Both are checked here, which is what keeps the two from drifting into one.
fn menu_label(target: usize) -> String {
    format!(
        "{}: modulate",
        mxm_mono_00_dsp::routing::TARGET_PANEL_NAMES[target]
    )
}

/// A route row's slider, and the cross that removes it.
fn row_label(target: usize, source: usize) -> String {
    format!("{} from {}", TARGET_NAMES[target], SOURCE_NAMES[source])
}
fn remove_label(target: usize, source: usize) -> String {
    format!(
        "Remove {} from {}",
        SOURCE_NAMES[source], TARGET_NAMES[target]
    )
}

/// A parameter by its **permanent id**, for the writes a host would make.
///
/// Going through `param_map` rather than through a field is deliberate: it is the same lookup a
/// preset load and a control-map role use, so a pair whose id moved would fail here too.
fn ptr(params: &MxmMono00Params, id: &str) -> ParamPtr {
    params
        .param_map()
        .into_iter()
        .find(|(pid, _, _)| pid == id)
        .map(|(_, p, _)| p)
        .unwrap_or_else(|| panic!("no parameter {id}"))
}

/// Every parameter pair for one target, as the panel edits them.
fn pair<'a>(
    params: &'a MxmMono00Params,
    target: usize,
    source: usize,
) -> mxm_modulation_params::Route<'a> {
    let (index, group) = params.routes.all()[target];
    assert_eq!(index, target);
    group.routes(target).into_iter().nth(source).unwrap()
}

/// The source each target's gestures are exercised with: the menu's last, the one furthest down a
/// list that scrolls. **One per target, not all twenty-five**: the gesture is the shared stack's,
/// the same code for every source, so a second source proves nothing the first did not — and every
/// pair drawn for every target cost twenty thousand frames of the whole editor.
/// `every_pair_is_the_parameter_its_row_names` checks all 450 as data.
const SWEPT: usize = SOURCES - 1;

/// **Every target's routes, added and removed through the shipped panel.**
///
/// Two gestures per target, each exactly one `begin`/`set`/`end` on one parameter, with every other
/// pair untouched — because a routing edit that quietly moves a neighbour is the failure this
/// instrument's matrix was always most exposed to. In one theme: a theme is colour, never geometry.
#[test]
fn every_target_adds_and_removes_a_source_through_one_host_gesture() {
    let theme = ThemePreference::Light;
    for t in 0..TARGETS {
        let section = owner(t);
        let params = MxmMono00Params::default();
        let telemetry = Telemetry::default();
        let host = ApplyingHost::default();
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(mxm_mono_00::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(vec2(1880.0, 960.0))
            .build_ui(|ui| {
                mxm_ui::theme::apply(ui.ctx());
                mxm_ui::typography::apply(ui.ctx());
                ui.ctx().set_theme(theme);
                editor::panel(
                    ui,
                    &params,
                    &telemetry,
                    &ParamSetter::new(&host),
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
        request_target(&harness.ctx, section);
        harness.run_steps(3);
        host.take();

        // Start from an empty input, so the `‹ modulate ›` list offers every source in a known
        // order.
        for s in 0..SOURCES {
            let route = pair(&params, t, s);
            if route.is_present() {
                unsafe { ptr(&params, ROUTE_IDS[t][s].1)._internal_set_normalized_value(0.0) };
            }
        }
        harness.run_steps(3);
        host.take();
        let before: Vec<bool> = (0..TARGETS)
            .flat_map(|o| (0..SOURCES).map(move |s| (o, s)))
            .map(|(o, s)| pair(&params, o, s).is_present())
            .collect();

        let s = SWEPT;
        // Cross a row breakpoint, then return to a window where every card can be clicked.
        harness.set_size(vec2(1400.0, 1080.0));
        harness.run_steps(3);
        harness.set_size(if t % 2 == 0 {
            vec2(1920.0, 1080.0)
        } else {
            vec2(1880.0, 960.0)
        });
        harness.run_steps(3);
        request_target(&harness.ctx, section);
        harness.run_steps(3);

        let card = target_rect(&harness.ctx, section);
        let label = menu_label(t);
        let node = harness
            .get_all_by_role_and_label(egui::accesskit::Role::ComboBox, &label)
            .find(|n| card.contains(n.rect().center()))
            .unwrap_or_else(|| panic!("{}: no `modulate` menu on {section:?}", TARGET_NAMES[t]));
        assert!(
            node.rect().height() >= mxm_ui::space::MIN_TARGET,
            "{}: menu smaller than the pointer floor",
            TARGET_NAMES[t]
        );
        assert!(
            card.contains_rect(node.rect()),
            "{}: menu {:?} leaves its card {card:?}",
            TARGET_NAMES[t],
            node.rect()
        );
        let resting = node.rect();
        node.hover();
        harness.run_steps(2);
        let hovered = harness
            .get_all_by_role_and_label(egui::accesskit::Role::ComboBox, &label)
            .find(|n| card.contains(n.rect().center()))
            .unwrap();
        assert_eq!(
            hovered.rect(),
            resting,
            "{}: hover moved the menu",
            TARGET_NAMES[t]
        );
        hovered.click();
        harness.run_steps(3);
        // **Twenty-five sources is a menu that scrolls**, and on a card low on the page its
        // last options open below the window. A player scrolls to them, so the test does too
        // — `mxm-mono-pr1`'s proofs reach a long list the same way.
        harness
            .get_by_role_and_label(egui::accesskit::Role::Button, SOURCE_NAMES[s])
            .scroll_to_me();
        harness.run_steps(3);
        harness
            .get_by_role_and_label(egui::accesskit::Role::Button, SOURCE_NAMES[s])
            .click();
        harness.run_steps(3);

        let route = pair(&params, t, s);
        assert!(
            route.is_present(),
            "{}: adding {} did not set its presence",
            TARGET_NAMES[t],
            SOURCE_NAMES[s]
        );
        assert_eq!(
            host.take(),
            ["begin", "set", "end"].map(|a| (route.present.name().to_owned(), a)),
            "{}: adding {} is one host gesture",
            TARGET_NAMES[t],
            SOURCE_NAMES[s]
        );

        // The row it revealed: a slider named for the target and its source, inside the card
        // as it is drawn now. **The card has grown**: the row opens under its target's title,
        // so the first route on a card's last stack lands below where the card used to end.
        let card = target_rect(&harness.ctx, section);
        let row = row_label(t, s);
        let slider = harness
            .get_all_by_role_and_label(egui::accesskit::Role::Slider, &row)
            .find(|n| card.contains(n.rect().center()));
        assert!(
            slider.is_some(),
            "{}: adding {} drew no row",
            TARGET_NAMES[t],
            SOURCE_NAMES[s]
        );

        // And the removal beside it: one write, and the depth is deliberately left alone.
        let depth = route.amount.normalised();
        harness
            .get_by_role_and_label(egui::accesskit::Role::Button, &remove_label(t, s))
            .click();
        harness.run_steps(3);
        let route = pair(&params, t, s);
        assert!(
            !route.is_present(),
            "{}: removing {} did not clear its presence",
            TARGET_NAMES[t],
            SOURCE_NAMES[s]
        );
        assert_eq!(
            route.amount.normalised(),
            depth,
            "{}: removing {} moved its depth",
            TARGET_NAMES[t],
            SOURCE_NAMES[s]
        );
        assert_eq!(
            host.take(),
            ["begin", "set", "end"].map(|a| (route.present.name().to_owned(), a)),
            "{}: removing {} is one host gesture",
            TARGET_NAMES[t],
            SOURCE_NAMES[s]
        );

        let after: Vec<bool> = (0..TARGETS)
            .flat_map(|o| (0..SOURCES).map(move |s| (o, s)))
            .map(|(o, s)| pair(&params, o, s).is_present())
            .collect();
        assert_eq!(
            before, after,
            "{}: the gestures left another pair's presence moved",
            TARGET_NAMES[t]
        );
    }
}

/// **All 450 pairs, as data** — a refused one registered nowhere — each offered one's stack reading, for every source, the parameters whose
/// permanent ids the table gives, under the name its row and its removal announce. This is what the
/// gesture test's one source per target leaves to be shown, and it needs no drawing.
#[test]
fn every_pair_is_the_parameter_its_row_names() {
    let params = MxmMono00Params::default();
    for t in 0..TARGETS {
        for s in 0..SOURCES {
            let route = pair(&params, t, s);
            let (amount, present) = ROUTE_IDS[t][s];
            // A pair the modulation standard refuses is no parameter at all.
            if offer(t, s) == Offer::Refused {
                let registered = params.param_map();
                assert!(
                    !registered
                        .iter()
                        .any(|(id, _, _)| id == amount || id == present),
                    "{} is refused, but a parameter is registered",
                    row_label(t, s)
                );
                continue;
            }
            assert_eq!(route.amount_id, amount, "{}", row_label(t, s));
            assert_eq!(route.present_id, present, "{}", row_label(t, s));
            // The id a host writes reaches the very parameter the row edits.
            assert_eq!(
                unsafe { ptr(&params, amount).name().to_owned() },
                route.amount.name()
            );
            assert_eq!(
                unsafe { ptr(&params, present).name().to_owned() },
                route.present.name()
            );
            assert_eq!(route.amount.name(), row_label(t, s));
        }
    }
}

/// **A removal is undoable with no undo stack**, which is the whole reason it does not touch the
/// amount: adding the source back restores the depth the player last set.
#[test]
fn removing_a_source_and_adding_it_back_restores_the_depth() {
    let params = MxmMono00Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let mut view = 0usize;
    let mut text_entry = HashMap::new();
    let mut presets = PresetUi::at(mxm_mono_00::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1880.0, 960.0))
        .build_ui(|ui| {
            mxm_ui::theme::apply(ui.ctx());
            mxm_ui::typography::apply(ui.ctx());
            editor::panel(
                ui,
                &params,
                &telemetry,
                &ParamSetter::new(&host),
                &mut view,
                &mut text_entry,
                &mut presets,
                &mut nav,
            );
        });
    let (t, s) = (target::CUTOFF, source::LFO1);
    request_target(&harness.ctx, owner(t));
    harness.run_steps(3);

    // A depth the player set, on the plug-out's own normalled route.
    unsafe { ptr(&params, ROUTE_IDS[t][s].0)._internal_set_normalized_value(0.8) };
    harness.run_steps(3);
    host.take();

    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, &remove_label(t, s))
        .click();
    harness.run_steps(3);
    assert!(!pair(&params, t, s).is_present());
    assert_eq!(pair(&params, t, s).amount.normalised(), 0.8);
    // The row is gone with it: an absent route draws nothing at all.
    assert!(
        harness
            .query_all_by_role_and_label(egui::accesskit::Role::Slider, &row_label(t, s))
            .next()
            .is_none(),
        "a removed route still draws its row"
    );

    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, &menu_label(t))
        .click();
    harness.run_steps(3);
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, SOURCE_NAMES[s])
        .click();
    harness.run_steps(3);
    assert!(pair(&params, t, s).is_present());
    assert_eq!(
        pair(&params, t, s).amount.normalised(),
        0.8,
        "re-adding the source did not restore the depth"
    );
}

/// **A target with nothing routed draws no group at all**, and its `‹ modulate ›` menu is what is
/// left — the owner's first ruling on the pilot's interface, checked on the shipped panel here.
#[test]
fn an_unrouted_target_draws_a_menu_and_no_rows() {
    let params = MxmMono00Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let mut view = 0usize;
    let mut text_entry = HashMap::new();
    let mut presets = PresetUi::at(mxm_mono_00::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1880.0, 960.0))
        .build_ui(|ui| {
            mxm_ui::theme::apply(ui.ctx());
            mxm_ui::typography::apply(ui.ctx());
            editor::panel(
                ui,
                &params,
                &telemetry,
                &ParamSetter::new(&host),
                &mut view,
                &mut text_entry,
                &mut presets,
                &mut nav,
            );
        });
    // LFO 1's rate input has no internal connection, so a fresh instance wires nothing to it.
    let t = target::LFO1_RATE;
    request_target(&harness.ctx, owner(t));
    harness.run_steps(3);
    // **Two cards share a painted name** — both LFOs' rate inputs read `Rate`, because each is
    // unambiguous inside its own card and that is the whole point of the painted form. So the menu
    // is found inside the card that owns it rather than by name alone.
    let card = target_rect(&harness.ctx, owner(t));
    assert!(
        harness
            .query_all_by_role_and_label(egui::accesskit::Role::ComboBox, &menu_label(t))
            .any(|n| card.contains(n.rect().center())),
        "{} has no `modulate` menu on its card",
        TARGET_NAMES[t]
    );
    for s in 0..SOURCES {
        assert!(
            harness
                .query_all_by_role_and_label(egui::accesskit::Role::Slider, &row_label(t, s))
                .next()
                .is_none(),
            "{} draws a row for {} with nothing routed",
            TARGET_NAMES[t],
            SOURCE_NAMES[s]
        );
    }
    assert!(
        host.take().is_empty(),
        "drawing an unrouted target must not edit"
    );
}

/// The stack belongs to the control it moves, not to a detached list at the card's foot: it stays
/// inside its own card at every width the panel reflows through. **Its first and its last row** are
/// checked: a stack is one column, so what lies between them lies inside too — every row of every
/// stack was 433 look-ups, each walking an accessibility tree holding all 433.
#[test]
fn every_routing_stack_stays_inside_the_card_that_owns_it_after_reflow() {
    for theme in [ThemePreference::Light] {
        let params = MxmMono00Params::default();
        let telemetry = Telemetry::default();
        let host = ApplyingHost::default();
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(mxm_mono_00::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(vec2(1880.0, 960.0))
            .build_ui(|ui| {
                mxm_ui::theme::apply(ui.ctx());
                mxm_ui::typography::apply(ui.ctx());
                ui.ctx().set_theme(theme);
                editor::panel(
                    ui,
                    &params,
                    &telemetry,
                    &ParamSetter::new(&host),
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
        // **Every route revealed**, because an absent one draws nothing: at Init thirteen of 433
        // pairs exist, so a check that only paints the init patch covers three per cent of the
        // surface.
        for t in 0..TARGETS {
            for s in (0..SOURCES).filter(|&s| offer(t, s) != Offer::Refused) {
                unsafe { ptr(&params, ROUTE_IDS[t][s].1)._internal_set_normalized_value(1.0) };
            }
        }
        for width in [384.0, 700.0, 1880.0] {
            // Tall headless canvas observes all narrow rows; not a physical/DPI fit claim.
            harness.set_size(vec2(width, if width < 1880.0 { 12_000.0 } else { 960.0 }));
            harness.run_steps(3);
            for t in 0..TARGETS {
                let section = owner(t);
                request_target(&harness.ctx, section);
                harness.run_steps(3);
                let card = target_rect(&harness.ctx, section);
                for s in [0, SOURCES - 1] {
                    let row = row_label(t, s);
                    let cross_label = remove_label(t, s);
                    let Some(node) = harness
                        .get_all_by_role_and_label(egui::accesskit::Role::Slider, &row)
                        .find(|n| card.contains(n.rect().center()))
                    else {
                        panic!(
                            "{} at {width}: no row for {}",
                            TARGET_NAMES[t], SOURCE_NAMES[s]
                        )
                    };
                    assert!(
                        card.contains_rect(node.rect()),
                        "{} at {width}: {}'s row {:?} leaves its card {card:?}",
                        TARGET_NAMES[t],
                        SOURCE_NAMES[s],
                        node.rect()
                    );
                    // The removal is on the same row, not pushed past the border — the defect the
                    // owner found on the pilot, which is why the row reserves its width first.
                    let cross = harness
                        .get_all_by_role_and_label(egui::accesskit::Role::Button, &cross_label)
                        .find(|n| card.contains(n.rect().center()))
                        .unwrap_or_else(|| {
                            panic!(
                                "{} at {width}: {}'s row has no removal inside the card",
                                TARGET_NAMES[t], SOURCE_NAMES[s]
                            )
                        });
                    assert!(
                        card.contains_rect(cross.rect()),
                        "{} at {width}: {}'s removal {:?} leaves its card {card:?}",
                        TARGET_NAMES[t],
                        SOURCE_NAMES[s],
                        cross.rect()
                    );
                }
            }
        }
        assert!(host.take().is_empty(), "drawing must not emit edits");
    }
}

/// **No card chooses where its output goes, and no source is chosen by a switch.** The plug-out's
/// DESTINATION ("Glide to", "Vibrato to"), its PWM switches and depths, its fixed VCA LFO and
/// KYBD CV, and SAMPLE MODE are all routes now, chosen on the card they move. A passive caption
/// with one of those names would satisfy nothing here, and a control carrying one fails it.
#[test]
fn no_card_chooses_a_destination_and_no_source_is_chosen_by_a_switch() {
    let retired = [
        "Glide",
        "Glide to",
        "Vibrato",
        "Vibrato to",
        "PWM source 1",
        "PWM source 2",
        "PWM 1",
        "PWM 2",
        "Tremolo",
        "Key follow",
        "S&H mode",
    ];
    let params = MxmMono00Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let mut view = 0usize;
    let mut text_entry = HashMap::new();
    let mut presets = PresetUi::at(mxm_mono_00::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1920.0, 1080.0))
        .build_ui(|ui| {
            mxm_ui::theme::apply(ui.ctx());
            mxm_ui::typography::apply(ui.ctx());
            editor::panel(
                ui,
                &params,
                &telemetry,
                &ParamSetter::new(&host),
                &mut view,
                &mut text_entry,
                &mut presets,
                &mut nav,
            );
        });
    for section in sections::SYNTH {
        request_target(&harness.ctx, section);
        harness.run_steps(3);
        for role in [
            egui::accesskit::Role::ComboBox,
            egui::accesskit::Role::Slider,
            egui::accesskit::Role::SpinButton,
        ] {
            for name in retired {
                assert!(
                    harness
                        .query_all_by_role_and_label(role, name)
                        .next()
                        .is_none(),
                    "{section:?} draws a {role:?} named {name:?}"
                );
            }
        }
        // And every stack the card owns is there to choose a source with.
        let card = target_rect(&harness.ctx, section);
        for &t in section.targets() {
            assert!(
                harness
                    .get_all_by_role_and_label(egui::accesskit::Role::ComboBox, &menu_label(t))
                    .any(|n| card.contains(n.rect().center())),
                "{}: no `modulate` menu on {section:?}",
                TARGET_NAMES[t]
            );
        }
    }
    assert!(host.take().is_empty(), "drawing must not emit edits");
}

fn painted_text(output: &egui::FullOutput) -> Vec<(String, Rect)> {
    fn collect(shape: &egui::Shape, out: &mut Vec<(String, Rect)>) {
        match shape {
            egui::Shape::Text(text) => out.push((
                text.galley.job.text.clone(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut out);
    }
    out
}

#[test]
fn navigation_has_no_patch_page_and_sample_hold_is_editable_on_synth() {
    let params = MxmMono00Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let mut view = 0;
    let mut text_entry = HashMap::new();
    let mut presets = PresetUi::at(mxm_mono_00::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1880.0, 960.0))
        .build_ui(|ui| {
            mxm_ui::theme::apply(ui.ctx());
            mxm_ui::typography::apply(ui.ctx());
            editor::panel(
                ui,
                &params,
                &telemetry,
                &ParamSetter::new(&host),
                &mut view,
                &mut text_entry,
                &mut presets,
                &mut nav,
            );
        });
    mxm_ui::paging::editor::request_card(&harness.ctx, mxm_ui::paging::Key(3));
    harness.run_steps(3);
    // The real S&H body and all three controls must be present, not just a new card heading — on
    // its card, which paints *Rate* and *Lag* because it already says *Sample & Hold*.
    let card = target_rect(&harness.ctx, Section::SampleHold);
    let texts = painted_text(harness.output());
    for label in ["Sample & Hold", "Rate", "Lag"] {
        assert!(
            texts.iter().any(|(text, rect)| text == label
                && card.contains_rect(*rect)
                && Rect::from_min_size(egui::Pos2::ZERO, vec2(1880.0, 960.0)).contains_rect(*rect)),
            "missing or clipped {label}"
        );
    }
    // What is sampled is chosen on the card as a source — the retired SAMPLE MODE's SAW position
    // is LFO 1's saw on the S&H input.
    harness
        .get_all_by_role_and_label(
            egui::accesskit::Role::ComboBox,
            &menu_label(target::SH_INPUT),
        )
        .find(|n| card.contains(n.rect().center()))
        .expect("the S&H input's menu is on its card")
        .click();
    harness.run_steps(3);
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::Button,
            SOURCE_NAMES[source::LFO1_CORE_SAW],
        )
        .click();
    harness.run_steps(3);
    let route = pair(&params, target::SH_INPUT, source::LFO1_CORE_SAW);
    assert!(route.is_present());
    assert_eq!(
        *host.0.lock().unwrap(),
        ["begin", "set", "end"].map(|action| (route.present.name().to_owned(), action))
    );
    host.0.lock().unwrap().clear();

    // ViewBar paints its labels: click their actual rectangles rather than assuming tab geometry.
    for (key, body) in [(11, "Phaser"), (3, "Sample & Hold")] {
        let report = mxm_ui::paging::editor::report(&harness.ctx).unwrap();
        let tab = report
            .plan
            .pages
            .iter()
            .find(|p| p.cards.contains(&mxm_ui::paging::Key(key)))
            .unwrap()
            .label
            .clone();
        let texts = painted_text(harness.output());
        assert!(
            !texts.iter().any(|(text, _)| text == "Parameters"),
            "Parameters is not a musician tab"
        );
        assert!(
            !texts.iter().any(|(text, _)| text == "Patch"),
            "the removed Patch tab returned"
        );
        let rect = texts.iter().find(|(text, _)| text == &tab).unwrap().1;
        harness.hover_at(rect.center());
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        harness.run_steps(3);
        assert!(
            painted_text(harness.output())
                .iter()
                .any(|(text, _)| text == body),
            "{tab} did not show {body}"
        );
        assert_eq!(
            harness.query_all_by_label_contains(" › ").count(),
            0,
            "matrix cells must not exist on any page"
        );
    }
    // Stable category addresses, a separate Parameters address, and no clamping of invalid CCs.
    for (index, body) in [
        (0, "Voice"),
        (1, "LFO 1"),
        (3, "Oscillator 1"),
        (5, "Phaser"),
        (127, "Routing"),
        (126, "Routing"),
    ] {
        telemetry.request_view(index);
        harness.run_steps(3);
        let texts = painted_text(harness.output());
        assert!(
            texts.iter().any(|(text, _)| text == body),
            "developer page {index}"
        );
        assert!(!texts.iter().any(|(text, _)| text == "Patch"));
    }
    assert!(
        host.0.lock().unwrap().is_empty(),
        "navigation must not alter the patch"
    );
}
