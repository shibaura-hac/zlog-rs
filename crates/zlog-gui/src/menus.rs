//! How the extensions' windows, toolbar buttons and QSO menu items are grouped.
//!
//! Each panel names its [`MenuGroup`]; the main window shows one menu per group
//! that has a panel (Check, Report, Operation, Device, Network, Tools), between Edit and
//! Rig. Separate menus rather than one Tools menu with submenus: during a contest a
//! window is one click away, and the groups stay short.

use zlog_ui::MenuGroup;

/// The language key of a group's menu title.
pub fn group_key(group: MenuGroup) -> &'static str {
    match group {
        MenuGroup::Check => "menu.group.check",
        MenuGroup::Operation => "menu.group.operation",
        MenuGroup::Device => "menu.group.device",
        MenuGroup::Network => "menu.group.network",
        MenuGroup::Report => "menu.group.report",
        MenuGroup::Tools => "menu.group.tools",
        MenuGroup::Help => "menu.help",
    }
}

/// Where `group` stands in the menu bar ([`MenuGroup::ALL`]; Help last).
pub fn position(group: MenuGroup) -> usize {
    MenuGroup::ALL
        .iter()
        .position(|g| *g == group)
        .unwrap_or(MenuGroup::ALL.len())
}

/// `items` (group, panel index, item) as one list per group, groups in the order
/// of [`MenuGroup::ALL`], items in the order given (the registry's); groups
/// without items are left out.
pub fn grouped<T>(
    items: impl IntoIterator<Item = (MenuGroup, usize, T)>,
) -> Vec<(MenuGroup, Vec<(usize, T)>)> {
    let mut groups: Vec<(MenuGroup, Vec<(usize, T)>)> =
        MenuGroup::ALL.iter().map(|g| (*g, Vec::new())).collect();
    for (g, i, t) in items {
        if let Some((_, list)) = groups.iter_mut().find(|(h, _)| *h == g) {
            list.push((i, t));
        }
    }
    groups.retain(|(_, list)| !list.is_empty());
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_grouped_in_menu_order_keeping_registry_order() {
        let items = [
            (MenuGroup::Report, 0, "score"),
            (MenuGroup::Check, 1, "partial"),
            (MenuGroup::Report, 2, "multi"),
            (MenuGroup::Tools, 3, "ports"),
            (MenuGroup::Check, 4, "super"),
            (MenuGroup::Operation, 5, "keyer"),
            (MenuGroup::Network, 6, "cluster"),
            (MenuGroup::Device, 7, "ports"),
        ];
        let g = grouped(items);
        let names: Vec<(MenuGroup, Vec<&str>)> = g
            .iter()
            .map(|(g, l)| (*g, l.iter().map(|(_, n)| *n).collect()))
            .collect();
        assert_eq!(
            names,
            [
                (MenuGroup::Check, vec!["partial", "super"]),
                (MenuGroup::Report, vec!["score", "multi"]),
                (MenuGroup::Operation, vec!["keyer"]),
                (MenuGroup::Device, vec!["ports"]),
                (MenuGroup::Network, vec!["cluster"]),
                (MenuGroup::Tools, vec!["ports"]),
            ]
        );
        assert!(grouped(Vec::<(MenuGroup, usize, ())>::new()).is_empty());
    }

    /// Every linked panel is in the group chosen for it.
    #[test]
    fn panels_are_in_their_menus() {
        let groups: std::collections::HashMap<String, MenuGroup> = crate::registry::panels()
            .iter()
            .map(|p| (p.id().to_string(), p.menu()))
            .collect();
        let expect = [
            ("score", MenuGroup::Report),
            ("multipliers", MenuGroup::Report),
            ("summary", MenuGroup::Report),
            ("maplot", MenuGroup::Report),
            ("partial-check", MenuGroup::Check),
            ("super-check", MenuGroup::Check),
            ("check-call", MenuGroup::Check),
            ("check-multi", MenuGroup::Check),
            ("keyer", MenuGroup::Operation),
            ("cw-decoder", MenuGroup::Operation),
            ("scope", MenuGroup::Operation),
            ("rate", MenuGroup::Operation),
            ("cluster", MenuGroup::Network),
            ("zserver", MenuGroup::Network),
            ("soumu", MenuGroup::Network),
            ("es", MenuGroup::Network),
            ("update", MenuGroup::Help),
            ("ports", MenuGroup::Device),
        ];
        for (id, group) in expect {
            if let Some(g) = groups.get(id) {
                assert_eq!(*g, group, "{id}");
            }
        }
    }

    /// Score and multipliers keep their toolbar buttons, which toggle the window.
    #[test]
    fn report_panels_have_window_buttons() {
        for p in crate::registry::panels() {
            if ["score", "multipliers"].contains(&p.id()) {
                let b = p.toolbar("en");
                assert_eq!(b.len(), 1, "{}", p.id());
                assert_eq!(b[0].kind, zlog_ui::ToolbarKind::Window);
                assert!(!b[0].tooltip.is_empty());
            }
        }
    }
}
