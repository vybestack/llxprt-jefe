//! Thin iocraft renderer for lowered host-rendered provider screens.
//!
//! Geometry comes exclusively from the frame-owned `state.resolved_layout`;
//! every visible panel is drawn at its resolved chrome/content rectangle.

use iocraft::prelude::*;
use unicode_width::UnicodeWidthStr;

use crate::provider_panel_view::{
    PanelProjection, PanelRender, PanelStatus, project_current_screen,
};
use crate::state::AppState;
use crate::theme::{ResolvedColors, ThemeColors};
use crate::ui::components::TerminalView;
use crate::ui::components::WorkbenchCard;
use crate::workbench::Rect;

/// Props for a descriptor-driven provider screen.
#[derive(Default, Props)]
pub struct ProviderScreenProps {
    /// Immutable state snapshot for this frame.
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active theme name for shared screen chrome.
    pub theme_name: String,
    /// Live embedded PTY snapshot for a private host terminal panel.
    pub terminal_snapshot: Option<crate::runtime::TerminalSnapshot>,
    /// Retained PTY scrollback lines.
    pub terminal_history_lines: Vec<String>,
    /// Actual PTY pane rows.
    pub terminal_pane_rows: usize,
    /// Actual PTY pane columns.
    pub terminal_pane_cols: usize,
}

fn projection_inputs(
    state: Option<&AppState>,
) -> Result<
    (
        &AppState,
        &crate::workbench::ScreenDescriptor,
        &crate::workbench::ResolvedLayout,
    ),
    String,
> {
    let state = state.ok_or_else(|| "provider screen state unavailable".to_owned())?;
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .ok_or_else(|| format!("screen descriptor unavailable: {}", state.screen().as_str()))?;
    let layout = state
        .resolved_layout
        .as_ref()
        .ok_or_else(|| format!("screen layout unavailable: {}", state.screen().as_str()))?;
    Ok((state, descriptor, layout))
}

/// Render the current lowered screen from its descriptor and resolved layout.
#[component]
pub fn ProviderScreen(props: &ProviderScreenProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let (state, descriptor, layout) = match projection_inputs(props.state.as_ref()) {
        Ok(inputs) => inputs,
        Err(error) => return projection_error_screen(error, &rc),
    };
    let view = match project_current_screen(state, descriptor, layout) {
        Ok(view) => view,
        Err(error) => return projection_error_screen(error.to_string(), &rc),
    };
    let shell_overlay_active = state.shell_overlay_active();
    let shell_resume_available = !shell_overlay_active
        && state.selected_repository().is_some_and(|repository| {
            crate::state::resolve_repository_shell(state, &repository.id).is_some()
        });
    let footer = state.footer_hints(crate::action_projection::FooterProjectionInput {
        screen: state.screen(),
        terminal_focused: state.terminal_focused,
        shell_overlay_active,
        shell_resume_available,
        actions_focus: None,
        mode_override: None,
    });
    let header = themed_header(
        &view.title,
        HeaderStatus {
            warning: state.warning_message.as_deref(),
            repository_count: state.visible_repository_indices().len(),
            running_count: state
                .agents
                .iter()
                .filter(|agent| agent.is_running())
                .count(),
            agent_count: state.visible_agent_count(),
            error_count: state.errors_state.count(),
            theme_name: &props.theme_name,
            kennel_mode: state.is_kennel_mode(),
        },
        &rc,
    );
    let mut children = global_chrome(&header, &footer, layout.outer, &rc);
    if view.too_small {
        children.push(absolute_text(
            layout.outer,
            "screen too small".to_owned(),
            rc.dim,
        ));
    } else {
        for panel in view.panels.iter().filter(|panel| panel.visible) {
            render_panel(panel, state, props, &rc, &mut children);
        }
    }

    element! {
        Box(
            position: Position::Relative,
            width: 100pct,
            height: 100pct,
            background_color: rc.bg,
        ) {
            #(children)
        }
    }
    .into_any()
}
struct HeaderStatus<'a> {
    warning: Option<&'a str>,
    repository_count: usize,
    running_count: usize,
    agent_count: usize,
    error_count: usize,
    theme_name: &'a str,
    kennel_mode: bool,
}

/// The resolved top-bar composition: the band's colors plus the three
/// segments its SpaceBetween row lays out (title+version, stats, theme).
struct ThemedHeader {
    background: Color,
    text: Color,
    left: String,
    center: String,
    right: String,
}

