//! Japanese contests for zLog:
//!
//! - [`jarl`]: JARL's four majors (ALL JA, 6m AND DOWN, Field Day, ACAG).
//! - [`user`]: user-defined contests from a zLog CFG file and its DAT list,
//!   chosen by the operator at run time ([`from_cfg`]).
//! - [`ja0`]: ALL JA0, in its two sections (JA0 stations and others).
//! - [`nyp`]: NEW YEAR PARTY.
//! - [`asahikawa`], [`high_school`], [`tokai_marathon`], [`yl`]: local contests,
//!   each a CFG and DAT file with its own rules.
//!
//! The zLog_cfgdat collection at github.com/jr8ppg/zLog_cfgdat
//! carries no licence, so none of its samples is embedded.

pub mod asahikawa;
pub mod cfg;
pub mod common;
pub mod file;
pub mod high_school;
pub mod ja0;
pub mod jarl;
pub mod nyp;
pub mod tokai_marathon;
pub mod user;
pub mod yl;

pub use cfg::Config;
pub use file::CfgFile;
pub use jarl::{JarlContest, JarlRules};
pub use user::{from_cfg, parse_dat, DatEntry, UserContest, UserDefinedDef};

use zlog_core::{Contest, ContestDef, ContestFile, ContestMode, Extension};

zlog_core::texts!();

/// Built-in Japanese contests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JapanContest {
    /// ALL JA0, section for JA0 stations (`TJA0ContestZero`).
    Ja0Zero,
    /// ALL JA0, section for other stations (`TJA0Contest`).
    Ja0Others,
    NewYearParty,
}

impl JapanContest {
    pub const ALL: [JapanContest; 3] = [
        JapanContest::Ja0Zero,
        JapanContest::Ja0Others,
        JapanContest::NewYearParty,
    ];

    pub fn id(self) -> &'static str {
        match self {
            JapanContest::Ja0Zero => "ja0-in",
            JapanContest::Ja0Others => "ja0-out",
            JapanContest::NewYearParty => "nyp",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.id().eq_ignore_ascii_case(id))
    }
}

impl ContestDef for JapanContest {
    fn id(&self) -> &str {
        JapanContest::id(*self)
    }

    fn caption(&self, language: &str) -> String {
        let key = match self {
            JapanContest::Ja0Zero => "caption.ja0in",
            JapanContest::Ja0Others => "caption.ja0out",
            JapanContest::NewYearParty => "caption.nyp",
        };
        texts().get(language, key).into()
    }

    fn group(&self, language: &str) -> String {
        texts().get(language, "group.japan").into()
    }

    /// The zLog Menu's mode buttons (UMenu.pas, cases 4-6).
    fn modes(&self) -> &[ContestMode] {
        match self {
            JapanContest::NewYearParty => &[
                ContestMode::Mix,
                ContestMode::Cw,
                ContestMode::Phone,
                ContestMode::Rtty,
                ContestMode::All,
            ],
            _ => &[ContestMode::Mix, ContestMode::Cw, ContestMode::All],
        }
    }

    fn build(&self, mode: ContestMode) -> Box<dyn Contest> {
        match self {
            JapanContest::Ja0Zero => Box::new(ja0::Ja0Contest::new(true, mode)),
            JapanContest::Ja0Others => Box::new(ja0::Ja0Contest::new(false, mode)),
            JapanContest::NewYearParty => Box::new(nyp::NypContest::new(mode)),
        }
    }
}

/// The extension that adds the built-in Japanese contests (JARL's four and the others). User-defined contests are
/// built from the operator's CFG file ([`CfgFile`]).
pub struct Japan;

impl Extension for Japan {
    fn name(&self) -> &str {
        "Japanese contests"
    }

    fn contests(&self) -> Vec<Box<dyn ContestDef>> {
        let mut all: Vec<Box<dyn ContestDef>> = JarlContest::ALL
            .into_iter()
            .map(|c| Box::new(c) as Box<dyn ContestDef>)
            .collect();
        all.extend(
            JapanContest::ALL
                .into_iter()
                .map(|c| Box::new(c) as Box<dyn ContestDef>),
        );
        all.extend([
            asahikawa::boxed(),
            high_school::boxed(),
            tokai_marathon::boxed(),
            yl::boxed(),
        ]);
        all
    }

    fn contest_files(&self) -> Vec<Box<dyn ContestFile>> {
        vec![Box::new(CfgFile)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_complete() {
        assert!(texts().missing().is_empty());
    }

    #[test]
    fn extension_lists_all_contests() {
        assert_eq!(Japan.contests().len(), 11);
    }
}
