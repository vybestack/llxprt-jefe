//! Crate-private display metadata for Help and footer projections.
//!
//! Rows contain ordering, action IDs, and human descriptions only. Chord labels
//! are projected from the immutable registry snapshot's effective bindings.

#[derive(Clone, Copy, Debug)]
pub struct HelpDisplayLine {
    pub section: u8,
    pub order: u16,
    pub actions: &'static [&'static str],
    pub description: &'static str,
}

pub const HELP_SECTIONS: &[(&str, u8)] = &[
    ("Navigation:", 0),
    ("Modes:", 1),
    ("Issues & PR detail:", 2),
    ("PR Changes (drill-down from PR detail):", 3),
    ("Actions:", 4),
    ("Filter & Sort (Issues, PRs, Actions):", 5),
    ("Dashboard:", 6),
    ("Other:", 7),
];

pub const HELP_DISPLAY_LINES: &[HelpDisplayLine] = &[
    help(
        0,
        0,
        &[
            "dashboard.navigate-up",
            "dashboard.navigate-down",
            "issues.list-up",
            "issues.list-down",
            "issues.detail-up",
            "issues.detail-down",
            "prs.list-up",
            "prs.list-down",
            "prs.detail-up",
            "prs.detail-down",
            "actions.repo-up",
            "actions.repo-down",
            "actions.run-up",
            "actions.run-down",
            "actions.detail-up",
            "actions.detail-down",
        ],
        "Select item / scroll detail",
    ),
    help(
        0,
        1,
        &[
            "dashboard.navigate-left",
            "dashboard.navigate-right",
            "issues.list-cycle-pane",
            "issues.detail-cycle-pane",
            "issues.repo-cycle-pane",
            "prs.list-cycle-pane",
            "prs.detail-cycle-pane",
            "prs.repo-cycle-pane",
        ],
        "Switch pane",
    ),
    help(
        0,
        2,
        &[
            "issues.detail-subfocus-next",
            "issues.detail-subfocus-previous",
            "prs.detail-next",
            "prs.detail-previous",
        ],
        "Focus next / previous detail section",
    ),
    help(
        0,
        3,
        &["dashboard.toggle-terminal"],
        "Toggle terminal focus",
    ),
    blank(0, 4),
    help(1, 0, &["github.open-issues"], "Open Issues mode"),
    help(
        1,
        1,
        &["github.open-pull-requests"],
        "Open Pull Requests mode",
    ),
    help(1, 2, &["github.open-actions"], "Open Actions mode"),
    blank(1, 3),
    help(2, 0, &["issues.open", "prs.open"], "Open detail"),
    help(2, 1, &["issues.comment", "prs.comment"], "Comment"),
    help(2, 2, &["issues.reply", "prs.reply"], "Reply"),
    help(2, 3, &["issues.edit", "prs.edit"], "Edit"),
    help(
        2,
        4,
        &["issues.send-agent", "prs.send-agent"],
        "Send to agent",
    ),
    help(2, 5, &["prs.resolve"], "Resolve / unresolve review thread"),
    help(
        2,
        6,
        &["prs.list-browser", "prs.open-browser"],
        "Open pull request in browser",
    ),
    help(2, 7, &["prs.open-merge"], "Merge pull request"),
    static_line(2, 8, "  Focus a review thread before resolving or replying"),
    blank(2, 9),
    help(
        3,
        0,
        &["prs.open-changes"],
        "Open Changes for the loaded pull request",
    ),
    help(
        3,
        1,
        &["prs.changes-activate"],
        "Focus selected file content",
    ),
    help(
        3,
        2,
        &["prs.changes-focus-files"],
        "Return focus to the changed-files list",
    ),
    help(
        3,
        3,
        &["prs.changes-edit"],
        "Change view / comment / reply / resolve selected row",
    ),
    help(
        3,
        4,
        &["prs.changes-back"],
        "Content to file list to pull-request detail",
    ),
    blank(3, 5),
    help(
        4,
        0,
        &[
            "actions.repo-up",
            "actions.repo-down",
            "actions.run-up",
            "actions.run-down",
            "actions.detail-up",
            "actions.detail-down",
        ],
        "Select repository, run, or focused job",
    ),
    help(
        4,
        1,
        &["actions.run-open", "actions.detail-expand"],
        "Open run detail / expand focused job",
    ),
    help(
        4,
        2,
        &["actions.detail-expand", "actions.detail-collapse"],
        "Expand / collapse focused job",
    ),
    help(
        4,
        3,
        &[
            "actions.detail-back",
            "actions.run-exit",
            "actions.repo-exit",
        ],
        "Collapse job, back to runs, then exit Actions",
    ),
    help(
        4,
        4,
        &["actions.detail-page-up", "actions.detail-page-down"],
        "Scroll job detail",
    ),
    help(
        4,
        5,
        &[
            "actions.repo-cycle",
            "actions.run-cycle",
            "actions.detail-cycle",
        ],
        "Cycle repository, runs, and detail focus",
    ),
    help(
        4,
        6,
        &["actions.open-filter", "actions.focus-search"],
        "Filter / search workflow runs",
    ),
    help(
        4,
        7,
        &["actions.dispatch", "actions.reload"],
        "Dispatch workflow / refresh runs",
    ),
    blank(4, 8),
    help(
        5,
        0,
        &[
            "issues.open-filter",
            "prs.open-filter",
            "actions.open-filter",
        ],
        "Open filter dialog",
    ),
    help(
        5,
        1,
        &[
            "issues.filter-next",
            "prs.filter-next",
            "actions.filter-next",
            "issues.filter-previous",
            "prs.filter-previous",
            "actions.filter-previous",
        ],
        "Move between filter fields",
    ),
    help(
        5,
        2,
        &[
            "issues.filter-choice-previous",
            "issues.filter-choice-next",
            "prs.filter-choice-previous",
            "prs.filter-choice-next",
            "actions.filter-choice-previous",
            "actions.filter-choice-next",
        ],
        "Cycle the active field value",
    ),
    help(
        5,
        3,
        &[
            "issues.filter-apply",
            "prs.filter-apply",
            "actions.filter-apply",
        ],
        "Apply filter and sort",
    ),
    help(
        5,
        4,
        &[
            "issues.filter-clear",
            "prs.filter-clear",
            "actions.filter-clear",
        ],
        "Clear the active field",
    ),
    help(
        5,
        5,
        &[
            "issues.filter-clear-all",
            "prs.filter-clear-all",
            "actions.filter-clear-all",
        ],
        "Clear all filter fields",
    ),
    help(
        5,
        6,
        &[
            "issues.filter-cancel",
            "prs.filter-cancel",
            "actions.filter-cancel",
        ],
        "Cancel / close dialog",
    ),
    static_line(
        5,
        7,
        "  Sort lives in the filter dialog (below filter fields).",
    ),
    blank(5, 8),
    help(6, 0, &["dashboard.new"], "New agent"),
    help(6, 1, &["dashboard.new-repository"], "New repository"),
    help(6, 2, &["dashboard.delete-selection"], "Delete selected"),
    help(6, 3, &["dashboard.kill-agent"], "Kill agent"),
    help(6, 4, &["dashboard.restart-agent"], "Restart agent"),
    help(
        6,
        5,
        &["dashboard.relaunch-agent"],
        "Relaunch dead / recover server-lost agents",
    ),
    help(6, 6, &["dashboard.open-split"], "Open Split mode"),
    help(
        6,
        7,
        &[
            "dashboard.grab-start",
            "dashboard.grab-drop",
            "dashboard.grab-up",
            "dashboard.grab-down",
        ],
        "Grab / move / drop reorder",
    ),
    help(
        6,
        8,
        &["dashboard.toggle-hidden-repositories"],
        "Toggle active-only repositories and agents",
    ),
    help(6, 9, &["dashboard.open-terminals"], "Open Terminal Manager"),
    help(6, 10, &["shell.open-external"], "Open external terminal"),
    help(
        6,
        11,
        &["shell.open-embedded", "shell.close"],
        "Open / resume or close embedded shell",
    ),
    help(
        6,
        12,
        &["shell.hide"],
        "Hide embedded shell (keeps it running)",
    ),
    help(
        6,
        13,
        &[
            "core.jump-agent.1",
            "core.jump-agent.2",
            "core.jump-agent.3",
            "core.jump-agent.4",
            "core.jump-agent.5",
            "core.jump-agent.6",
            "core.jump-agent.7",
            "core.jump-agent.8",
            "core.jump-agent.9",
        ],
        "Jump to agent shortcut",
    ),
    blank(6, 14),
    help(7, 0, &["dashboard.open-theme-picker"], "Theme picker"),
    help(
        7,
        1,
        &["dashboard.open-help", "split.open-help", "issues.open-help"],
        "This help",
    ),
    help(7, 2, &["core.emergency-exit"], "Quit"),
    static_line(7, 3, "  qqq         Quit (rapid sequence)"),
];

