//! Top level: the zLog Menu until a contest is open, then the main window.

use crate::dialog::dialog;
use crate::i18n::{self, tr};
use crate::main_window::{self, Field, View};
use crate::menu_form::MenuForm;
use crate::options::{rig_window, Options};
use crate::registry::{panels, registry, settings_pages};
use crate::rig::{RigConfig, RigControl};
use crate::session::Session;
use crate::settings::{RecentContest, Settings};
use crate::windows;

/// A window added by an extension, and whether it is open.
struct PanelSlot {
    panel: Box<dyn zlog_ui::Panel>,
    open: bool,
}

pub struct ZLogApp {
    session: Option<Session>,
    panels: Vec<PanelSlot>,
    menu: MenuForm,
    view: View,
    show_about: bool,
    settings: Settings,
    options: Options,
    rig: RigControl,
    show_rig: bool,
    /// Spots from the panels, kept for [`SPOT_LIFETIME`] seconds.
    spots: Vec<zlog_ui::Spot>,
    /// When the panels' settings were last checked for changes ([`PANEL_SAVE_EVERY`]).
    panels_checked: std::time::Instant,
    /// A setup waiting for the answer whether to convert old Other QSOs.
    asking_old_other: Option<crate::session::Setup>,
    /// The QSO locked on Z-Link while the edit dialog is open.
    #[cfg(feature = "zserver")]
    edit_lock: Option<zlog_core::Qso>,
}

/// How often the panels' settings are saved when one of them changed.
const PANEL_SAVE_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// How long a spot stays in [`zlog_ui::View::spots`], in seconds.
const SPOT_LIFETIME: i64 = 30 * 60;

/// The manual of zLog, opened from the Help menu.
const MANUAL_URL: &str = "https://use.zlog.org";

impl ZLogApp {
    pub fn new() -> Self {
        let mut settings = Settings::load();
        // The language chosen in the menu wins over the locale of the system.
        if let Some(code) = settings.language() {
            i18n::set_language(&code);
        }
        let menu = MenuForm::from_settings(&mut settings);
        ZLogApp {
            session: None,
            panels: panels()
                .into_iter()
                .map(|mut panel| {
                    panel.load(settings.section(&panel_section(panel.id())));
                    PanelSlot { panel, open: false }
                })
                .collect(),
            menu,
            view: View::default(),
            show_about: false,
            options: Options::new(settings_pages()),
            settings,
            rig: RigControl::default(),
            show_rig: false,
            spots: Vec::new(),
            panels_checked: std::time::Instant::now(),
            asking_old_other: None,
            #[cfg(feature = "zserver")]
            edit_lock: None,
        }
    }

    /// Open the contest of a setup, or show why it failed in the zLog Menu.
    fn open(&mut self, setup: crate::session::Setup) {
        match Session::open(setup, registry()) {
            Ok(mut s) => {
                s.apply_preferences(&self.settings.preferences());
                if s.asks_start_time {
                    self.view.start_time =
                        Some(main_window::StartTimeForm::new(s.start_time(), true));
                }
                self.session = Some(s);
                self.view.focus = Some(Field::Call);
            }
            Err(e) => self.menu.error = Some(e),
        }
    }

    /// Tell Z-Link when the edit dialog opens and closes (`TEditDialog.Init` locks
    /// the QSO, closing unlocks it).
    #[cfg(feature = "zserver")]
    fn track_edit_lock(&mut self) {
        let editing = self.view.editing.as_ref().and_then(|(id, _)| {
            let s = self.session.as_ref()?;
            s.find(*id).map(|i| s.log().qsos[i].clone())
        });
        match (&self.edit_lock, editing) {
            (None, Some(q)) => {
                zlog_zserver::edit_started(&q);
                self.edit_lock = Some(q);
            }
            (Some(_), None) => {
                if let Some(q) = self.edit_lock.take() {
                    zlog_zserver::edit_ended(&q);
                }
            }
            _ => {}
        }
    }

    /// Let every panel write what it keeps, and save the settings file when any
    /// section changed (on exit, when the log is closed and every
    /// [`PANEL_SAVE_EVERY`]).
    fn save_panels(&mut self) {
        let mut changed = false;
        for slot in &self.panels {
            let section = self.settings.section(&panel_section(slot.panel.id()));
            let mut fresh = section.clone();
            slot.panel.save(&mut fresh);
            if fresh != *section {
                *section = fresh;
                changed = true;
            }
        }
        if changed {
            let _ = self.settings.save();
        }
        self.panels_checked = std::time::Instant::now();
    }

    fn close(&mut self) {
        if let Some(mut s) = self.session.take() {
            s.close();
        }
        self.view = View::default();
        self.save_panels();
    }
}

