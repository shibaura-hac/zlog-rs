//! The QSO edit dialog and the delete confirmation.

use crate::dialog::{dialog_with_buttons, modal_with_buttons, ok_cancel};
use crate::i18n::{tr, trf};
use zlog_core::qso::QslState;
use zlog_core::{Band, DelphiTime, Mode, Power, Qso};

use crate::main_window::View;
use crate::session::{Session, Status};

/// QSO edit dialog (`TEditDialog`).
pub fn edit_dialog(ctx: &egui::Context, s: &mut Session, v: &mut View) {
    let Some((id, qso)) = v.editing.as_mut() else {
        return;
    };
    let id = *id;
    // Deleted by another station while open: nothing left to edit.
    if s.find(id).is_none() {
        s.status = Status::warning(tr("status.qso_gone"));
        v.editing = None;
        return;
    }
    let (lo, hi) = s.rules().band_range();
    let warc = s.rules().uses_warc();
    // The time being typed, kept between frames until the dialog closes.
    let time_id = egui::Id::unique(("edit_time", id.0));
    let mut time_text: String = ctx
        .data_mut(|d| d.get_temp(time_id))
        .unwrap_or_else(|| qso.time.to_string());
    let mut result: Option<Option<Qso>> = None;
    // Closing the dialog from its title bar is the same as Cancel.
    let mut open = true;
    let shown = dialog_with_buttons(
        ctx,
        "edit",
        if v.inserting {
            tr("edit.insert_title")
        } else {
            tr("edit.title")
        },
        [420.0, 400.0],
        &mut open,
        |ui| {
            egui::Grid::new("edit_grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("column.time"));
                let ok = parse_time(&time_text).is_some();
                ui.add(
                    egui::TextEdit::singleline(&mut time_text)
                        .text_color_opt((!ok).then_some(ui.visuals().error_fg_color)),
                );
                ui.end_row();
                ui.label(tr("column.call"));
                ui.text_edit_singleline(&mut qso.callsign);
                ui.end_row();
                ui.label(tr("edit.rst"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut qso.rst_sent));
                    ui.add(egui::DragValue::new(&mut qso.rst_rcvd));
                });
                ui.end_row();
                ui.label(tr("column.sent"));
                ui.text_edit_singleline(&mut qso.nr_sent);
                ui.end_row();
                ui.label(tr("column.rcvd"));
                ui.text_edit_singleline(&mut qso.nr_rcvd);
                ui.end_row();
                ui.label(tr("column.band"));
                egui::ComboBox::from_id_salt("edit_band")
                    .selected_text(qso.band.label())
                    .show_ui(ui, |ui| {
                        // Only the bands of the contest (WARC only where it counts).
                        for b in Band::range(lo, hi).filter(|b| warc || !b.is_warc()) {
                            ui.selectable_value(&mut qso.band, b, b.label());
                        }
                    });
                ui.end_row();
                ui.label(tr("column.mode"));
                egui::ComboBox::from_id_salt("edit_mode")
                    .selected_text(qso.mode.label())
                    .show_ui(ui, |ui| {
                        for m in Mode::ALL {
                            ui.selectable_value(&mut qso.mode, *m, m.label());
                        }
                    });
                ui.end_row();
                ui.label(tr("edit.power"));
                egui::ComboBox::from_id_salt("edit_power")
                    .selected_text(qso.power.label())
                    .show_ui(ui, |ui| {
                        for p in Power::ALL {
                            ui.selectable_value(&mut qso.power, *p, p.label());
                        }
                    });
                ui.end_row();
                ui.label(tr("edit.tx"));
                ui.add(egui::DragValue::new(&mut qso.tx).range(0..=99));
                ui.end_row();
                ui.label(tr("edit.freq"));
                ui.text_edit_singleline(&mut qso.freq);
                ui.end_row();
                ui.label(tr("edit.qsl"));
                ui.horizontal(|ui| {
                    for (state, key) in [
                        (QslState::None, "edit.qsl_none"),
                        (QslState::PseQsl, "edit.pse_qsl"),
                        (QslState::NoQsl, "edit.no_qsl"),
                    ] {
                        ui.radio_value(&mut qso.qsl_state, state, tr(key));
                    }
                });
                ui.end_row();
                ui.label(tr("column.memo"));
                ui.text_edit_singleline(&mut qso.memo);
                ui.end_row();
                ui.label("");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut qso.cq, tr("edit.cq"));
                    ui.checkbox(&mut qso.dupe, tr("edit.dupe"));
                    ui.checkbox(&mut qso.forced, tr("edit.forced"));
                    ui.checkbox(&mut qso.invalid, tr("edit.invalid"));
                });
                ui.end_row();
            });
        },
        |ui| ok_cancel(ui, true, true),
    );
    match shown.and_then(|(_, pressed)| pressed) {
        Some(true) => {
            // An unreadable time keeps the old one.
            if let Some(t) = parse_time(&time_text) {
                qso.time = t;
            }
            let mut q = qso.clone();
            s.finish_edit(&mut q);
            result = Some(Some(q));
        }
        Some(false) => result = Some(None),
        None => {}
    }
    if !open {
        result = Some(None);
    }
    ctx.data_mut(|d| {
        if result.is_some() {
            d.remove::<String>(time_id);
        } else {
            d.insert_temp(time_id, time_text);
        }
    });
    match result {
        Some(Some(q)) => {
            if v.inserting {
                s.insert_before(id, q);
            } else {
                s.replace(id, q);
            }
            v.editing = None;
        }
        Some(None) => v.editing = None,
        None => {}
    }
}

