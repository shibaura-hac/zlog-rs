//! Settings kept between runs, in one file in the system's configuration folder
//! (`%APPDATA%` on Windows, `~/Library/Application Support` on macOS).
//!
//! The file has sections of `key = value` lines; each part of the application and
//! each extension owns one section and sees no other.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use zlog_core::qso::QslState;
use zlog_core::{Band, Power, QsyRule, Station};
use zlog_ui::Section;

/// Preferences applied to the open log (zlog.ini `[Preferences]` and `[Z-Link]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    /// `CountDown` with `CountDownMinute` (default off, 10) and `QSYCount` with
    /// `CountPerHour` (default off, 8).
    pub qsy_rule: QsyRule,
    /// Minutes of the countdown rule kept while it is off (`CountDownMinute`).
    pub countdown_minutes: u32,
    /// Band changes per hour kept while that rule is off (`CountPerHour`).
    pub qsy_per_hour: u32,
    /// `QslDefault` (default 0, none): the QSL state not written into `.ZLO` memos.
    pub qsl_default: QslState,
    /// `[Z-Link] PCName`: this PC's name, stored in logged QSOs and sent to Z-Server.
    pub pc_name: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            qsy_rule: QsyRule::default(),
            countdown_minutes: 10,
            qsy_per_hour: 8,
            qsl_default: QslState::None,
            pc_name: String::new(),
        }
    }
}

impl Preferences {
    /// Turn the countdown rule on or off, keeping its minutes.
    pub fn set_countdown(&mut self, on: bool) {
        self.qsy_rule.countdown_minutes = on.then_some(self.countdown_minutes);
    }

    /// Turn the band-changes-per-hour rule on or off, keeping its count.
    pub fn set_qsy_count(&mut self, on: bool) {
        self.qsy_rule.max_qsy_per_hour = on.then_some(self.qsy_per_hour);
    }
}

/// How many recent contests the zLog Menu lists.
const RECENT_KEPT: usize = 8;

/// How many logs are remembered with their contest file.
const LOGS_KEPT: usize = 50;

/// A recently used contest: a built-in one by identifier, or a contest file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecentContest {
    Id(String),
    File(PathBuf),
}

impl RecentContest {
    fn parse(v: &str) -> Self {
        match v.strip_prefix("file:") {
            Some(p) => RecentContest::File(PathBuf::from(p)),
            None => RecentContest::Id(v.to_string()),
        }
    }

    fn format(&self) -> String {
        match self {
            RecentContest::Id(id) => id.clone(),
            RecentContest::File(p) => format!("file:{}", p.display()),
        }
    }
}