fn themed_header(title: &str, status: HeaderStatus<'_>, rc: &ResolvedColors) -> ThemedHeader {
    let kennel = if status.kennel_mode {
        " (Kennel mode)"
    } else {
        ""
    };
    let center_text = super::status_bar::status_bar_stats(
        status.warning,
        status.repository_count,
        status.running_count,
        status.agent_count,
        status.error_count,
    );
    ThemedHeader {
        background: rc.border,
        text: rc.bg,
        left: format!("{title}{kennel} - {}", crate::VERSION),
        center: center_text,
        right: status.theme_name.to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GlobalChromeRects {
    title: Rect,
    footer: Rect,
}

fn global_chrome_rects(outer: Rect) -> GlobalChromeRects {
    GlobalChromeRects {
        title: Rect {
            col: outer.col,
            row: outer.row.saturating_sub(1),
            width: outer.width,
            height: u16::from(outer.row > 0),
        },
        footer: Rect {
            col: outer.col,
            row: outer.row.saturating_add(outer.height),
            width: outer.width,
            height: 1,
        },
    }
}

fn global_chrome(
    header: &ThemedHeader,
    footer: &str,
    outer: Rect,
    rc: &ResolvedColors,
) -> Vec<AnyElement<'static>> {
    let rects = global_chrome_rects(outer);
    let identity = crate::process_identity_label(std::process::id(), crate::GIT_COMMIT);
    vec![
        themed_title_bar(rects.title, header),
        absolute_text(
            rects.footer,
            footer_line(footer, &identity, rects.footer.width),
            rc.dim,
        ),
    ]
}

/// Compose the footer band: hints on the left, the process identity on the
/// right edge of the same row.
///
/// The pre-cutover `KeybindBar` right-aligned `pid:<pid> <commit>` on every
/// screen it drew, and `pid-commit-corner.json` asserts the `pid:` prefix.
/// The provider runtime's footer dropped the label when the dashboard moved
/// onto `global_chrome` (#734). The hints are the actionable half, so a band
/// too narrow for both keeps them and drops the identity rather than the
/// other way round.
fn footer_line(hints: &str, identity: &str, width: u16) -> String {
    let width = usize::from(width);
    let identity_width = UnicodeWidthStr::width(identity);
    let Some(hint_budget) = width
        .checked_sub(identity_width)
        .and_then(|budget| budget.checked_sub(1))
        .filter(|budget| *budget > 0)
    else {
        return crate::list_viewport::fit_text_to_width(hints, width);
    };
    let hints = crate::list_viewport::fit_text_to_width(hints, hint_budget);
    let gap = hint_budget
        .saturating_sub(UnicodeWidthStr::width(hints.as_str()))
        .saturating_add(1);
    format!("{hints}{}{identity}", " ".repeat(gap))
}

/// The shared top bar: `StatusBar` semantics on the title band's resolved
/// rectangle — one reverse-video band, three segments justified between the
/// edges, one cell of end padding.
fn themed_title_bar(rect: Rect, header: &ThemedHeader) -> AnyElement<'static> {
    element! {
        Box(
            position: Position::Absolute,
            left: u32::from(rect.col),
            top: u32::from(rect.row),
            width: u32::from(rect.width),
            height: u32::from(rect.height),
        ) {
            Box(
                flex_direction: FlexDirection::Row,
                width: 100pct,
                height: 100pct,
                background_color: header.background,
                justify_content: JustifyContent::SpaceBetween,
                padding_left: 1u32,
                padding_right: 1u32,
            ) {
                Text(content: header.left.clone(), weight: Weight::Bold, color: header.text)
                Text(content: header.center.clone(), color: header.text)
                Text(content: header.right.clone(), color: header.text)
            }
        }
    }
    .into_any()
}