/// The QSO time typed in the edit dialog, as shown ("2026-06-06 21:05:00"); the
/// seconds may be left out (zLog's date and time pickers).
pub fn parse_time(text: &str) -> Option<DelphiTime> {
    let t = chrono::NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%d %H:%M"))
        .ok()?;
    use chrono::{Datelike, Timelike};
    Some(DelphiTime::from_ymd_hms(
        t.year(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
    ))
}

pub fn confirm_delete(ctx: &egui::Context, s: &mut Session, v: &mut View) {
    let Some(id) = v.confirm_delete else {
        return;
    };
    let Some(index) = s.find(id) else {
        s.status = Status::warning(tr("status.qso_gone"));
        v.confirm_delete = None;
        return;
    };
    let call = s.log().qsos[index].callsign.clone();
    let (_, response) = modal_with_buttons(
        ctx,
        "confirm_delete",
        |ui| ui.label(trf("delete.confirm", &[&call])),
        |ui| ok_cancel(ui, true, true),
    );
    match response {
        Some(true) => {
            s.delete(id);
            v.confirm_delete = None;
            v.selected = if s.log().qsos.is_empty() {
                None
            } else {
                Some(index.min(s.log().qsos.len() - 1))
            };
        }
        Some(false) => v.confirm_delete = None,
        None => {}
    }
}

/// The start-time dialog (`TStartTimeDialog`, opened by `TMainForm.InputStartTime`):
/// OK sets the start time (judging every QSO when asked), Cancel keeps it.
pub fn start_time_dialog(ctx: &egui::Context, s: &mut Session, v: &mut View) {
    let Some(form) = v.start_time.as_mut() else {
        return;
    };
    let valid = form.parse().is_some();
    let new_log = form.new_log;
    let (_, done) = modal_with_buttons(
        ctx,
        "start_time",
        |ui| {
            ui.heading(tr("start.title"));
            egui::Grid::new("start_grid").num_columns(2).show(ui, |ui| {
                let err = (!valid).then_some(ui.visuals().error_fg_color);
                ui.label(tr("start.date"));
                ui.add(egui::TextEdit::singleline(&mut form.date).text_color_opt(err));
                ui.end_row();
                ui.label(tr("start.time"));
                ui.add(egui::TextEdit::singleline(&mut form.time).text_color_opt(err));
                ui.end_row();
            });
            if !new_log {
                ui.checkbox(&mut form.judge, tr("start.judge"));
            }
        },
        // Enter is not OK here: it ends typing the date or the time.
        |ui| {
            ok_cancel(ui, valid, false).or_else(|| {
                ui.input(|i| i.key_pressed(egui::Key::Escape))
                    .then_some(false)
            })
        },
    );
    match done {
        Some(true) => {
            if let Some(start) = form.parse() {
                let judge = form.judge && !form.new_log;
                let changed = s.set_start_time(start, judge);
                s.status = Status::info(trf("status.start_time_set", &[&changed]));
            }
            v.start_time = None;
        }
        Some(false) => {
            s.asks_start_time = false;
            v.start_time = None;
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_round_trips_through_its_text() {
        let t = DelphiTime::from_ymd_hms(2026, 6, 6, 21, 5, 9);
        assert_eq!(parse_time(&t.to_string()), Some(t));
        assert_eq!(
            parse_time(" 2026-06-06 21:05 "),
            Some(DelphiTime::from_ymd_hms(2026, 6, 6, 21, 5, 0))
        );
    }

    #[test]
    fn malformed_time_is_refused() {
        for text in [
            "",
            "21:05",
            "2026-02-30 10:00:00",
            "2026-06-06 24:00:00",
            "garbage",
        ] {
            assert_eq!(parse_time(text), None, "{text}");
        }
    }
}
