#[test]
fn live_event_uses_the_schema_retained_at_declaration() {
    let mut state = ProviderPanelState::default();
    let (panel, _) = list_panel(&mut state, &["known"], None);

    assert!(matches!(
        state.submit_live_event(panel, PanelEvent::Selected { id: id("known") },),
        Ok(EventOutcome::Event(_))
    ));
    assert!(matches!(
        state.submit_live_event(
            panel,
            PanelEvent::Action {
                id: id("undeclared"),
                arguments: TypedMap::new(),
            },
        ),
        Ok(EventOutcome::None)
    ));
}