/// The Language menu; the choice is saved in the settings for the next start.
fn language_menu(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.menu_button(tr("menu.language"), |ui| {
        for l in i18n::LANGUAGES {
            if ui.radio(i18n::current() == l.code, l.name).clicked() {
                i18n::set_language(l.code);
                crate::fonts::install(ui.ctx());
                settings.set_language(l.code);
                let _ = settings.save();
            }
        }
    });
}

impl eframe::App for ZLogApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.session.is_some() && self.panels_checked.elapsed() >= PANEL_SAVE_EVERY {
            self.save_panels();
        }
        if self.session.is_none() && self.asking_old_other.is_some() {
            if let Some(answer) = ask_old_other(&ctx) {
                if let Some(mut setup) = self.asking_old_other.take() {
                    setup.convert_old_other = answer;
                    self.open(setup);
                }
            }
        }
        let Some(session) = self.session.as_mut() else {
            let linked = registry();
            let contests = linked.contests();
            egui::Panel::top("menu").show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| language_menu(ui, &mut self.settings))
            });
            {
                let shown = self.menu.show(ui, &linked);
                if let Some(folder) = self.menu.take_cfg_folder_change() {
                    self.settings.set_cfg_folder(&folder);
                    let _ = self.settings.save();
                }
                if let Some(mut setup) = shown.filter(|_| self.asking_old_other.is_none()) {
                    self.settings.set_station(&setup.station);
                    let recent = match &setup.contest_file {
                        Some(file) => Some(RecentContest::File(file.clone())),
                        None => contests
                            .get(setup.contest)
                            .map(|c| RecentContest::Id(c.id().to_string())),
                    };
                    if let Some(r) = recent {
                        self.settings.add_recent_contest(r);
                        self.menu.recent = self.settings.recent_contests();
                    }
                    self.settings
                        .set_log_contest_file(&setup.path, setup.contest_file.as_deref());
                    self.menu.log_files = self.settings.log_contest_files();
                    let _ = self.settings.save();
                    setup.use_contest_period = self.settings.use_contest_period();
                    if crate::session::asks_old_other(&setup.path) {
                        self.asking_old_other = Some(setup);
                    } else {
                        self.open(setup);
                    }
                }
            }
            return;
        };

        session.poll();
        route_keys(
            &ctx,
            session,
            &mut self.view,
            &mut self.panels,
            &self.options,
            &self.spots,
            &self.rig,
        );
        self.rig.tick(
            std::time::Instant::now(),
            &mut session.entry,
            &session.station,
        );

        let mut close = false;
        let mut close_all = false;
        let dialogs = open_dialogs(
            &self.view,
            &self.options,
            self.show_rig,
            &self.panels,
            self.show_about,
        );
        crate::dialog::set_keep_on_top(&ctx, self.settings.keep_dialogs_on_top());
        let save_key = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
        if ctx.input_mut(|i| i.consume_shortcut(&save_key)) {
            session.save();
        }
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(tr("menu.file"), |ui| {
                    if ui.button(tr("menu.file.open")).clicked() {
                        close = true;
                    }
                    let save = egui::Button::new(tr("menu.file.save"))
                        .shortcut_text(ui.ctx().format_shortcut(&save_key));
                    if ui.add(save).clicked() {
                        session.save();
                    }
                    for (key, format) in [
                        ("menu.file.save_as", zlog_core::Format::Zlox),
                        ("menu.file.save_as_zlo", zlog_core::Format::Zlo),
                    ] {
                        if ui.button(tr(key)).clicked() {
                            let name = session
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned());
                            let picked = crate::dialog::choose_log_file(
                                name.as_deref().unwrap_or("contest.zlox"),
                                format,
                            );
                            if let Some(p) = picked {
                                session.save_as(p);
                            }
                        }
                    }
                    ui.separator();
                    file_formats(ui, session);
                    ui.separator();
                    // `menuCorrectStartTime`, enabled for contests that use the period.
                    let period = session.rules().uses_period() && session.use_contest_period;
                    if ui
                        .add_enabled(period, egui::Button::new(tr("menu.file.start_time")))
                        .clicked()
                    {
                        self.view.start_time =
                            Some(main_window::StartTimeForm::new(session.start_time(), false));
                    }
                    ui.separator();
                    if ui.button(tr("menu.file.exit")).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button(tr("menu.edit"), |ui| {
                    edit_menu(ui, session, &mut self.view, &self.panels);
                });
                let groups = crate::menus::grouped(
                    self.panels
                        .iter()
                        .enumerate()
                        .map(|(i, s)| (s.panel.menu(), i, ())),
                );
                // The Device menu is always there, for the rig.
                let device = !groups.iter().any(|(g, _)| *g == zlog_ui::MenuGroup::Device);
                let groups = groups
                    .into_iter()
                    .chain(device.then(|| (zlog_ui::MenuGroup::Device, Vec::new())));
                let mut groups: Vec<_> = groups.collect();
                groups.sort_by_key(|(g, _)| crate::menus::position(*g));
                for (group, list) in groups {
                    ui.menu_button(tr(crate::menus::group_key(group)), |ui| {
                        if group == zlog_ui::MenuGroup::Device {
                            let config = RigConfig::from_section(self.settings.section("rig"));
                            device_menu(ui, &mut self.rig, &mut self.show_rig, &config);
                            if !list.is_empty() {
                                ui.separator();
                            }
                        }
                        for (i, ()) in list {
                            let slot = &mut self.panels[i];
                            ui.checkbox(&mut slot.open, slot.panel.title(i18n::current()));
                        }
                    });
                }
                ui.menu_button(tr("menu.window"), |ui| {
                    let any = !dialogs.is_empty();
                    if ui
                        .add_enabled(any, egui::Button::new(tr("menu.window.front")))
                        .clicked()
                    {
                        for (id, _) in &dialogs {
                            crate::dialog::focus(&ctx, id);
                        }
                    }
                    let mut on_top = self.settings.keep_dialogs_on_top();
                    if ui.checkbox(&mut on_top, tr("menu.window.on_top")).changed() {
                        self.settings.set_keep_dialogs_on_top(on_top);
                        let _ = self.settings.save();
                        crate::dialog::set_keep_on_top(&ctx, on_top);
                        let level = if on_top {
                            egui::WindowLevel::AlwaysOnTop
                        } else {
                            egui::WindowLevel::Normal
                        };
                        for (id, _) in &dialogs {
                            ctx.send_viewport_cmd_to(
                                crate::dialog::viewport_id(id),
                                egui::ViewportCommand::WindowLevel(level),
                            );
                        }
                    }
                    if ui
                        .add_enabled(any, egui::Button::new(tr("menu.window.close_all")))
                        .clicked()
                    {
                        close_all = true;
                    }
                    if any {
                        ui.separator();
                    }
                    for (id, title) in &dialogs {
                        if ui.button(title).clicked() {
                            crate::dialog::focus(&ctx, id);
                        }
                    }
                });
                ui.menu_button(tr("menu.settings"), |ui| {
                    if ui.button(tr("menu.settings.options")).clicked() {
                        self.options.show_with(&mut self.settings);
                    }
                    ui.separator();
                    ui.menu_button(tr("menu.settings.toolbar"), |ui| {
                        for slot in &self.panels {
                            if slot.panel.toolbar(i18n::current()).is_empty() {
                                continue;
                            }
                            let id = slot.panel.id();
                            let mut shown = self.settings.toolbar_shown(id);
                            if ui
                                .checkbox(&mut shown, slot.panel.title(i18n::current()))
                                .changed()
                            {
                                self.settings.set_toolbar_shown(id, shown);
                                let _ = self.settings.save();
                            }
                        }
                    });
                    language_menu(ui, &mut self.settings);
                });
                ui.menu_button(tr("menu.help"), |ui| {
                    if ui.button(tr("menu.help.about")).clicked() {
                        self.show_about = true;
                    }
                    if ui.button(tr("menu.help.manual")).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(MANUAL_URL));
                    }
                    // Extensions' Help items (the updater, say) open their windows.
                    let help: Vec<usize> = (0..self.panels.len())
                        .filter(|&i| self.panels[i].panel.menu() == zlog_ui::MenuGroup::Help)
                        .collect();
                    if !help.is_empty() {
                        ui.separator();
                    }
                    for i in help {
                        let slot = &mut self.panels[i];
                        if ui.button(slot.panel.title(i18n::current())).clicked() {
                            slot.open = true;
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                if ui
                    .button(tr("toolbar.save"))
                    .on_hover_text(ui.ctx().format_shortcut(&save_key))
                    .clicked()
                {
                    session.save();
                }
                toolbar(ui, &mut self.panels, &mut self.settings);
            });
        });
        if close_all {
            // The edit dialog and Options close as by Cancel.
            self.view.editing = None;
            self.options.open = false;
            self.show_rig = false;
            self.show_about = false;
            for slot in &mut self.panels {
                slot.open = false;
            }
        }
        egui::Panel::bottom("status").show(ui, |ui| main_window::status_bar(ui, session));
        let typed = egui::Panel::bottom("entry")
            .show(ui, |ui| main_window::entry_row(ui, session, &mut self.view))
            .inner;
        if typed {
            for slot in &mut self.panels {
                slot.panel.entry_typed();
            }
        }
        let panels = &self.panels;
        let extra = |q: &zlog_core::Qso| qso_items(panels, q);
        egui::CentralPanel::default().show(ui, |ui| {
            main_window::log_grid(ui, session, &mut self.view, &extra)
        });

        match self.view.grid_action.take() {
            Some((id, main_window::GridAction::Extension(i, item))) => {
                if let Some(q) = session.find(id).map(|n| session.log().qsos[n].clone()) {
                    if let Some(slot) = self.panels.get_mut(i) {
                        if slot.panel.on_qso_menu(&item, &q) {
                            slot.open = true;
                        }
                    }
                }
            }
            Some((id, action)) => {
                main_window::apply_grid_action(session, &mut self.view, id, action)
            }
            None => {}
        }
        windows::edit_dialog(&ctx, session, &mut self.view);
        windows::start_time_dialog(&ctx, session, &mut self.view);
        windows::confirm_delete(&ctx, session, &mut self.view);
        if self.options.ui(&ctx, &mut self.settings) {
            session.station = self.options.station().clone();
            session.use_contest_period = self.options.use_contest_period();
            session.apply_preferences(self.options.preferences());
            self.menu.set_station(self.settings.station());
            if self.rig.connected() {
                self.rig.connect(self.options.rig().clone());
            }
        }
        let rig_config = RigConfig::from_section(self.settings.section("rig"));
        rig_window(&ctx, &mut self.rig, &mut self.show_rig, &rig_config);
        let view = panel_view(&ctx, session, &self.spots, &self.rig);
        for slot in &mut self.panels {
            slot.panel.tick(&view);
        }
        for slot in &mut self.panels {
            let (id, title, size) = (
                slot.panel.id().to_string(),
                slot.panel.title(view.language),
                slot.panel.default_size(),
            );
            dialog(&ctx, &id, &title, size, &mut slot.open, |ui| {
                slot.panel.ui(ui, &view)
            });
        }
        let now = chrono::Utc::now().timestamp();
        for slot in &mut self.panels {
            self.spots.extend(slot.panel.take_spots());
        }
        self.spots.retain(|s| now - s.time < SPOT_LIFETIME);
        // Requests from panels to the entry row (a decoded callsign clicked).
        let actions: Vec<zlog_ui::EntryAction> = self
            .panels
            .iter_mut()
            .flat_map(|s| s.panel.take_actions())
            .collect();
        for action in actions {
            match action {
                zlog_ui::EntryAction::Callsign(c) => {
                    pick_callsign(&mut session.entry, c);
                    session.check_call();
                    self.view.focus = Some(Field::Call);
                }
                zlog_ui::EntryAction::Number(n) => {
                    session.entry.nr_rcvd = n;
                    session.check_number();
                    self.view.focus = Some(Field::Number);
                }
                zlog_ui::EntryAction::Qsy(hz, mode) => {
                    self.rig.qsy(hz, mode, &mut session.entry, &session.station);
                }
                zlog_ui::EntryAction::Cq(cq) => session.entry.cq = cq,
            }
        }
        dialog(
            &ctx,
            "about",
            tr("menu.help.about"),
            [300.0, 200.0],
            &mut self.show_about,
            |ui| {
                ui.heading("zLog");
                ui.label(env!("CARGO_PKG_VERSION"));
                ui.separator();
                ui.label(tr("about.extensions"));
                for name in registry().names() {
                    ui.label(format!("  {name}"));
                }
            },
        );

        let title = format!(
            "zLog - {} - {}",
            session.rules().name(),
            session.path.display()
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        // The clock in the entry row, and extensions waiting on the outside.
        ctx.request_repaint_after(std::time::Duration::from_millis(250));

        #[cfg(feature = "zserver")]
        self.track_edit_lock();
        if close {
            self.close();
        }
    }

    fn on_exit(&mut self) {
        self.save_panels();
        self.close();
    }
}

