//! What an extension may add to the zLog window.
//!
//! A panel is a window of its own, listed in the menu of its [`MenuGroup`]. The application
//! draws it every frame with a read-only [`View`] of the open contest; a panel keeps
//! whatever state it likes but never sees the application's internals.
//!
//! Panels that only show the log (a map, a rate graph) should derive what they show
//! from [`View::log`] each time, rather than from events, so they are right from the
//! moment a file is opened and after every edit or deletion.

use zlog_core::{Band, Contest, Log, Mode, Power, Qso, ScoreSheet, Station};

/// The menu of the main window a panel's window is listed in, and the group of its
/// toolbar buttons and right-click items. The application names the groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum MenuGroup {
    /// Checking the station being worked: partial and super check, check call.
    Check,
    /// Running the station: keyer, CW decoder, band scope, rate.
    Operation,
    /// Hardware: the rig (drawn by the application) and serial ports.
    Device,
    /// Talking to the outside: cluster, Z-Server, licence look-up, news.
    Network,
    /// Results: score, multipliers, summary sheet, map.
    Report,
    /// Everything else (serial ports, say).
    #[default]
    Tools,
    /// The Help menu, below the application's own items (manual, updates). Not a
    /// menu of its own and without toolbar buttons, so it is not in [`MenuGroup::ALL`].
    Help,
}

impl MenuGroup {
    /// Every group with a menu of its own, in the order of the menu bar.
    pub const ALL: [MenuGroup; 6] = [
        MenuGroup::Check,
        MenuGroup::Report,
        MenuGroup::Operation,
        MenuGroup::Device,
        MenuGroup::Network,
        MenuGroup::Tools,
    ];
}

/// What a toolbar button of a panel does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolbarKind {
    /// Open or close the panel's window (shown pressed while it is open).
    Window,
    /// Run [`Panel::on_toolbar_action`] with this identifier.
    Action(String),
}

/// A button a panel puts on the toolbar of the main window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarButton {
    /// Short text on the button, in the language asked for.
    pub label: String,
    /// Longer text shown when the pointer rests on it.
    pub tooltip: String,
    pub kind: ToolbarKind,
}

impl ToolbarButton {
    /// A button opening and closing the panel's window.
    pub fn window(label: impl Into<String>, tooltip: impl Into<String>) -> Self {
        ToolbarButton {
            label: label.into(),
            tooltip: tooltip.into(),
            kind: ToolbarKind::Window,
        }
    }

    /// A button running [`Panel::on_toolbar_action`] with `id`.
    pub fn action(
        id: impl Into<String>,
        label: impl Into<String>,
        tooltip: impl Into<String>,
    ) -> Self {
        ToolbarButton {
            label: label.into(),
            tooltip: tooltip.into(),
            kind: ToolbarKind::Action(id.into()),
        }
    }
}

/// An item a panel adds to the right-click menu of a QSO in the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QsoMenuItem {
    /// Passed back to [`Panel::on_qso_menu`].
    pub id: String,
    /// Text of the item, in the language asked for.
    pub label: String,
    pub enabled: bool,
}

impl QsoMenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        QsoMenuItem {
            id: id.into(),
            label: label.into(),
            enabled: true,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// A station reported on the air (a cluster or skimmer spot, a Z-Server notice).
///
/// Panels hand spots out with [`Panel::take_spots`]; the application keeps the
/// recent ones and shows them to every panel in [`View::spots`].
#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    pub callsign: String,
    /// Frequency in hertz.
    pub hz: u64,
    pub mode: Option<Mode>,
    /// Who reported it.
    pub spotter: String,
    pub comment: String,
    /// When it was received, in seconds since the Unix epoch.
    pub time: i64,
}

/// The open contest as a panel sees it.
pub struct View<'a> {
    pub log: &'a Log,
    pub contest: &'a dyn Contest,
    /// What the contest computed at the last change.
    pub sheet: &'a ScoreSheet,
    /// Callsign being typed in the entry row, upper case.
    pub callsign: &'a str,
    /// The operator's own station (callsign, location, power).
    pub station: &'a Station,
    /// Received number being typed in the entry row.
    pub number: &'a str,
    /// RST to send, and the power, of the entry row.
    pub rst_sent: &'a str,
    pub power: Power,
    /// Band and mode of the entry row.
    pub band: Band,
    pub mode: Mode,
    /// Language code of the user interface (`en`, `ja`).
    pub language: &'a str,
    /// Spots received recently, oldest first.
    pub spots: &'a [Spot],
    /// Frequency of the rig in hertz, when one is connected.
    pub hz: Option<u64>,
    /// Whether the entry row is in CQ (run) rather than S&P mode (`CurrentQSO.CQ`,
    /// toggled with Shift+O, `actionToggleCqSp`); stored in `Qso::cq` when logged.
    pub cq: bool,
    /// Operator of the entry row (`CurrentQSO.Operator`); written into `Qso::operator`.
    pub operator: &'a str,
    /// Length of the contest period in hours from [`Log::start_time`], or 0 when
    /// QSOs are not checked against a period (zLog `UseContestPeriod` off, or a
    /// contest without one); see [`Log::out_of_period`].
    pub period_hours: u32,
    /// Name of this PC on the network (zLog `Settings._pcname`, zlog.ini `[Z-Link]
    /// PCName`), stored in logged QSOs and sent to Z-Server.
    pub pc_name: &'a str,
    /// Entry field holding the keyboard focus, if any (zLog runs the partial check
    /// by number only from the number field, main.pas `NumberEditChange`).
    pub focus: Option<EntryField>,
}

