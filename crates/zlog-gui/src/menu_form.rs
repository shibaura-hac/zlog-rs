//! The zLog Menu dialog shown at start (`TMenuForm`): contest, mode, station, file.

use crate::i18n::tr;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use zlog_core::{ContestDef, ContestFile, ContestMode, Power, Registry, Station};

use crate::session::Setup;
use crate::settings::{RecentContest, Settings};

/// A contest opened from a file (a CFG) and chosen in the form, as zLog's
/// "user-defined contest" button (`TMenuForm.SelectButtonClick`).
struct ChosenFile {
    path: PathBuf,
    def: Box<dyn ContestDef>,
    warnings: Vec<String>,
}

pub struct MenuForm {
    /// Index into the registry's contest list.
    contest: usize,
    /// Text typed in the search box; filters across all groups.
    search: String,
    /// Group shown in the list; `None` is the recently used contests.
    group: Option<String>,
    /// Recently used contests, newest first.
    pub recent: Vec<RecentContest>,
    /// The chosen contest file; when set, it is the contest instead of `contest`.
    file: Option<ChosenFile>,
    /// Titles of contest files shown in the recent list, read once each.
    titles: HashMap<PathBuf, String>,
    /// Folder listed by the CFG picker (zLog's `CFGDATPath`).
    cfg_folder: String,
    folder_changed: bool,
    /// Logs with the contest file they were made with ([`Settings::log_contest_files`]).
    pub log_files: Vec<(PathBuf, PathBuf)>,
    /// The file name last looked up in `log_files`.
    checked_path: String,
    /// The contest file of the log being opened, not found: ask to locate it.
    locate: Option<PathBuf>,
    picker: Option<CfgPicker>,
    mode: ContestMode,
    station: Station,
    /// One power on every band, or `None` for zLog's default per band.
    power: Option<Power>,
    coeff: String,
    path: String,
    pub error: Option<String>,
}

impl MenuForm {
    /// Take the station from the settings again (after Options OK), so that a later
    /// OK here does not write back an older one.
    pub fn set_station(&mut self, station: Station) {
        self.station = station;
    }

    /// The form filled from the settings: station, recent contests, CFG folder.
    pub fn from_settings(settings: &mut Settings) -> Self {
        MenuForm {
            station: settings.station(),
            recent: settings.recent_contests(),
            cfg_folder: settings.cfg_folder(),
            log_files: settings.log_contest_files(),
            ..MenuForm::default()
        }
    }

    /// The CFG folder when the operator changed it since the last call.
    pub fn take_cfg_folder_change(&mut self) -> Option<String> {
        std::mem::take(&mut self.folder_changed).then(|| self.cfg_folder.clone())
    }

    /// The chosen contest: the contest file, else the built-in one.
    fn current<'a>(&'a self, contests: &'a [Box<dyn ContestDef>]) -> Option<&'a dyn ContestDef> {
        match &self.file {
            Some(f) => Some(&*f.def),
            None => contests.get(self.contest).map(|c| &**c),
        }
    }