/// A value as written to the file: as it is when it reads back unchanged, else in
/// double quotes with `\\`, `\n` and `\r` escaped (multi-line keyer texts, values
/// with spaces at either end). Files written before quoting stay readable, as
/// backslashes outside quotes are taken literally (Windows paths).
fn escape(v: &str) -> String {
    let plain = !v.contains(['\n', '\r'])
        && v.trim() == v
        && !(v.len() >= 2 && v.starts_with('"') && v.ends_with('"'));
    if plain {
        return v.to_string();
    }
    let mut out = String::from("\"");
    for c in v.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The value of a line, undoing [`escape`].
fn unescape(v: &str) -> String {
    let Some(inner) = v
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .filter(|_| v.len() >= 2)
    else {
        return v.to_string();
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(c) => out.push(c),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
thread_local! {
    /// The settings file of the test running on this thread ([`Settings::path`]).
    pub static TEST_PATH: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    sections: BTreeMap<String, Section>,
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        // Tests keep their settings in a file of their own, or none at all.
        #[cfg(test)]
        return TEST_PATH.with(|p| p.borrow().clone());
        #[cfg(not(test))]
        dirs::config_dir().map(|d| d.join("zlog-rs").join("settings.ini"))
    }

    /// The saved settings, or empty ones when there are none yet.
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.format())
    }

    pub fn parse(text: &str) -> Self {
        let mut sections: BTreeMap<String, Section> = BTreeMap::new();
        let mut current = String::new();
        for line in text.lines().map(str::trim) {
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = name.to_string();
            } else if let Some((k, v)) = line.split_once('=') {
                sections
                    .entry(current.clone())
                    .or_default()
                    .insert(k.trim().into(), unescape(v.trim()));
            }
        }
        Settings { sections }
    }

    pub fn format(&self) -> String {
        let mut out = String::new();
        for (name, section) in &self.sections {
            out.push_str(&format!("[{name}]\n"));
            for (k, v) in section {
                out.push_str(&format!("{k} = {}\n", escape(v)));
            }
            out.push('\n');
        }
        out
    }

    /// The language chosen in the Language menu, if any (`window` section).
    pub fn language(&mut self) -> Option<String> {
        self.section("window").get("language").cloned()
    }

    pub fn set_language(&mut self, code: &str) {
        self.section("window")
            .insert("language".into(), code.into());
    }

    pub fn section(&mut self, name: &str) -> &mut Section {
        self.sections.entry(name.to_string()).or_default()
    }

    /// Whether dialogs float above other windows (Window menu, `window` section).
    pub fn keep_dialogs_on_top(&mut self) -> bool {
        self.section("window")
            .get("keep_on_top")
            .is_some_and(|v| v == "1")
    }

    pub fn set_keep_dialogs_on_top(&mut self, on: bool) {
        let v = if on { "1" } else { "0" };
        self.section("window")
            .insert("keep_on_top".into(), v.into());
    }

    /// Whether the toolbar buttons of the panel `id` are shown (Settings menu; shown
    /// unless hidden).
    pub fn toolbar_shown(&mut self, id: &str) -> bool {
        !self.hidden_toolbars().iter().any(|h| h == id)
    }

    pub fn set_toolbar_shown(&mut self, id: &str, shown: bool) {
        let mut hidden = self.hidden_toolbars();
        hidden.retain(|h| h != id);
        if !shown {
            hidden.push(id.to_string());
        }
        hidden.sort();
        self.section("window")
            .insert("hidden_toolbars".into(), hidden.join(","));
    }

    fn hidden_toolbars(&mut self) -> Vec<String> {
        self.section("window")
            .get("hidden_toolbars")
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The station, from the `station` section.
    pub fn station(&mut self) -> Station {
        let s = self.section("station");
        let text = |k: &str| s.get(k).cloned().unwrap_or_default();
        let power = Band::ALL
            .iter()
            .filter_map(|b| {
                let code = s.get(&format!("power.{}", b.label()))?.chars().next()?;
                Some((*b, Power::from_code(code)?))
            })
            .collect();
        Station {
            callsign: text("callsign"),
            operator: text("operator"),
            prov: text("prov"),
            city: text("city"),
            power,
            age: text("age"),
            cq_zone: text("cq_zone"),
            iaru_zone: text("iaru_zone"),
            itu_region: text("itu_region"),
            iota: text("iota"),
            handle_cw: text("handle_cw"),
            handle_ph: text("handle_ph"),
            tx_nr: s
                .get("tx_nr")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0u8)
                .min(15),
        }
    }

    /// zLog's `UseContestPeriod` (`[Preferences]`), on by default.
    pub fn use_contest_period(&mut self) -> bool {
        self.section("preferences")
            .get("use_contest_period")
            .is_none_or(|v| v != "false")
    }

    pub fn set_use_contest_period(&mut self, on: bool) {
        self.section("preferences")
            .insert("use_contest_period".into(), on.to_string());
    }

    /// The preferences, from the `preferences` section. A PC name missing there is
    /// read from the old key of the Z-Server panel (`zserver`, `pc-name`).
    pub fn preferences(&mut self) -> Preferences {
        let d = Preferences::default();
        let s = self.section("preferences");
        let num = |k: &str, d: u32| {
            s.get(k)
                .and_then(|v| v.parse::<u32>().ok())
                .map_or(d, |n| n.clamp(1, 99))
        };
        let on = |k: &str| s.get(k).is_some_and(|v| v == "true");
        let countdown_minutes = num("countdown_minutes", d.countdown_minutes);
        let qsy_per_hour = num("qsy_per_hour", d.qsy_per_hour);
        let qsy_rule = QsyRule {
            countdown_minutes: on("countdown").then_some(countdown_minutes),
            max_qsy_per_hour: on("qsy_count").then_some(qsy_per_hour),
        };
        let qsl_default = s
            .get("qsl_default")
            .and_then(|v| v.parse::<u8>().ok())
            .map_or(QslState::None, QslState::from_byte);
        let mut pc_name = s.get("pc_name").cloned().unwrap_or_default();
        if pc_name.is_empty() {
            pc_name = self
                .section("zserver")
                .get("pc-name")
                .cloned()
                .unwrap_or_default();
        }
        Preferences {
            qsy_rule,
            countdown_minutes,
            qsy_per_hour,
            qsl_default,
            pc_name,
        }
    }

    pub fn set_preferences(&mut self, p: &Preferences) {
        let s = self.section("preferences");
        let rule = p.qsy_rule;
        s.insert(
            "countdown".into(),
            rule.countdown_minutes.is_some().to_string(),
        );
        s.insert("countdown_minutes".into(), p.countdown_minutes.to_string());
        s.insert(
            "qsy_count".into(),
            rule.max_qsy_per_hour.is_some().to_string(),
        );
        s.insert("qsy_per_hour".into(), p.qsy_per_hour.to_string());
        s.insert("qsl_default".into(), p.qsl_default.to_byte().to_string());
        s.insert("pc_name".into(), p.pc_name.clone());
    }

    /// The contests opened last, newest first (`recent.0`, `recent.1`, ...; a file
    /// contest as `file:<path>`). The old comma list `recent` is read when there are
    /// none of these.
    pub fn recent_contests(&mut self) -> Vec<RecentContest> {
        let s = self.section("menu");
        let numbered: Vec<RecentContest> = (0..RECENT_KEPT)
            .map_while(|i| s.get(&format!("recent.{i}")))
            .map(|v| RecentContest::parse(v))
            .collect();
        if !numbered.is_empty() {
            return numbered;
        }
        s.get("recent")
            .map(|r| {
                r.split(',')
                    .filter(|x| !x.is_empty())
                    .map(|x| RecentContest::Id(x.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn add_recent_contest(&mut self, contest: RecentContest) {
        let mut all = self.recent_contests();
        all.retain(|x| *x != contest);
        all.insert(0, contest);
        all.truncate(RECENT_KEPT);
        let s = self.section("menu");
        s.remove("recent");
        s.retain(|k, _| !k.starts_with("recent."));
        for (i, c) in all.iter().enumerate() {
            s.insert(format!("recent.{i}"), c.format());
        }
    }

    /// The folder the CFG picker lists (zLog's `CFGDATPath`).
    pub fn cfg_folder(&mut self) -> String {
        self.section("menu")
            .get("cfg_folder")
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_cfg_folder(&mut self, folder: &str) {
        self.section("menu")
            .insert("cfg_folder".into(), folder.to_string());
    }

    /// Which contest file each log was made with (`log.N` and `file.N` of the
    /// `contest_files` section, newest first), so that reopening a log chooses it
    /// again (zLog keeps only the last one, `LastCFGFileName`).
    pub fn log_contest_files(&mut self) -> Vec<(PathBuf, PathBuf)> {
        let s = self.section("contest_files");
        (0..LOGS_KEPT)
            .map_while(|i| Some((s.get(&format!("log.{i}"))?, s.get(&format!("file.{i}"))?)))
            .map(|(l, f)| (PathBuf::from(l), PathBuf::from(f)))
            .collect()
    }

    /// Remember the contest file of `log`, or forget it (`None`, a built-in contest).
    pub fn set_log_contest_file(&mut self, log: &Path, file: Option<&Path>) {
        let mut all = self.log_contest_files();
        all.retain(|(l, _)| l != log);
        if let Some(f) = file {
            all.insert(0, (log.to_path_buf(), f.to_path_buf()));
        }
        all.truncate(LOGS_KEPT);
        let s = self.section("contest_files");
        s.clear();
        for (i, (l, f)) in all.iter().enumerate() {
            s.insert(format!("log.{i}"), l.display().to_string());
            s.insert(format!("file.{i}"), f.display().to_string());
        }
    }

    pub fn set_station(&mut self, st: &Station) {
        let s = self.section("station");
        s.insert("callsign".into(), st.callsign.clone());
        s.insert("operator".into(), st.operator.clone());
        s.insert("prov".into(), st.prov.clone());
        s.insert("city".into(), st.city.clone());
        s.insert("age".into(), st.age.clone());
        s.insert("cq_zone".into(), st.cq_zone.clone());
        s.insert("iaru_zone".into(), st.iaru_zone.clone());
        s.insert("itu_region".into(), st.itu_region.clone());
        s.insert("iota".into(), st.iota.clone());
        s.insert("tx_nr".into(), st.tx_nr.to_string());
        s.insert("handle_cw".into(), st.handle_cw.clone());
        s.insert("handle_ph".into(), st.handle_ph.clone());
        s.retain(|k, _| !k.starts_with("power."));
        for (b, p) in &st.power {
            if let Some(c) = p.code() {
                s.insert(format!("power.{}", b.label()), c.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_contests_and_contest_files_round_trip() {
        let mut s = Settings::parse("[menu]\nrecent = acag,allja\n");
        assert_eq!(
            s.recent_contests(),
            vec![
                RecentContest::Id("acag".into()),
                RecentContest::Id("allja".into())
            ]
        );
        let cfg = PathBuf::from("/cfg/a=b, c.CFG");
        s.add_recent_contest(RecentContest::File(cfg.clone()));
        s.add_recent_contest(RecentContest::Id("allja".into()));
        s.set_cfg_folder("/cfg");
        s.set_log_contest_file(Path::new("/logs/x.zlox"), Some(&cfg));
        s.set_log_contest_file(Path::new("/logs/y.zlox"), Some(&cfg));
        s.set_log_contest_file(Path::new("/logs/y.zlox"), None);
        let mut back = Settings::parse(&s.format());
        assert_eq!(
            back.recent_contests(),
            vec![
                RecentContest::Id("allja".into()),
                RecentContest::File(cfg.clone()),
                RecentContest::Id("acag".into()),
            ]
        );
        assert_eq!(back.cfg_folder(), "/cfg");
        assert_eq!(
            back.log_contest_files(),
            vec![(PathBuf::from("/logs/x.zlox"), cfg)]
        );
    }

    #[test]
    fn window_settings_round_trip() {
        let mut s = Settings::default();
        assert!(!s.keep_dialogs_on_top());
        assert!(s.toolbar_shown("score"));
        s.set_keep_dialogs_on_top(true);
        s.set_toolbar_shown("score", false);
        s.set_toolbar_shown("multipliers", false);
        s.set_toolbar_shown("multipliers", true);
        let mut back = Settings::parse(&s.format());
        assert!(back.keep_dialogs_on_top());
        assert!(!back.toolbar_shown("score"));
        assert!(back.toolbar_shown("multipliers"));
        back.set_keep_dialogs_on_top(false);
        assert!(!Settings::parse(&back.format()).keep_dialogs_on_top());
    }

    #[test]
    fn multi_line_values_round_trip() {
        let mut s = Settings::default();
        let pad = "CQ TEST\n[x]\nk = v\n back\\slash \"q\" ";
        s.section("keyer").insert("pad".into(), pad.into());
        s.section("keyer")
            .insert("path".into(), "C:\\new\\zlog".into());
        let back = Settings::parse(&s.format());
        assert_eq!(back, s);
        // Old files: backslashes and quotes inside values are taken as they are.
        let old = Settings::parse("[a]\np = C:\\new\\x\nq = say \"hi\" now\n");
        assert_eq!(old.sections["a"]["p"], "C:\\new\\x");
        assert_eq!(old.sections["a"]["q"], "say \"hi\" now");
    }

    #[test]
    fn language_round_trip() {
        let mut s = Settings::default();
        assert_eq!(s.language(), None);
        s.set_language("en");
        assert_eq!(
            Settings::parse(&s.format()).language().as_deref(),
            Some("en")
        );
    }

    #[test]
    fn round_trip() {
        let mut s = Settings::default();
        let mut st = Station {
            callsign: "JA1ZLO".into(),
            prov: "10".into(),
            age: "45".into(),
            cq_zone: "25".into(),
            iaru_zone: "45".into(),
            itu_region: "3".into(),
            iota: "AS007".into(),
            tx_nr: 3,
            handle_cw: "TAKA".into(),
            handle_ph: "Taka".into(),
            ..Station::default()
        };
        st.power.insert(Band::B430, Power::P002);
        s.set_station(&st);
        s.section("rig").insert("model".into(), "IC-7300".into());
        assert!(s.use_contest_period());
        s.set_use_contest_period(false);
        let back = Settings::parse(&s.format());
        assert_eq!(back, s);
        let mut back = back;
        assert_eq!(back.station(), st);
        assert!(!back.use_contest_period());
    }

    #[test]
    fn preferences_defaults_and_round_trip() {
        // zLog: CountDown off/10, QSYCount off/8, QslDefault 0, PCName empty.
        let mut s = Settings::default();
        assert_eq!(s.preferences(), Preferences::default());
        let mut p = Preferences {
            countdown_minutes: 15,
            qsl_default: QslState::NoQsl,
            pc_name: "PC1".into(),
            ..Preferences::default()
        };
        p.set_countdown(true);
        p.set_qsy_count(true);
        assert_eq!(p.qsy_rule.countdown_minutes, Some(15));
        assert_eq!(p.qsy_rule.max_qsy_per_hour, Some(8));
        s.set_preferences(&p);
        let mut back = Settings::parse(&s.format());
        assert_eq!(back.preferences(), p);
        p.set_countdown(false);
        assert_eq!(p.qsy_rule.countdown_minutes, None);
        assert_eq!(p.countdown_minutes, 15, "kept while off");
    }

    #[test]
    fn pc_name_falls_back_to_the_old_zserver_key() {
        let mut s = Settings::parse("[zserver]\npc-name = OLD\n");
        assert_eq!(s.preferences().pc_name, "OLD");
        let mut p = s.preferences();
        p.pc_name = "NEW".into();
        s.set_preferences(&p);
        assert_eq!(s.preferences().pc_name, "NEW");
    }
}
