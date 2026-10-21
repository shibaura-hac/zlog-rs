//! The extensions linked into this build.
//!
//! Each extension crate is a Cargo feature of this crate. To add one: add the crate
//! as an optional dependency and a feature in `Cargo.toml`, then one line here.
//! Log extensions ([`registry`]) receive events in the order listed;
//! window extensions ([`panels`]) appear in their group menus and on the toolbar in the order listed.

use zlog_core::Registry;
use zlog_ui::Panel;

pub fn registry() -> Registry {
    let registry = Registry::new();
    #[cfg(feature = "rule-ja")]
    let registry = registry.with(zlog_rule_ja::Japan);
    #[cfg(feature = "rule-dx")]
    let registry = registry.with(zlog_rule_dx::World);
    #[cfg(feature = "base")]
    let registry = registry.with(zlog_rule::Base);
    #[cfg(feature = "qxsl")]
    let registry = registry.with(zlog_qxsl::Qxsl);
    #[cfg(feature = "zserver")]
    let registry = registry.with(zlog_zserver::ZServer);
    registry
}

// One push per feature reads better than a vec! that cannot hold #[cfg] items.
#[allow(unused_mut, clippy::vec_init_then_push)]
pub fn panels() -> Vec<Box<dyn Panel>> {
    let mut panels: Vec<Box<dyn Panel>> = Vec::new();
    #[cfg(feature = "score")]
    panels.push(Box::new(zlog_score::Score));
    #[cfg(feature = "score")]
    panels.push(Box::new(zlog_score::Multipliers::default()));
    #[cfg(feature = "maplot")]
    panels.push(Box::new(zlog_maplot::Maplot::default()));
    #[cfg(feature = "check")]
    panels.extend(zlog_check::panels());
    #[cfg(feature = "cw")]
    panels.push(Box::new(zlog_cw::CwPanel::default()));
    #[cfg(feature = "keyer")]
    panels.push(Box::new(zlog_keyer::KeyerPanel::default()));
    #[cfg(feature = "zserver")]
    panels.push(Box::new(zlog_zserver::ZServerPanel::default()));
    #[cfg(feature = "cluster")]
    panels.push(Box::new(zlog_cluster::Cluster::default()));
    #[cfg(feature = "scope")]
    panels.push(Box::new(zlog_scope::BandScope::default()));
    #[cfg(feature = "soumu")]
    panels.push(Box::new(zlog_soumu::Soumu::default()));
    #[cfg(feature = "rate")]
    panels.push(Box::new(zlog_rate::Rate::default()));
    #[cfg(feature = "summary")]
    panels.push(Box::new(zlog_summary::Summary::default()));
    #[cfg(feature = "es")]
    panels.push(Box::new(zlog_es::EsInfo::default()));
    #[cfg(feature = "ports")]
    panels.push(Box::new(zlog_ports::ComPorts::default()));
    #[cfg(feature = "update")]
    panels.push(Box::new(zlog_update::Updater::new(env!(
        "CARGO_PKG_REPOSITORY"
    ))));
    panels
}

/// Pages extensions add to the Options dialog, in the order listed.
#[allow(unused_mut, clippy::vec_init_then_push)]
pub fn settings_pages() -> Vec<Box<dyn zlog_ui::SettingsPage>> {
    let mut pages: Vec<Box<dyn zlog_ui::SettingsPage>> = Vec::new();
    pages
}
