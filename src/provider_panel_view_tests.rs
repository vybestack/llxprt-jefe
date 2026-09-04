include!("provider_panel_view_tests_core.rs");
include!("provider_panel_view_tests_native.rs");
include!("provider_panel_view_tests_host.rs");
include!("provider_panel_view_tests_workbench.rs");
include!("provider_panel_view_tests_interaction.rs");
include!("provider_panel_view_tests_model_pipeline.rs");
/// Which panel the focus authority marks focused (issue #731).
#[path = "provider_panel_view_focus_tests.rs"]
mod focus;
#[path = "provider_panel_view_tests_origins.rs"]
mod origins;
