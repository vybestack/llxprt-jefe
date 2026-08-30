//! Resolver tests over the shipped screens (issue #384, CW04-03, CW04-04,
//! CW04-07, CW04-08).

use super::descriptor::ScreenDescriptor;
use super::geometry::Rect;
use super::ids::{PanelId, ScreenInstanceId};
use super::resolve::{
    LayoutGeneration, PanelState, ResolvedLayout, pty_content_rect, repair_focus, resolve_layout,
};
use super::screens::{PTY_PANEL_TYPE, builtin_screens};

fn screens() -> Vec<ScreenDescriptor> {
    builtin_screens()
        .unwrap_or_else(|error| unreachable!("compiled screens are valid: {error}"))
        .screens()
        .to_vec()
}

fn resolve(descriptor: &ScreenDescriptor, cols: u16, rows: u16) -> ResolvedLayout {
    resolve_layout(
        descriptor,
        ScreenInstanceId::next(),
        Rect::new(0, 0, cols, rows),
        &PanelState::all_visible(),
    )
    .unwrap_or_else(|error| unreachable!("resolution must not fail: {error}"))
}

#[test]
fn every_screen_resolves_at_a_comfortable_size() {
    for descriptor in screens() {
        let layout = resolve(&descriptor, 120, 40);
        assert!(
            layout.too_small.is_none(),
            "screen {} must fit at 120x40",
            descriptor.id
        );
        assert!(
            layout.visible_panels().count() >= 2,
            "screen {} should show more than one panel at 120x40",
            descriptor.id
        );
    }
}

#[test]
fn every_declared_panel_appears_exactly_once_in_the_snapshot() {
    for descriptor in screens() {
        let layout = resolve(&descriptor, 120, 40);
        assert_eq!(layout.panels.len(), descriptor.panels.len());
        for panel in &descriptor.panels {
            let matches = layout
                .panels
                .iter()
                .filter(|resolved| resolved.id == panel.id)
                .count();
            assert_eq!(matches, 1, "screen {} panel {}", descriptor.id, panel.id);
        }
    }
}