    /// Choose the contest defined by the file at `path`; a file that cannot be read
    /// leaves the choice as it was and shows why.
    fn choose_file(&mut self, path: &Path, registry: &Registry) {
        match registry.open_contest_file(path, crate::i18n::current()) {
            Ok(opened) => {
                let title = opened.def.caption(crate::i18n::current());
                self.titles.insert(path.to_path_buf(), title);
                self.file = Some(ChosenFile {
                    path: path.to_path_buf(),
                    def: opened.def,
                    warnings: opened.warnings,
                });
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// When the file name changed, choose the contest file its log was made with
    /// (zLog's `LastCFGFileName`), or ask for it when it is gone.
    fn check_log_path(&mut self, registry: &Registry) {
        if self.path == self.checked_path {
            return;
        }
        self.checked_path = self.path.clone();
        let log = PathBuf::from(self.path.trim());
        match contest_file_of_log(&log, &self.log_files, &self.cfg_folder) {
            Some(LogContestFile::Found(p)) => self.choose_file(&p, registry),
            Some(LogContestFile::Missing(p)) => self.locate = Some(p),
            None => {}
        }
    }

    fn open_picker(&mut self, files: &[Box<dyn ContestFile>]) {
        let mut picker = CfgPicker {
            open: true,
            folder: self.cfg_folder.clone(),
            ..CfgPicker::default()
        };
        picker.rescan(files);
        self.picker = Some(picker);
    }
}

impl Default for MenuForm {
    fn default() -> Self {
        MenuForm {
            contest: 0,
            search: String::new(),
            group: None,
            recent: Vec::new(),
            file: None,
            titles: HashMap::new(),
            cfg_folder: String::new(),
            folder_changed: false,
            log_files: Vec::new(),
            checked_path: String::new(),
            locate: None,
            picker: None,
            mode: ContestMode::Mix,
            station: Station::default(),
            power: None,
            coeff: "1".into(),
            path: String::new(),
            error: None,
        }
    }
}

impl MenuForm {
    /// Draw the form filling `ui` (the main window below its menu bar): OK and Exit
    /// pinned to the bottom, the rest scrolling. Returns the setup when OK is
    /// pressed with valid input; Exit closes the program.
    pub fn show(&mut self, ui: &mut egui::Ui, registry: &Registry) -> Option<Setup> {
        self.check_log_path(registry);
        self.dialogs(ui.ctx(), registry);
        let ((_, pressed), _) = crate::dialog::window_parts(
            ui,
            "menu_form",
            None::<fn(&mut egui::Ui)>,
            Some(|ui: &mut egui::Ui| {
                ui.horizontal(|ui| {
                    if ui.button(tr("button.ok")).clicked() {
                        Some(true)
                    } else if ui.button(tr("menu.file.exit")).clicked() {
                        Some(false)
                    } else {
                        None
                    }
                })
                .inner
            }),
            |ui, _| self.form(ui, registry),
        );
        match pressed.flatten() {
            Some(true) => match self.validate() {
                Ok(setup) => return Some(setup),
                Err(e) => self.error = Some(e),
            },
            Some(false) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            None => {}
        }
        None
    }

    /// The fields of the form: contest and mode, then the station in two columns.
    fn form(&mut self, ui: &mut egui::Ui, registry: &Registry) {
        let contests = registry.contests();
        ui.heading(tr("menu_form.title"));
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            self.contest_selector(ui, registry);
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(tr("menu_form.mode"));
                    let modes = self.current(contests).map_or(&[][..], |c| c.modes());
                    let mut mode = self.mode;
                    for &m in modes {
                        ui.radio_value(&mut mode, m, mode_caption(m));
                    }
                    self.mode = mode;
                });
            });
        });
        if let Some(f) = &self.file {
            ui.label(format!(
                "{}: {}",
                tr("menu_form.cfg_file"),
                f.path.display()
            ));
            for w in &f.warnings {
                ui.colored_label(egui::Color32::from_rgb(200, 120, 0), w);
            }
        }
        ui.add_space(6.0);
        // Two label-field pairs a row, so the common case fits without scrolling.
        let st = &mut self.station;
        let pairs: [(&str, &mut String, f32); 9] = [
            ("menu_form.callsign", &mut st.callsign, 120.0),
            ("menu_form.operator", &mut st.operator, 120.0),
            ("menu_form.prov", &mut st.prov, 60.0),
            ("menu_form.city", &mut st.city, 80.0),
            ("menu_form.age", &mut st.age, 60.0),
            ("menu_form.iota", &mut st.iota, 80.0),
            ("menu_form.cq_zone", &mut st.cq_zone, 60.0),
            ("menu_form.iaru_zone", &mut st.iaru_zone, 60.0),
            ("menu_form.itu_region", &mut st.itu_region, 60.0),
        ];
        egui::Grid::new("menu_station")
            .num_columns(4)
            .show(ui, |ui| {
                for (i, (key, value, width)) in pairs.into_iter().enumerate() {
                    ui.label(tr(key));
                    upper_edit(ui, value, width);
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
                ui.label(tr("menu_form.tx_nr"));
                ui.add(egui::DragValue::new(&mut st.tx_nr).range(0..=15));
                ui.end_row();
            });
        egui::Grid::new("menu_fields")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label(tr("menu_form.power"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.power, None, tr("menu_form.power_by_band"));
                    for p in ['H', 'M', 'L', 'P'] {
                        ui.radio_value(&mut self.power, Power::from_code(p), p.to_string());
                    }
                });
                ui.end_row();
                ui.label(tr("menu_form.coeff"));
                ui.add_enabled(
                    self.current(contests).is_some_and(|c| c.uses_coeff()),
                    egui::TextEdit::singleline(&mut self.coeff).desired_width(60.0),
                );
                ui.end_row();
                ui.label(tr("menu_form.file"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(260.0));
                    // A save panel cannot pick an existing file on macOS, so opening
                    // and creating are separate buttons.
                    if ui.button(tr("menu_form.open")).clicked() {
                        if let Some(p) = crate::dialog::open_log_file() {
                            self.path = p.display().to_string();
                        }
                    }
                    if ui.button(tr("menu_form.new")).clicked() {
                        if let Some(p) =
                            crate::dialog::choose_log_file("contest.zlox", zlog_core::Format::Zlox)
                        {
                            self.path = p.display().to_string();
                        }
                    }
                });
                ui.end_row();
            });
        if let Some(e) = &self.error {
            ui.colored_label(egui::Color32::RED, e);
        }
    }

    /// Groups on the left, the contests of the chosen group (or the search results)
    /// on the right; the layout grows with the number of contests linked in. Below
    /// the groups, "User-defined contest..." opens the CFG picker.
    fn contest_selector(&mut self, ui: &mut egui::Ui, registry: &Registry) {
        let contests = registry.contests();
        let lang = crate::i18n::current();
        let mut groups: Vec<String> = Vec::new();
        for c in contests {
            let g = c.group(lang);
            if !groups.contains(&g) {
                groups.push(g);
            }
        }
        if self.group.is_none() && self.recent.is_empty() {
            self.group = groups.first().cloned();
        }
        let needle = self.search.trim().to_lowercase();
        let shown: Vec<Row> = if !needle.is_empty() {
            (0..contests.len())
                .filter(|i| {
                    contests[*i].caption(lang).to_lowercase().contains(&needle)
                        || contests[*i].id().contains(&needle)
                })
                .map(Row::Builtin)
                .collect()
        } else if let Some(g) = &self.group {
            (0..contests.len())
                .filter(|i| contests[*i].group(lang) == *g)
                .map(Row::Builtin)
                .collect()
        } else {
            recent_rows(&self.recent, contests)
        };
        // Titles of the contest files listed, read once each.
        for row in &shown {
            if let Row::File(p) = row {
                if !self.titles.contains_key(p) {
                    let title = registry
                        .contest_files()
                        .iter()
                        .find(|f| f.accepts(p))
                        .and_then(|f| f.title(p))
                        .unwrap_or_else(|| file_name(p));
                    self.titles.insert(p.clone(), title);
                }
            }
        }
        let mut pick: Option<Row> = None;
        let mut picker = false;
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label(tr("menu_form.contest"));
                ui.horizontal(|ui| {
                    ui.label(tr("menu_form.search"));
                    ui.text_edit_singleline(&mut self.search);
                });
                // Two columns of fixed height: groups, and the contests of the group.
                let size = egui::vec2(340.0, 170.0);
                let layout = egui::Layout::left_to_right(egui::Align::Min);
                ui.allocate_ui_with_layout(size, layout, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("groups")
                        .max_height(size.y)
                        .show(ui, |ui| {
                            ui.set_width(110.0);
                            ui.vertical(|ui| {
                                if !self.recent.is_empty() {
                                    ui.selectable_value(
                                        &mut self.group,
                                        None,
                                        tr("menu_form.recent"),
                                    );
                                }
                                for g in &groups {
                                    ui.selectable_value(&mut self.group, Some(g.clone()), g);
                                }
                                if !registry.contest_files().is_empty()
                                    && ui
                                        .selectable_label(false, tr("menu_form.user_defined"))
                                        .clicked()
                                {
                                    picker = true;
                                }
                            });
                        });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("contests")
                        .max_height(size.y)
                        .show(ui, |ui| {
                            ui.set_width(210.0);
                            ui.vertical(|ui| {
                                for row in shown {
                                    let (on, caption) = match &row {
                                        Row::Builtin(i) => (
                                            self.file.is_none() && self.contest == *i,
                                            contests[*i].caption(lang),
                                        ),
                                        Row::File(p) => (
                                            self.file.as_ref().is_some_and(|f| f.path == *p),
                                            self.titles.get(p).cloned().unwrap_or_default(),
                                        ),
                                    };
                                    if ui.radio(on, caption).clicked() {
                                        pick = Some(row);
                                    }
                                }
                            });
                        });
                });
            });
        });
        match pick {
            Some(Row::Builtin(i)) => {
                self.contest = i;
                self.file = None;
            }
            Some(Row::File(p)) => self.choose_file(&p, registry),
            None => {}
        }
        if picker {
            self.open_picker(registry.contest_files());
        }
        // Keep the mode one the chosen contest offers.
        if let Some(c) = self.current(contests) {
            if !c.modes().contains(&self.mode) {
                self.mode = c.modes().first().copied().unwrap_or(ContestMode::Mix);
            }
        }
    }

    /// The CFG picker and the question for a log's missing contest file, each over
    /// the zLog Menu while open.
    fn dialogs(&mut self, ctx: &egui::Context, registry: &Registry) {
        let files = registry.contest_files();
        if let Some(picker) = &mut self.picker {
            let chosen = picker.show(ctx, files);
            if picker.folder != self.cfg_folder {
                // zLog records the folder only while none is set; here the last
                // folder listed is kept, which is what the operator expects next time.
                self.cfg_folder = picker.folder.clone();
                self.folder_changed = true;
            }
            if !picker.open {
                self.picker = None;
            }
            if let Some(p) = chosen {
                self.picker = None;
                self.choose_file(&p, registry);
            }
        }
        if let Some(missing) = self.locate.clone() {
            let (_, answer) = crate::dialog::modal_with_buttons(
                ctx,
                "locate_contest_file",
                |ui| {
                    ui.set_max_width(420.0);
                    ui.label(
                        tr("menu_form.cfg_missing").replace("{0}", &missing.display().to_string()),
                    );
                },
                |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(tr("menu_form.locate")).clicked() {
                            Some(true)
                        } else if ui.button(tr("button.cancel")).clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Escape))
                        {
                            Some(false)
                        } else {
                            None
                        }
                    })
                    .inner
                },
            );
            match answer {
                Some(true) => {
                    self.locate = None;
                    if let Some(p) = pick_contest_file(files) {
                        self.choose_file(&p, registry);
                    }
                }
                Some(false) => self.locate = None,
                None => {}
            }
        }
    }

    fn validate(&self) -> Result<Setup, String> {
        if self.path.trim().is_empty() {
            return Err(tr("menu_form.need_file").into());
        }
        let coeff: f64 = self
            .coeff
            .trim()
            .parse()
            .map_err(|_| tr("menu_form.bad_coeff"))?;
        Ok(Setup {
            contest: self.contest,
            contest_file: self.file.as_ref().map(|f| f.path.clone()),
            mode: self.mode,
            station: {
                let mut st = self.station.clone();
                if let Some(p) = self.power {
                    st.set_power_everywhere(p);
                }
                st
            },
            coeff,
            path: PathBuf::from(self.path.trim()),
            // Filled in by the caller from the settings and the operator's answer.
            use_contest_period: true,
            convert_old_other: false,
        })
    }
}

