use egui::{Align2, Sense, Stroke, Ui, pos2, vec2};
use overseer_core::{Ability, Atlas, Kit};
use overseer_ui::{Face, art, caps_text, colour, space};

use super::{Draft, face, side::words};
use crate::controls::{self, Tone};

pub(super) const ANY: &str = "Any Agent";
pub(super) const SPIKE: &str = "Spike";

const TILE: f32 = 38.0;
const FOUND: usize = 12;

pub(super) fn agent(
    ui: &mut Ui,
    atlas: &Atlas,
    draft: &mut Draft,
    (main, default): (Option<&str>, &mut DefaultAgent<'_>),
) {
    if draft.choosing || draft.lineup.agent.is_empty() {
        choose(ui, atlas, draft, main);
        return;
    }
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Label,
            true,
            format!("Agent, {}", draft.lineup.agent),
        )
    });
    let portrait = egui::Rect::from_min_size(rect.min + vec2(0.0, 2.0), vec2(40.0, 40.0));
    face(ui.painter(), portrait, &draft.lineup.agent, 1.0, main);
    let _name = caps_text(
        ui.painter(),
        pos2(portrait.right() + space::LG, rect.center().y),
        Align2::LEFT_CENTER,
        &draft.lineup.agent,
        Face::Heavy.at(18.0),
        colour::TEXT_STRONG,
    );
    let button = egui::Rect::from_min_size(
        pos2(rect.right() - 86.0, rect.center().y - 12.0),
        vec2(86.0, 24.0),
    );
    if ui
        .put(button, |ui: &mut Ui| {
            controls::button(ui, "Change", Tone::Plain, true)
        })
        .clicked()
    {
        draft.choosing = true;
        draft.find.clear();
    }
    if draft.lineup.agent != default.0 {
        let make = egui::Rect::from_min_size(button.min - vec2(126.0, 0.0), vec2(118.0, 24.0));
        if ui
            .put(make, |ui: &mut Ui| {
                controls::button(ui, "Make Default", Tone::Plain, true)
            })
            .clicked()
        {
            *default.1 = Some(draft.lineup.agent.clone());
        }
    }
}

pub(super) type DefaultAgent<'a> = (&'a str, &'a mut Option<String>);

fn choose(ui: &mut Ui, atlas: &Atlas, draft: &mut Draft, main: Option<&str>) {
    let wide = ui.available_width();
    let _find = controls::field(ui, &mut draft.find, "Find an agent", wide);
    let find = draft.find.trim().to_lowercase();
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
        let names = [ANY, SPIKE]
            .into_iter()
            .chain(atlas.agents.iter().map(|k| k.name.as_str()));
        for name in names.filter(|n| find.is_empty() || n.to_lowercase().contains(&find)) {
            if tile(ui, name, draft.lineup.agent == name, main) {
                picked = Some(name.to_owned());
            }
        }
    });
    if let Some(name) = picked {
        let keeps = name == ANY
            || draft
                .lineup
                .ability
                .as_deref()
                .and_then(|a| owner_of(atlas, a))
                .is_some_and(|(k, _)| k.name == name);
        if !keeps {
            draft.lineup.ability = None;
        }
        draft.lineup.agent = name;
        draft.choosing = false;
        draft.find.clear();
    }
}

pub(super) fn tile(ui: &mut Ui, agent: &str, on: bool, main: Option<&str>) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(TILE, TILE), Sense::click());
    let lit = if on || response.hovered() { 1.0 } else { 0.62 };
    face(ui.painter(), rect.shrink(2.0), agent, lit, main);
    if on {
        ui.painter().rect_stroke(
            rect,
            0,
            Stroke::new(2.0, colour::TEXT_STRONG),
            egui::StrokeKind::Inside,
        );
    }
    let response = response
        .on_hover_text(agent)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, agent));
    response.clicked()
}

pub(super) fn utility(ui: &mut Ui, atlas: &Atlas, draft: &mut Draft) {
    let any = draft.lineup.agent == ANY;
    let list: Vec<(&Kit, &Ability)> = if any {
        let wide = ui.available_width();
        let _find = controls::field(
            ui,
            &mut draft.find_ability,
            "Find an ability or ult, like Incendiary",
            wide,
        );
        let find = draft.find_ability.trim().to_lowercase();
        if find.is_empty() {
            let Some(picked) = draft
                .lineup
                .ability
                .as_deref()
                .and_then(|a| owner_of(atlas, a))
            else {
                words(
                    ui,
                    "Type to find any ability or ult in the game.",
                    colour::TEXT_FAINT,
                );
                return;
            };
            vec![picked]
        } else {
            every(atlas)
                .filter(|(k, a)| {
                    k.name.to_lowercase().contains(&find)
                        || a.name
                            .as_deref()
                            .is_some_and(|n| n.to_lowercase().contains(&find))
                })
                .take(FOUND)
                .collect()
        }
    } else {
        every(atlas)
            .filter(|(k, _)| k.name == draft.lineup.agent)
            .collect()
    };
    if list.is_empty() {
        words(ui, "Nothing by that name.", colour::TEXT_FAINT);
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for (kit, ability) in list {
            let name = ability.name.clone().unwrap_or_default();
            let label = if any {
                format!("{} {} {name}", kit.name, ability.key)
            } else {
                format!("{} {name}", ability.key)
            };
            let icon = ability
                .icon
                .as_deref()
                .and_then(|path| art::file(ui.ctx(), path));
            let on = draft.lineup.ability.as_deref() == Some(name.as_str());
            if controls::chip_with(ui, icon.as_ref(), &label, on).clicked() {
                draft.lineup.ability = if on { None } else { Some(name) };
            }
        }
    });
}

fn every(atlas: &Atlas) -> impl Iterator<Item = (&Kit, &Ability)> {
    atlas
        .agents
        .iter()
        .flat_map(|k| k.abilities.iter().map(move |a| (k, a)))
}

fn owner_of<'a>(atlas: &'a Atlas, ability: &str) -> Option<(&'a Kit, &'a Ability)> {
    every(atlas).find(|(_, a)| a.name.as_deref() == Some(ability))
}
