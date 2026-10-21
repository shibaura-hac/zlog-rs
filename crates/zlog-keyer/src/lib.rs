//! CW keying (`UzLogKeyer`, `WinKeyer`, `UzLogCW`, `UCwMessageEditor`,
//! `UCwMessagePad`, `UCWKeyBoard`): a keyer on WinKeyer, on a serial port's DTR or
//! RTS line, or simulated; banks A and B of F1 to F12 messages with `$`-macros;
//! CQ repeat; a message pad and a CW keyboard.
//!
//! Not implemented: parallel-port and USBIF4CW keying (Windows-only hardware and
//! drivers), SO2R switching, paddle keying by the PC, RTTY messages (bank 3), and
//! keying through the rig's CAT.

pub mod keyer;
pub mod macros;
pub mod morse;
pub mod serial;
pub mod winkeyer;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use egui::Ui;
use zlog_core::Mode;
use zlog_ui::{Panel, Section, View};

pub use keyer::{Keyer, KeyerEvent, SimKeyer};
pub use macros::{expand, Abbrev, Fields};
pub use morse::Timing;

zlog_core::texts!();

/// zLog's default bank A (`def_cw_messages`); bank B starts empty.
pub const DEFAULT_MESSAGES: [&str; 12] = [
    "CQ TEST $M TEST",
    "$C $R$X",
    "TU $M TEST",
    "",
    "NR?",
    "$C?",
    "$M",
    "$R$X",
    "TU",
    "",
    "",
    "",
];

/// Which keyer to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    None,
    Simulated,
    WinKeyer,
    /// Key DTR or RTS of a serial port.
    Serial,
}

impl Device {
    const ALL: [Device; 4] = [
        Device::None,
        Device::Simulated,
        Device::WinKeyer,
        Device::Serial,
    ];

    fn key(self) -> &'static str {
        match self {
            Device::None => "none",
            Device::Simulated => "simulated",
            Device::WinKeyer => "winkeyer",
            Device::Serial => "serial",
        }
    }

    fn from_key(s: &str) -> Device {
        Device::ALL
            .into_iter()
            .find(|d| d.key() == s)
            .unwrap_or(Device::None)
    }
}

/// The settings of the panel (`CW` section of zLog.ini and the station data the
/// macros need).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub device: Device,
    pub port: String,
    /// Keying line of serial keying; PTT is the other line.
    pub key_line: serial::Line,
    pub ptt: bool,
    /// PTT lead and tail in milliseconds (`PTTBefore`, `PTTAfter`, zLog default 25
    /// and 0); used clamped to 1..=255 as `SetPTTDelay` does, see [`ptt_delay`].
    pub ptt_before_ms: u32,
    pub ptt_after_ms: u32,
    pub timing: Timing,
    pub sidetone: bool,
    pub paddle_swap: bool,
    /// Messages of banks A and B (`CWStrBank[1..2]`, keys `F1`, `F1B`).
    pub banks: [[String; 12]; 2],
    /// Seconds between CQs (`CQRepeat`) and the most CQs in a row (`CQMax`).
    pub cq_repeat: f64,
    pub cq_max: u32,
    /// Local hours at which `$G` changes to GM, GA and GE (section `greetings`).
    pub greeting_hours: [u32; 3],
    pub abbrev: Abbrev,
    pub not_send_leading_zeros: bool,
    /// Free messages of the message pad, one per line.
    pub pad: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            device: Device::None,
            port: String::new(),
            key_line: serial::Line::Dtr,
            ptt: false,
            ptt_before_ms: 25,
            ptt_after_ms: 0,
            timing: Timing::default(),
            sidetone: true,
            paddle_swap: false,
            banks: [
                DEFAULT_MESSAGES.map(String::from),
                std::array::from_fn(|_| String::new()),
            ],
            cq_repeat: 2.0,
            cq_max: 15,
            greeting_hours: macros::GREETING_HOURS,
            abbrev: Abbrev::default(),
            not_send_leading_zeros: false,
            pad: String::new(),
        }
    }
}