fn render_panel(
    panel: &PanelProjection,
    state: &AppState,
    props: &ProviderScreenProps,
    rc: &ResolvedColors,
    children: &mut Vec<AnyElement<'static>>,
) {
    if panel.render == PanelRender::EmbeddedTerminal {
        render_embedded_terminal(panel, state, props, children);
        return;
    }
    if panel.render == PanelRender::WorkbenchCards {
        render_workbench_cards(panel, state, props, rc, children);
        return;
    }
    let border_color = match panel.status {
        PanelStatus::Failed => rc.error,
        _ if panel.focused => rc.border_focused,
        _ => rc.border,
    };
    let border_style = panel_border_style(panel.focused);
    let chrome = panel.chrome;
    children.push(
        element! {
            Box(
                position: Position::Absolute,
                left: u32::from(chrome.col),
                top: u32::from(chrome.row),
                width: u32::from(chrome.width),
                height: u32::from(chrome.height),
                border_style: border_style,
                border_color,
            ) {}
        }
        .into_any(),
    );
    let title = fitted_panel_title(panel, chrome);
    let title_rect = panel_title_rect(chrome, &title);
    children.push(absolute_text(title_rect, title, border_color));

    let rows = panel
        .lines
        .iter()
        .cloned()
        .map(|line| element! { Text(content: line, color: rc.fg) })
        .collect::<Vec<_>>();
    let content = panel.content;
    children.push(
        element! {
            Box(
                position: Position::Absolute,
                left: u32::from(content.col),
                top: u32::from(content.row),
                width: u32::from(content.width),
                height: u32::from(content.height),
                flex_direction: FlexDirection::Column,
            ) {
                #(rows)
            }
        }
        .into_any(),
    );
}
fn render_embedded_terminal(
    panel: &PanelProjection,
    state: &AppState,
    props: &ProviderScreenProps,
    children: &mut Vec<AnyElement<'static>>,
) {
    let chrome = panel.chrome;
    let session_live = state
        .selected_agent()
        .is_some_and(|agent| agent.status == crate::domain::AgentStatus::Running);
    children.push(
        element! {
            Box(
                position: Position::Absolute,
                left: u32::from(chrome.col),
                top: u32::from(chrome.row),
                width: u32::from(chrome.width),
                height: u32::from(chrome.height),
            ) {
                TerminalView(
                    snapshot: props.terminal_snapshot.clone(),
                    focused: panel.focused,
                    title: panel.title.clone(),
                    colors: props.colors.clone(),
                    selection: state.selection,
                    session_live,
                    history_lines: props.terminal_history_lines.clone(),
                    terminal_history_offset: state.terminal_history_offset,
                    override_theme: state.override_agent_theme,
                    pane_rows: props.terminal_pane_rows,
                    pane_cols: props.terminal_pane_cols,
                    focused_hint: None,
                )
            }
        }
        .into_any(),
    );
}

fn panel_title(panel: &PanelProjection) -> String {
    if panel.focused {
        format!(" ▶ {} ", panel.title)
    } else {
        format!(" {} ", panel.title)
    }
}

/// The panel's border set: double lines for the focused pane, rounded for
/// every other pane, mirroring the retired split-screen presentation.
fn panel_border_style(focused: bool) -> BorderStyle {
    if focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    }
}

/// Render the retained workbench card grid inside its panel content rect.
///
/// Mirrors the retired split-screen composition: card rows of
/// [`WorkbenchCard`] components, a dim "Page X of Y" line when paging, or
/// the empty-state reason. The viewport is the panel's own content
/// rectangle, so the grid lays out for the space it actually gets.
fn render_workbench_cards(
    panel: &PanelProjection,
    state: &AppState,
    props: &ProviderScreenProps,
    rc: &ResolvedColors,
    children: &mut Vec<AnyElement<'static>>,
) {
    let content = panel.content;
    let view =
        crate::provider_panel_view::workbench_view_from_state(state, content.width, content.height);

    if let Some(reason) = &view.empty_reason {
        children.push(absolute_text(content, reason.clone(), rc.dim));
        return;
    }

    let columns = view.layout.columns.max(1);
    let selected = state.selected_agent().map(|agent| agent.id.clone());
    let mut grid: Vec<_> = view
        .cards
        .chunks(columns)
        .map(|chunk| {
            workbench_card_row(
                chunk,
                view.layout.card_width,
                view.layout.todo_window,
                selected.as_ref(),
                &props.colors,
            )
        })
        .collect();
    if view.layout.page_count > 1 {
        grid.push(absolute_text(
            content,
            format!(
                " Page {} of {}",
                view.layout.page + 1,
                view.layout.page_count
            ),
            rc.dim,
        ));
    }
    children.push(
        element! {
            Box(
                position: Position::Absolute,
                left: u32::from(content.col),
                top: u32::from(content.row),
                width: u32::from(content.width),
                height: u32::from(content.height),
                flex_direction: FlexDirection::Column,
                background_color: rc.bg,
            ) {
                #(grid)
            }
        }
        .into_any(),
    );
}