/// Keys of the main window: F1 to F12 (with or without Shift) and Esc go first to
/// the first panel that takes them (a CW keyer), then the entry row and the log grid
/// get the rest. zLog's `actionCQAbort` (ShortCut 27) runs before the grid's
/// `KeyDown`, so Esc aborts CW even with a grid row selected.
fn route_keys(
    ctx: &egui::Context,
    session: &mut Session,
    view: &mut View,
    panels: &mut [PanelSlot],
    options: &Options,
    spots: &[zlog_ui::Spot],
    rig: &RigControl,
) {
    if keys_for_panels(ctx, view, options) {
        let pv = panel_view(ctx, session, spots, rig);
        for n in 1..=12u8 {
            let Some(key) = egui::Key::from_name(&format!("F{n}")) else {
                continue;
            };
            for (modifiers, shift) in [
                (egui::Modifiers::NONE, false),
                (egui::Modifiers::SHIFT, true),
            ] {
                if pressed(ctx, modifiers, key)
                    && panels
                        .iter_mut()
                        .any(|s| s.panel.function_key(n, shift, &pv))
                {
                    ctx.input_mut(|i| i.consume_key(modifiers, key));
                }
            }
        }
        if pressed(ctx, egui::Modifiers::NONE, egui::Key::Escape)
            && panels.iter_mut().any(|s| s.panel.escape())
        {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
    }
    let hz = rig.state().and_then(|s| s.freq());
    main_window::handle_keys(ctx, session, view, hz);
}

/// The question whether to convert old Other QSOs (`TMainForm.LoadNewContestFromFile`):
/// `Some(answer)` once answered; Esc, Enter and closing answer No (zLog's default
/// button).
fn ask_old_other(ctx: &egui::Context) -> Option<bool> {
    let (_, answer) = crate::dialog::modal_with_buttons(
        ctx,
        "old_other",
        |ui| {
            ui.set_max_width(420.0);
            ui.label(tr("open.old_other"));
        },
        |ui| {
            ui.horizontal(|ui| {
                if ui.button(tr("button.yes")).clicked() {
                    Some(true)
                } else if ui.button(tr("button.no")).clicked()
                    || ui.input(|i| {
                        i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)
                    })
                {
                    Some(false)
                } else {
                    None
                }
            })
            .inner
        },
    );
    answer
}