fn get<T: std::str::FromStr>(s: &Section, key: &str, default: T) -> T {
    s.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn get_char(s: &Section, key: &str, default: char) -> char {
    s.get(key).and_then(|v| v.chars().next()).unwrap_or(default)
}

const GREETING_KEYS: [&str; 3] = ["greeting_morning", "greeting_afternoon", "greeting_evening"];

/// A PTT delay as the keyer uses it: 1 to 255 ms (`SetPTTDelay`).
pub fn ptt_delay(ms: u32) -> u32 {
    ms.clamp(1, 255)
}

fn message_key(bank: usize, n: usize) -> String {
    format!("F{}{}", n + 1, if bank == 0 { "" } else { "B" })
}

impl Settings {
    pub fn load(s: &Section) -> Self {
        let d = Settings::default();
        let text = |key: &str, default: &str| s.get(key).cloned().unwrap_or(default.into());
        let banks = std::array::from_fn(|b| {
            std::array::from_fn(|n| text(&message_key(b, n), &d.banks[b][n]))
        });
        Settings {
            device: Device::from_key(&text("device", "none")),
            port: text("port", ""),
            key_line: if text("key_line", "dtr") == "rts" {
                serial::Line::Rts
            } else {
                serial::Line::Dtr
            },
            ptt: get(s, "ptt", d.ptt),
            ptt_before_ms: get(s, "ptt_before", d.ptt_before_ms),
            ptt_after_ms: get(s, "ptt_after", d.ptt_after_ms),
            timing: Timing {
                wpm: get(s, "speed", d.timing.wpm).clamp(morse::MIN_WPM, morse::MAX_WPM),
                weight: get(s, "weight", d.timing.weight).min(100),
                space_factor: get(s, "space_factor", d.timing.space_factor).max(1),
                ei_space_factor: get(s, "ei_space_factor", d.timing.ei_space_factor).max(1),
            },
            sidetone: get(s, "sidetone", d.sidetone),
            paddle_swap: get(s, "paddle_reverse", d.paddle_swap),
            banks,
            cq_repeat: get(s, "cq_repeat", d.cq_repeat).clamp(0.0, 60.0),
            cq_max: get(s, "cq_max", d.cq_max),
            greeting_hours: std::array::from_fn(|i| get(s, GREETING_KEYS[i], d.greeting_hours[i])),
            abbrev: Abbrev {
                zero: get_char(s, "zero", 'O'),
                one: get_char(s, "one", 'A'),
                nine: get_char(s, "nine", 'N'),
            },
            not_send_leading_zeros: get(s, "not_send_leading_zeros", false),
            pad: text("pad", ""),
        }
    }

    pub fn save(&self, s: &mut Section) {
        let mut put = |k: &str, v: String| {
            s.insert(k.into(), v);
        };
        put("device", self.device.key().into());
        put("port", self.port.clone());
        let line = match self.key_line {
            serial::Line::Dtr => "dtr",
            serial::Line::Rts => "rts",
        };
        put("key_line", line.into());
        put("ptt", self.ptt.to_string());
        put("ptt_before", self.ptt_before_ms.to_string());
        put("ptt_after", self.ptt_after_ms.to_string());
        put("speed", self.timing.wpm.to_string());
        put("weight", self.timing.weight.to_string());
        put("space_factor", self.timing.space_factor.to_string());
        put("ei_space_factor", self.timing.ei_space_factor.to_string());
        put("sidetone", self.sidetone.to_string());
        put("paddle_reverse", self.paddle_swap.to_string());
        for (b, bank) in self.banks.iter().enumerate() {
            for (n, m) in bank.iter().enumerate() {
                put(&message_key(b, n), m.clone());
            }
        }
        put("cq_repeat", self.cq_repeat.to_string());
        put("cq_max", self.cq_max.to_string());
        for (k, h) in GREETING_KEYS.iter().zip(self.greeting_hours) {
            put(k, h.to_string());
        }
        put("zero", self.abbrev.zero.to_string());
        put("one", self.abbrev.one.to_string());
        put("nine", self.abbrev.nine.to_string());
        put(
            "not_send_leading_zeros",
            self.not_send_leading_zeros.to_string(),
        );
        put("pad", self.pad.clone());
    }

    /// The macro values for the entry row of `view` at `unix_secs`, when the PC's
    /// clock shows `local_hour` (for `$G`: `GetGreetingsCode` uses `HourOf(Now)`,
    /// whatever the log's time zone).
    ///
    /// The station (`$M`, `$O`, `$V`, `$Q` taken as the city number) is the
    /// application's; `$F`, `$R` and the power are the entry row's, `$R` being 599 on
    /// CW (59 otherwise) while empty. `$S` is the serial of the entry row
    /// (`aQSO.SerialStr` in `TMainForm.SetStr`), the contest's next serial on the
    /// band (`Contest::next_serial_on`, `TLog.GetCurrentSerial`); for a contest
    /// without serial numbers it falls back to the next QSO number of the log.
    pub fn fields(&self, view: &View<'_>, unix_secs: u64, local_hour: u32) -> Fields {
        let station = view.station;
        let power = view.power;
        let sent = view.contest.sent_number(view.band, power, station);
        let serial = view
            .contest
            .next_serial_of_tx(view.band, view.station.tx_nr)
            .map(|n| n as i64)
            .unwrap_or(view.log.qsos.len() as i64 + 1);
        let last = view.log.qsos.last();
        let mut f = Fields {
            callsign: view.callsign.to_string(),
            sent: sent.clone(),
            nr_sent: sent,
            rst_sent: if !view.rst_sent.is_empty() {
                view.rst_sent
            } else if view.mode == Mode::Cw {
                "599"
            } else {
                "59"
            }
            .into(),
            nr_rcvd: view.number.to_string(),
            cq_zone: station.cq_zone.clone(),
            iaru_zone: station.iaru_zone.clone(),
            qth: station.city.clone(),
            prov: station.prov.clone(),
            operator: station.operator.clone(),
            iota: station.iota.clone(),
            serial: if self.not_send_leading_zeros {
                serial.to_string()
            } else {
                format!("{serial:03}")
            },
            power_code: power.code().map(String::from).unwrap_or_default(),
            power: power.label().into(),
            age: station.age.to_uppercase(),
            last_callsign: last.map(|q| q.callsign.clone()).unwrap_or_default(),
            last_number: last.map(|q| q.nr_rcvd.clone()).unwrap_or_default(),
            greeting: String::new(),
            time_utc: String::new(),
            my_call: station.callsign.to_uppercase(),
        };
        self.stamp(&mut f, unix_secs, local_hour);
        f
    }

    /// Set the fields that change with the clock, `$D` and `$G`.
    pub fn stamp(&self, f: &mut Fields, unix_secs: u64, local_hour: u32) {
        let day = unix_secs % 86_400;
        f.greeting = macros::greeting(local_hour, &self.greeting_hours).into();
        f.time_utc = format!("{:02}{:02}", day / 3600, day % 3600 / 60);
    }

    /// Time between CQs in whole seconds, as `StartCqRepeatTimer` truncates
    /// `CQRepeat`.
    pub fn cq_interval(&self) -> Duration {
        Duration::from_secs(self.cq_repeat.clamp(0.0, 60.0).trunc() as u64)
    }
}

/// Seconds since the Unix epoch and the PC's local hour.
fn now() -> (u64, u32) {
    use chrono::Timelike;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (secs, chrono::Local::now().hour())
}

/// The keyer and what it last said, shared with the CQ repeat thread.
#[derive(Default)]
struct Shared {
    keyer: Option<Box<dyn Keyer>>,
    status: String,
    /// WPM from the keyer's speed knob, for the panel to take over.
    pot_wpm: Option<u32>,
    /// The macro values of the entry row as of the last frame, for the CQ repeat to
    /// expand its message again before each CQ.
    fields: Fields,
}

impl Shared {
    /// Read the keyer's events. The paddle stops the message being sent
    /// (`TMainForm.OnPaddle` calls `CQAbort`, main.pas); true when it was used, for
    /// the caller to stop the CQ repeat as well.
    fn pump(&mut self) -> bool {
        let Some(k) = self.keyer.as_mut() else {
            return false;
        };
        let mut paddle = false;
        for e in k.poll() {
            match e {
                KeyerEvent::Finished => {}
                KeyerEvent::Speed(w) => self.pot_wpm = Some(w),
                KeyerEvent::Paddle(on) => paddle |= on,
                KeyerEvent::Error(e) => self.status = e,
            }
        }
        if paddle {
            k.abort();
        }
        paddle
    }
}

type Opened = Result<Box<dyn Keyer>, String>;

/// Open the configured keyer (WinKeyer takes about a second).
fn open(s: &Settings) -> Opened {
    match s.device {
        Device::None => Err("no keyer".into()),
        Device::Simulated => Ok(Box::new(SimKeyer::new(s.timing))),
        Device::WinKeyer => {
            let port = winkeyer::open_port(&s.port)?;
            let setup = winkeyer::Setup {
                wpm: s.timing.wpm,
                paddle_swap: s.paddle_swap,
                sidetone: s.sidetone,
                ptt_before_ms: ptt_delay(s.ptt_before_ms),
                ptt_after_ms: ptt_delay(s.ptt_after_ms),
            };
            winkeyer::WinKeyer::open(port, &setup)
                .map(|k| Box::new(k) as Box<dyn Keyer>)
                .map_err(|e| e.to_string())
        }
        Device::Serial => {
            let port = serial::open_port(&s.port)?;
            let setup = serial::LineSetup {
                key: s.key_line,
                ptt: s.ptt,
                ptt_before: Duration::from_millis(ptt_delay(s.ptt_before_ms) as u64),
                ptt_after: Duration::from_millis(ptt_delay(s.ptt_after_ms) as u64),
            };
            Ok(Box::new(serial::LineKeyer::new(port, setup, s.timing)))
        }
    }
}

/// Send the message `text` makes up to `max` times, `interval` after each end, until
/// `stop` (`CQRepeatProc`, `StartCqRepeatTimer`). The message is made again before
/// each CQ, as zLog expands it on every play, so `$D`, `$G` and the entry row are
/// current.
fn cq_loop(
    shared: Arc<Mutex<Shared>>,
    text: impl Fn(&Shared) -> String,
    interval: Duration,
    max: u32,
    stop: Arc<AtomicBool>,
) {
    let pause = |len: Duration| {
        let mut left = len;
        while !left.is_zero() && !stop.load(Ordering::SeqCst) {
            let d = left.min(Duration::from_millis(20));
            thread::sleep(d);
            left -= d;
        }
    };
    for _ in 0..max {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let mut s = shared.lock().unwrap();
        let text = text(&s);
        match s.keyer.as_mut() {
            Some(k) => k.send(&text),
            None => break,
        }
        drop(s);
        loop {
            pause(Duration::from_millis(20));
            let mut s = shared.lock().unwrap();
            if s.pump() {
                stop.store(true, Ordering::SeqCst);
            }
            let busy = s.keyer.as_ref().is_some_and(|k| k.busy());
            if stop.load(Ordering::SeqCst) || !busy {
                break;
            }
        }
        pause(interval);
    }
    stop.store(true, Ordering::SeqCst);
}

/// The CW keyer window.
pub struct KeyerPanel {
    settings: Settings,
    shared: Arc<Mutex<Shared>>,
    opening: Option<Receiver<Opened>>,
    /// Bank the function keys play, 0 for A and 1 for B.
    bank: usize,
    cq: Arc<AtomicBool>,
    /// Text of the CW keyboard and how much of it has been sent.
    keyboard: String,
    keyboard_sent: usize,
    ports: Vec<String>,
}

impl Default for KeyerPanel {
    fn default() -> Self {
        KeyerPanel {
            settings: Settings::default(),
            shared: Arc::default(),
            opening: None,
            bank: 0,
            cq: Arc::new(AtomicBool::new(true)),
            keyboard: String::new(),
            keyboard_sent: 0,
            ports: Vec::new(),
        }
    }
}

impl KeyerPanel {
    fn configured(&self) -> bool {
        self.shared.lock().unwrap().keyer.is_some()
    }

    fn connect(&mut self) {
        self.disconnect();
        let s = self.settings.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(open(&s));
        });
        self.opening = Some(rx);
    }

    fn disconnect(&mut self) {
        self.stop();
        self.shared.lock().unwrap().keyer = None;
    }

    fn stop(&mut self) {
        self.cq.store(true, Ordering::SeqCst);
        if let Some(k) = self.shared.lock().unwrap().keyer.as_mut() {
            k.abort();
        }
    }

    /// Expand and send message `n` (0..12) of `bank`, stopping what is being sent
    /// first (`PlayMessageCW` calls `WinKeyerClear`, or `ClrBuffer` without the FIFO
    /// option, which is not implemented); false without a keyer.
    fn play(&mut self, bank: usize, n: usize, view: &View<'_>) -> bool {
        self.cq.store(true, Ordering::SeqCst);
        let text = self.expanded(bank, n, view);
        let mut s = self.shared.lock().unwrap();
        let Some(k) = s.keyer.as_mut() else {
            return false;
        };
        if !text.trim().is_empty() {
            k.abort();
            k.send(&text);
        }
        true
    }

    fn expanded(&self, bank: usize, n: usize, view: &View<'_>) -> String {
        let (secs, hour) = now();
        let f = self.settings.fields(view, secs, hour);
        expand(&self.settings.banks[bank][n], &f, &self.settings.abbrev)
    }

    /// Take the keyer when it has opened, read its events and keep the macro values
    /// for the CQ repeat; every frame, open or closed.
    fn update(&mut self, view: &View<'_>) {
        if let Some(rx) = &self.opening {
            if let Ok(r) = rx.try_recv() {
                self.opening = None;
                let mut s = self.shared.lock().unwrap();
                match r {
                    Ok(k) => {
                        s.keyer = Some(k);
                        s.status.clear();
                    }
                    Err(e) => s.status = e,
                }
            }
        }
        let (secs, hour) = now();
        let fields = self.settings.fields(view, secs, hour);
        let pot = {
            let mut s = self.shared.lock().unwrap();
            if s.pump() {
                self.cq.store(true, Ordering::SeqCst);
            }
            s.fields = fields;
            s.pot_wpm.take()
        };
        if let Some(w) = pot {
            self.settings.timing.wpm = w.clamp(morse::MIN_WPM, morse::MAX_WPM);
        }
    }

    fn start_cq(&mut self, view: &View<'_>) {
        self.stop();
        let text = self.expanded(self.bank, 0, view);
        if text.trim().is_empty() || !self.configured() {
            return;
        }
        self.shared.lock().unwrap().fields = {
            let (secs, hour) = now();
            self.settings.fields(view, secs, hour)
        };
        let stop = Arc::new(AtomicBool::new(false));
        self.cq = stop.clone();
        let shared = self.shared.clone();
        let interval = self.settings.cq_interval();
        let max = self.settings.cq_max.max(1);
        let settings = self.settings.clone();
        let template = settings.banks[self.bank][0].clone();
        let make = move |s: &Shared| {
            let mut f = s.fields.clone();
            let (secs, hour) = now();
            settings.stamp(&mut f, secs, hour);
            expand(&template, &f, &settings.abbrev)
        };
        thread::spawn(move || cq_loop(shared, make, interval, max, stop));
    }

    fn send_text(&mut self, text: &str) {
        if let Some(k) = self.shared.lock().unwrap().keyer.as_mut() {
            k.send(text);
        }
    }

    fn set_wpm(&mut self, wpm: u32) {
        self.settings.timing.wpm = wpm.clamp(morse::MIN_WPM, morse::MAX_WPM);
        if let Some(k) = self.shared.lock().unwrap().keyer.as_mut() {
            k.set_wpm(self.settings.timing.wpm);
        }
    }

    fn device_ui(&mut self, ui: &mut Ui, lang: &str) {
        let t = |k| texts().get(lang, k);
        ui.horizontal(|ui| {
            ui.label(t("device"));
            egui::ComboBox::from_id_salt("keyer-device")
                .selected_text(t(self.settings.device.key()))
                .show_ui(ui, |ui| {
                    for d in Device::ALL {
                        ui.selectable_value(&mut self.settings.device, d, t(d.key()));
                    }
                });
            if matches!(self.settings.device, Device::WinKeyer | Device::Serial) {
                ui.label(t("port"));
                let combo = egui::ComboBox::from_id_salt("keyer-port")
                    .selected_text(self.settings.port.clone())
                    .show_ui(ui, |ui| {
                        for p in &self.ports {
                            ui.selectable_value(&mut self.settings.port, p.clone(), p);
                        }
                    });
                if combo.response.clicked() {
                    self.ports = serialport::available_ports()
                        .map(|v| v.into_iter().map(|p| p.port_name).collect())
                        .unwrap_or_default();
                }
                ui.add(egui::TextEdit::singleline(&mut self.settings.port).desired_width(120.0));
            }
            if self.opening.is_some() {
                ui.spinner();
            } else if self.configured() {
                if ui.button(t("disconnect")).clicked() {
                    self.disconnect();
                }
            } else if self.settings.device != Device::None && ui.button(t("connect")).clicked() {
                self.connect();
            }
        });
        if self.settings.device == Device::Serial {
            ui.horizontal(|ui| {
                ui.label(t("key_line"));
                ui.radio_value(&mut self.settings.key_line, serial::Line::Dtr, "DTR");
                ui.radio_value(&mut self.settings.key_line, serial::Line::Rts, "RTS");
                ui.checkbox(&mut self.settings.ptt, t("ptt_other_line"));
            });
        }
        if self.settings.device == Device::WinKeyer {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.settings.sidetone, t("sidetone"));
                ui.checkbox(&mut self.settings.paddle_swap, t("paddle_reverse"));
            });
        }
        if matches!(self.settings.device, Device::WinKeyer | Device::Serial) {
            ui.horizontal(|ui| {
                ui.label(t("ptt_delay"));
                ui.add(egui::DragValue::new(&mut self.settings.ptt_before_ms).range(1..=255));
                ui.add(egui::DragValue::new(&mut self.settings.ptt_after_ms).range(0..=255));
                ui.label("ms");
            });
        }
    }

    fn speed_ui(&mut self, ui: &mut Ui, lang: &str) {
        let t = |k| texts().get(lang, k);
        ui.horizontal(|ui| {
            let mut wpm = self.settings.timing.wpm;
            ui.label(t("speed"));
            if ui.button("-").clicked() {
                wpm = wpm.saturating_sub(1);
            }
            ui.add(egui::DragValue::new(&mut wpm).range(morse::MIN_WPM..=morse::MAX_WPM));
            if ui.button("+").clicked() {
                wpm += 1;
            }
            ui.label("WPM");
            if wpm != self.settings.timing.wpm {
                self.set_wpm(wpm);
            }
            ui.label(t("weight"));
            ui.add(egui::DragValue::new(&mut self.settings.timing.weight).range(0..=100));
            ui.label(t("space_factor"));
            ui.add(egui::DragValue::new(&mut self.settings.timing.space_factor).range(1..=500));
            ui.label(t("ei_space_factor"));
            ui.add(egui::DragValue::new(&mut self.settings.timing.ei_space_factor).range(1..=500));
        });
    }

    fn buttons_ui(&mut self, ui: &mut Ui, view: &View<'_>) {
        let lang = view.language;
        let t = |k| texts().get(lang, k);
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.bank, 0, t("bank_a"));
            ui.radio_value(&mut self.bank, 1, t("bank_b"));
            let running = !self.cq.load(Ordering::SeqCst);
            if ui.selectable_label(running, t("cq_repeat")).clicked() {
                if running {
                    self.stop();
                } else {
                    self.start_cq(view);
                }
            }
            ui.add(
                egui::DragValue::new(&mut self.settings.cq_repeat)
                    .range(0.0..=60.0)
                    .speed(0.1)
                    .suffix(" s"),
            );
            ui.label(t("cq_max"));
            ui.add(egui::DragValue::new(&mut self.settings.cq_max).range(1..=999));
            if ui.button(t("stop")).clicked() {
                self.stop();
            }
        });
        ui.horizontal_wrapped(|ui| {
            for n in 0..12 {
                let preview = self.expanded(self.bank, n, view);
                let b = ui.button(format!("F{}", n + 1)).on_hover_text(preview);
                if b.clicked() {
                    self.play(self.bank, n, view);
                }
            }
        });
    }

    fn editor_ui(&mut self, ui: &mut Ui, lang: &str) {
        let t = |k| texts().get(lang, k);
        egui::CollapsingHeader::new(t("messages")).show(ui, |ui| {
            egui::Grid::new("keyer-messages")
                .num_columns(2)
                .show(ui, |ui| {
                    for n in 0..12 {
                        ui.label(format!("F{}", n + 1));
                        let m = &mut self.settings.banks[self.bank][n];
                        ui.add(egui::TextEdit::singleline(m).desired_width(f32::INFINITY));
                        ui.end_row();
                    }
                });
            ui.label(t("macro_help"));
            ui.horizontal(|ui| {
                ui.label(t("abbrev"));
                for c in [
                    &mut self.settings.abbrev.zero,
                    &mut self.settings.abbrev.one,
                    &mut self.settings.abbrev.nine,
                ] {
                    let mut s = c.to_string();
                    if ui
                        .add(egui::TextEdit::singleline(&mut s).desired_width(16.0))
                        .changed()
                    {
                        if let Some(n) = s.chars().last() {
                            *c = n.to_ascii_uppercase();
                        }
                    }
                }
                ui.checkbox(
                    &mut self.settings.not_send_leading_zeros,
                    t("no_leading_zeros"),
                );
            });
        });
    }

    fn pad_ui(&mut self, ui: &mut Ui, view: &View<'_>) {
        let t = |k| texts().get(view.language, k);
        egui::CollapsingHeader::new(t("pad")).show(ui, |ui| {
            let lines: Vec<String> = self
                .settings
                .pad
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(String::from)
                .collect();
            ui.horizontal_wrapped(|ui| {
                for l in lines {
                    if ui.button(&l).clicked() {
                        let (secs, hour) = now();
                        let f = self.settings.fields(view, secs, hour);
                        let text = expand(&l, &f, &self.settings.abbrev);
                        self.send_text(&text);
                    }
                }
            });
            ui.add(
                egui::TextEdit::multiline(&mut self.settings.pad)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
        });
    }

    /// Type to send (`TCWKeyBoard.ConsoleKeyPress`): each new character is sent as
    /// it is typed; deleting cannot unsend.
    fn keyboard_ui(&mut self, ui: &mut Ui, lang: &str) {
        let t = |k| texts().get(lang, k);
        ui.label(t("keyboard"));
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.keyboard)
                    .desired_width(ui.available_width() - 60.0),
            );
            if r.changed() {
                self.keyboard = self.keyboard.to_uppercase();
                let chars: Vec<char> = self.keyboard.chars().collect();
                if chars.len() < self.keyboard_sent {
                    self.keyboard_sent = chars.len();
                }
                let new: String = chars[self.keyboard_sent..].iter().collect();
                self.keyboard_sent = chars.len();
                if !new.is_empty() {
                    self.send_text(&new);
                }
            }
            if ui.button(t("clear")).clicked() {
                self.keyboard.clear();
                self.keyboard_sent = 0;
            }
        });
    }
}