fn upper_edit(ui: &mut egui::Ui, text: &mut String, width: f32) {
    ui.add(egui::TextEdit::singleline(text).desired_width(width));
    *text = text.to_uppercase();
}

fn mode_caption(m: ContestMode) -> &'static str {
    match m {
        ContestMode::Mix => "PH/CW(MIX)",
        ContestMode::Cw => "CW",
        ContestMode::Phone => "PH",
        ContestMode::Rtty => "RTTY",
        ContestMode::All => "ALL",
    }
}

/// A row of the contest list: a built-in contest, or a contest file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Row {
    Builtin(usize),
    File(PathBuf),
}

/// The rows of the recent list: built-in contests still linked in, and every
/// contest file (shown by title; one that is gone fails when chosen).
fn recent_rows(recent: &[RecentContest], contests: &[Box<dyn ContestDef>]) -> Vec<Row> {
    recent
        .iter()
        .filter_map(|r| match r {
            RecentContest::Id(id) => contests.iter().position(|c| c.id() == id).map(Row::Builtin),
            RecentContest::File(p) => Some(Row::File(p.clone())),
        })
        .collect()
}

/// Where the contest file of a log is, when the log was made with one.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LogContestFile {
    Found(PathBuf),
    /// Neither where it was nor in the CFG folder.
    Missing(PathBuf),
}