/// Whether keys of the main window may go to the panels: no modal dialog is open
/// and no text field other than the entry row's has the keyboard.
fn keys_for_panels(ctx: &egui::Context, view: &View, options: &Options) -> bool {
    let modal = view.editing.is_some()
        || view.confirm_delete.is_some()
        || view.start_time.is_some()
        || options.open;
    let other_text =
        ctx.memory(|m| m.focused()).is_some() && main_window::focused_field(ctx).is_none();
    !modal && !other_text
}

/// Whether `key` was pressed this frame with exactly `modifiers`, without consuming it.
fn pressed(ctx: &egui::Context, modifiers: egui::Modifiers, key: egui::Key) -> bool {
    ctx.input(|i| {
        i.events.iter().any(|e| {
            matches!(e, egui::Event::Key { key: k, pressed: true, modifiers: m, .. }
                if *k == key && m.matches_exact(modifiers))
        })
    })
}

/// What the panels see of the open contest.
fn panel_view<'a>(
    ctx: &egui::Context,
    session: &'a Session,
    spots: &'a [zlog_ui::Spot],
    rig: &RigControl,
) -> zlog_ui::View<'a> {
    zlog_ui::View {
        log: session.log(),
        contest: session.rules(),
        sheet: session.sheet(),
        callsign: &session.entry.callsign,
        station: &session.station,
        number: &session.entry.nr_rcvd,
        rst_sent: &session.entry.rst_sent,
        power: session.entry.power,
        band: session.entry.band,
        mode: session.entry.mode,
        language: i18n::current(),
        spots,
        hz: rig.state().and_then(|s| s.freq()),
        cq: session.entry.cq,
        operator: &session.station.operator,
        period_hours: session.period_hours(),
        pc_name: &session.pc_name,
        focus: main_window::focused_field(ctx).map(entry_field),
    }
}

