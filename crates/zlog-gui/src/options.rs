//! The Options dialog and the rig window.

use egui::{Color32, RichText, Ui};
use zlog_core::qso::QslState;
use zlog_core::{Band, Power, Station};
use zlog_rig::RigMode;
use zlog_rig_cat::MODELS;
use zlog_ui::SettingsPage;

use crate::dialog::{dialog_with_buttons, dialog_with_pages, ok_cancel};
use crate::i18n::{self, tr};
use crate::rig::{RigConfig, RigControl, SIMULATOR};
use crate::settings::{Preferences, Settings};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Station,
    Preferences,
    Rig,
    Extension(usize),
}

/// The Options dialog, editing a copy of the settings until OK.
pub struct Options {
    pub open: bool,
    page: Page,
    station: Station,
    /// zLog's `UseContestPeriod`.
    use_contest_period: bool,
    /// QSY rules, default QSL state and PC name.
    preferences: Preferences,
    rig: RigConfig,
    ports: Vec<String>,
    pages: Vec<Box<dyn SettingsPage>>,
}

impl Options {
    pub fn new(pages: Vec<Box<dyn SettingsPage>>) -> Self {
        Options {
            open: false,
            page: Page::Station,
            station: Station::default(),
            use_contest_period: true,
            preferences: Preferences::default(),
            rig: RigConfig::from_section(&Default::default()),
            ports: Vec::new(),
            pages,
        }
    }

    /// Open with the current settings.
    pub fn show_with(&mut self, settings: &mut Settings) {
        self.station = settings.station();
        self.use_contest_period = settings.use_contest_period();
        self.preferences = settings.preferences();
        self.rig = RigConfig::from_section(settings.section("rig"));
        self.ports = zlog_rig::serial::available_ports()
            .map(|ps| ps.into_iter().map(|p| p.port_name).collect())
            .unwrap_or_default();
        self.open = true;
    }

    /// Draw the dialog; returns true when OK was pressed (settings were updated and saved).
    pub fn ui(&mut self, ctx: &egui::Context, settings: &mut Settings) -> bool {
        let mut open = self.open;
        let titles: Vec<(Page, String)> = [
            (Page::Station, tr("options.station").to_string()),
            (Page::Preferences, tr("options.preferences").to_string()),
            (Page::Rig, tr("options.rig").to_string()),
        ]
        .into_iter()
        .chain(
            self.pages
                .iter()
                .enumerate()
                .map(|(i, p)| (Page::Extension(i), p.title(i18n::current()))),
        )
        .collect();
        let page = &mut self.page;
        let shown = dialog_with_pages(
            ctx,
            "options",
            tr("options.title"),
            [640.0, 560.0],
            &mut open,
            |ui| {
                for (p, title) in &titles {
                    ui.selectable_value(page, *p, title);
                }
                *page
            },
            |ui, page| match page {
                Page::Station => station_page(ui, &mut self.station),
                Page::Preferences => {
                    ui.checkbox(
                        &mut self.use_contest_period,
                        tr("options.use_contest_period"),
                    );
                    ui.add_space(6.0);
                    preferences_page(ui, &mut self.preferences);
                }
                Page::Rig => rig_page(ui, &mut self.rig, &self.ports),
                Page::Extension(i) => {
                    let page = &mut self.pages[i];
                    let section = settings.section(page.id());
                    page.ui(ui, section, i18n::current());
                }
            },
            |ui| ok_cancel(ui, true, false),
        );
        let pressed = shown.and_then(|(_, b)| b);
        let ok = pressed == Some(true);
        if pressed == Some(false) {
            self.open = false;
        }
        if !open {
            self.open = false;
        }
        if ok {
            settings.set_station(&self.station);
            settings.set_use_contest_period(self.use_contest_period);
            settings.set_preferences(&self.preferences);
            self.rig.to_section(settings.section("rig"));
            let _ = settings.save();
            self.open = false;
        }
        ok
    }

    pub fn station(&self) -> &Station {
        &self.station
    }

    pub fn use_contest_period(&self) -> bool {
        self.use_contest_period
    }

    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    pub fn rig(&self) -> &RigConfig {
        &self.rig
    }
}

