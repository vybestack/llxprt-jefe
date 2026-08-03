//! Supplemental source-audited S4 action rows.
//!
//! This internal split replaces the provisional modal/workspace inventory with
//! exact current controls while raw text and cursor mutation remain outside the
//! action registry.

use super::{H, Spec};

pub(super) const SPECS: &[Spec] = &[
    spec!(
        "dashboard.pre-mode",
        "dashboard.pre-mode.toggle-terminal",
        H::ToggleTerminalFocus,
        ["F12"]
    ),
    spec!(protected "help", "help.close", H::HelpClose, ["Esc", "Shift+?"]),
    spec!("help", "help.scroll-up", H::HelpScrollUp, ["Up"]),
    spec!("help", "help.scroll-down", H::HelpScrollDown, ["Down"]),
    spec!("help", "help.page-up", H::HelpPageUp, ["PageUp"]),
    spec!("help", "help.page-down", H::HelpPageDown, ["PageDown"]),
    spec!("help", "help.home", H::HelpHome, ["Home"]),
    spec!("help", "help.end", H::HelpEnd, ["End"]),
    spec!(protected
        "modal.confirm",
        "confirm.cancel",
        H::ConfirmCancel,
        ["Esc", "n", "Shift+N"]
    ),
    spec!(
        "modal.confirm",
        "confirm.cycle-focus",
        H::ConfirmCycleFocus,
        ["Left", "Right", "Tab", "BackTab"]
    ),
    spec!(
        "modal.confirm",
        "confirm.accept",
        H::ConfirmAccept,
        ["Enter"]
    ),
    spec!(
        "modal.confirm",
        "confirm.toggle-workdir",
        H::ConfirmToggleDeleteWorkDir,
        [" ", "d", "Shift+D", "Up", "Down"]
    ),
    spec!(protected "modal.auth", "auth.cancel", H::AuthCancel, ["Esc"]),
    spec!(
        "modal.auth",
        "auth.retry",
        H::AuthRetry,
        ["r", "Shift+R", "Enter"]
    ),
    spec!(protected "modal.form", "form.cancel", H::FormCancel, ["Esc"]),
    spec!("modal.form", "form.submit", H::FormSubmit, ["Enter"]),
    spec!(
        "modal.form",
        "form.next-field",
        H::FormNextField,
        ["Tab", "Down"]
    ),
    spec!(
        "modal.form",
        "form.previous-field",
        H::FormPreviousField,
        ["BackTab", "Up"]
    ),
    spec!("modal.form", "form.space", H::FormNextField, [" "]),
    spec!("modal.theme", "theme.up", H::ThemeUp, ["Up"]),
    spec!("modal.theme", "theme.down", H::ThemeDown, ["Down"]),
    spec!(
        "modal.theme",
        "theme.toggle-override",
        H::ThemeToggleOverride,
        ["Tab"]
    ),
    spec!("modal.theme", "theme.apply", H::ThemeApply, ["Enter"]),
    spec!(protected "modal.theme", "theme.cancel", H::ThemeCancel, ["Esc"]),
    spec!("search", "search.apply", H::SearchApply, ["Enter"]),
    spec!(protected "search", "search.cancel", H::SearchCancel, ["Esc"]),
    spec!(
        "search",
        "search.ignore-arrows",
        H::SearchBackspace,
        ["Up", "Down", "Left", "Right"]
    ),
    spec!(
        "dashboard.search",
        "dashboard.search.apply",
        H::SearchApply,
        ["Enter"]
    ),
    spec!(protected
        "dashboard.search",
        "dashboard.search.cancel",
        H::SearchCancel,
        ["Esc"]
    ),
    spec!(protected "issues", "issues.exit", H::IssuesExit, ["a", "Esc"]),
    spec!("issues", "issues.refocus-list", H::IssuesBack, ["i"]),
    spec!("issues", "issues.open-prs", H::IssuesBack, ["p"]),
    spec!("issues", "issues.contextual-f12", H::IssuesBack, ["F12"]),
    spec!(
        "issues",
        "issues.open-help",
        H::OpenHelp,
        ["Shift+?", "h", "F1"]
    ),
    spec!("issues.repo-list", "issues.repo-up", H::NavigateUp, ["Up"]),
    spec!(
        "issues.repo-list",
        "issues.repo-down",
        H::NavigateDown,
        ["Down"]
    ),
    spec!(
        "issues.repo-list",
        "issues.repo-cycle-pane",
        H::IssuesCyclePane,
        ["Left", "Right", "Tab", "BackTab"]
    ),
    spec!("issues.list", "issues.list-up", H::NavigateUp, ["Up"]),
    spec!("issues.list", "issues.list-down", H::NavigateDown, ["Down"]),
    spec!(
        "issues.list",
        "issues.list-page-up",
        H::NavigatePageUp,
        ["PageUp"]
    ),
    spec!(
        "issues.list",
        "issues.list-page-down",
        H::NavigatePageDown,
        ["PageDown"]
    ),
    spec!("issues.list", "issues.list-home", H::NavigateHome, ["Home"]),
    spec!("issues.list", "issues.list-end", H::NavigateEnd, ["End"]),
    spec!("issues.list", "issues.open", H::IssuesOpen, ["Enter"]),
    spec!("issues.list", "issues.new", H::IssuesNew, ["n", "Shift+N"]),
    spec!(
        "issues.list",
        "issues.open-filter",
        H::IssuesOpenFilter,
        ["f"]
    ),
    spec!(
        "issues.list",
        "issues.focus-search",
        H::IssuesFocusSearch,
        ["/"]
    ),
    spec!(
        "issues.list",
        "issues.open-close",
        H::IssuesEdit,
        ["Shift+C"]
    ),
    spec!(
        "issues.list",
        "issues.open-delete",
        H::IssuesEdit,
        ["Shift+D"]
    ),
    spec!(
        "issues.list",
        "issues.list-cycle-pane",
        H::IssuesCyclePane,
        ["Left", "Right", "Tab", "BackTab"]
    ),
    spec!(protected "issues.detail", "issues.back", H::IssuesBack, ["Esc"]),
    spec!("issues.detail", "issues.detail-up", H::NavigateUp, ["Up"]),
    spec!(
        "issues.detail",
        "issues.detail-down",
        H::NavigateDown,
        ["Down"]
    ),
    spec!(
        "issues.detail",
        "issues.detail-page-up",
        H::NavigatePageUp,
        ["PageUp"]
    ),
    spec!(
        "issues.detail",
        "issues.detail-page-down",
        H::NavigatePageDown,
        ["PageDown"]
    ),
    spec!(
        "issues.detail",
        "issues.detail-cycle-pane",
        H::IssuesCyclePane,
        ["Left", "Right"]
    ),
    spec!(
        "issues.detail",
        "issues.detail-subfocus-next",
        H::IssuesOpen,
        ["Tab", "j"]
    ),
    spec!(
        "issues.detail",
        "issues.detail-subfocus-previous",
        H::IssuesNew,
        ["BackTab", "k"]
    ),
    spec!("issues.detail", "issues.edit", H::IssuesEdit, ["e"]),
    spec!("issues.detail", "issues.comment", H::IssuesComment, ["c"]),
    spec!("issues.detail", "issues.reply", H::IssuesReply, ["r"]),
    spec!(
        "issues.detail",
        "issues.send-agent",
        H::IssuesSendToAgent,
        ["Shift+S"]
    ),
    spec!(
        "issues.detail",
        "issues.detail-close",
        H::IssuesEdit,
        ["Shift+C"]
    ),
    spec!(
        "issues.detail",
        "issues.detail-delete",
        H::IssuesEdit,
        ["Shift+D"]
    ),
    spec!(
        "issues.detail",
        "issues.open-property",
        H::IssuesEdit,
        [
            "Shift+L", "Shift+A", "Shift+M", "Shift+T", "Shift+Y", "Shift+W"
        ]
    ),
    spec!(
        "issues.inline",
        "issues.inline-submit",
        H::IssuesSubmitInline,
        ["Alt+Enter", "Ctrl+Enter"]
    ),
    spec!(protected
        "issues.inline",
        "issues.inline-cancel",
        H::IssuesCancelInline,
        ["Ctrl+C", "Esc"]
    ),
    spec!(
        "issues.inline",
        "issues.inline-rewrite",
        H::IssuesEdit,
        ["Ctrl+R"]
    ),
    spec!(protected "issues.new-form", "issues.new-cancel", H::IssuesCancelInline, ["Esc"]),
    spec!(
        "issues.new-form",
        "issues.new-submit",
        H::IssuesSubmitInline,
        ["Alt+Enter", "Ctrl+Enter"]
    ),
    spec!(
        "issues.new-form",
        "issues.new-next",
        H::FormNextField,
        ["Enter", "Tab", "Down"]
    ),
    spec!(
        "issues.new-form",
        "issues.new-previous",
        H::FormPreviousField,
        ["BackTab", "Up"]
    ),
    spec!(
        "issues.new-form",
        "issues.new-space",
        H::FormNextField,
        [" "]
    ),
    spec!(
        "issues.agent-chooser",
        "issues.chooser-previous",
        H::IssuesChooserPrevious,
        ["Up"]
    ),
    spec!(
        "issues.agent-chooser",
        "issues.chooser-next",
        H::IssuesChooserNext,
        ["Down"]
    ),
    spec!(
        "issues.agent-chooser",
        "issues.chooser-confirm",
        H::IssuesChooserConfirm,
        ["Enter"]
    ),
    spec!(protected
        "issues.agent-chooser",
        "issues.chooser-cancel",
        H::IssuesChooserCancel,
        ["Esc"]
    ),
    spec!(
        "issues.property",
        "issues.property-up",
        H::IssuesChooserPrevious,
        ["Up"]
    ),
    spec!(
        "issues.property",
        "issues.property-down",
        H::IssuesChooserNext,
        ["Down"]
    ),
    spec!(
        "issues.property",
        "issues.property-toggle",
        H::IssuesChooserNext,
        [" "]
    ),
    spec!(
        "issues.property",
        "issues.property-confirm",
        H::IssuesChooserConfirm,
        ["Enter"]
    ),
    spec!(protected
        "issues.property",
        "issues.property-cancel",
        H::IssuesChooserCancel,
        ["Esc"]
    ),
    spec!(
        "issues.close-reason",
        "issues.close-up",
        H::IssuesChooserPrevious,
        ["Up"]
    ),
    spec!(
        "issues.close-reason",
        "issues.close-down",
        H::IssuesChooserNext,
        ["Down"]
    ),
    spec!(
        "issues.close-reason",
        "issues.close-confirm",
        H::IssuesChooserConfirm,
        ["Enter"]
    ),
    spec!(protected
        "issues.close-reason",
        "issues.close-cancel",
        H::IssuesChooserCancel,
        ["Esc"]
    ),
    spec!(
        "issues.delete-confirm",
        "issues.delete-confirm",
        H::IssuesChooserConfirm,
        ["Enter"]
    ),
    spec!(protected
        "issues.delete-confirm",
        "issues.delete-cancel",
        H::IssuesChooserCancel,
        ["Esc"]
    ),
    spec!(
        "issues.search",
        "issues.search-apply",
        H::SearchApply,
        ["Enter"]
    ),
    spec!(protected
        "issues.search",
        "issues.search-cancel",
        H::SearchCancel,
        ["Esc"]
    ),
    spec!(
        "issues.filter",
        "issues.filter-apply",
        H::FilterApply,
        ["Enter"]
    ),
    spec!(protected
        "issues.filter",
        "issues.filter-cancel",
        H::FilterCancel,
        ["Esc"]
    ),
    spec!(
        "issues.filter",
        "issues.filter-exit",
        H::IssuesExit,
        ["Ctrl+C"]
    ),
    spec!(
        "issues.filter",
        "issues.filter-next",
        H::FilterNextField,
        ["Tab"]
    ),
    spec!(
        "issues.filter",
        "issues.filter-previous",
        H::FilterPreviousField,
        ["BackTab"]
    ),
    spec!(
        "issues.filter",
        "issues.filter-clear",
        H::FilterClearCurrent,
        ["Delete"]
    ),
    spec!(
        "issues.filter",
        "issues.filter-clear-all",
        H::FilterClearAll,
        ["Ctrl+L"]
    ),
    spec!(
        "issues.filter",
        "issues.filter-choice-previous",
        H::FilterPreviousChoice,
        ["Left"]
    ),
    spec!(
        "issues.filter",
        "issues.filter-choice-next",
        H::FilterNextChoice,
        ["Right", "Up", "Down", " "]
    ),
    spec!(protected "prs", "prs.exit", H::PullRequestsExit, ["a", "Esc"]),
    spec!(
        "prs",
        "prs.refocus-list",
        H::PullRequestsBack,
        ["p", "Shift+P"]
    ),
    spec!("prs", "prs.open-filter", H::PullRequestsOpenFilter, ["f"]),
    spec!(
        "prs",
        "prs.open-actions",
        H::PullRequestsOpen,
        ["g", "Shift+G"]
    ),
    spec!(
        "prs",
        "prs.open-issues",
        H::PullRequestsOpen,
        ["i", "Shift+I"]
    ),
    spec!("prs", "prs.contextual-f12", H::PullRequestsBack, ["F12"]),
    spec!("prs.repo-list", "prs.repo-up", H::NavigateUp, ["Up"]),
    spec!("prs.repo-list", "prs.repo-down", H::NavigateDown, ["Down"]),
    spec!(
        "prs.repo-list",
        "prs.repo-cycle-pane",
        H::PullRequestsCyclePane,
        ["Left", "Right", "Tab", "BackTab"]
    ),
    spec!("prs.list", "prs.list-up", H::NavigateUp, ["Up"]),
    spec!("prs.list", "prs.list-down", H::NavigateDown, ["Down"]),
    spec!(
        "prs.list",
        "prs.list-page-up",
        H::NavigatePageUp,
        ["PageUp"]
    ),
    spec!(
        "prs.list",
        "prs.list-page-down",
        H::NavigatePageDown,
        ["PageDown"]
    ),
    spec!("prs.list", "prs.list-home", H::NavigateHome, ["Home"]),
    spec!("prs.list", "prs.list-end", H::NavigateEnd, ["End"]),
    spec!("prs.list", "prs.open", H::PullRequestsOpen, ["Enter"]),
    spec!(
        "prs.list",
        "prs.list-browser",
        H::PullRequestsOpenBrowser,
        ["o"]
    ),
    spec!(
        "prs.list",
        "prs.list-close",
        H::PullRequestsEdit,
        ["Shift+W"]
    ),
    spec!(
        "prs.list",
        "prs.list-delete",
        H::PullRequestsEdit,
        ["Shift+D"]
    ),
    spec!("prs.list", "prs.new", H::PullRequestsEdit, ["n", "Shift+N"]),
    spec!(
        "prs.list",
        "prs.list-cycle-pane",
        H::PullRequestsCyclePane,
        ["Left", "Right", "Tab", "BackTab"]
    ),
    spec!(protected "prs.detail", "prs.back", H::PullRequestsBack, ["Esc"]),
    spec!("prs.detail", "prs.detail-up", H::NavigateUp, ["Up"]),
    spec!("prs.detail", "prs.detail-down", H::NavigateDown, ["Down"]),
    spec!(
        "prs.detail",
        "prs.detail-page-up",
        H::NavigatePageUp,
        ["PageUp"]
    ),
    spec!(
        "prs.detail",
        "prs.detail-page-down",
        H::NavigatePageDown,
        ["PageDown"]
    ),
    spec!(
        "prs.detail",
        "prs.detail-cycle-pane",
        H::PullRequestsCyclePane,
        ["Left", "Right"]
    ),
    spec!(
        "prs.detail",
        "prs.detail-next",
        H::PullRequestsOpen,
        ["Tab", "j"]
    ),
    spec!(
        "prs.detail",
        "prs.detail-previous",
        H::PullRequestsBack,
        ["BackTab", "k"]
    ),
    spec!("prs.detail", "prs.comment", H::PullRequestsComment, ["c"]),
    spec!("prs.detail", "prs.reply", H::PullRequestsReply, ["r"]),
    spec!(
        "prs.detail",
        "prs.resolve",
        H::PullRequestsResolveThread,
        ["Shift+R"]
    ),
    spec!("prs.detail", "prs.edit", H::PullRequestsEdit, ["e"]),
    spec!(
        "prs.detail",
        "prs.send-agent",
        H::PullRequestsSendToAgent,
        ["Shift+S"]
    ),
    spec!(
        "prs.detail",
        "prs.open-browser",
        H::PullRequestsOpenBrowser,
        ["o"]
    ),
    spec!(
        "prs.detail",
        "prs.open-merge",
        H::PullRequestsOpenMerge,
        ["m"]
    ),
    spec!("prs.detail", "prs.open-changes", H::PullRequestsEdit, ["d"]),
    spec!(
        "prs.detail",
        "prs.detail-delete",
        H::PullRequestsEdit,
        ["Shift+D"]
    ),
    spec!(
        "prs.detail",
        "prs.open-property",
        H::PullRequestsEdit,
        ["Shift+L", "Shift+A", "Shift+M", "Shift+T", "Shift+W"]
    ),
    spec!(protected "prs.changes", "prs.changes-back", H::PullRequestsBack, ["Esc"]),
    spec!("prs.changes", "prs.changes-up", H::NavigateUp, ["Up"]),
    spec!("prs.changes", "prs.changes-down", H::NavigateDown, ["Down"]),
    spec!(
        "prs.changes",
        "prs.changes-page-up",
        H::NavigatePageUp,
        ["PageUp"]
    ),
    spec!(
        "prs.changes",
        "prs.changes-page-down",
        H::NavigatePageDown,
        ["PageDown"]
    ),
    spec!("prs.changes", "prs.changes-home", H::NavigateHome, ["Home"]),
    spec!("prs.changes", "prs.changes-end", H::NavigateEnd, ["End"]),
    spec!(
        "prs.changes",
        "prs.changes-activate",
        H::PullRequestsOpen,
        ["Enter", "Tab"]
    ),
    spec!(
        "prs.changes",
        "prs.changes-focus-files",
        H::PullRequestsBack,
        ["BackTab"]
    ),
    spec!(
        "prs.changes",
        "prs.changes-edit",
        H::PullRequestsEdit,
        ["v", "c", "r", "Shift+R"]
    ),
    spec!(
        "prs.inline",
        "prs.inline-submit",
        H::PullRequestsSubmitInline,
        ["Alt+Enter", "Ctrl+Enter"]
    ),
    spec!(protected "prs.inline", "prs.inline-cancel", H::PullRequestsCancelInline, ["Esc"]),
    spec!(
        "prs.agent-chooser",
        "prs.agent-up",
        H::PullRequestsChooserPrevious,
        ["Up"]
    ),
    spec!(
        "prs.agent-chooser",
        "prs.agent-down",
        H::PullRequestsChooserNext,
        ["Down"]
    ),
    spec!(
        "prs.agent-chooser",
        "prs.agent-confirm",
        H::PullRequestsChooserConfirm,
        ["Enter"]
    ),
    spec!(protected "prs.agent-chooser", "prs.agent-cancel", H::PullRequestsChooserCancel, ["Esc"]),
    spec!(
        "prs.merge-chooser",
        "prs.merge-up",
        H::PullRequestsChooserPrevious,
        ["Up"]
    ),
    spec!(
        "prs.merge-chooser",
        "prs.merge-down",
        H::PullRequestsChooserNext,
        ["Down"]
    ),
    spec!(
        "prs.merge-chooser",
        "prs.merge-confirm",
        H::PullRequestsChooserConfirm,
        ["Enter"]
    ),
    spec!(protected "prs.merge-chooser", "prs.merge-cancel", H::PullRequestsChooserCancel, ["Esc"]),
    spec!(
        "prs.property",
        "prs.property-up",
        H::PullRequestsChooserPrevious,
        ["Up"]
    ),
    spec!(
        "prs.property",
        "prs.property-down",
        H::PullRequestsChooserNext,
        ["Down"]
    ),
    spec!(
        "prs.property",
        "prs.property-toggle",
        H::PullRequestsChooserNext,
        [" "]
    ),
    spec!(
        "prs.property",
        "prs.property-confirm",
        H::PullRequestsChooserConfirm,
        ["Enter"]
    ),
    spec!(protected "prs.property", "prs.property-cancel", H::PullRequestsChooserCancel, ["Esc"]),
    spec!(protected "prs.new-form", "prs.new-cancel", H::PullRequestsCancelInline, ["Esc"]),
    spec!(
        "prs.new-form",
        "prs.new-submit",
        H::PullRequestsSubmitInline,
        ["Alt+Enter", "Ctrl+Enter"]
    ),
    spec!("prs.new-form", "prs.new-next", H::FormNextField, ["Tab"]),
    spec!(
        "prs.new-form",
        "prs.new-previous",
        H::FormPreviousField,
        ["BackTab"]
    ),
    spec!(
        "prs.new-form",
        "prs.new-branch-up",
        H::PullRequestsChooserPrevious,
        ["Up"]
    ),
    spec!(
        "prs.new-form",
        "prs.new-branch-down",
        H::PullRequestsChooserNext,
        ["Down"]
    ),
    spec!(
        "prs.delete-confirm",
        "prs.delete-confirm",
        H::PullRequestsChooserConfirm,
        ["Enter"]
    ),
    spec!(protected
        "prs.delete-confirm",
        "prs.delete-cancel",
        H::PullRequestsChooserCancel,
        ["Esc"]
    ),
    spec!("prs.search", "prs.search-apply", H::SearchApply, ["Enter"]),
    spec!(protected "prs.search", "prs.search-cancel", H::SearchCancel, ["Esc"]),
    spec!("prs.filter", "prs.filter-apply", H::FilterApply, ["Enter"]),
    spec!(protected "prs.filter", "prs.filter-cancel", H::FilterCancel, ["Esc"]),
    spec!("prs.filter", "prs.filter-next", H::FilterNextField, ["Tab"]),
    spec!(
        "prs.filter",
        "prs.filter-previous",
        H::FilterPreviousField,
        ["BackTab"]
    ),
    spec!(
        "prs.filter",
        "prs.filter-clear",
        H::FilterClearCurrent,
        ["Delete"]
    ),
    spec!(
        "prs.filter",
        "prs.filter-clear-all",
        H::FilterClearAll,
        ["Ctrl+L"]
    ),
    spec!(
        "prs.filter",
        "prs.filter-choice-previous",
        H::FilterPreviousChoice,
        ["Left"]
    ),
    spec!(
        "prs.filter",
        "prs.filter-choice-next",
        H::FilterNextChoice,
        ["Right", "Up", "Down", " "]
    ),
    spec!("actions", "actions.reload", H::ActionsReload, ["r"]),
    spec!(
        "actions",
        "actions.open-filter",
        H::ActionsOpenFilter,
        ["f"]
    ),
    spec!(
        "actions",
        "actions.focus-search",
        H::ActionsFocusSearch,
        ["/"]
    ),
    spec!("actions", "actions.dispatch", H::ActionsActivate, ["d"]),
    spec!(protected "actions.repo-list", "actions.repo-exit", H::ActionsExit, ["Esc"]),
    spec!("actions.repo-list", "actions.repo-up", H::ActionsUp, ["Up"]),
    spec!(
        "actions.repo-list",
        "actions.repo-down",
        H::ActionsDown,
        ["Down"]
    ),
    spec!(
        "actions.repo-list",
        "actions.repo-cycle",
        H::ActionsActivate,
        ["Left", "Right", "Tab"]
    ),
    spec!(protected "actions.run-list", "actions.run-exit", H::ActionsExit, ["Esc"]),
    spec!("actions.run-list", "actions.run-up", H::ActionsUp, ["Up"]),
    spec!(
        "actions.run-list",
        "actions.run-down",
        H::ActionsDown,
        ["Down"]
    ),
    spec!(
        "actions.run-list",
        "actions.run-page-up",
        H::ActionsPageUp,
        ["PageUp"]
    ),
    spec!(
        "actions.run-list",
        "actions.run-page-down",
        H::ActionsPageDown,
        ["PageDown"]
    ),
    spec!(
        "actions.run-list",
        "actions.run-home",
        H::ActionsActivate,
        ["Home"]
    ),
    spec!(
        "actions.run-list",
        "actions.run-end",
        H::ActionsActivate,
        ["End"]
    ),
    spec!(
        "actions.run-list",
        "actions.run-open",
        H::ActionsActivate,
        ["Enter"]
    ),
    spec!(
        "actions.run-list",
        "actions.run-cycle",
        H::ActionsActivate,
        ["Left", "Right", "Tab"]
    ),
    spec!(protected "actions.detail", "actions.detail-back", H::ActionsBack, ["Esc"]),
    spec!("actions.detail", "actions.detail-up", H::ActionsUp, ["Up"]),
    spec!(
        "actions.detail",
        "actions.detail-down",
        H::ActionsDown,
        ["Down"]
    ),
    spec!(
        "actions.detail",
        "actions.detail-page-up",
        H::ActionsPageUp,
        ["PageUp"]
    ),
    spec!(
        "actions.detail",
        "actions.detail-page-down",
        H::ActionsPageDown,
        ["PageDown"]
    ),
    spec!(
        "actions.detail",
        "actions.detail-expand",
        H::ActionsActivate,
        ["Enter", "Right"]
    ),
    spec!(
        "actions.detail",
        "actions.detail-collapse",
        H::ActionsActivate,
        ["Left"]
    ),
    spec!(
        "actions.detail",
        "actions.detail-cycle",
        H::ActionsActivate,
        ["Tab"]
    ),
    spec!(
        "actions.search",
        "actions.search-apply",
        H::SearchApply,
        ["Enter"]
    ),
    spec!(protected "actions.search", "actions.search-cancel", H::SearchCancel, ["Esc"]),
    spec!(
        "actions.filter",
        "actions.filter-apply",
        H::FilterApply,
        ["Enter"]
    ),
    spec!(protected "actions.filter", "actions.filter-cancel", H::FilterCancel, ["Esc"]),
    spec!(
        "actions.filter",
        "actions.filter-next",
        H::FilterNextField,
        ["Tab"]
    ),
    spec!(
        "actions.filter",
        "actions.filter-previous",
        H::FilterPreviousField,
        ["BackTab"]
    ),
    spec!(
        "actions.filter",
        "actions.filter-clear",
        H::FilterClearCurrent,
        ["Delete"]
    ),
    spec!(
        "actions.filter",
        "actions.filter-clear-all",
        H::FilterClearAll,
        ["Ctrl+L"]
    ),
    spec!(
        "actions.filter",
        "actions.filter-choice-previous",
        H::FilterPreviousChoice,
        ["Left"]
    ),
    spec!(
        "actions.filter",
        "actions.filter-choice-next",
        H::FilterNextChoice,
        ["Right", "Up", "Down", " "]
    ),
];

