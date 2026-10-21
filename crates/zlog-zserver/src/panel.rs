//! The Z-Server window: connection settings (`TformZLinkTelnetSet`), the console
//! (`TZLinkForm`), the chat (`TChatForm`) and the stations' rig status (`TFreqList`).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, ScrollArea, Ui};
use zlog_core::{Band, Mode};
use zlog_ui::{Panel, Section, View};

use crate::session::{LineBreak, OnConnect, Prompt, Session, State};
use crate::{protocol, texts};

/// `hh:nn:ss` of `now` as zLog's `CurrentTime`: UTC when the log records UTC,
/// local time otherwise.
fn freq_time(log: &zlog_core::Log, now: chrono::DateTime<chrono::Utc>) -> String {
    const FORMAT: &str = "%H:%M:%S";
    match log.utc_offset_minutes() {
        None => now.format(FORMAT).to_string(),
        Some(_) => now.with_timezone(&chrono::Local).format(FORMAT).to_string(),
    }
}

pub struct ZServerPanel {
    session: Arc<Mutex<Session>>,
    input: String,
    notice: Option<&'static str>,
    /// Rig status (band, hertz, mode, CQ) last sent with `FREQ`, and when.
    freq_sent: Option<((Band, u64, Mode, bool), Instant)>,
}

impl Default for ZServerPanel {
    fn default() -> Self {
        ZServerPanel::new(crate::shared())
    }
}

impl ZServerPanel {
    pub fn new(session: Arc<Mutex<Session>>) -> Self {
        ZServerPanel {
            session,
            input: String::new(),
            notice: None,
            freq_sent: None,
        }
    }

    /// `TZLinkForm.SendRigStatus`: the entry row's band ordinal, the rig's band, frequency and mode, at most once a
    /// second, when they change (zLog sends on every rig poll), with the CQ/SP flag of
    /// the entry row.
    ///
    /// zLog's multiplier station (`IsMultiStation`: category Multi-Op Single-TX and
    /// TX# 1) sends `30` as band ordinal; the category is not in the view, so here a
    /// station with TX# 1 counts as the multiplier station (departure).
    fn send_freq(&mut self, s: &mut Session, view: &View<'_>) {
        let Some(hz) = view.hz else {
            return;
        };
        let now = Instant::now();
        let status = (view.band, hz, view.mode, view.cq);
        if let Some((sent, at)) = self.freq_sent {
            if sent == status || now.duration_since(at) < Duration::from_secs(1) {
                return;
            }
        }
        let time = freq_time(view.log, chrono::Utc::now());
        let pc = s.settings.pc_name.clone();
        let multi = view.station.tx_nr == 1;
        // The rig's band (`FCurrentRig.CurrentBand`); the entry row's when the
        // frequency is out of every band. The view has no rig mode, so the entry
        // row's mode stands for it.
        let rig_band = zlog_rig::sync::band_of(hz).unwrap_or(view.band);
        s.write(&protocol::freq(
            view.band, rig_band, multi, hz, view.mode, view.cq, &time, &pc,
        ));
        self.freq_sent = Some((status, now));
    }

    /// What zLog sends as the entry row changes, whether the window is open or not:
    /// `SendBand` on a band change, `SendOperator` on an operator change
    /// (`TMainForm.OpMenuClick`), and the rig status.
    pub fn follow(&mut self, view: &View<'_>) {
        let session = self.session.clone();
        let mut s = session.lock().unwrap();
        s.pump();
        s.entry_band = Some(view.band);
        s.entry_operator = view.operator.to_string();
        // The PC name is set in the Options (`Settings._pcname`), not in this panel.
        s.settings.pc_name = view.pc_name.to_string();
        if !s.connected() {
            return;
        }
        if s.band_sent.is_some_and(|b| b != view.band) {
            s.send_band(view.band);
        }
        if s.operator_sent
            .as_deref()
            .is_some_and(|o| o != view.operator)
        {
            s.send_operator();
        }
        self.send_freq(&mut s, view);
    }

    fn settings_ui(ui: &mut Ui, s: &mut Session, lang: &str) {
        let t = texts();
        let editable = s.state == State::Disconnected;
        ui.add_enabled_ui(editable, |ui| {
            egui::Grid::new("zserver-settings").show(ui, |ui| {
                ui.label(t.get(lang, "host"));
                ui.text_edit_singleline(&mut s.settings.host);
                ui.label(t.get(lang, "port"));
                ui.add(egui::TextEdit::singleline(&mut s.settings.port).desired_width(60.0));
                ui.end_row();
                ui.label(t.get(lang, "on-connect"));
                egui::ComboBox::from_id_salt("zserver-on-connect")
                    .selected_text(t.get(lang, s.settings.on_connect.key()))
                    .show_ui(ui, |ui| {
                        for o in OnConnect::ALL {
                            ui.selectable_value(
                                &mut s.settings.on_connect,
                                o,
                                t.get(lang, o.key()),
                            );
                        }
                    });
                ui.label(t.get(lang, "line-break"));
                egui::ComboBox::from_id_salt("zserver-line-break")
                    .selected_text(format!("{:?}", s.settings.line_break))
                    .show_ui(ui, |ui| {
                        for l in LineBreak::ALL {
                            ui.selectable_value(&mut s.settings.line_break, l, format!("{l:?}"));
                        }
                    });
                ui.end_row();
            });
        });
    }
}

impl Panel for ZServerPanel {
    fn id(&self) -> &str {
        "zserver"
    }