fn station_page(ui: &mut Ui, st: &mut Station) {
    egui::Grid::new("station").num_columns(2).show(ui, |ui| {
        for (key, value) in [
            ("menu_form.callsign", &mut st.callsign),
            ("menu_form.operator", &mut st.operator),
            ("menu_form.prov", &mut st.prov),
            ("menu_form.city", &mut st.city),
            ("menu_form.age", &mut st.age),
            ("menu_form.cq_zone", &mut st.cq_zone),
            ("menu_form.iaru_zone", &mut st.iaru_zone),
            ("menu_form.itu_region", &mut st.itu_region),
            ("menu_form.iota", &mut st.iota),
        ] {
            ui.label(tr(key));
            ui.text_edit_singleline(value);
            *value = value.to_uppercase();
            ui.end_row();
        }
        // Handles (`_myhandle_cw`, `_myhandle_ph`, macro `$H`), kept as typed.
        for (key, value) in [
            ("menu_form.handle_cw", &mut st.handle_cw),
            ("menu_form.handle_ph", &mut st.handle_ph),
        ] {
            ui.label(tr(key));
            ui.text_edit_singleline(value);
            ui.end_row();
        }
        ui.label(tr("menu_form.tx_nr"));
        ui.add(egui::DragValue::new(&mut st.tx_nr).range(0..=15));
        ui.end_row();
    });
    ui.add_space(6.0);
    ui.label(RichText::new(tr("options.power_per_band")).strong());
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            egui::Grid::new("power").striped(true).show(ui, |ui| {
                for band in Band::range(Band::B1_9, Band::HIGHEST) {
                    ui.monospace(band.label());
                    let mut power = st.power_for(band);
                    for code in ['H', 'M', 'L', 'P'] {
                        let p = Power::from_code(code).unwrap();
                        ui.radio_value(&mut power, p, code.to_string());
                    }
                    if power != st.power_for(band) {
                        st.power.insert(band, power);
                    }
                    ui.end_row();
                }
            });
        });
}

/// The QSY rules (UOptions2.pas `groupQsyAssist`: `CountDown`, `QSYCount`, 1 to 99),
/// the default QSL state (`groupMyQslDefault`) and the PC name (`[Z-Link] PCName`).
fn preferences_page(ui: &mut Ui, p: &mut Preferences) {
    egui::Grid::new("preferences")
        .num_columns(2)
        .show(ui, |ui| {
            let mut on = p.qsy_rule.countdown_minutes.is_some();
            ui.checkbox(&mut on, tr("options.countdown"));
            ui.add_enabled(
                on,
                egui::DragValue::new(&mut p.countdown_minutes).range(1..=99),
            );
            p.set_countdown(on);
            ui.end_row();
            let mut on = p.qsy_rule.max_qsy_per_hour.is_some();
            ui.checkbox(&mut on, tr("options.qsy_count"));
            ui.add_enabled(on, egui::DragValue::new(&mut p.qsy_per_hour).range(1..=99));
            p.set_qsy_count(on);
            ui.end_row();
            ui.label(tr("options.qsl_default"));
            ui.horizontal(|ui| {
                for (state, key) in [
                    (QslState::None, "options.qsl_none"),
                    (QslState::PseQsl, "options.qsl_pse"),
                    (QslState::NoQsl, "options.qsl_no"),
                ] {
                    ui.radio_value(&mut p.qsl_default, state, tr(key));
                }
            });
            ui.end_row();
            ui.label(tr("options.pc_name"));
            ui.text_edit_singleline(&mut p.pc_name);
            ui.end_row();
        });
}

fn rig_page(ui: &mut Ui, rig: &mut RigConfig, ports: &[String]) {
    egui::Grid::new("rig").num_columns(2).show(ui, |ui| {
        ui.label(tr("rig.model"));
        egui::ComboBox::from_id_salt("rig_model")
            .selected_text(MODELS[rig.model].name)
            .show_ui(ui, |ui| {
                for (i, m) in MODELS.iter().enumerate() {
                    if ui.selectable_value(&mut rig.model, i, m.name).clicked() {
                        rig.baud = m.baud;
                    }
                }
            });
        ui.end_row();
        ui.label(tr("rig.port"));
        let shown = if rig.port == SIMULATOR {
            tr("rig.simulator").to_string()
        } else {
            rig.port.clone()
        };
        egui::ComboBox::from_id_salt("rig_port")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut rig.port, SIMULATOR.to_string(), tr("rig.simulator"));
                for p in ports {
                    ui.selectable_value(&mut rig.port, p.clone(), p);
                }
            });
        ui.end_row();
        ui.label(tr("rig.baud"));
        egui::ComboBox::from_id_salt("rig_baud")
            .selected_text(rig.baud.to_string())
            .show_ui(ui, |ui| {
                for b in [4800, 9600, 19200, 38400, 57600, 115200] {
                    ui.selectable_value(&mut rig.baud, b, b.to_string());
                }
            });
        ui.end_row();
        ui.label(tr("rig.transverter"));
        ui.add(egui::DragValue::new(&mut rig.transverter_khz).range(0..=300_000_000));
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut rig.init_freq_on_mode, tr("rig.init_freq_on_mode"));
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut rig.ignore_rig_mode, tr("rig.ignore_rig_mode"));
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut rig.rtty_keeps_rig_mode, tr("rig.rtty_keeps_rig_mode"));
        ui.end_row();
    });
}

