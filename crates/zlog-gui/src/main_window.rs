//! The main window (`TMainForm`): menu, toolbar, log grid, entry row and status bar.
//!
//! Keys follow the zLog manual:
//! Space moves from the callsign to the number (checking dupes and filling the
//! number from earlier QSOs) and back from other fields, Enter logs,
//! Shift+B / Shift+M / Shift+P change band, mode and power, Shift+O toggles CQ and S&P,
//! Alt+C / Alt+N / Alt+M / Alt+R jump to callsign, number, memo and RST,
//! Shift+Up / Shift+Down change the S of an RST, Ctrl+K clears callsign and number,
//! Up moves into the log, where Enter edits and Delete deletes.

use crate::i18n::{tr, trf};
use egui::{Color32, Event, Id, Key, Modifiers, RichText, Sense};
use egui_extras::{Column, TableBuilder};
use zlog_core::{Band, Mode, Power};

use crate::entry::Entry;
use crate::session::{self, NotLogged, Session};

/// Columns shared by the log grid and the entry row, so that fields line up as in zLog.
/// Each column is a text key and a width in characters.
const COLUMNS: &[(&str, f32)] = &[
    ("column.status", 2.0),
    ("column.date", 5.0),
    ("column.time", 5.0),
    ("column.call", 11.0),
    ("column.rst", 4.0),
    ("column.sent", 8.0),
    ("column.rst", 4.0),
    ("column.rcvd", 8.0),
    ("column.multi", 7.0),
    ("column.band", 6.0),
    ("column.mode", 5.0),
    ("column.power", 2.0),
    ("column.op", 7.0),
    ("column.memo", 14.0),
    ("column.points", 3.0),
];

const ROW_HEIGHT: f32 = 18.0;

/// Entry fields that can hold the keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Call,
    RstSent,
    RstRcvd,
    Number,
    Memo,
}

impl Field {
    fn id(self) -> Id {
        Id::unique(("entry", self as u8))
    }
}

/// State of the window that is not part of the log.
#[derive(Default)]
pub struct View {
    /// Selected row of the log grid; `Some` while the grid has the keyboard.
    pub selected: Option<usize>,
    /// The QSO in the edit dialog, by ID (rows move while Z-Link applies changes).
    pub editing: Option<(zlog_core::QsoId, zlog_core::Qso)>,
    /// The edit dialog inserts its QSO before the one it was opened on (zLog's
    /// Insert QSO, `_ActInsert`) instead of replacing it.
    pub inserting: bool,
    /// The QSO to delete once confirmed, by ID.
    pub confirm_delete: Option<zlog_core::QsoId>,
    pub focus: Option<Field>,
    /// The entry field that last had the keyboard, given it back after a click on
    /// the band, mode or power box (zLog `LastFocus.SetFocus`).
    pub last_field: Option<Field>,
    /// Select the whole text of this field when it is next drawn (after a dupe).
    pub select_all: Option<Field>,
    /// Column widths in points, shared by the grid and the entry row; empty until first drawn.
    pub widths: Vec<f32>,
    /// Column being resized: index, its width and the pointer x when the drag began.
    pub resizing: Option<(usize, f32, f32)>,
    /// The start-time dialog, while open.
    pub start_time: Option<StartTimeForm>,
    /// What was chosen in the grid's right-click menu this frame, for the QSO with
    /// this ID.
    pub grid_action: Option<(zlog_core::QsoId, GridAction)>,
}

/// Fields of the start-time dialog (`TStartTimeDialog`).
#[derive(Debug, Clone, PartialEq)]
pub struct StartTimeForm {
    pub date: String,
    pub time: String,
    /// Judge every QSO against the new period (zLog asks this after OK of Correct
    /// start time, default No).
    pub judge: bool,
    /// Opened for a new log (no judge question; Cancel keeps the suggested time).
    pub new_log: bool,
}

impl StartTimeForm {
    pub fn new(start: zlog_core::DelphiTime, new_log: bool) -> Self {
        let (y, mo, d, h, mi, _) = start.to_ymd_hms();
        StartTimeForm {
            date: format!("{y:04}-{mo:02}-{d:02}"),
            time: format!("{h:02}:{mi:02}"),
            judge: false,
            new_log,
        }
    }

