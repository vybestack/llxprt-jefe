//! Bottom keybinding help bar.

use iocraft::prelude::*;

use crate::app::Screen;
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the keybinding bar.
#[derive(Default, Props)]
pub struct KeybindBarProps {
    /// Current screen to determine which keys to show.
    pub screen: Option<Screen>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Bottom-of-screen keybinding hints.
#[component]
pub fn KeybindBar(props: &KeybindBarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    let bindings = match props.screen.unwrap_or(Screen::Dashboard) {
        Screen::Dashboard => {
            " ^/v navigate  </> pane  r repo  a list  t terminal  s split  F12 focus/unfocus  k kill  d delete  l relaunch(dead)  q quit"
        }
        Screen::CommandPalette => " type to filter  ^/v navigate  enter select  esc close",
        Screen::Terminal => " F12 unfocus (only)  q quit",
        Screen::NewAgent => " esc cancel  enter launch (toy)  q quit",
        Screen::NewRepository => " esc cancel  enter create (toy)  q quit",
        Screen::EditAgent => " tab/shift-tab fields  esc cancel  enter save  q quit",
        Screen::EditRepository => " tab/shift-tab fields  esc cancel  enter save  q quit",
        Screen::Split => " a arm reorder  ↑/↓ move selected  enter unselect  m main+pty focus  esc main no pty focus",
    };

    element! {
        Box(
            width: 100pct,
            height: 1u32,
            background_color: rc.bg,
        ) {
            Text(content: bindings.to_owned(), color: rc.dim)
        }
    }
}