#[derive(Clone, Copy, Debug)]
pub struct FooterDisplayHint {
    pub description: &'static str,
    pub resume_description: Option<&'static str>,
    pub actions: &'static [&'static str],
    pub order: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct FooterModeGroup {
    pub mode: FooterMode,
    pub hints: &'static [FooterDisplayHint],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FooterMode {
    Dashboard,
    Split,
    Issues,
    PullRequests,
    Actions,
    Errors,
    Terminals,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionsFocusKind {
    RepoList,
    RunList,
    Detail,
}

#[derive(Clone, Copy, Debug)]
pub struct ActionsFocusGroup {
    pub focus: ActionsFocusKind,
    pub hints: &'static [FooterDisplayHint],
}

pub const SHELL_OVERLAY_HINTS: &[FooterDisplayHint] = &[
    hint("hide shell", &["shell.hide"], 1),
    hint("close shell", &["shell.close"], 2),
];

pub const TERMINAL_FOCUSED_HINTS: &[FooterDisplayHint] =
    &[hint("unfocus", &["core.leave-terminal"], 1)];

pub const ACTIONS_FOCUS_GROUPS: &[ActionsFocusGroup] = &[
    ActionsFocusGroup {
        focus: ActionsFocusKind::RepoList,
        hints: &[
            hint("repositories", &["actions.repo-up", "actions.repo-down"], 1),
            hint("runs / pane", &["actions.repo-cycle"], 2),
            hint("filter", &["actions.open-filter"], 4),
            hint("search", &["actions.focus-search"], 5),
            hint("dispatch", &["actions.dispatch"], 6),
            hint("refresh", &["actions.reload"], 7),
            hint("exit", &["actions.repo-exit"], 8),
        ],
    },
    ActionsFocusGroup {
        focus: ActionsFocusKind::RunList,
        hints: &[
            hint("runs", &["actions.run-up", "actions.run-down"], 1),
            hint("detail", &["actions.run-open"], 2),
            hint("repository / pane", &["actions.run-cycle"], 3),
            hint("filter", &["actions.open-filter"], 4),
            hint("search", &["actions.focus-search"], 5),
            hint("dispatch", &["actions.dispatch"], 6),
            hint("refresh", &["actions.reload"], 7),
            hint("exit", &["actions.run-exit"], 8),
        ],
    },
    ActionsFocusGroup {
        focus: ActionsFocusKind::Detail,
        hints: &[
            hint("jobs", &["actions.detail-up", "actions.detail-down"], 1),
            hint("expand", &["actions.detail-expand"], 2),
            hint("collapse", &["actions.detail-collapse"], 3),
            hint("collapse / back", &["actions.detail-back"], 4),
            hint(
                "scroll",
                &["actions.detail-page-up", "actions.detail-page-down"],
                5,
            ),
            hint("pane", &["actions.detail-cycle"], 6),
            raw_hint("? help", 7),
        ],
    },
];

pub const FOOTER_MODE_GROUPS: &[FooterModeGroup] = &[
    FooterModeGroup {
        mode: FooterMode::Dashboard,
        hints: &[
            hint(
                "navigate",
                &["dashboard.navigate-up", "dashboard.navigate-down"],
                1,
            ),
            hint(
                "pane",
                &["dashboard.navigate-left", "dashboard.navigate-right"],
                2,
            ),
            hint(
                "terminal focus",
                &["dashboard.focus-terminal", "dashboard.toggle-terminal"],
                3,
            ),
            hint("shells", &["dashboard.open-terminals"], 4),
            resume_hint("shell", "resume shell", &["shell.open-embedded"], 5),
            hint("external term", &["shell.open-external"], 6),
            hint(
                "active-only (repos+agents)",
                &["dashboard.toggle-hidden-repositories"],
                7,
            ),
            hint("search", &["dashboard.focus-search"], 8),
            hint(
                "jump agent",
                &[
                    "core.jump-agent.1",
                    "core.jump-agent.2",
                    "core.jump-agent.3",
                    "core.jump-agent.4",
                    "core.jump-agent.5",
                    "core.jump-agent.6",
                    "core.jump-agent.7",
                    "core.jump-agent.8",
                    "core.jump-agent.9",
                ],
                9,
            ),
            hint("new-agent", &["dashboard.new"], 10),
            hint("new-repo", &["dashboard.new-repository"], 11),
            hint("delete", &["dashboard.delete-selection"], 12),
            hint("kill", &["dashboard.kill-agent"], 13),
            hint("restart", &["dashboard.restart-agent"], 14),
            hint("relaunch/recover", &["dashboard.relaunch-agent"], 15),
            hint("reorder", &["dashboard.grab-start"], 16),
            hint("split", &["dashboard.open-split"], 17),
            hint("theme", &["dashboard.open-theme-picker"], 18),
            hint("help", &["dashboard.open-help"], 19),
            hint("quit", &["core.emergency-exit"], 98),
            raw_hint("qqq quit", 99),
        ],
    },
    FooterModeGroup {
        mode: FooterMode::Split,
        hints: &[
            hint("select", &["split.navigate-up", "split.navigate-down"], 1),
            hint("grab", &["split.enter-grab"], 2),
            raw_hint("m move", 3),
            hint("back", &["split.back"], 4),
            hint("help", &["split.open-help"], 5),
            hint("quit", &["core.emergency-exit"], 98),
            raw_hint("qqq quit", 99),
        ],
    },
    FooterModeGroup {
        mode: FooterMode::Issues,
        hints: &[
            hint(
                "items",
                &[
                    "issues.list-up",
                    "issues.list-down",
                    "issues.detail-up",
                    "issues.detail-down",
                ],
                1,
            ),
            hint(
                "panes",
                &[
                    "issues.list-cycle-pane",
                    "issues.detail-cycle-pane",
                    "issues.repo-cycle-pane",
                ],
                2,
            ),
            hint("detail", &["issues.open"], 3),
            hint("new issue", &["issues.new"], 4),
            hint("filter", &["issues.open-filter"], 5),
            hint("search", &["issues.focus-search"], 6),
            hint(
                "detail focus",
                &[
                    "issues.detail-subfocus-next",
                    "issues.detail-subfocus-previous",
                ],
                7,
            ),
            hint("list", &["issues.refocus-list"], 8),
            hint("reply", &["issues.reply"], 9),
            hint("send to agent", &["issues.send-agent"], 10),
            hint("edit", &["issues.edit"], 11),
            hint("comment", &["issues.comment"], 12),
            hint("close", &["issues.open-close", "issues.detail-close"], 13),
            hint(
                "delete",
                &["issues.open-delete", "issues.detail-delete"],
                14,
            ),
            hint(
                "labels / assignees / milestone / title / type / state",
                &["issues.open-property"],
                15,
            ),
            hint("exit", &["issues.exit"], 16),
            hint("back / exit", &["issues.back"], 17),
        ],
    },
    FooterModeGroup {
        mode: FooterMode::PullRequests,
        hints: &[
            hint(
                "items",
                &[
                    "prs.list-up",
                    "prs.list-down",
                    "prs.detail-up",
                    "prs.detail-down",
                ],
                1,
            ),
            hint(
                "panes",
                &[
                    "prs.detail-cycle-pane",
                    "prs.repo-cycle-pane",
                    "prs.list-cycle-pane",
                ],
                2,
            ),
            hint("detail", &["prs.open"], 3),
            hint("filter", &["prs.open-filter"], 4),
            raw_hint("/ search", 5),
            hint(
                "detail focus",
                &["prs.detail-next", "prs.detail-previous"],
                6,
            ),
            hint("resolve", &["prs.resolve"], 7),
            hint("reply", &["prs.reply"], 8),
            hint("send to agent", &["prs.send-agent"], 9),
            hint("comment", &["prs.comment"], 10),
            hint("open", &["prs.list-browser", "prs.open-browser"], 11),
            hint("merge", &["prs.open-merge"], 12),
            hint(
                "labels / assignees / milestone / title / state",
                &["prs.open-property"],
                13,
            ),
            hint("list", &["prs.refocus-list"], 14),
            hint("exit", &["prs.exit"], 15),
            hint("back / exit", &["prs.back"], 16),
        ],
    },
    FooterModeGroup {
        mode: FooterMode::Errors,
        hints: &[
            hint("errors", &["errors.up", "errors.down"], 1),
            hint("detail", &["errors.activate"], 2),
            hint("pane", &["errors.cycle-pane"], 3),
            hint("scroll", &["errors.page-up", "errors.page-down"], 4),
            hint("clear", &["errors.clear"], 5),
            hint("exit", &["errors.back"], 6),
        ],
    },
    FooterModeGroup {
        mode: FooterMode::Terminals,
        hints: &[
            hint(
                "shells",
                &["terminal-manager.up", "terminal-manager.down"],
                1,
            ),
            hint("focus running shell", &["terminal-manager.focus-shell"], 2),
            hint("close", &["terminal-manager.close-shell"], 3),
            hint("back to dashboard", &["terminal-manager.back"], 4),
            raw_hint("? help", 5),
        ],
    },
];

const fn help(
    section: u8,
    order: u16,
    actions: &'static [&'static str],
    description: &'static str,
) -> HelpDisplayLine {
    HelpDisplayLine {
        section,
        order,
        actions,
        description,
    }
}

const fn static_line(section: u8, order: u16, description: &'static str) -> HelpDisplayLine {
    help(section, order, &[], description)
}

const fn blank(section: u8, order: u16) -> HelpDisplayLine {
    static_line(section, order, "")
}

const fn hint(
    description: &'static str,
    actions: &'static [&'static str],
    order: u16,
) -> FooterDisplayHint {
    FooterDisplayHint {
        description,
        resume_description: None,
        actions,
        order,
    }
}

const fn resume_hint(
    description: &'static str,
    resume_description: &'static str,
    actions: &'static [&'static str],
    order: u16,
) -> FooterDisplayHint {
    FooterDisplayHint {
        description,
        resume_description: Some(resume_description),
        actions,
        order,
    }
}

const fn raw_hint(text: &'static str, order: u16) -> FooterDisplayHint {
    hint(text, &[], order)
}