/// One row-major grid row of [`WorkbenchCard`] components.
fn workbench_card_row(
    chunk: &[crate::workbench_view::WorkbenchCard],
    card_width: usize,
    todo_window: usize,
    selected: Option<&crate::domain::AgentId>,
    colors: &ThemeColors,
) -> AnyElement<'static> {
    let cards: Vec<_> = chunk
        .iter()
        .map(|card| {
            let is_selected = selected.is_some_and(|id| *id == card.agent_id);
            element! {
                WorkbenchCard(
                    card: Some(card.clone()),
                    card_width: card_width,
                    todo_window: todo_window,
                    selected: is_selected,
                    colors: colors.clone(),
                )
            }
            .into_any()
        })
        .collect();
    element! {
        Box(
            flex_direction: FlexDirection::Row,
            width: 100pct,
            background_color: ResolvedColors::from_theme(Some(colors)).bg,
        ) {
            #(cards)
        }
    }
    .into_any()
}

fn fitted_panel_title(panel: &PanelProjection, chrome: Rect) -> String {
    crate::ui::util::truncate_with_ellipsis(
        &panel_title(panel),
        usize::from(chrome.width.saturating_sub(2)),
    )
}

fn panel_title_rect(chrome: Rect, title: &str) -> Rect {
    let width = u16::try_from(UnicodeWidthStr::width(title))
        .unwrap_or(u16::MAX)
        .min(chrome.width.saturating_sub(2));
    Rect {
        col: chrome.col.saturating_add(1),
        row: chrome.row,
        width,
        height: u16::from(width > 0),
    }
}

fn absolute_text(rect: Rect, content: String, color: Color) -> AnyElement<'static> {
    element! {
        Box(
            position: Position::Absolute,
            left: u32::from(rect.col),
            top: u32::from(rect.row),
            width: u32::from(rect.width),
            height: u32::from(rect.height),
        ) {
            Text(content: content, color: color, wrap: TextWrap::NoWrap)
        }
    }
    .into_any()
}