#[test]
fn visible_panels_never_overlap_and_stay_inside_the_outer_rect() {
    for descriptor in screens() {
        for cols in [20_u16, 40, 80, 120, 200] {
            for rows in [10_u16, 24, 40, 60] {
                let layout = resolve(&descriptor, cols, rows);
                let visible: Vec<Rect> =
                    layout.visible_panels().map(|panel| panel.chrome).collect();
                for (index, rect) in visible.iter().enumerate() {
                    assert!(
                        rect.right() <= u32::from(cols) && rect.bottom() <= u32::from(rows),
                        "screen {} panel escapes {cols}x{rows}: {rect}",
                        descriptor.id
                    );
                    for other in &visible[index + 1..] {
                        assert!(
                            !rect.intersects(*other),
                            "screen {} overlapping panels at {cols}x{rows}: {rect} and {other}",
                            descriptor.id
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_hidden_panel_has_no_hit_content_or_chrome_region() {
    // Hiding is driven explicitly rather than by picking a size that happens to
    // collapse something, so every screen genuinely exercises the assertion.
    let mut asserted = 0_u32;
    for descriptor in screens() {
        for optional in descriptor.panels.iter().filter(|panel| !panel.required) {
            let state = PanelState::all_visible().hiding(&optional.id);
            let layout = resolve_layout(
                &descriptor,
                ScreenInstanceId::next(),
                Rect::new(0, 0, 120, 40),
                &state,
            )
            .unwrap_or_else(|error| unreachable!("resolution must not fail: {error}"));
            let Some(panel) = layout.panel(&optional.id) else {
                unreachable!("every declared panel is in the snapshot");
            };
            assert!(!panel.visible, "screen {}", descriptor.id);
            assert_eq!(panel.hit_region, None, "screen {}", descriptor.id);
            assert!(panel.content.is_empty(), "screen {}", descriptor.id);
            assert!(panel.chrome.is_empty(), "screen {}", descriptor.id);
            asserted += 1;
        }
    }
    assert!(
        asserted >= 10,
        "expected a broad sweep, asserted {asserted}"
    );
}

#[test]
fn a_visible_pty_panel_always_has_a_nonzero_content_rectangle() {
    for descriptor in screens() {
        let pty_panels: Vec<&PanelId> = descriptor
            .panels
            .iter()
            .filter(|panel| panel.panel_type.as_str() == PTY_PANEL_TYPE)
            .map(|panel| &panel.id)
            .collect();
        if pty_panels.is_empty() {
            continue;
        }
        for cols in 1_u16..=80 {
            for rows in 1_u16..=24 {
                let layout = resolve(&descriptor, cols, rows);
                for id in &pty_panels {
                    let Some(resolved) = layout.panel(id) else {
                        unreachable!("every declared panel is in the snapshot");
                    };
                    if resolved.visible {
                        assert!(
                            resolved.content.width >= 1 && resolved.content.height >= 1,
                            "screen {} pty panel {id} got a zero content rect at {cols}x{rows}",
                            descriptor.id
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_pty_resize_rectangle_is_never_zero_and_never_names_a_hidden_panel() {
    for descriptor in screens() {
        let pty_panels: Vec<&PanelId> = descriptor
            .panels
            .iter()
            .filter(|panel| panel.panel_type.as_str() == PTY_PANEL_TYPE)
            .map(|panel| &panel.id)
            .collect();
        for cols in 1_u16..=80 {
            for rows in 1_u16..=24 {
                let layout = resolve(&descriptor, cols, rows);
                for id in &pty_panels {
                    let Some(rect) = pty_content_rect(&descriptor, &layout, id) else {
                        continue;
                    };
                    assert!(
                        rect.width >= 1 && rect.height >= 1,
                        "screen {} produced a zero pty rect at {cols}x{rows}",
                        descriptor.id
                    );
                    assert_eq!(
                        layout.panel(id).map(|panel| panel.visible),
                        Some(true),
                        "screen {} named a hidden pty panel at {cols}x{rows}",
                        descriptor.id
                    );
                }
            }
        }
    }
}

#[test]
fn a_non_pty_panel_has_no_pty_rectangle() {
    for descriptor in screens() {
        let layout = resolve(&descriptor, 120, 40);
        for panel in &descriptor.panels {
            if panel.panel_type.as_str() == PTY_PANEL_TYPE {
                continue;
            }
            assert_eq!(
                pty_content_rect(&descriptor, &layout, &panel.id),
                None,
                "screen {} panel {}",
                descriptor.id,
                panel.id
            );
        }
    }
}

#[test]
fn the_too_small_fallback_shows_exactly_the_first_required_focusable_panel() {
    for descriptor in screens() {
        for cols in 1_u16..=80 {
            for rows in 1_u16..=24 {
                let layout = resolve(&descriptor, cols, rows);
                let Some(too_small) = layout.too_small else {
                    continue;
                };
                let visible: Vec<&str> = layout
                    .visible_panels()
                    .map(|panel| panel.id.as_str())
                    .collect();
                let expected = descriptor
                    .first_required_focusable()
                    .map(|panel| panel.id.as_str());
                assert_eq!(
                    visible,
                    expected.into_iter().collect::<Vec<&str>>(),
                    "screen {} at {cols}x{rows}",
                    descriptor.id
                );
                assert_eq!(too_small.available.cols, cols);
                assert_eq!(too_small.available.rows, rows);
            }
        }
    }
}

#[test]
fn a_degenerate_outer_rect_yields_no_visible_panel_and_never_panics() {
    for descriptor in screens() {
        for (cols, rows) in [(0_u16, 0_u16), (0, 40), (120, 0)] {
            let layout = resolve(&descriptor, cols, rows);
            assert_eq!(
                layout.visible_panels().count(),
                0,
                "screen {} at {cols}x{rows}",
                descriptor.id
            );
        }
    }
}

#[test]
fn resolution_is_deterministic_for_the_same_inputs() {
    for descriptor in screens() {
        for cols in [17_u16, 41, 79, 120] {
            for rows in [7_u16, 13, 24, 40] {
                let instance = ScreenInstanceId::next();
                let outer = Rect::new(0, 0, cols, rows);
                // Geometry is deterministic; only the frame identity advances.
                let first =
                    resolve_layout(&descriptor, instance, outer, &PanelState::all_visible());
                let second =
                    resolve_layout(&descriptor, instance, outer, &PanelState::all_visible());
                let (mut first, mut second) = (
                    first.expect("resolution must not fail"),
                    second.expect("resolution must not fail"),
                );
                assert_ne!(
                    first.generation, second.generation,
                    "each commit receives a fresh LayoutGeneration ({}x{}); only its identity may differ",
                    descriptor.id, rows
                );
                first.generation = LayoutGeneration::zero();
                second.generation = LayoutGeneration::zero();
                assert_eq!(first, second, "screen {} at {cols}x{rows}", descriptor.id);
            }
        }
    }
}

#[test]
fn every_resolved_panel_carries_the_snapshot_identity() {
    for descriptor in screens() {
        let instance = ScreenInstanceId::next();
        let layout = resolve_layout(
            &descriptor,
            instance,
            Rect::new(0, 0, 120, 40),
            &PanelState::all_visible(),
        )
        .unwrap_or_else(|error| unreachable!("resolution must not fail: {error}"));
        assert_eq!(layout.screen_instance, instance);
    }
}

#[test]
fn any_application_hidden_panel_is_absent_from_the_snapshot() {
    for descriptor in screens() {
        for optional in descriptor.panels.iter().filter(|panel| !panel.required) {
            let state = PanelState::all_visible().hiding(&optional.id);
            let layout = resolve_layout(
                &descriptor,
                ScreenInstanceId::next(),
                Rect::new(0, 0, 120, 40),
                &state,
            )
            .unwrap_or_else(|error| unreachable!("resolution must not fail: {error}"));
            assert_eq!(
                layout.panel(&optional.id).map(|panel| panel.visible),
                Some(false),
                "screen {} panel {}",
                descriptor.id,
                optional.id
            );
        }
    }
}

#[test]
fn hit_testing_finds_the_panel_that_owns_a_cell() {
    for descriptor in screens() {
        let layout = resolve(&descriptor, 120, 40);
        for panel in layout.visible_panels() {
            let found = layout.panel_at(panel.chrome.col, panel.chrome.row);
            assert_eq!(
                found.map(|hit| &hit.id),
                Some(&panel.id),
                "screen {} panel {}",
                descriptor.id,
                panel.id
            );
        }
        assert!(layout.panel_at(200, 200).is_none());
    }
}

#[test]
fn focus_stays_on_a_visible_panel_when_one_is_visible() {
    for descriptor in screens() {
        let layout = resolve(&descriptor, 120, 40);
        for prior in &descriptor.focus_order {
            let repaired = repair_focus(&descriptor, &layout, Some(prior));
            let Some(repaired) = repaired else {
                unreachable!("a comfortable layout always has a focusable panel");
            };
            assert_eq!(&repaired, prior, "screen {}", descriptor.id);
        }
    }
}

#[test]
fn focus_advances_from_a_hidden_panel_to_the_next_visible_one_cyclically() {
    for descriptor in screens() {
        // Force collapses, then check every prior focus lands somewhere visible.
        let layout = resolve(&descriptor, 30, 8);
        if layout.visible_panels().count() == 0 {
            continue;
        }
        for prior in &descriptor.focus_order {
            let Some(repaired) = repair_focus(&descriptor, &layout, Some(prior)) else {
                unreachable!("a layout with a visible panel always repairs focus");
            };
            assert_eq!(
                layout.panel(&repaired).map(|panel| panel.visible),
                Some(true),
                "screen {} repaired focus to a hidden panel",
                descriptor.id
            );
        }
    }
}

#[test]
fn focus_falls_back_to_the_initial_focus_when_no_prior_focus_exists() {
    for descriptor in screens() {
        let layout = resolve(&descriptor, 120, 40);
        assert_eq!(
            repair_focus(&descriptor, &layout, None),
            Some(descriptor.initial_focus),
            "screen {}",
            descriptor.id
        );
    }
}

#[test]
fn focus_repair_reports_nothing_when_no_panel_is_visible() {
    for descriptor in screens() {
        let layout = resolve(&descriptor, 0, 0);
        assert_eq!(
            repair_focus(&descriptor, &layout, None),
            None,
            "screen {}",
            descriptor.id
        );
    }
}

#[test]
fn the_too_small_notice_reports_a_need_greater_than_what_was_available() {
    let descriptors = screens();
    let Some(descriptor) = descriptors
        .iter()
        .find(|descriptor| descriptor.id.as_str() == "core.repositories")
    else {
        unreachable!("the repositories screen is compiled in");
    };
    let layout = resolve(descriptor, 10, 4);
    let Some(too_small) = layout.too_small else {
        unreachable!("10x4 cannot fit the repositories screen");
    };
    assert!(
        too_small.needed.cols > too_small.available.cols
            || too_small.needed.rows > too_small.available.rows,
        "the notice must state a shortfall, got {too_small:?}"
    );
}

#[test]
fn every_committed_frame_receives_a_monotonic_layout_generation() {
    let descriptors = screens();
    let Some(descriptor) = descriptors
        .iter()
        .find(|descriptor| descriptor.id.as_str() == "core.repositories")
    else {
        unreachable!("the repositories screen is compiled in");
    };
    let mut previous = None;
    for (cols, rows, hidden) in [
        (120, 40, None),
        (119, 40, None),
        (120, 39, None),
        (120, 40, Some(true)),
        (120, 40, None),
    ] {
        let state = match hidden {
            Some(true) => PanelState::all_visible().hiding(&descriptor.panels[0].id),
            _ => PanelState::all_visible(),
        };
        let generation = resolve_layout(
            descriptor,
            ScreenInstanceId::next(),
            Rect::new(0, 0, cols, rows),
            &state,
        )
        .unwrap_or_else(|error| unreachable!("resolution must not fail: {error}"))
        .generation;
        if let Some(previous) = previous {
            assert!(
                generation.raw() > previous,
                "every committed frame must advance the generation: {previous} -> {}",
                generation.raw()
            );
        }
        previous = Some(generation.raw());
    }
}

#[test]
fn a_fresh_instance_resolution_receives_a_new_layout_generation() {
    let descriptor = &screens()[0];
    let first = resolve(descriptor, 100, 30);
    let second = resolve(descriptor, 100, 30);
    assert_ne!(
        first.generation, second.generation,
        "two committed frames must not collapse to one generation, even for identical inputs"
    );
    assert_ne!(
        first.generation.raw(),
        0,
        "a committed frame is never generation zero"
    );
}

#[test]
fn a_declared_panel_frame_carries_the_committed_generation_and_instance() {
    let descriptors = screens();
    let Some(descriptor) = descriptors
        .iter()
        .find(|descriptor| descriptor.id.as_str() == "core.repositories")
    else {
        unreachable!("the repositories screen is compiled in");
    };
    let instance = ScreenInstanceId::next();
    let layout = resolve_layout(
        descriptor,
        instance,
        Rect::new(0, 0, 120, 40),
        &PanelState::all_visible(),
    )
    .unwrap_or_else(|error| unreachable!("resolution must not fail: {error}"));
    let panel = &descriptor.panels[0];
    let frame = layout
        .panel_frame(&panel.id)
        .unwrap_or_else(|| unreachable!("a declared panel always carries a frame"));
    assert_eq!(frame.generation, layout.generation);
    assert_eq!(frame.screen_instance, instance);
    assert_eq!(frame.panel, panel.id);
}

#[test]
fn a_hidden_panel_still_carries_its_frame() {
    let descriptors = screens();
    let Some(descriptor) = descriptors
        .iter()
        .find(|descriptor| descriptor.id.as_str() == "core.repositories")
    else {
        unreachable!("the repositories screen is compiled in");
    };
    let hidden_panel = &descriptor.panels[0];
    let layout = resolve_layout(
        descriptor,
        ScreenInstanceId::next(),
        Rect::new(0, 0, 120, 40),
        &PanelState::all_visible().hiding(&hidden_panel.id),
    )
    .unwrap_or_else(|error| unreachable!("resolution must not fail: {error}"));
    let frame = layout
        .panel_frame(&hidden_panel.id)
        .unwrap_or_else(|| unreachable!("a hidden panel still carries a frame"));
    assert_eq!(
        frame.generation, layout.generation,
        "deferral of a hidden panel's PTY work stays bound to the committed frame"
    );
}

#[test]
fn an_undeclared_panel_has_no_frame() {
    let descriptors = screens();
    let Some(descriptor) = descriptors
        .iter()
        .find(|descriptor| descriptor.id.as_str() == "core.repositories")
    else {
        unreachable!("the repositories screen is compiled in");
    };
    let Some(other) = descriptors
        .iter()
        .find(|candidate| candidate.id.as_str() != "core.repositories")
    else {
        unreachable!("more than one builtin screen is compiled in");
    };
    let foreign_panel = other
        .panels
        .iter()
        .find(|panel| descriptor.panel(&panel.id).is_none())
        .map(|panel| panel.id)
        .unwrap_or_else(|| unreachable!("screens do not share every panel id"));
    let layout = resolve(descriptor, 120, 40);
    assert!(
        layout.panel_frame(&foreign_panel).is_none(),
        "a panel this snapshot never declared has no frame to carry"
    );
}

#[test]
fn frames_from_separate_commits_differ_only_by_generation() {
    let descriptor = &screens()[0];
    let instance = ScreenInstanceId::next();
    let commit = || {
        resolve_layout(
            descriptor,
            instance,
            Rect::new(0, 0, 100, 30),
            &PanelState::all_visible(),
        )
        .unwrap_or_else(|error| unreachable!("resolution must not fail: {error}"))
    };
    let first = commit();
    let second = commit();
    let panel = &descriptor.panels[0];
    let first_frame = first
        .panel_frame(&panel.id)
        .unwrap_or_else(|| unreachable!("a declared panel always carries a frame"));
    let mut second_frame = second
        .panel_frame(&panel.id)
        .unwrap_or_else(|| unreachable!("a declared panel always carries a frame"));
    assert_ne!(
        first_frame, second_frame,
        "two committed frames must not hand a consumer one identity"
    );
    second_frame.generation = first_frame.generation;
    assert_eq!(
        first_frame, second_frame,
        "within one screen instance the generation is the only frame discriminator"
    );
}