    /// The time typed, if it is a valid date and time.
    pub fn parse(&self) -> Option<zlog_core::DelphiTime> {
        crate::windows::parse_time(&format!("{} {}", self.date.trim(), self.time.trim()))
    }
}

/// Items of the log grid's right-click menu (zLog `GridMenu` in main.dfm).
#[derive(Debug, Clone, PartialEq)]
pub enum GridAction {
    /// `EditQSO`.
    Edit,
    /// `InsertQSO1`.
    Insert,
    /// `DeleteQSO1`.
    Delete,
    /// `menuChangeBand`.
    Band(Band),
    /// `menuChangeMode`.
    Mode(Mode),
    /// `menuChangePower`.
    Power(Power),
    /// The invalid flag (zLog sets it in Edit Status / the edit dialog).
    ToggleInvalid,
    /// An item an extension added: the panel's index and the item's identifier.
    Extension(usize, String),
}

/// Items extensions add to the right-click menu of a QSO: one list per menu group,
/// each item with the index of its panel.
pub type ExtensionItems = Vec<Vec<(usize, zlog_ui::QsoMenuItem)>>;

/// Carry out a grid menu item on the QSO with this ID. Edit, Insert and Delete open
/// the same dialogs as Enter, Insert and Delete on the grid.
pub fn apply_grid_action(s: &mut Session, v: &mut View, id: zlog_core::QsoId, action: GridAction) {
    let Some(index) = s.find(id) else {
        s.status = session::Status::warning(tr("status.qso_gone"));
        return;
    };
    let q = s.log().qsos[index].clone();
    match action {
        GridAction::Edit => {
            v.editing = Some((id, q));
            v.inserting = false;
        }
        GridAction::Insert => {
            v.editing = Some((id, Session::insertion_from(&q)));
            v.inserting = true;
        }
        GridAction::Delete => v.confirm_delete = Some(id),
        GridAction::Band(b) => {
            s.change_band(&[id], b);
        }
        GridAction::Mode(m) => {
            s.change_mode(&[id], m);
        }
        GridAction::Power(p) => {
            s.change_power(&[id], p);
        }
        GridAction::ToggleInvalid => {
            s.toggle_invalid(&[id]);
        }
        // Carried out by the application, which has the panels.
        GridAction::Extension(..) => {}
    }
}

/// The right-click menu of a grid row, also the Edit menu (with the extensions'
/// items left out); returns the item chosen.
pub fn grid_menu(ui: &mut egui::Ui, s: &Session, extra: &ExtensionItems) -> Option<GridAction> {
    let mut chosen = None;
    let key = |k: Key| {
        ui.ctx()
            .format_shortcut(&egui::KeyboardShortcut::new(Modifiers::NONE, k))
    };
    let keys = [key(Key::Enter), key(Key::Delete), key(Key::Insert)];
    for (text, k, action) in [
        (tr("grid.edit"), &keys[0], GridAction::Edit),
        (tr("grid.delete"), &keys[1], GridAction::Delete),
        (tr("grid.insert"), &keys[2], GridAction::Insert),
    ] {
        if ui.add(egui::Button::new(text).shortcut_text(k)).clicked() {
            chosen = Some(action);
            ui.close();
        }
    }
    let mut item = |ui: &mut egui::Ui, text: &str, action: GridAction| {
        if ui.button(text).clicked() {
            chosen = Some(action);
            ui.close();
        }
    };
    ui.separator();
    let (lo, hi) = s.rules().band_range();
    let warc = s.rules().uses_warc();
    ui.menu_button(tr("grid.band"), |ui| {
        for b in Band::range(lo, hi).filter(|b| warc || !b.is_warc()) {
            item(ui, b.label(), GridAction::Band(b));
        }
    });
    ui.menu_button(tr("grid.mode"), |ui| {
        for &m in Mode::ALL {
            item(ui, m.label(), GridAction::Mode(m));
        }
    });
    ui.menu_button(tr("grid.power"), |ui| {
        for p in [Power::P001, Power::P002, Power::P005, Power::P010] {
            let code = p.code().map(String::from).unwrap_or_default();
            item(ui, &code, GridAction::Power(p));
        }
    });
    ui.separator();
    item(ui, tr("grid.invalid"), GridAction::ToggleInvalid);
    drop(item);
    for group in extra {
        ui.separator();
        for (panel, it) in group {
            if ui
                .add_enabled(it.enabled, egui::Button::new(&it.label))
                .clicked()
            {
                chosen = Some(GridAction::Extension(*panel, it.id.clone()));
                ui.close();
            }
        }
    }
    chosen
}