fn khz(hz: Option<u64>) -> String {
    hz.map(|h| format!("{:.1}", h as f64 / 1000.0))
        .unwrap_or_else(|| "-".into())
}

/// The rig window: what the rig reports, connect and disconnect, and the controls of
/// the simulated radio when it is in use.
pub fn rig_window(ctx: &egui::Context, rig: &mut RigControl, open: &mut bool, config: &RigConfig) {
    let connected = rig.connected();
    let shown = dialog_with_buttons(
        ctx,
        "rig",
        tr("window.rig"),
        [380.0, 300.0],
        open,
        |ui| rig_state(ui, rig, config),
        |ui| {
            let key = if connected {
                "rig.disconnect"
            } else {
                "rig.connect"
            };
            ui.button(tr(key)).clicked()
        },
    );
    if shown.is_some_and(|(_, clicked)| clicked) {
        if connected {
            rig.disconnect();
        } else {
            rig.connect(config.clone());
        }
    }
}

/// What the rig reports, and the controls of the simulated radio when it is in use.
fn rig_state(ui: &mut Ui, rig: &mut RigControl, config: &RigConfig) {
    {
        if config.port == crate::rig::SIMULATOR {
            ui.label(RichText::new(tr("rig.simulator")).strong());
        } else {
            ui.label(RichText::new(MODELS[config.model].name).strong());
            // The settings Connect uses; they are changed in Options.
            ui.label(format!(
                "{}: {}   {}: {}",
                tr("rig.port"),
                config.port,
                tr("rig.baud"),
                config.baud
            ));
        }
        if rig.connected() {
            let st = rig.state().unwrap_or_default();
            egui::Grid::new("rig_state").num_columns(2).show(ui, |ui| {
                ui.label(tr("rig.vfo_a"));
                ui.monospace(khz(st.freq_a));
                ui.end_row();
                ui.label(tr("rig.vfo_b"));
                ui.monospace(khz(st.freq_b));
                ui.end_row();
                ui.label(tr("rig.mode"));
                ui.monospace(
                    st.mode
                        .map(|m| format!("{m:?}"))
                        .unwrap_or_else(|| "-".into()),
                );
                ui.end_row();
            });
            if let Some(e) = rig.error() {
                ui.colored_label(Color32::RED, e);
            }
        } else {
            ui.weak(tr("rig.not_connected"));
            if let Some(m) = &rig.message {
                ui.colored_label(Color32::RED, m);
            }
        }
        if let Some(sim) = rig.sim.clone() {
            ui.separator();
            ui.label(RichText::new(tr("rig.simulator")).strong());
            let mut khz = sim.with(|r| match r.vfo {
                zlog_rig::Vfo::A => r.freq_a,
                zlog_rig::Vfo::B => r.freq_b,
            }) as f64
                / 1000.0;
            ui.horizontal(|ui| {
                ui.label(tr("rig.sim.dial"));
                let dial = egui::DragValue::new(&mut khz)
                    .speed(0.1)
                    .range(1800.0..=10_250_000.0)
                    .fixed_decimals(1);
                if ui.add(dial).changed() {
                    sim.tune((khz * 1000.0).round() as u64);
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("rig.sim.mode"));
                for m in [
                    RigMode::Cw,
                    RigMode::Lsb,
                    RigMode::Usb,
                    RigMode::Fm,
                    RigMode::Am,
                    RigMode::Rtty,
                ] {
                    if ui.button(format!("{m:?}")).clicked() {
                        sim.press_mode(m);
                    }
                }
            });
            let mut on = sim.with(|r| r.power_on);
            if ui.checkbox(&mut on, tr("rig.sim.power")).changed() {
                sim.with(|r| r.power_on = on);
            }
        }
    }
}