fn projection_error_screen(message: String, rc: &ResolvedColors) -> AnyElement<'static> {
    element! {
        Box(
            position: Position::Relative,
            width: 100pct,
            height: 100pct,
            background_color: rc.bg,
        ) {
            Text(content: message, color: rc.error, wrap: TextWrap::Wrap)
        }
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_panel_view::{PanelHitTarget, PanelStatus};
    use crate::workbench::PanelId;

    fn projection(focused: bool, chrome: Rect, content: Rect) -> PanelProjection {
        PanelProjection {
            id: PanelId::from_static("main"),
            title: "Main".to_owned(),
            visible: true,
            focused,
            chrome,
            content,
            status: PanelStatus::Active,
            lines: vec!["content".to_owned()],
            max_scroll_offset: 0,
            hit_targets: vec![None::<PanelHitTarget>],
            rect_hit_targets: Vec::new(),
            render: crate::provider_panel_view::PanelRender::Control,
        }
    }

    #[test]
    fn global_title_and_footer_use_the_resolved_screen_bands() {
        let outer = Rect::new(0, 1, 100, 28);

        let rects = global_chrome_rects(outer);

        assert_eq!(rects.title, Rect::new(0, 0, 100, 1));
        assert_eq!(rects.footer, Rect::new(0, 29, 100, 1));
        assert!(!rects.title.contains(0, outer.row));
        assert!(!rects.footer.contains(0, outer.row + outer.height - 1));
    }

    /// Issue #734: the pre-cutover `KeybindBar` right-aligned
    /// `pid:<pid> <commit>` on every screen it drew, and
    /// `pid-commit-corner.json` asserts the `pid:` prefix. The provider
    /// runtime's footer dropped it when the dashboard moved onto
    /// `global_chrome`.
    #[test]
    fn the_footer_right_aligns_the_process_identity_label() {
        let line = footer_line("q quit", "pid:4242 abc1234", 40);

        assert_eq!(line.len(), 40, "the band fills its resolved rectangle");
        assert!(
            line.starts_with("q quit"),
            "hints keep the left edge: {line}"
        );
        assert!(
            line.ends_with("pid:4242 abc1234"),
            "the identity keeps the right edge: {line}"
        );
    }

    /// A footer narrow enough that the two segments would collide keeps the
    /// hints, which are the actionable half.
    #[test]
    fn a_footer_too_narrow_for_both_segments_keeps_the_hints() {
        let line = footer_line("q quit | ? help", "pid:4242 abc1234", 16);

        assert_eq!(line, "q quit | ? help");
    }

    #[test]
    fn shared_header_preserves_status_semantics() {
        let rc = ResolvedColors::from_theme(Some(&ThemeColors::default()));
        let header = themed_header(
            "Screen",
            HeaderStatus {
                warning: Some("provider unavailable"),
                repository_count: 3,
                running_count: 1,
                agent_count: 2,
                error_count: 4,
                theme_name: "Nord",
                kennel_mode: true,
            },
            &rc,
        );

        // The three segments split the same words the legacy single-line
        // header carried; nothing may be dropped in the split.
        assert_eq!(
            header.left,
            format!("Screen (Kennel mode) - {}", crate::VERSION)
        );
        assert_eq!(header.center, "WARN: provider unavailable | 4 errors");
        assert_eq!(header.right, "Nord");
    }

    /// Issue #723 fix 1: the provider screen's top bar must carry the
    /// `StatusBar` band styling — `rc.border` background over `rc.bg` text —
    /// not the plain bright-on-terminal header text #715 left behind.
    #[test]
    fn themed_header_uses_the_status_bar_band_colors() {
        let rc = ResolvedColors::from_theme(Some(&ThemeColors::default()));

        let header = themed_header(
            "LLxprt Jefe",
            HeaderStatus {
                warning: None,
                repository_count: 3,
                running_count: 0,
                agent_count: 3,
                error_count: 0,
                theme_name: "Green Screen",
                kennel_mode: false,
            },
            &rc,
        );

        assert_eq!(header.background, rc.border);
        assert_eq!(header.text, rc.bg);
        assert_eq!(header.left, format!("LLxprt Jefe - {}", crate::VERSION));
        assert_eq!(header.center, "3 repos | 0/3 running");
        assert_eq!(header.right, "Green Screen");
    }

    /// Issue #723 fix 2: the focused pane draws the double-line border set;
    /// every other pane keeps the rounded border.
    #[test]
    fn panel_border_style_is_double_iff_focused() {
        assert_eq!(panel_border_style(true), BorderStyle::Double);
        assert_eq!(panel_border_style(false), BorderStyle::Round);
    }

    #[test]
    fn focused_panel_title_and_content_preserve_resolved_coordinates() {
        let chrome = Rect::new(2, 3, 20, 10);
        let content = Rect::new(3, 5, 18, 7);
        let panel = projection(true, chrome, content);
        let title = panel_title(&panel);

        assert_eq!(title, " ▶ Main ");
        assert_eq!(panel_title_rect(chrome, &title), Rect::new(3, 3, 8, 1));
        assert_eq!(panel.chrome, chrome);
        assert_eq!(panel.content, content);
        assert!(!panel_title_rect(chrome, &title).contains(content.col, content.row));
    }

    #[test]
    fn panel_title_is_clipped_inside_narrow_resolved_chrome() {
        let chrome = Rect::new(7, 4, 5, 3);
        let panel = projection(false, chrome, Rect::new(8, 5, 3, 1));
        let title = fitted_panel_title(&panel, chrome);

        assert_eq!(title, " M…");
        assert_eq!(panel_title_rect(chrome, &title), Rect::new(8, 4, 3, 1));
    }

    #[test]
    fn panel_title_uses_terminal_columns_for_wide_unicode() {
        let chrome = Rect::new(2, 3, 8, 4);
        let mut panel = projection(false, chrome, Rect::new(3, 4, 6, 2));
        panel.title = "界界界".to_owned();

        let title = fitted_panel_title(&panel, chrome);

        assert_eq!(title, " 界界…");
        assert_eq!(UnicodeWidthStr::width(title.as_str()), 6);
        assert_eq!(panel_title_rect(chrome, &title), Rect::new(3, 3, 6, 1));
    }

    #[test]
    fn missing_projection_inputs_are_explicit_errors() {
        let Err(missing_state) = projection_inputs(None) else {
            panic!("missing state must be rejected");
        };
        assert_eq!(missing_state, "provider screen state unavailable");

        let mut state = AppState::test_fixture();
        let Err(missing_layout) = projection_inputs(Some(&state)) else {
            panic!("missing layout must be rejected");
        };
        assert_eq!(missing_layout, "screen layout unavailable: core.dashboard");

        state.nav.current_mut().screen = crate::workbench::ScreenIdentity::Package(
            crate::workbench::PluginScreenId::from_static("vendor.missing"),
        );
        let Err(missing_descriptor) = projection_inputs(Some(&state)) else {
            panic!("missing descriptor must be rejected");
        };
        assert_eq!(
            missing_descriptor,
            "screen descriptor unavailable: vendor.missing"
        );
    }
}
