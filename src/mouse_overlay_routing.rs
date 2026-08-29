//! Mouse ownership for blocking declared and provider overlays.

pub use jefe::ui::orchestration::consume_blocking_overlay_mouse;

#[cfg(test)]
#[path = "mouse_overlay_routing_tests.rs"]
mod tests;