/// The contest file remembered for `log`: where it was, else a file of the same
/// name in the CFG folder (`TMenuForm.FormShow`: `CfgDatPath + ExtractFileName`).
fn contest_file_of_log(
    log: &Path,
    remembered: &[(PathBuf, PathBuf)],
    folder: &str,
) -> Option<LogContestFile> {
    let (_, file) = remembered.iter().find(|(l, _)| l == log)?;
    if file.is_file() {
        return Some(LogContestFile::Found(file.clone()));
    }
    if !folder.is_empty() {
        let moved = Path::new(folder).join(file.file_name()?);
        if moved.is_file() {
            return Some(LogContestFile::Found(moved));
        }
    }
    Some(LogContestFile::Missing(file.clone()))
}

/// The contest files in `dir` with their titles, by file name
/// (`TSelectUserDefinedContest.ScanCfgFiles`).
fn scan_folder(dir: &Path, files: &[Box<dyn ContestFile>]) -> Vec<(PathBuf, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, String)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| {
            let kind = files.iter().find(|f| f.accepts(&p))?;
            let title = kind.title(&p).unwrap_or_default();
            Some((p, title))
        })
        .collect();
    found.sort_by_key(|(p, _)| file_name(p).to_lowercase());
    found
}

/// Indices of the entries whose file name or title contains `filter`, ignoring
/// case (`TSelectUserDefinedContest.ShowCfgFiles`).
fn filter_entries(entries: &[(PathBuf, String)], filter: &str) -> Vec<usize> {
    let needle = filter.trim().to_uppercase();
    (0..entries.len())
        .filter(|&i| {
            let (p, title) = &entries[i];
            needle.is_empty()
                || file_name(p).to_uppercase().contains(&needle)
                || title.to_uppercase().contains(&needle)
        })
        .collect()
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Pick a contest file of any kind linked in.
fn pick_contest_file(files: &[Box<dyn ContestFile>]) -> Option<PathBuf> {
    let lang = crate::i18n::current();
    let exts: Vec<&str> = files.iter().flat_map(|f| f.extensions().to_vec()).collect();
    let name = files.first().map(|f| f.name(lang)).unwrap_or_default();
    crate::dialog::import_file(&name, &exts)
}

/// The list of CFG files of a folder with their contest names, as zLog's
/// `TSelectUserDefinedContest`; also lets the operator browse for one file.
#[derive(Default)]
struct CfgPicker {
    open: bool,
    folder: String,
    entries: Vec<(PathBuf, String)>,
    filter: String,
    selected: Option<usize>,
}

impl CfgPicker {
    fn rescan(&mut self, files: &[Box<dyn ContestFile>]) {
        self.entries = scan_folder(Path::new(&self.folder), files);
        self.selected = None;
    }

    /// Draw the picker; returns the file chosen (OK, a double click or Browse file).
    fn show(&mut self, ctx: &egui::Context, files: &[Box<dyn ContestFile>]) -> Option<PathBuf> {
        let mut chosen = None;
        let mut rescan = false;
        let shown = filter_entries(&self.entries, &self.filter);
        let ok_enabled = self.selected.is_some();
        let result = crate::dialog::dialog_with_buttons(
            ctx,
            "cfg_picker",
            tr("cfg_picker.title"),
            [560.0, 420.0],
            &mut self.open,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("cfg_picker.folder"));
                    let edit =
                        ui.add(egui::TextEdit::singleline(&mut self.folder).desired_width(300.0));
                    if edit.lost_focus() {
                        rescan = true;
                    }
                    if ui.button(tr("cfg_picker.choose_folder")).clicked() {
                        if let Some(d) = crate::dialog::pick_folder(&self.folder) {
                            self.folder = d.display().to_string();
                            rescan = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("menu_form.search"));
                    ui.text_edit_singleline(&mut self.filter);
                    if ui.button(tr("cfg_picker.browse_file")).clicked() {
                        chosen = pick_contest_file(files);
                    }
                });
                ui.separator();
                if self.entries.is_empty() {
                    ui.label(tr("cfg_picker.none"));
                }
                egui::Grid::new("cfg_files").striped(true).show(ui, |ui| {
                    ui.strong(tr("cfg_picker.file"));
                    ui.strong(tr("cfg_picker.contest"));
                    ui.end_row();
                    for &i in &shown {
                        let (p, title) = &self.entries[i];
                        let r = ui.selectable_label(self.selected == Some(i), file_name(p));
                        if r.clicked() {
                            self.selected = Some(i);
                        }
                        if r.double_clicked() {
                            chosen = Some(p.clone());
                        }
                        ui.label(title);
                        ui.end_row();
                    }
                });
            },
            |ui| crate::dialog::ok_cancel(ui, ok_enabled, true),
        );
        match result.and_then(|(_, b)| b) {
            Some(true) => {
                chosen = self
                    .selected
                    .and_then(|i| self.entries.get(i))
                    .map(|(p, _)| p.clone())
            }
            Some(false) => self.open = false,
            None => {}
        }
        if rescan {
            self.rescan(files);
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("zlog-gui-menu-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn recent_rows_keep_files_and_linked_contests() {
        let contests = zlog_core::Registry::new().with(zlog_rule_ja::Japan);
        let contests = contests.contests();
        let id = contests[1].id().to_string();
        let recent = vec![
            RecentContest::File(PathBuf::from("/x/a.cfg")),
            RecentContest::Id("gone".into()),
            RecentContest::Id(id),
        ];
        assert_eq!(
            recent_rows(&recent, contests),
            vec![Row::File(PathBuf::from("/x/a.cfg")), Row::Builtin(1)]
        );
    }

    #[test]
    fn a_logs_contest_file_is_found_moved_or_missing() {
        let dir = scratch("log");
        let moved = dir.join("moved");
        std::fs::create_dir_all(&moved).unwrap();
        let cfg = dir.join("a.cfg");
        std::fs::write(&cfg, "#A\n").unwrap();
        let log = PathBuf::from("/logs/a.zlox");
        let gone = PathBuf::from("/nowhere/b.cfg");
        let remembered = vec![
            (log.clone(), cfg.clone()),
            (PathBuf::from("/b.zlox"), gone.clone()),
        ];
        assert_eq!(
            contest_file_of_log(&log, &remembered, ""),
            Some(LogContestFile::Found(cfg))
        );
        assert_eq!(
            contest_file_of_log(Path::new("/c.zlox"), &remembered, ""),
            None
        );
        let b = Path::new("/b.zlox");
        assert_eq!(
            contest_file_of_log(b, &remembered, ""),
            Some(LogContestFile::Missing(gone))
        );
        std::fs::write(moved.join("b.cfg"), "#B\n").unwrap();
        assert_eq!(
            contest_file_of_log(b, &remembered, &moved.display().to_string()),
            Some(LogContestFile::Found(moved.join("b.cfg")))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "rule-ja")]
    #[test]
    fn folder_lists_cfg_files_with_titles_and_filters() {
        let dir = scratch("scan");
        std::fs::write(dir.join("b.CFG"), "#Beta Contest\n").unwrap();
        std::fs::write(dir.join("a.cfg"), "#Alpha\n").unwrap();
        std::fs::write(dir.join("a.dat"), "x\n").unwrap();
        let reg = zlog_core::Registry::new().with(zlog_rule_ja::Japan);
        let entries = scan_folder(&dir, reg.contest_files());
        let titles: Vec<&str> = entries.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(titles, ["Alpha", "Beta Contest"]);
        assert_eq!(filter_entries(&entries, "beta"), [1]);
        assert_eq!(filter_entries(&entries, "A.C"), [0]);
        assert_eq!(filter_entries(&entries, ""), [0, 1]);

        // Choosing a file makes it the contest, with its own modes and coefficient.
        let mut form = MenuForm {
            path: "x.zlox".into(),
            ..MenuForm::default()
        };
        form.choose_file(&dir.join("b.CFG"), &reg);
        assert_eq!(
            form.current(reg.contests()).unwrap().caption("en"),
            "Beta Contest"
        );
        let setup = form.validate().unwrap();
        assert_eq!(setup.contest_file, Some(dir.join("b.CFG")));
        form.choose_file(&dir.join("none.cfg"), &reg);
        assert!(form.error.is_some());
        assert_eq!(
            form.validate().unwrap().contest_file,
            Some(dir.join("b.CFG"))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