pub(super) const CONTEXT_STACK_SPECS: &[(&[&str], bool)] = &[
    (&["dashboard.search", "dashboard.pre-mode", "global"], false),
    (&["issues.repo-list", "issues", "global"], false),
    (&["issues.list", "issues", "global"], false),
    (&["issues.detail", "issues", "global"], false),
    (&["issues.inline", "global"], false),
    (&["issues.new-form", "global"], false),
    (&["issues.agent-chooser", "global"], false),
    (&["issues.property", "global"], false),
    (&["issues.close-reason", "global"], false),
    (&["issues.delete-confirm", "global"], false),
    (&["issues.search", "global"], false),
    (&["issues.filter", "global"], false),
    (&["prs.repo-list", "prs", "global"], false),
    (&["prs.list", "prs", "global"], false),
    (&["prs.detail", "prs", "global"], false),
    (&["prs.changes", "prs", "global"], false),
    (&["prs.inline", "global"], false),
    (&["prs.agent-chooser", "global"], false),
    (&["prs.merge-chooser", "global"], false),
    (&["prs.property", "global"], false),
    (&["prs.delete-confirm", "global"], false),
    (&["prs.new-form", "global"], false),
    (&["prs.search", "global"], false),
    (&["prs.filter", "global"], false),
    (&["actions.repo-list", "actions", "global"], false),
    (&["actions.run-list", "actions", "global"], false),
    (&["actions.detail", "actions", "global"], false),
    (&["actions.search", "global"], false),
    (&["actions.filter", "global"], false),
    (&["help", "global"], false),
    (&["modal.confirm", "global"], false),
    (&["modal.auth", "global"], false),
    (&["modal.form", "global"], false),
    (&["modal.theme", "global"], false),
    (&["search", "global"], false),
    (&["help", "dashboard.pre-mode", "global"], false),
    (&["modal.confirm", "dashboard.pre-mode", "global"], false),
    (&["modal.form", "dashboard.pre-mode", "global"], false),
    (&["modal.theme", "dashboard.pre-mode", "global"], false),
    (&["search", "dashboard.pre-mode", "global"], false),
];