fn char_width(ui: &egui::Ui) -> f32 {
    ui.fonts_mut(|f| f.glyph_width(&egui::TextStyle::Monospace.resolve(ui.style()), '0'))
}

/// Current column widths, starting from [`COLUMNS`] in characters.
fn widths(ui: &egui::Ui, v: &mut View) -> Vec<f32> {
    if v.widths.is_empty() {
        let cw = char_width(ui);
        v.widths = COLUMNS.iter().map(|(_, w)| w * cw + 6.0).collect();
    }
    v.widths.clone()
}

/// Consume a shortcut and the text it would otherwise type into a field.
fn shortcut(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> bool {
    ctx.input_mut(|i| {
        let hit = i.consume_key(modifiers, key);
        if hit {
            i.events.retain(|e| !matches!(e, Event::Text(_)));
        }
        hit
    })
}

/// The entry field that has the keyboard, if any.
pub fn focused_field(ctx: &egui::Context) -> Option<Field> {
    [
        Field::Call,
        Field::RstSent,
        Field::RstRcvd,
        Field::Number,
        Field::Memo,
    ]
    .into_iter()
    .find(|f| ctx.memory(|m| m.has_focus(f.id())))
}

/// Whether Shift+B / M / P / O act in a field: not in the memo, where they type
/// capitals (zLog turns `EnableShiftKeyAction` off while the memo has focus).
pub fn shift_keys_act(field: Option<Field>) -> bool {
    field != Some(Field::Memo)
}

/// Handle the entry-row keys before the fields are drawn, so the fields never see them.
/// `hz` is the rig frequency, recorded in a QSO logged with Enter.
pub fn handle_keys(ctx: &egui::Context, s: &mut Session, v: &mut View, hz: Option<u64>) {
    if v.editing.is_some() || v.confirm_delete.is_some() || v.start_time.is_some() {
        return;
    }
    if let Some(f) = focused_field(ctx) {
        v.last_field = Some(f);
    }
    if let Some(sel) = v.selected {
        let n = s.log().qsos.len();
        if shortcut(ctx, Modifiers::NONE, Key::ArrowUp) {
            v.selected = Some(sel.min(n).saturating_sub(1));
        } else if shortcut(ctx, Modifiers::NONE, Key::ArrowDown) {
            v.selected = if sel + 1 < n { Some(sel + 1) } else { None };
            if v.selected.is_none() {
                v.focus = Some(Field::Call);
            }
        } else if shortcut(ctx, Modifiers::NONE, Key::Enter) {
            // Rows deleted elsewhere (Z-Link) may leave the selection past the end.
            if let Some(q) = s.log().qsos.get(sel) {
                v.editing = Some((zlog_core::QsoId::of(q), q.clone()));
                v.inserting = false;
            }
        } else if shortcut(ctx, Modifiers::NONE, Key::Insert) {
            // Insert QSO on the selected row (`InsertQSO1Click`).
            if let Some(q) = s.log().qsos.get(sel) {
                let new = Session::insertion_from(q);
                v.editing = Some((zlog_core::QsoId::of(q), new));
                v.inserting = true;
            }
        } else if shortcut(ctx, Modifiers::NONE, Key::Delete) {
            if let Some(q) = s.log().qsos.get(sel) {
                v.confirm_delete = Some(zlog_core::QsoId::of(q));
            }
        } else if shortcut(ctx, Modifiers::NONE, Key::Escape) {
            v.selected = None;
            v.focus = Some(Field::Call);
        }
        return;
    }

    let field = focused_field(ctx);
    let shift = shift_keys_act(field);
    if shift && shortcut(ctx, Modifiers::SHIFT, Key::B) {
        s.cycle_band(false);
    }
    if shift && shortcut(ctx, Modifiers::SHIFT, Key::M) {
        s.cycle_mode(false);
    }
    if shift && shortcut(ctx, Modifiers::SHIFT, Key::P) {
        s.entry.cycle_power();
    }
    // Shift+O toggles CQ and S&P mode (`actionToggleCqSp`).
    if shift && shortcut(ctx, Modifiers::SHIFT, Key::O) {
        s.entry.cq = !s.entry.cq;
    }
    // Shift+Up / Shift+Down change the S figure of the RST being edited.
    for (key, up) in [(Key::ArrowUp, true), (Key::ArrowDown, false)] {
        match field {
            Some(Field::RstSent) if shortcut(ctx, Modifiers::SHIFT, key) => {
                Entry::bump_s(&mut s.entry.rst_sent, up)
            }
            Some(Field::RstRcvd) if shortcut(ctx, Modifiers::SHIFT, key) => {
                Entry::bump_s(&mut s.entry.rst_rcvd, up)
            }
            _ => {}
        }
    }
    for (key, target) in [
        (Key::C, Field::Call),
        (Key::N, Field::Number),
        (Key::M, Field::Memo),
        (Key::R, Field::RstRcvd),
    ] {
        if shortcut(ctx, Modifiers::ALT, key) {
            v.focus = Some(target);
        }
    }
    // Ctrl+K clears callsign, number and memo (`actionClearCallAndRpt`).
    if shortcut(ctx, Modifiers::COMMAND, Key::K) {
        s.entry.clear_call_and_number();
        s.status = Default::default();
        v.focus = Some(Field::Call);
    }
    let space = |ctx: &egui::Context| shortcut(ctx, Modifiers::NONE, Key::Space);
    match field {
        // As zLog with dupes allowed: a dupe is reported, and the cursor still moves on.
        Some(Field::Call) if space(ctx) => {
            let dupe = s.check_call();
            s.fill_exchange();
            if !dupe {
                s.check_number();
            }
            v.focus = Some(Field::Number);
        }
        Some(Field::Memo) | None => {}
        Some(_) if space(ctx) => v.focus = Some(Field::Call),
        Some(_) => {}
    }
    if field.is_some() && shortcut(ctx, Modifiers::NONE, Key::Enter) {
        match s.log_entry_on(session::now_for(s.log()), hz) {
            Ok(()) | Err(NotLogged::NoCallsign) => v.focus = Some(Field::Call),
            Err(NotLogged::InvalidNumber) => {
                v.focus = Some(Field::Number);
                v.select_all = Some(Field::Number);
            }
            Err(NotLogged::Refused) => v.focus = field,
        }
    }
    if field.is_some() && !s.log().qsos.is_empty() && shortcut(ctx, Modifiers::NONE, Key::ArrowUp) {
        v.selected = Some(s.log().qsos.len() - 1);
        ctx.memory_mut(|m| m.surrender_focus(field.unwrap().id()));
    }
}

/// The log grid with the last QSO at the bottom.
/// `extra` gives the extensions' items of a QSO's right-click menu.
pub fn log_grid(
    ui: &mut egui::Ui,
    s: &Session,
    v: &mut View,
    extra: &dyn Fn(&zlog_core::Qso) -> ExtensionItems,
) {
    let widths = widths(ui, v);
    let mut drag: Option<(usize, egui::Response)> = None;
    let mut table = TableBuilder::new(ui)
        .striped(true)
        .sense(Sense::click())
        .stick_to_bottom(true)
        .min_scrolled_height(0.0)
        .max_scroll_height(f32::INFINITY);
    for w in &widths {
        table = table.column(Column::exact(*w));
    }
    if let Some(sel) = v.selected {
        table = table.scroll_to_row(sel, None);
    }
    table
        .header(ROW_HEIGHT, |mut header| {
            for (i, (title, _)) in COLUMNS.iter().enumerate() {
                header.col(|ui| {
                    ui.label(RichText::new(tr(title)).monospace().strong());
                    // Drag the right edge of a heading to resize the column and its entry field.
                    let r = ui.max_rect();
                    let edge =
                        egui::Rect::from_x_y_ranges(r.right() - 3.0..=r.right() + 3.0, r.y_range());
                    let handle = ui.interact(edge, Id::unique(("column_edge", i)), Sense::drag());
                    if handle.hovered() || handle.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                    }
                    if handle.dragged() || handle.drag_stopped() {
                        drag = Some((i, handle));
                    }
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, s.log().qsos.len(), |mut row| {
                let i = row.index();
                let q = &s.log().qsos[i];
                row.set_selected(v.selected == Some(i));
                let (_, mo, d, h, mi, _) = q.time.to_ymd_hms();
                let power = q.power.code().map(String::from);
                let cells = [
                    if q.invalid {
                        "X"
                    } else if q.dupe {
                        "D"
                    } else {
                        ""
                    }
                    .to_string(),
                    format!("{mo:02}/{d:02}"),
                    format!("{h:02}:{mi:02}"),
                    q.callsign.clone(),
                    q.rst_sent.to_string(),
                    q.nr_sent.clone(),
                    q.rst_rcvd.to_string(),
                    q.nr_rcvd.clone(),
                    if q.new_multi1 {
                        q.multi1.clone()
                    } else {
                        String::new()
                    },
                    q.band.label().to_string(),
                    q.mode.label().to_string(),
                    power.unwrap_or_default(),
                    q.operator.clone(),
                    q.memo.clone(),
                    q.points.to_string(),
                ];
                let color = if q.dupe || q.invalid {
                    Some(Color32::from_rgb(200, 60, 60))
                } else {
                    None
                };
                for text in cells {
                    row.col(|ui| {
                        let mut t = RichText::new(text).monospace();
                        if let Some(c) = color {
                            t = t.color(c);
                        }
                        ui.add(egui::Label::new(t).selectable(false));
                    });
                }
                let response = row.response();
                if response.clicked() {
                    v.selected = Some(i);
                }
                if response.double_clicked() {
                    v.editing = Some((zlog_core::QsoId::of(q), q.clone()));
                    v.inserting = false;
                }
                // Right click selects the row and opens zLog's grid menu on it.
                if response.secondary_clicked() {
                    v.selected = Some(i);
                }
                response.context_menu(|ui| {
                    if let Some(a) = grid_menu(ui, s, &extra(q)) {
                        v.grid_action = Some((zlog_core::QsoId::of(q), a));
                    }
                });
            });
        });
    // Width follows the pointer from where the drag began, however often egui redraws.
    match drag {
        Some((i, handle)) => {
            let x = handle.interact_pointer_pos().map_or(0.0, |p| p.x);
            let (_, start_width, start_x) = *v.resizing.get_or_insert((i, v.widths[i], x));
            v.widths[i] = (start_width + x - start_x).max(12.0);
            if handle.drag_stopped() {
                v.resizing = None;
            }
        }
        None => v.resizing = None,
    }
}

/// The entry row, aligned with the grid columns. Returns whether the operator typed
/// into the callsign or number field (zLog `OtherKeyPressed`).
pub fn entry_row(ui: &mut egui::Ui, s: &mut Session, v: &mut View) -> bool {
    let typed_before = (s.entry.callsign.clone(), s.entry.nr_rcvd.clone());
    let widths = widths(ui, v);
    let width = |col: usize| widths[col];
    let now = session::now_for(s.log());
    let (_, mo, d, h, mi, _) = now.to_ymd_hms();
    let sent = s.sent_number();
    let operator = s.station.operator.clone();

    ui.horizontal(|ui| {
        fixed_label(ui, width(0), "");
        fixed_label(ui, width(1), &format!("{mo:02}/{d:02}"));
        fixed_label(ui, width(2), &format!("{h:02}:{mi:02}"));
        edit(ui, width(3), &mut s.entry.callsign, Field::Call, v);
        edit(ui, width(4), &mut s.entry.rst_sent, Field::RstSent, v);
        fixed_label(ui, width(5), &sent);
        edit(ui, width(6), &mut s.entry.rst_rcvd, Field::RstRcvd, v);
        let before = s.entry.nr_rcvd.clone();
        edit(ui, width(7), &mut s.entry.nr_rcvd, Field::Number, v);
        if s.entry.nr_rcvd != before {
            s.check_number();
        }
        fixed_label(ui, width(8), "");
        let mut clicked = false;
        if button(ui, width(9), s.entry.band.label()) {
            s.cycle_band(false);
            clicked = true;
        }
        if button(ui, width(10), s.entry.mode.label()) {
            s.cycle_mode(false);
            clicked = true;
        }
        let code = s.entry.power.code().map(String::from).unwrap_or_default();
        if button(ui, width(11), &code) {
            s.entry.cycle_power();
            clicked = true;
        }
        if clicked {
            v.focus = Some(refocus_after_click(v.last_field));
        }
        fixed_label(ui, width(12), &operator);
        edit(ui, width(13), &mut s.entry.memo, Field::Memo, v);
    });
    s.entry.callsign = s.entry.callsign.to_uppercase();
    s.entry.nr_rcvd = s.entry.nr_rcvd.to_uppercase();
    (s.entry.callsign.clone(), s.entry.nr_rcvd.clone()) != typed_before
}

fn edit(ui: &mut egui::Ui, width: f32, text: &mut String, field: Field, v: &mut View) {
    if v.select_all == Some(field) {
        let mut state =
            egui::text_edit::TextEditState::load(ui.ctx(), field.id()).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::select_all_str(text)));
        state.store(ui.ctx(), field.id());
        v.select_all = None;
    }
    let response = ui.add_sized(
        [width, ROW_HEIGHT + 2.0],
        egui::TextEdit::singleline(text)
            .id(field.id())
            .font(egui::TextStyle::Monospace),
    );
    if v.focus == Some(field) {
        response.request_focus();
        v.focus = None;
    }
    if response.gained_focus() {
        v.selected = None;
    }
}