/// A field of the entry row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryField {
    Call,
    RstSent,
    RstRcvd,
    Number,
    Memo,
}

pub trait Panel {
    /// Stable identifier, used to remember whether the panel is open.
    fn id(&self) -> &str;

    /// Title of the window and of its item in the menu of its group.
    fn title(&self, language: &str) -> String;

    /// Default size of the window in points.
    fn default_size(&self) -> [f32; 2] {
        [400.0, 300.0]
    }

    /// The menu its window is listed in, and the group of its toolbar buttons.
    fn menu(&self) -> MenuGroup {
        MenuGroup::Tools
    }

    /// Buttons the panel puts on the toolbar, in `language` (none by default); the
    /// operator may hide them from the Settings menu.
    fn toolbar(&self, _language: &str) -> Vec<ToolbarButton> {
        Vec::new()
    }

    /// A toolbar button of kind [`ToolbarKind::Action`] was pressed.
    fn on_toolbar_action(&mut self, _id: &str) {}

    /// Items the panel adds to the right-click menu of `qso` in the log, in
    /// `language`; shown after the application's own items.
    fn qso_menu(&self, _language: &str, _qso: &Qso) -> Vec<QsoMenuItem> {
        Vec::new()
    }

    /// An item from [`Panel::qso_menu`] was chosen for `qso`. The panel may answer
    /// through [`Panel::take_actions`] or [`Panel::take_spots`], or open its window
    /// by returning true.
    fn on_qso_menu(&mut self, _id: &str, _qso: &Qso) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, view: &View<'_>);

    /// Requests to the entry row made during the last [`Panel::ui`] (a decoded
    /// callsign clicked, say); the application takes them after each frame.
    fn take_actions(&mut self) -> Vec<EntryAction> {
        Vec::new()
    }

    /// Spots received since the last call, for every panel to see.
    fn take_spots(&mut self) -> Vec<Spot> {
        Vec::new()
    }

    /// Called by the application every frame for every panel, whether its window is
    /// open or not, before [`Panel::ui`]; for work that must go on while closed (a
    /// band scope taking in spots). Must not block.
    fn tick(&mut self, _view: &View<'_>) {}

    /// Function key `n` (1 to 12) was pressed in the main window, with Shift when
    /// `shift` (zLog plays the other message bank then); return true when the panel
    /// used it. Panels are asked in order until one takes the key; called even while
    /// the panel's window is closed, but not while a dialog or another text field
    /// has the keyboard.
    fn function_key(&mut self, _n: u8, _shift: bool, _view: &View<'_>) -> bool {
        false
    }

    /// Esc was pressed in the entry row (zLog `actionCQAbort`: stop sending CW and
    /// the CQ repeat); return true when the panel used it. Dispatched as
    /// [`Panel::function_key`] is; the entry row still handles Esc itself.
    fn escape(&mut self) -> bool {
        false
    }

    /// The operator typed into the callsign or number field of the entry row (zLog
    /// `OtherKeyPressed`, which stops the CQ repeat). Called for every panel.
    fn entry_typed(&mut self) {}

    /// Restore what the panel saved (a host name, a file path) at start-up.
    fn load(&mut self, _section: &Section) {}

    /// Save what the panel wants back at the next start-up.
    fn save(&self, _section: &mut Section) {}
}

/// The settings of one extension: text values by key, kept and saved by the
/// application under the extension's own name. An extension sees only its section.
pub type Section = std::collections::BTreeMap<String, String>;

/// A page of the Options dialog, contributed by an extension.
pub trait SettingsPage {
    /// Name of the section this page edits; also its identifier.
    fn id(&self) -> &str;

    fn title(&self, language: &str) -> String;

    fn ui(&mut self, ui: &mut egui::Ui, section: &mut Section, language: &str);
}

/// A request from a panel to the entry row; the application carries it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryAction {
    /// Put this callsign into the callsign field.
    Callsign(String),
    /// Put this text into the received-number field.
    Number(String),
    /// Tune the rig to this frequency in hertz (and mode), or, without a rig, move the
    /// entry row to its band.
    Qsy(u64, Option<Mode>),
    /// Put the entry row into CQ (`true`) or S&P mode (zLog `SetCQ`).
    Cq(bool),
}