/// A callsign picked in a panel replaces the call and clears the number
/// (`SetYourCallsignEx`, main.pas); a following `Number` action may fill it again.
fn pick_callsign(entry: &mut crate::entry::Entry, callsign: String) {
    entry.callsign = callsign;
    entry.nr_rcvd.clear();
}

/// The entry field as the panels name it.
fn entry_field(f: Field) -> zlog_ui::EntryField {
    match f {
        Field::Call => zlog_ui::EntryField::Call,
        Field::RstSent => zlog_ui::EntryField::RstSent,
        Field::RstRcvd => zlog_ui::EntryField::RstRcvd,
        Field::Number => zlog_ui::EntryField::Number,
        Field::Memo => zlog_ui::EntryField::Memo,
    }
}

/// The Edit menu: what the right-click menu of the selected QSO offers, then the
/// entry row's own keys.
fn edit_menu(ui: &mut egui::Ui, session: &mut Session, view: &mut View, panels: &[PanelSlot]) {
    let selected = view
        .selected
        .and_then(|i| session.log().qsos.get(i))
        .map(|q| (zlog_core::QsoId::of(q), q.clone()));
    ui.add_enabled_ui(selected.is_some(), |ui| {
        let extra = selected
            .as_ref()
            .map(|(_, q)| qso_items(panels, q))
            .unwrap_or_default();
        if let Some(a) = main_window::grid_menu(ui, session, &extra) {
            if let Some((id, _)) = &selected {
                view.grid_action = Some((*id, a));
            }
        }
    });
    if selected.is_none() {
        ui.weak(tr("menu.edit.select_row"));
    }
    ui.separator();
    let shift = |k| {
        ui.ctx()
            .format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, k))
    };
    let keys = [
        shift(egui::Key::B),
        shift(egui::Key::M),
        shift(egui::Key::P),
        shift(egui::Key::O),
        ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::K,
        )),
    ];
    let item = |ui: &mut egui::Ui, key: &str, k: &str| {
        let clicked = ui
            .add(egui::Button::new(tr(key)).shortcut_text(k))
            .clicked();
        if clicked {
            ui.close();
        }
        clicked
    };
    if item(ui, "menu.edit.next_band", &keys[0]) {
        session.cycle_band(false);
    }
    if item(ui, "menu.edit.next_mode", &keys[1]) {
        session.cycle_mode(false);
    }
    if item(ui, "menu.edit.next_power", &keys[2]) {
        session.entry.cycle_power();
    }
    if item(ui, "menu.edit.cq_sp", &keys[3]) {
        session.entry.cq = !session.entry.cq;
    }
    if item(ui, "menu.edit.clear", &keys[4]) {
        session.entry.clear_call_and_number();
        session.status = Default::default();
        view.focus = Some(Field::Call);
    }
}

