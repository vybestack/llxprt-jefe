use jefe::state::AppState;
use jefe::state::provider_view::ProviderViewProjection;

pub fn provider_projection(
    snapshot: &AppState,
    viewport_rows: u16,
) -> Option<ProviderViewProjection> {
    snapshot.provider_surface_projection(usize::from(viewport_rows))
}