    fn menu(&self) -> zlog_ui::MenuGroup {
        zlog_ui::MenuGroup::Network
    }

    fn title(&self, language: &str) -> String {
        texts().get(language, "title").into()
    }

    fn default_size(&self) -> [f32; 2] {
        [560.0, 520.0]
    }

    /// Read the settings (zlog.ini `[Z-Link]`: `TELNEThost`, `Port`,
    /// `TELNETlinebreak`; the others are this client's). `PCName` is an application
    /// setting that comes in [`View::pc_name`]; the application reads this
    /// section's old `pc-name` key once when it has none of its own.
    fn load(&mut self, section: &Section) {
        let get = |k: &str| section.get(k).cloned().unwrap_or_default();
        let mut s = self.session.lock().unwrap();
        let st = &mut s.settings;
        st.host = get("host");
        st.port = section
            .get("port")
            .cloned()
            .unwrap_or_else(|| "telnet".into());
        st.line_break = get("line-break")
            .parse::<usize>()
            .ok()
            .and_then(|i| LineBreak::ALL.get(i).copied())
            .unwrap_or_default();
        st.on_connect = OnConnect::ALL
            .into_iter()
            .find(|o| o.key() == get("on-connect"))
            .unwrap_or_default();
        st.prompt = Prompt::ALL
            .into_iter()
            .find(|p| p.key() == get("prompt"))
            .unwrap_or_default();
    }

    fn save(&self, section: &mut Section) {
        let s = self.session.lock().unwrap();
        let st = &s.settings;
        section.insert("host".into(), st.host.clone());
        section.insert("port".into(), st.port.clone());
        // Moved to the application's preferences; the application read it on start.
        section.remove("pc-name");
        section.insert("line-break".into(), st.line_break.index().to_string());
        section.insert("on-connect".into(), st.on_connect.key().into());
        section.insert("prompt".into(), st.prompt.key().into());
    }

    fn tick(&mut self, view: &View<'_>) {
        self.follow(view);
    }

    fn ui(&mut self, ui: &mut Ui, view: &View<'_>) {
        let t = texts();
        let lang = view.language;
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        let session = self.session.clone();
        let mut s = session.lock().unwrap();
        s.pump();

        Self::settings_ui(ui, &mut s, lang);
        ui.horizontal(|ui| match s.state {
            State::Disconnected => {
                if ui.button(t.get(lang, "connect")).clicked() {
                    self.notice = s.connect_tcp().err();
                }
                if let Some(n) = self.notice {
                    ui.colored_label(Color32::RED, t.get(lang, n));
                } else if let Some(e) = &s.error {
                    ui.colored_label(Color32::RED, format!("{}: {e}", t.get(lang, "failed")));
                }
            }
            State::Connecting => {
                if ui.button(t.get(lang, "disconnect")).clicked() {
                    s.disconnect();
                }
                ui.spinner();
                ui.label(t.get(lang, "connecting"));
            }
            State::Connected => {
                if ui.button(t.get(lang, "disconnect")).clicked() {
                    s.disconnect();
                }
                ui.label(t.get(lang, "connected"));
            }
        });

        ui.separator();
        ui.label(RichText::new(t.get(lang, "stations")).strong());
        if s.stations.is_empty() {
            ui.weak(t.get(lang, "none"));
        }
        for line in s.stations.values() {
            ui.label(RichText::new(line).monospace());
        }

        ui.separator();
        ui.label(RichText::new(t.get(lang, "chat")).strong());
        ScrollArea::vertical()
            .id_salt("zserver-chat")
            .max_height(160.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for c in &s.chat {
                    let text = RichText::new(&c.text).monospace();
                    ui.label(if c.red {
                        text.color(Color32::RED)
                    } else {
                        text
                    });
                }
            });
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("zserver-prompt")
                .selected_text(t.get(lang, s.settings.prompt.key()))
                .show_ui(ui, |ui| {
                    for p in Prompt::ALL {
                        ui.selectable_value(&mut s.settings.prompt, p, t.get(lang, p.key()));
                    }
                });
            let edit = ui.add_enabled(
                s.connected(),
                egui::TextEdit::singleline(&mut self.input).desired_width(300.0),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let send = ui
                .add_enabled(s.connected(), egui::Button::new(t.get(lang, "send")))
                .clicked();
            if (enter || send) && !self.input.is_empty() {
                let prompt = s.settings.prompt.text(
                    view.band,
                    &s.settings.pc_name,
                    &s.entry_operator,
                    &view.station.callsign,
                );
                let now = chrono::Local::now().format("%H:%M").to_string();
                s.send_chat(&now, &prompt, &self.input);
                self.input.clear();
                edit.request_focus();
            }
        });

        ui.separator();
        egui::CollapsingHeader::new(t.get(lang, "console"))
            .id_salt("zserver-console")
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .max_height(160.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for l in &s.console {
                            ui.label(RichText::new(l).monospace().small());
                        }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// N3: a UTC log sends UTC; a local-time log sends local time.
    #[test]
    fn freq_time_follows_the_log() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 10, 21, 3, 4, 5).unwrap();
        let mut log = zlog_core::Log::default();
        log.header.rst_sent = zlog_core::log::USE_UTC;
        assert_eq!(freq_time(&log, now), "03:04:05");
        let local = now
            .with_timezone(&chrono::Local)
            .format("%H:%M:%S")
            .to_string();
        log.header.rst_sent = (-540i16) as u16;
        assert!(log.utc_offset_minutes().is_some());
        assert_eq!(freq_time(&log, now), local);
    }
}