/// The extensions' toolbar buttons, grouped by menu group with separators, in
/// registry order; panels hidden from the Settings menu are left out.
fn toolbar(ui: &mut egui::Ui, panels: &mut [PanelSlot], settings: &mut Settings) {
    let lang = i18n::current();
    let buttons = crate::menus::grouped(panels.iter().enumerate().filter_map(|(i, s)| {
        let b = s.panel.toolbar(lang);
        (!b.is_empty() && settings.toolbar_shown(s.panel.id())).then(|| (s.panel.menu(), i, b))
    }));
    for (_, list) in buttons {
        ui.separator();
        for (i, bs) in list {
            for b in bs {
                let slot = &mut panels[i];
                match &b.kind {
                    zlog_ui::ToolbarKind::Window => {
                        ui.toggle_value(&mut slot.open, &b.label)
                            .on_hover_text(&b.tooltip);
                    }
                    zlog_ui::ToolbarKind::Action(id) => {
                        if ui.button(&b.label).on_hover_text(&b.tooltip).clicked() {
                            slot.panel.on_toolbar_action(id);
                        }
                    }
                }
            }
        }
    }
}

/// The items the panels add to the right-click menu of `q`, by menu group.
fn qso_items(panels: &[PanelSlot], q: &zlog_core::Qso) -> main_window::ExtensionItems {
    let lang = i18n::current();
    let items = panels.iter().enumerate().flat_map(|(i, s)| {
        let g = s.panel.menu();
        s.panel
            .qso_menu(lang, q)
            .into_iter()
            .map(move |it| (g, i, it))
    });
    crate::menus::grouped(items)
        .into_iter()
        .map(|(_, list)| list)
        .collect()
}

/// The dialogs open now, as (identifier, title), for the Window menu.
fn open_dialogs(
    view: &View,
    options: &Options,
    show_rig: bool,
    panels: &[PanelSlot],
    show_about: bool,
) -> Vec<(String, String)> {
    let lang = i18n::current();
    let mut all = Vec::new();
    if view.editing.is_some() {
        all.push(("edit".to_string(), tr("edit.title").to_string()));
    }
    if options.open {
        all.push(("options".into(), tr("options.title").into()));
    }
    if show_rig {
        all.push(("rig".into(), tr("window.rig").into()));
    }
    for slot in panels.iter().filter(|s| s.open) {
        all.push((slot.panel.id().to_string(), slot.panel.title(lang)));
    }
    if show_about {
        all.push(("about".into(), tr("menu.help.about").into()));
    }
    all
}

/// Settings section of a panel.
fn panel_section(id: &str) -> String {
    format!("panel.{id}")
}