impl Panel for KeyerPanel {
    fn id(&self) -> &str {
        "keyer"
    }

    fn menu(&self) -> zlog_ui::MenuGroup {
        zlog_ui::MenuGroup::Operation
    }

    fn title(&self, language: &str) -> String {
        texts().get(language, "title").into()
    }

    fn default_size(&self) -> [f32; 2] {
        [640.0, 520.0]
    }

    fn ui(&mut self, ui: &mut Ui, view: &View<'_>) {
        let lang = view.language;
        self.update(view);
        let (busy, status) = {
            let s = self.shared.lock().unwrap();
            let busy = s.keyer.as_ref().is_some_and(|k| k.busy());
            (busy, s.status.clone())
        };
        self.device_ui(ui, lang);
        self.speed_ui(ui, lang);
        ui.separator();
        self.buttons_ui(ui, view);
        self.keyboard_ui(ui, lang);
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.editor_ui(ui, lang);
            self.pad_ui(ui, view);
        });
        ui.separator();
        let state = if self.opening.is_some() {
            "opening"
        } else if !self.configured() {
            "not_configured"
        } else if busy {
            "sending"
        } else {
            "idle"
        };
        ui.horizontal(|ui| {
            ui.label(texts().get(lang, state));
            if !status.is_empty() {
                ui.colored_label(ui.visuals().warn_fg_color, status);
            }
        });
        if busy || self.opening.is_some() || !self.cq.load(Ordering::SeqCst) {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }

    fn tick(&mut self, view: &View<'_>) {
        self.update(view);
    }

    /// On CW, F1 to F12 play the message of the current bank and Shift+F1 to F12 that
    /// of the other bank (`PlayMessage`: `mCW` only; `actionPlayMessageB01`); any
    /// function key stops CQ repeat. On other modes the keys are left to other
    /// panels (zLog plays voice there).
    fn function_key(&mut self, n: u8, shift: bool, view: &View<'_>) -> bool {
        if !(1..=12).contains(&n) || view.mode != Mode::Cw {
            return false;
        }
        if self.shared.lock().unwrap().pump() {
            self.cq.store(true, Ordering::SeqCst);
        }
        let bank = if shift { 1 - self.bank } else { self.bank };
        self.play(bank, n as usize - 1, view)
    }

    /// Esc stops CW and the CQ repeat (`actionCQAbort`); true when there was
    /// something to stop.
    fn escape(&mut self) -> bool {
        let running = !self.cq.load(Ordering::SeqCst);
        let busy = self
            .shared
            .lock()
            .unwrap()
            .keyer
            .as_ref()
            .is_some_and(|k| k.busy());
        self.stop();
        running || busy
    }

    /// Restore the settings and reopen the keyer.
    fn load(&mut self, section: &Section) {
        self.settings = Settings::load(section);
        if self.settings.device != Device::None {
            self.connect();
        }
    }

    fn save(&self, section: &mut Section) {
        self.settings.save(section);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zlog_core::{Band, Contest, ContestMode, Log, Power, Qso, ScoreSheet, Scoring, Station};

    struct Fixed;

    impl Contest for Fixed {
        fn name(&self) -> &str {
            "test"
        }
        fn band_range(&self) -> (Band, Band) {
            (Band::B1_9, Band::B50)
        }
        fn contest_mode(&self) -> ContestMode {
            ContestMode::Cw
        }
        fn sent_number(&self, _band: Band, power: Power, station: &Station) -> String {
            format!("{}{}", station.prov, power.code().unwrap_or('?'))
        }
        fn valid_number(&self, _q: &Qso) -> bool {
            true
        }
        fn score(&mut self, _log: &Log) -> Scoring {
            Scoring {
                qsos: Vec::new(),
                sheet: ScoreSheet::default(),
            }
        }
    }

    /// A contest with serial numbers: 41 on 7 MHz, 1 elsewhere.
    struct Serials;

    impl Contest for Serials {
        fn name(&self) -> &str {
            "serials"
        }
        fn band_range(&self) -> (Band, Band) {
            Fixed.band_range()
        }
        fn contest_mode(&self) -> ContestMode {
            ContestMode::Cw
        }
        fn sent_number(&self, band: Band, power: Power, station: &Station) -> String {
            Fixed.sent_number(band, power, station)
        }
        fn next_serial_on(&self, band: Band) -> Option<i32> {
            Some(if band == Band::B7 { 41 } else { 1 })
        }
        fn valid_number(&self, _q: &Qso) -> bool {
            true
        }
        fn score(&mut self, log: &Log) -> Scoring {
            Fixed.score(log)
        }
    }

    fn with_view<R>(f: impl FnOnce(&View<'_>) -> R) -> R {
        with_mode(Mode::Cw, f)
    }

    fn with_mode<R>(mode: Mode, f: impl FnOnce(&View<'_>) -> R) -> R {
        with_contest(&Fixed, mode, f)
    }

    fn with_contest<R>(contest: &dyn Contest, mode: Mode, f: impl FnOnce(&View<'_>) -> R) -> R {
        let mut log = Log::default();
        log.qsos.push(Qso {
            callsign: "JA1YAA".into(),
            nr_rcvd: "11H".into(),
            ..Qso::default()
        });
        let sheet = ScoreSheet::default();
        let station = Station {
            callsign: "jr1abc".into(),
            prov: "10".into(),
            ..Station::default()
        };
        let view = View {
            station: &station,
            number: "25M",
            rst_sent: "",
            power: Power::P010,
            log: &log,
            contest,
            sheet: &sheet,
            callsign: "JA1ZLO",
            band: Band::B7,
            mode,
            language: "en",
            spots: &[],
            hz: None,
            cq: false,
            operator: "",
            period_hours: 0,
            pc_name: "",
            focus: None,
        };
        f(&view)
    }

    #[test]
    fn texts_complete() {
        assert!(texts().missing().is_empty());
    }

    #[test]
    fn settings_round_trip() {
        let mut s = Settings {
            device: Device::Serial,
            key_line: serial::Line::Rts,
            ..Settings::default()
        };
        s.banks[1][3] = "QRZ?".into();
        s.abbrev.nine = '9';
        s.timing.wpm = 32;
        s.pad = "TU\nQRL?".into();
        let mut sec = Section::new();
        s.save(&mut sec);
        assert_eq!(sec["F1"], "CQ TEST $M TEST");
        assert_eq!(sec["F4B"], "QRZ?");
        assert_eq!(Settings::load(&sec), s);
        assert_eq!(Settings::load(&Section::new()), Settings::default());
    }

    #[test]
    fn fields_from_view() {
        let s = Settings::default();
        // 1970-01-01 03:04 UTC; the default log header has offset 0.
        let f = with_view(|v| s.fields(v, 3 * 3600 + 4 * 60, 3));
        assert_eq!(f.sent, "10H");
        assert_eq!(f.serial, "002");
        assert_eq!(f.time_utc, "0304");
        assert_eq!(f.last_callsign, "JA1YAA");
        assert_eq!(f.last_number, "11H");
        assert_eq!(f.power_code, "H");
        assert_eq!(f.nr_rcvd, "25M");
        assert_eq!(f.rst_sent, "599");
        assert_eq!(f.greeting, "GM");
        assert_eq!(expand("$C $R$X $M", &f, &s.abbrev), "JA1ZLO 5NN10H JR1ABC");
    }

    #[test]
    fn serial_is_the_contest_serial_on_the_band() {
        let mut s = Settings::default();
        let f = with_contest(&Serials, Mode::Cw, |v| s.fields(v, 0, 0));
        assert_eq!(f.serial, "041");
        s.not_send_leading_zeros = true;
        let f = with_contest(&Serials, Mode::Cw, |v| s.fields(v, 0, 0));
        assert_eq!(f.serial, "41");
        // Without serial numbers: the next QSO number of the log.
        let f = with_view(|v| s.fields(v, 0, 0));
        assert_eq!(f.serial, "2");
    }

    #[test]
    fn function_keys_need_a_keyer() {
        let mut p = KeyerPanel::default();
        assert!(!with_view(|v| p.function_key(1, false, v)));
        p.shared.lock().unwrap().keyer = Some(Box::new(SimKeyer::new(Timing::default())));
        assert!(with_view(|v| p.function_key(1, false, v)));
        assert!(with_view(|v| p.function_key(4, false, v)));
        assert!(!with_view(|v| p.function_key(13, false, v)));
        let mut s = p.shared.lock().unwrap();
        assert!(s.keyer.as_ref().unwrap().busy());
        s.keyer.as_mut().unwrap().abort();
    }

    #[test]
    fn cq_repeat_sends_up_to_max() {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let fast = Timing {
            wpm: 60,
            ..Timing::default()
        };
        shared.lock().unwrap().keyer = Some(Box::new(SimKeyer::new(fast)));
        let stop = Arc::new(AtomicBool::new(false));
        cq_loop(
            shared.clone(),
            |_| "E".into(),
            Duration::ZERO,
            3,
            stop.clone(),
        );
        assert!(stop.load(Ordering::SeqCst));
        assert!(!shared.lock().unwrap().keyer.as_ref().unwrap().busy());
    }

    /// Records what it is told; busy as set.
    #[derive(Clone, Default)]
    struct Recorder {
        calls: Arc<Mutex<Vec<String>>>,
        busy: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<KeyerEvent>>>,
    }

    impl Keyer for Recorder {
        fn send(&mut self, text: &str) {
            self.calls.lock().unwrap().push(format!("send {text}"));
        }
        fn abort(&mut self) {
            self.busy.store(false, Ordering::SeqCst);
            self.calls.lock().unwrap().push("abort".into());
        }
        fn set_wpm(&mut self, _wpm: u32) {}
        fn wpm(&self) -> u32 {
            25
        }
        fn set_ptt(&mut self, _on: bool) {}
        fn busy(&self) -> bool {
            self.busy.load(Ordering::SeqCst)
        }
        fn poll(&mut self) -> Vec<KeyerEvent> {
            std::mem::take(&mut *self.events.lock().unwrap())
        }
    }

    fn recording_panel() -> (KeyerPanel, Recorder) {
        let p = KeyerPanel::default();
        let r = Recorder::default();
        p.shared.lock().unwrap().keyer = Some(Box::new(r.clone()));
        (p, r)
    }

    fn calls(r: &Recorder) -> Vec<String> {
        std::mem::take(&mut *r.calls.lock().unwrap())
    }

    #[test]
    fn a_message_stops_the_one_being_sent() {
        let (mut p, r) = recording_panel();
        assert!(with_view(|v| p.function_key(1, false, v)));
        assert!(with_view(|v| p.function_key(2, false, v)));
        assert_eq!(
            calls(&r),
            [
                "abort",
                "send CQ TEST JR1ABC TEST",
                "abort",
                "send JA1ZLO 5NN10H"
            ]
        );
    }

    #[test]
    fn an_empty_message_sends_nothing() {
        let (mut p, r) = recording_panel();
        // F4 and F10 are empty by default, bank B entirely; spaces count as empty.
        assert!(with_view(|v| p.function_key(4, false, v)));
        assert!(with_view(|v| p.function_key(10, true, v)));
        p.settings.banks[0][4] = "   ".into();
        assert!(with_view(|v| p.function_key(5, false, v)));
        assert!(calls(&r).is_empty());
    }

    #[test]
    fn function_keys_only_on_cw() {
        let (mut p, r) = recording_panel();
        for mode in [Mode::Ssb, Mode::Fm, Mode::Am, Mode::Rtty] {
            assert!(!with_mode(mode, |v| p.function_key(1, false, v)));
            assert!(!with_mode(mode, |v| p.function_key(1, true, v)));
        }
        assert!(calls(&r).is_empty());
        assert!(with_mode(Mode::Cw, |v| p.function_key(9, false, v)));
        assert_eq!(calls(&r), ["abort", "send TU"]);
    }

    #[test]
    fn function_key_numbers_out_of_range() {
        let (mut p, r) = recording_panel();
        for n in [0, 13, 255] {
            assert!(!with_view(|v| p.function_key(n, false, v)));
            assert!(!with_view(|v| p.function_key(n, true, v)));
        }
        assert!(calls(&r).is_empty());
    }

    #[test]
    fn shift_plays_the_other_bank() {
        let (mut p, r) = recording_panel();
        p.settings.banks[1][0] = "QRZ?".into();
        p.settings.banks[1][11] = "$M".into();
        assert!(with_view(|v| p.function_key(1, true, v)));
        assert!(with_view(|v| p.function_key(12, true, v)));
        assert_eq!(calls(&r), ["abort", "send QRZ?", "abort", "send JR1ABC"]);
        // With bank B selected, Shift plays bank A.
        p.bank = 1;
        assert!(with_view(|v| p.function_key(1, false, v)));
        assert!(with_view(|v| p.function_key(3, true, v)));
        assert_eq!(
            calls(&r),
            ["abort", "send QRZ?", "abort", "send TU JR1ABC TEST"]
        );
    }

    #[test]
    fn escape_stops_cw_and_cq_repeat() {
        let (mut p, r) = recording_panel();
        // Nothing to stop.
        assert!(!p.escape());
        r.busy.store(true, Ordering::SeqCst);
        assert!(p.escape());
        assert!(!r.busy());
        assert!(calls(&r).contains(&"abort".to_string()));
        // A CQ repeat running between CQs.
        p.cq = Arc::new(AtomicBool::new(false));
        let cq = p.cq.clone();
        assert!(p.escape());
        assert!(cq.load(Ordering::SeqCst));
        assert!(!p.escape());
        // Without a keyer.
        let mut q = KeyerPanel::default();
        assert!(!q.escape());
    }

    #[test]
    fn paddle_stops_message_and_cq_repeat() {
        let (mut p, r) = recording_panel();
        r.busy.store(true, Ordering::SeqCst);
        p.cq = Arc::new(AtomicBool::new(false));
        let cq = p.cq.clone();
        r.events.lock().unwrap().push(KeyerEvent::Paddle(true));
        with_view(|v| p.tick(v));
        assert!(cq.load(Ordering::SeqCst));
        assert!(!r.busy());
        assert_eq!(calls(&r), ["abort"]);
    }

    #[test]
    fn paddle_release_and_other_events_stop_nothing() {
        let (mut p, r) = recording_panel();
        r.busy.store(true, Ordering::SeqCst);
        p.cq = Arc::new(AtomicBool::new(false));
        let cq = p.cq.clone();
        r.events.lock().unwrap().extend([
            KeyerEvent::Paddle(false),
            KeyerEvent::Finished,
            KeyerEvent::Speed(30),
        ]);
        with_view(|v| p.tick(v));
        assert!(!cq.load(Ordering::SeqCst));
        assert!(r.busy());
        assert!(calls(&r).is_empty());
        // No events at all.
        with_view(|v| p.tick(v));
        assert!(!cq.load(Ordering::SeqCst));
    }

    #[test]
    fn paddle_once_among_many_events_aborts_once() {
        let (mut p, r) = recording_panel();
        r.events.lock().unwrap().extend([
            KeyerEvent::Paddle(true),
            KeyerEvent::Paddle(false),
            KeyerEvent::Paddle(true),
        ]);
        with_view(|v| p.tick(v));
        assert_eq!(calls(&r), ["abort"]);
    }

    #[test]
    fn paddle_without_a_keyer() {
        let mut s = Shared::default();
        assert!(!s.pump());
    }

    #[test]
    fn ptt_delay_defaults_and_range() {
        let s = Settings::default();
        assert_eq!((s.ptt_before_ms, s.ptt_after_ms), (25, 0));
        assert_eq!(ptt_delay(0), 1);
        assert_eq!(ptt_delay(1), 1);
        assert_eq!(ptt_delay(25), 25);
        assert_eq!(ptt_delay(255), 255);
        assert_eq!(ptt_delay(256), 255);
        assert_eq!(ptt_delay(u32::MAX), 255);
    }

    #[test]
    fn cq_interval_in_whole_seconds() {
        let at = |secs: f64| {
            Settings {
                cq_repeat: secs,
                ..Settings::default()
            }
            .cq_interval()
        };
        assert_eq!(at(2.5), Duration::from_secs(2));
        assert_eq!(at(2.0), Duration::from_secs(2));
        assert_eq!(at(0.9), Duration::ZERO);
        assert_eq!(at(0.0), Duration::ZERO);
        assert_eq!(at(-3.0), Duration::ZERO);
        assert_eq!(at(59.99), Duration::from_secs(59));
        assert_eq!(at(1000.0), Duration::from_secs(60));
        assert_eq!(at(f64::NAN), Duration::ZERO);
    }

    #[test]
    fn greeting_by_pc_local_hour() {
        let s = Settings::default();
        // 03:04 UTC, but the PC clock shows 20 o'clock.
        let f = with_view(|v| s.fields(v, 3 * 3600 + 4 * 60, 20));
        assert_eq!(f.greeting, "GE");
        assert_eq!(f.time_utc, "0304");
        let s = Settings {
            greeting_hours: [5, 10, 15],
            ..Settings::default()
        };
        assert_eq!(with_view(|v| s.fields(v, 0, 4)).greeting, "GE");
        assert_eq!(with_view(|v| s.fields(v, 0, 9)).greeting, "GM");
        assert_eq!(with_view(|v| s.fields(v, 0, 10)).greeting, "GA");
        assert_eq!(with_view(|v| s.fields(v, 0, 15)).greeting, "GE");
    }

    #[test]
    fn greeting_hours_settings() {
        let s = Settings {
            greeting_hours: [6, 11, 17],
            ..Settings::default()
        };
        let mut sec = Section::new();
        s.save(&mut sec);
        assert_eq!(sec["greeting_afternoon"], "11");
        assert_eq!(Settings::load(&sec), s);
        let mut bad = Section::new();
        bad.insert("greeting_morning".into(), "abc".into());
        bad.insert("greeting_evening".into(), "".into());
        bad.insert("greeting_afternoon".into(), "13".into());
        assert_eq!(Settings::load(&bad).greeting_hours, [0, 13, 18]);
    }

    #[test]
    fn stamp_sets_time_and_greeting() {
        let s = Settings::default();
        let mut f = Fields::default();
        s.stamp(&mut f, 86_400 + 23 * 3600 + 59 * 60 + 59, 13);
        assert_eq!(f.time_utc, "2359");
        assert_eq!(f.greeting, "GA");
        s.stamp(&mut f, 0, 0);
        assert_eq!(f.time_utc, "0000");
        assert_eq!(f.greeting, "GM");
    }

    #[test]
    fn cq_repeat_makes_the_message_again_each_time() {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let r = Recorder::default();
        shared.lock().unwrap().keyer = Some(Box::new(r.clone()));
        shared.lock().unwrap().fields.callsign = "JA1ZLO".into();
        let n = std::sync::atomic::AtomicU32::new(0);
        let make = |s: &Shared| {
            let i = n.fetch_add(1, Ordering::SeqCst);
            format!("{} {i}", s.fields.callsign)
        };
        let stop = Arc::new(AtomicBool::new(false));
        cq_loop(shared, make, Duration::ZERO, 3, stop);
        assert_eq!(
            calls(&r),
            ["send JA1ZLO 0", "send JA1ZLO 1", "send JA1ZLO 2"]
        );
    }

    #[test]
    fn cq_repeat_without_a_keyer_or_stopped() {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        cq_loop(
            shared.clone(),
            |_| "E".into(),
            Duration::ZERO,
            3,
            stop.clone(),
        );
        assert!(stop.load(Ordering::SeqCst));
        let r = Recorder::default();
        shared.lock().unwrap().keyer = Some(Box::new(r.clone()));
        cq_loop(shared, |_| "E".into(), Duration::ZERO, 3, stop);
        assert!(calls(&r).is_empty());
    }

    #[test]
    fn tick_keeps_the_entry_row_for_cq_repeat() {
        let (mut p, _r) = recording_panel();
        with_view(|v| p.tick(v));
        let s = p.shared.lock().unwrap();
        assert_eq!(s.fields.callsign, "JA1ZLO");
        assert_eq!(s.fields.my_call, "JR1ABC");
    }
}