/// The field the keyboard returns to after a click on the band, mode or power box
/// (`LastFocus.SetFocus`; the callsign before any field had it).
pub fn refocus_after_click(last: Option<Field>) -> Field {
    last.unwrap_or(Field::Call)
}

fn fixed_label(ui: &mut egui::Ui, width: f32, text: &str) {
    ui.add_sized(
        [width, ROW_HEIGHT + 2.0],
        egui::Label::new(RichText::new(text).monospace()),
    );
}

fn button(ui: &mut egui::Ui, width: f32, text: &str) -> bool {
    ui.add_sized(
        [width, ROW_HEIGHT + 2.0],
        egui::Button::new(RichText::new(text).monospace()),
    )
    .clicked()
}

pub fn status_bar(ui: &mut egui::Ui, s: &Session) {
    ui.horizontal(|ui| {
        let text = RichText::new(&s.status.text);
        ui.label(if s.status.warning {
            text.color(Color32::RED)
        } else {
            text
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(trf("status.qso_count", &[&s.log().qsos.len()]));
            ui.separator();
            // CQ in blue, S&P in fuchsia, as zLog's `panelCQMode`.
            ui.label(if s.entry.cq {
                RichText::new(tr("status.cq")).color(Color32::BLUE)
            } else {
                RichText::new(tr("status.sp")).color(Color32::from_rgb(255, 0, 255))
            });
            ui.separator();
            ui.label(s.rules().name());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_keys_type_capitals_in_the_memo() {
        assert!(!shift_keys_act(Some(Field::Memo)));
        for f in [
            None,
            Some(Field::Call),
            Some(Field::Number),
            Some(Field::RstSent),
        ] {
            assert!(shift_keys_act(f), "{f:?}");
        }
    }

    #[test]
    fn click_on_a_box_returns_to_the_last_field() {
        assert_eq!(refocus_after_click(Some(Field::Number)), Field::Number);
        assert_eq!(refocus_after_click(Some(Field::Memo)), Field::Memo);
        assert_eq!(refocus_after_click(None), Field::Call);
    }
}