/// Import and Export submenus, one item per format an extension provides.
fn file_formats(ui: &mut egui::Ui, session: &mut Session) {
    let items: Vec<(usize, String, Vec<String>, bool, bool)> = session
        .formats()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let exts = f.extensions().iter().map(|e| e.to_string()).collect();
            (
                i,
                f.name().to_string(),
                exts,
                f.can_import(),
                f.can_export(),
            )
        })
        .collect();
    ui.add_enabled_ui(!items.is_empty(), |ui| {
        ui.menu_button(tr("menu.file.import"), |ui| {
            for (i, name, exts, can_import, _) in &items {
                if *can_import && ui.button(name).clicked() {
                    let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
                    if let Some(p) = crate::dialog::import_file(name, &exts) {
                        session.import(*i, &p);
                    }
                }
            }
        });
        ui.menu_button(tr("menu.file.export"), |ui| {
            for (i, name, exts, _, can_export) in &items {
                if *can_export && ui.button(name).clicked() {
                    let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
                    if let Some(p) = crate::dialog::export_file(name, &exts) {
                        session.export(*i, &p);
                    }
                }
            }
        });
    });
}

/// One line saying which rig is set up and whether it is connected; the
/// simulator is named as such.
fn rig_status(model: &str, port: &str, connected: bool, error: Option<String>) -> String {
    let rig = if port == crate::rig::SIMULATOR {
        tr("rig.simulator").to_string()
    } else {
        i18n::trf("menu.device.rig_on", &[&model, &port])
    };
    let state = match (connected, error) {
        (_, Some(e)) => i18n::trf("menu.device.error", &[&e]),
        (true, None) => tr("menu.device.connected").to_string(),
        (false, None) => tr("rig.not_connected").to_string(),
    };
    i18n::trf("menu.device.status", &[&rig, &state])
}

/// The rig's items at the top of the Device menu: its state, then
/// "Connect rig…" (opens the rig window with the settings to connect with) and
/// "Disconnect".
fn device_menu(ui: &mut egui::Ui, rig: &mut RigControl, show_rig: &mut bool, config: &RigConfig) {
    let connected = rig.connected();
    let status = rig_status(
        zlog_rig_cat::MODELS[config.model].name,
        &config.port,
        connected,
        rig.error().or_else(|| rig.message.clone()),
    );
    ui.weak(status);
    if ui
        .add_enabled(!connected, egui::Button::new(tr("menu.device.connect")))
        .clicked()
    {
        *show_rig = true;
    }
    if ui
        .add_enabled(connected, egui::Button::new(tr("menu.device.disconnect")))
        .clicked()
    {
        rig.disconnect();
    }
    ui.checkbox(show_rig, tr("menu.rig.window"));
}

/// What the headless tests ([`crate::gui_tests`]) need to reach between clicks.
#[cfg(test)]
impl ZLogApp {
    /// Close every dialog and question, as by Cancel, keeping the log open.
    pub fn close_dialogs(&mut self) {
        self.view.editing = None;
        self.view.confirm_delete = None;
        self.view.start_time = None;
        self.options.open = false;
        self.show_rig = false;
        self.show_about = false;
        self.asking_old_other = None;
        self.menu.close_dialogs();
        for slot in &mut self.panels {
            slot.open = false;
        }
    }

    /// Open a contest as OK in the zLog Menu does after its checks.
    pub fn open_setup(&mut self, setup: crate::session::Setup) {
        self.open(setup);
    }

    /// The open log: its contest name and number of QSOs.
    pub fn log_state(&self) -> Option<(String, usize)> {
        self.session
            .as_ref()
            .map(|s| (s.rules().name().to_string(), s.log().qsos.len()))
    }

    /// The status line and the entry row's callsign and number.
    pub fn entry_state(&self) -> Option<(String, String, String)> {
        self.session.as_ref().map(|s| {
            (
                s.status.text.clone(),
                s.entry.callsign.clone(),
                s.entry.nr_rcvd.clone(),
            )
        })
    }

    /// Whether the old-Other question is waiting for an answer.
    pub fn asking_old_other(&self) -> bool {
        self.asking_old_other.is_some()
    }

    /// The rig is connected (to the simulator, in tests).
    pub fn rig_connected(&self) -> bool {
        self.rig.connected()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;

    #[test]
    fn picking_a_callsign_clears_the_number() {
        let mut e = Entry::new(
            zlog_core::Band::B7,
            zlog_core::Mode::Cw,
            zlog_core::Power::P010,
        );
        e.callsign = "JA1ZLO".into();
        e.nr_rcvd = "10H".into();
        e.rst_rcvd = "579".into();
        pick_callsign(&mut e, "JA1YAA".into());
        assert_eq!(e.callsign, "JA1YAA");
        assert_eq!(e.nr_rcvd, "");
        // Other fields stay.
        assert_eq!(e.rst_rcvd, "579");
        // An empty pick clears both.
        pick_callsign(&mut e, String::new());
        assert_eq!((e.callsign.as_str(), e.nr_rcvd.as_str()), ("", ""));
    }

    /// A panel that takes Esc (a CW keyer sending).
    struct Keyer(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl zlog_ui::Panel for Keyer {
        fn id(&self) -> &str {
            "keyer"
        }
        fn title(&self, _language: &str) -> String {
            "keyer".into()
        }
        fn ui(&mut self, _ui: &mut egui::Ui, _view: &zlog_ui::View<'_>) {}
        fn escape(&mut self) -> bool {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        }
    }

    #[test]
    fn escape_aborts_cw_before_the_grid_sees_it() {
        let dir = std::env::temp_dir().join(format!("zlog-gui-esc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let reg = zlog_core::Registry::new().with(zlog_rule_ja::Japan);
        let setup = crate::session::Setup {
            contest: 0,
            contest_file: None,
            mode: zlog_core::ContestMode::Mix,
            station: zlog_core::Station::default(),
            coeff: 1.0,
            path: dir.join("esc.zlox"),
            use_contest_period: true,
            convert_old_other: false,
        };
        let mut session = Session::open(setup, reg).unwrap();
        let mut view = View {
            selected: Some(0),
            ..View::default()
        };
        let aborted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut panels = vec![PanelSlot {
            panel: Box::new(Keyer(aborted.clone())),
            open: false,
        }];
        let options = Options::new(Vec::new());
        let rig = RigControl::default();
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            route_keys(
                ui.ctx(),
                &mut session,
                &mut view,
                &mut panels,
                &options,
                &[],
                &rig,
            );
        });
        out.textures_delta.clear();
        assert!(
            aborted.load(std::sync::atomic::Ordering::SeqCst),
            "CW aborted"
        );
        assert_eq!(view.selected, Some(0), "the grid did not get Esc");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A panel adding one QSO menu item in its group.
    struct Adds(zlog_ui::MenuGroup, &'static str);

    impl zlog_ui::Panel for Adds {
        fn id(&self) -> &str {
            self.1
        }
        fn title(&self, _language: &str) -> String {
            self.1.into()
        }
        fn menu(&self) -> zlog_ui::MenuGroup {
            self.0
        }
        fn ui(&mut self, _ui: &mut egui::Ui, _view: &zlog_ui::View<'_>) {}
        fn qso_menu(&self, _language: &str, qso: &zlog_core::Qso) -> Vec<zlog_ui::QsoMenuItem> {
            vec![zlog_ui::QsoMenuItem::new(self.1, &qso.callsign)]
        }
    }

    #[test]
    fn qso_menu_items_are_grouped_by_menu_group() {
        use zlog_ui::MenuGroup as G;
        let slot = |g, id| PanelSlot {
            panel: Box::new(Adds(g, id)),
            open: false,
        };
        let panels = [
            slot(G::Network, "cluster"),
            slot(G::Check, "check"),
            slot(G::Network, "soumu"),
        ];
        let q = zlog_core::Qso {
            callsign: "JA1ZLO".into(),
            ..Default::default()
        };
        let items = qso_items(&panels, &q);
        let ids: Vec<Vec<(usize, String)>> = items
            .iter()
            .map(|g| g.iter().map(|(i, it)| (*i, it.id.clone())).collect())
            .collect();
        assert_eq!(
            ids,
            [
                vec![(1, "check".to_string())],
                vec![(0, "cluster".to_string()), (2, "soumu".to_string())],
            ]
        );
        assert_eq!(items[0][0].1.label, "JA1ZLO");
    }

    #[test]
    fn entry_fields_map_one_to_one() {
        use zlog_ui::EntryField as E;
        assert_eq!(entry_field(Field::Call), E::Call);
        assert_eq!(entry_field(Field::RstSent), E::RstSent);
        assert_eq!(entry_field(Field::RstRcvd), E::RstRcvd);
        assert_eq!(entry_field(Field::Number), E::Number);
        assert_eq!(entry_field(Field::Memo), E::Memo);
    }

    #[test]
    fn rig_status_names_the_simulator_and_the_state() {
        crate::i18n::set_language("en");
        let sim = rig_status("IC-7300", crate::rig::SIMULATOR, true, None);
        assert_eq!(sim, "Rig: Simulator — connected");
        let off = rig_status("IC-7300", "/dev/cu.SLAB", false, None);
        assert_eq!(off, "Rig: IC-7300 on /dev/cu.SLAB — Not connected");
        let err = rig_status("IC-7300", "COM3", false, Some("busy".into()));
        assert_eq!(err, "Rig: IC-7300 on COM3 — error: busy");
    }
}
