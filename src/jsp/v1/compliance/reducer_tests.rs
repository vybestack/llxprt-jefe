//! Reference reducer unit tests.

#[cfg(test)]
mod tests {
    use super::super::reducer::*;
    use crate::domain::observation::{
        EventRecord, FieldState, NativeActivityState, NativeActivityValue, ObservationEvent,
        ObservationIdentity, OpaqueId, Provenance, TodoList, ToolCallValue, ToolLabel, ToolPhase,
        WaitReason,
    };
    use crate::jsp::Snapshot;
    use crate::jsp::v1::compliance::projection::{
        ActivityProjection, MessagePresence, ObservationHealth, ProjectionProvenance,
        ToolPhaseProjection, WaitProjection,
    };

    fn apply_ok(reducer: &mut ReferenceReducer, record: &EventRecord) {
        match reducer.apply_event(record) {
            Ok(()) => {}
            Err(e) => panic!("event must apply: {e:?}"),
        }
    }

    fn apply_err(reducer: &mut ReferenceReducer, record: &EventRecord) -> ReducerError {
        match reducer.apply_event(record) {
            Ok(()) => panic!("event must fail"),
            Err(e) => e,
        }
    }

    fn id(agent: &str, generation: u64, epoch: &str) -> ObservationIdentity {
        ObservationIdentity {
            agent_id: OpaqueId(agent.to_string()),
            lifecycle_generation: generation,
            source_epoch: OpaqueId(epoch.to_string()),
        }
    }

    fn minimal_snapshot(identity: ObservationIdentity, seq: u64) -> Snapshot {
        Snapshot {
            identity,
            source_sequence: seq,
            cursor: seq.saturating_sub(1),
            bridge_observed_ms: 0,
            native_session: crate::domain::observation::NativeSession {
                repository: crate::domain::observation::RepositoryRef("r".to_string()),
                path: crate::domain::observation::PathRef("/p".to_string()),
                agent_kind: crate::domain::observation::AgentKindLabel("llxprt".to_string()),
                pid: 1,
                display_name: crate::domain::observation::DisplayName("d".to_string()),
            },
            process_binding: FieldState::Unsupported,
            native_activity: FieldState::known(
                Provenance::Authoritative,
                NativeActivityValue {
                    state: NativeActivityState::Idle,
                },
            ),
            current_wait: FieldState::known(Provenance::Authoritative, None),
            current_turn: FieldState::Unsupported,
            todos: FieldState::Unsupported,
            last_displayed_assistant_message: FieldState::Unsupported,
            last_created_tool_call: FieldState::Unsupported,
            source_terminal_state: FieldState::Unsupported,
            source_error_state: FieldState::Unsupported,
        }
    }

    #[test]
    fn snapshot_atomically_replaces_state() {
        let mut reducer = ReferenceReducer::new();
        // minimal_snapshot(_, 5) sets source_sequence=5, cursor=4.
        // last_sequence must track cursor (4), not source_sequence (5).
        reducer.apply_snapshot(&minimal_snapshot(id("a", 1, "e"), 5));
        let p = reducer.projection();
        assert_eq!(p.agent_id, "a");
        assert_eq!(
            p.last_sequence, 4,
            "last_sequence tracks cursor, not source_sequence"
        );
        assert_eq!(p.activity, ActivityProjection::Idle);
    }

    #[test]
    fn snapshot_cursor_seeds_next_event_not_source_sequence() {
        // Regression for ordering semantics (issue 477 correction #2):
        // snapshot_full has source_sequence 42, cursor 41. The next event
        // must be exactly 42 (cursor + 1), not 43 (source_sequence + 1).
        let mut reducer = ReferenceReducer::new();
        let mut snap = minimal_snapshot(id("a", 1, "e"), 0);
        snap.source_sequence = 42;
        snap.cursor = 41;
        reducer.apply_snapshot(&snap);
        assert_eq!(
            reducer.projection().last_sequence,
            41,
            "last_sequence is seeded from cursor"
        );
        // Event 42 must apply contiguously.
        let event_42 = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 42,
            bridge_observed_ms: 0,
            event: ObservationEvent::ActivityChanged {
                state: NativeActivityState::Acting,
            },
        };
        apply_ok(&mut reducer, &event_42);
        assert_eq!(reducer.projection().activity, ActivityProjection::Acting);
        assert_eq!(reducer.projection().last_sequence, 42);
    }

    #[test]
    fn contiguous_event_applies_once() {
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(id("a", 1, "e"), 0));
        let record = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 1,
            bridge_observed_ms: 0,
            event: ObservationEvent::ActivityChanged {
                state: NativeActivityState::Acting,
            },
        };
        apply_ok(&mut reducer, &record);
        assert_eq!(reducer.projection().activity, ActivityProjection::Acting);
    }

    #[test]
    fn duplicate_sequence_is_noop() {
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(id("a", 1, "e"), 0));
        let record = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 1,
            bridge_observed_ms: 0,
            event: ObservationEvent::ActivityChanged {
                state: NativeActivityState::Acting,
            },
        };
        apply_ok(&mut reducer, &record);
        let dup = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 1,
            bridge_observed_ms: 0,
            event: ObservationEvent::ActivityChanged {
                state: NativeActivityState::Idle,
            },
        };
        apply_ok(&mut reducer, &dup);
        assert_eq!(
            reducer.projection().activity,
            ActivityProjection::Acting,
            "duplicate must not mutate"
        );
    }

    #[test]
    fn gap_is_rejected_and_marks_stale() {
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(id("a", 1, "e"), 0));
        let record = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 3,
            bridge_observed_ms: 0,
            event: ObservationEvent::ActivityChanged {
                state: NativeActivityState::Acting,
            },
        };
        let err = apply_err(&mut reducer, &record);
        assert!(matches!(err, ReducerError::Gap { .. }));
        assert_eq!(
            reducer.projection().activity,
            ActivityProjection::Idle,
            "no partial mutation on gap"
        );
        assert_eq!(
            reducer.projection().observation_health,
            ObservationHealth::Stale
        );
    }

    #[test]
    fn identity_mismatch_is_rejected() {
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(id("a", 1, "e1"), 0));
        let record = EventRecord {
            identity: id("a", 1, "e2"),
            source_sequence: 1,
            bridge_observed_ms: 0,
            event: ObservationEvent::ActivityChanged {
                state: NativeActivityState::Acting,
            },
        };
        let err = apply_err(&mut reducer, &record);
        assert!(matches!(err, ReducerError::IdentityMismatch));
        // Mismatched identity must not degrade the primary stream's health.
        assert_eq!(
            reducer.projection().observation_health,
            ObservationHealth::Live
        );
    }

    #[test]
    fn stale_todo_revision_is_ignored() {
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(id("a", 1, "e"), 0));
        let r1 = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 1,
            bridge_observed_ms: 0,
            event: ObservationEvent::TodosReplaced {
                todos: TodoList {
                    revision: 2,
                    items: vec![],
                },
            },
        };
        apply_ok(&mut reducer, &r1);
        let stale = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 2,
            bridge_observed_ms: 0,
            event: ObservationEvent::TodosReplaced {
                todos: TodoList {
                    revision: 1,
                    items: vec![],
                },
            },
        };
        apply_ok(&mut reducer, &stale);
        let p = reducer.projection();
        assert_eq!(p.todos_revision, Some(2), "stale revision ignored");
    }

    #[test]
    fn last_created_tool_is_headline() {
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(id("a", 1, "e"), 0));
        let create_a = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 1,
            bridge_observed_ms: 0,
            event: ObservationEvent::ToolCallCreated {
                tool: ToolCallValue {
                    label: ToolLabel("tool_a".to_string()),
                    phase: ToolPhase::Proposed,
                },
            },
        };
        apply_ok(&mut reducer, &create_a);
        let create_b = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 2,
            bridge_observed_ms: 0,
            event: ObservationEvent::ToolCallCreated {
                tool: ToolCallValue {
                    label: ToolLabel("tool_b".to_string()),
                    phase: ToolPhase::Proposed,
                },
            },
        };
        apply_ok(&mut reducer, &create_b);
        let phase_a = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 3,
            bridge_observed_ms: 0,
            event: ObservationEvent::ToolCallPhaseChanged {
                tool: ToolCallValue {
                    label: ToolLabel("tool_a".to_string()),
                    phase: ToolPhase::Executing,
                },
            },
        };
        apply_ok(&mut reducer, &phase_a);
        let p = reducer.projection();
        assert_eq!(p.tool_label.as_deref(), Some("tool_b"));
        assert_eq!(p.tool_phase, ToolPhaseProjection::Proposed);
    }

    #[test]
    fn wait_orthogonal_to_activity() {
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(id("a", 1, "e"), 0));
        let open = EventRecord {
            identity: id("a", 1, "e"),
            source_sequence: 1,
            bridge_observed_ms: 0,
            event: ObservationEvent::WaitOpened {
                reason: WaitReason::Question,
            },
        };
        apply_ok(&mut reducer, &open);
        let p = reducer.projection();
        assert_eq!(p.wait, WaitProjection::Question);
        // activity unchanged
        assert_eq!(p.activity, ActivityProjection::Idle);
    }

    fn activity_event(identity: ObservationIdentity, sequence: u64) -> EventRecord {
        EventRecord {
            identity,
            source_sequence: sequence,
            bridge_observed_ms: sequence,
            event: ObservationEvent::ActivityChanged {
                state: NativeActivityState::Acting,
            },
        }
    }

    #[test]
    fn snapshot_is_required_before_event_or_heartbeat() {
        let identity = id("a", 1, "e");
        let mut reducer = ReferenceReducer::new();
        assert_eq!(
            reducer.apply_event(&activity_event(identity.clone(), 1)),
            Err(ReducerError::SnapshotRequired)
        );
        let heartbeat = crate::domain::observation::HeartbeatRecord {
            identity,
            bridge_observed_ms: 1,
        };
        assert_eq!(
            reducer.apply_heartbeat(&heartbeat),
            Err(ReducerError::SnapshotRequired)
        );
    }

    #[test]
    fn gap_and_disconnect_block_documents_until_fresh_snapshot() {
        let identity = id("a", 1, "e");
        let mut reducer = ReferenceReducer::new();
        let snapshot = minimal_snapshot(identity.clone(), 0);
        reducer.apply_snapshot(&snapshot);
        assert!(matches!(
            reducer.apply_event(&activity_event(identity.clone(), 3)),
            Err(ReducerError::Gap {
                expected: 1,
                actual: 3
            })
        ));
        assert_eq!(
            reducer.apply_event(&activity_event(identity.clone(), 1)),
            Err(ReducerError::FreshSnapshotRequired)
        );
        let heartbeat = crate::domain::observation::HeartbeatRecord {
            identity: identity.clone(),
            bridge_observed_ms: 2,
        };
        assert_eq!(
            reducer.apply_heartbeat(&heartbeat),
            Err(ReducerError::FreshSnapshotRequired)
        );
        reducer.apply_snapshot(&snapshot);
        assert!(
            reducer
                .apply_event(&activity_event(identity.clone(), 1))
                .is_ok()
        );
        reducer.apply_disconnect(true);
        assert_eq!(
            reducer.apply_heartbeat(&heartbeat),
            Err(ReducerError::FreshSnapshotRequired)
        );
    }

    #[test]
    fn illegal_transition_is_atomic_and_does_not_consume_sequence() {
        let identity = id("a", 1, "e");
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(identity.clone(), 0));
        let illegal = EventRecord {
            identity: identity.clone(),
            source_sequence: 1,
            bridge_observed_ms: 1,
            event: ObservationEvent::WaitResolved,
        };
        assert!(matches!(
            reducer.apply_event(&illegal),
            Err(ReducerError::IllegalTransition {
                transition: "wait.resolved"
            })
        ));
        assert_eq!(reducer.projection().last_sequence, 0);
        assert!(reducer.apply_event(&activity_event(identity, 1)).is_ok());
        assert_eq!(reducer.projection().last_sequence, 1);
    }

    #[test]
    fn stale_todo_preserves_projection_while_consuming_sequence() {
        let identity = id("a", 1, "e");
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(identity.clone(), 0));
        let current = EventRecord {
            identity: identity.clone(),
            source_sequence: 1,
            bridge_observed_ms: 1,
            event: ObservationEvent::TodosReplaced {
                todos: TodoList {
                    revision: 2,
                    items: vec![],
                },
            },
        };
        apply_ok(&mut reducer, &current);
        let before = reducer.projection();
        let stale = EventRecord {
            identity,
            source_sequence: 2,
            bridge_observed_ms: 2,
            event: ObservationEvent::TodosReplaced {
                todos: TodoList {
                    revision: 1,
                    items: vec![],
                },
            },
        };
        apply_ok(&mut reducer, &stale);
        let after = reducer.projection();
        assert_eq!(after.todos_state, before.todos_state);
        assert_eq!(after.todos_revision, before.todos_revision);
        assert_eq!(after.todos_count, before.todos_count);
        assert_eq!(after.last_sequence, 2);
    }

    #[test]
    fn source_error_and_session_end_project_without_payload() {
        let identity = id("a", 1, "e");
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(identity.clone(), 0));
        let error = EventRecord {
            identity: identity.clone(),
            source_sequence: 1,
            bridge_observed_ms: 1,
            event: ObservationEvent::SourceError {
                error: crate::domain::observation::SourceErrorValue {
                    summary: crate::domain::observation::DiagnosticSummary("secret".to_string()),
                    code: crate::domain::observation::BoundedText("token".to_string()),
                },
            },
        };
        apply_ok(&mut reducer, &error);
        let ended = EventRecord {
            identity,
            source_sequence: 2,
            bridge_observed_ms: 2,
            event: ObservationEvent::SessionEnded,
        };
        apply_ok(&mut reducer, &ended);
        let projection = reducer.projection();
        assert_eq!(projection.source_error, MessagePresence::Present);
        assert_eq!(
            projection.source_error_provenance,
            ProjectionProvenance::Authoritative
        );
        assert!(projection.session_ended);
        let json = serde_json::to_string(&projection)
            .unwrap_or_else(|error| panic!("serialize projection: {error}"));
        assert!(!json.contains("secret") && !json.contains("token"));
    }

    #[test]
    fn identity_error_is_payload_free_and_lifecycle_snapshot_resets_liveness() {
        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&minimal_snapshot(
            id("private-agent", 1, "private-epoch"),
            0,
        ));
        reducer.set_process_alive(true);
        let error = apply_err(&mut reducer, &activity_event(id("attacker", 1, "token"), 1));
        let diagnostic = error.to_string();
        assert_eq!(diagnostic, "JSP-R-IDENTITY: stream identity mismatch");
        assert!(!diagnostic.contains("attacker") && !diagnostic.contains("token"));
        reducer.apply_snapshot(&minimal_snapshot(id("new-agent", 2, "new-epoch"), 0));
        assert_eq!(reducer.projection().process_alive, None);
    }

    #[test]
    fn epoch_refresh_preserves_liveness_but_process_or_generation_change_resets_it() {
        let mut reducer = ReferenceReducer::new();
        let mut snapshot = minimal_snapshot(id("a", 1, "e1"), 0);
        snapshot.process_binding = FieldState::known(
            Provenance::Authoritative,
            crate::domain::observation::ProcessBinding {
                pid: 1,
                started_at_ms: 10,
            },
        );
        reducer.apply_snapshot(&snapshot);
        reducer.set_process_alive(true);
        snapshot.identity.source_epoch = OpaqueId("e2".to_string());
        reducer.apply_snapshot(&snapshot);
        assert_eq!(reducer.projection().process_alive, Some(true));

        let mut changed_process = snapshot.clone();
        changed_process.process_binding = FieldState::known(
            Provenance::Authoritative,
            crate::domain::observation::ProcessBinding {
                pid: 2,
                started_at_ms: 20,
            },
        );
        reducer.apply_snapshot(&changed_process);
        assert_eq!(reducer.projection().process_alive, None);
        reducer.set_process_alive(false);
        changed_process.identity.lifecycle_generation = 2;
        reducer.apply_snapshot(&changed_process);
        assert_eq!(reducer.projection().process_alive, None);
    }

    #[test]
    fn event_provenance_matches_fresh_snapshot_and_authoritative_transitions() {
        let identity = id("a", 1, "e");
        let displayed = crate::domain::observation::DisplayedAssistantMessage {
            content: crate::domain::observation::BoundedText("message".to_string()),
            committed_ms: 1,
        };
        let mut event_reducer = ReferenceReducer::new();
        event_reducer.apply_snapshot(&minimal_snapshot(identity.clone(), 0));
        apply_ok(
            &mut event_reducer,
            &EventRecord {
                identity: identity.clone(),
                source_sequence: 1,
                bridge_observed_ms: 1,
                event: ObservationEvent::AssistantMessageDisplayed {
                    message: displayed.clone(),
                },
            },
        );
        let mut equivalent = minimal_snapshot(identity.clone(), 0);
        equivalent.cursor = 1;
        equivalent.last_displayed_assistant_message =
            FieldState::known(Provenance::Inferred, displayed);
        let mut snapshot_reducer = ReferenceReducer::new();
        snapshot_reducer.apply_snapshot(&equivalent);
        assert_eq!(event_reducer.projection(), snapshot_reducer.projection());
    }

    #[test]
    fn turn_end_and_tool_events_set_authoritative_provenance() {
        let identity = id("a", 1, "e");
        let mut lifecycle = ReferenceReducer::new();
        lifecycle.apply_snapshot(&minimal_snapshot(identity.clone(), 0));
        let events = [
            ObservationEvent::TurnStarted,
            ObservationEvent::TurnEnded {
                outcome: crate::domain::observation::TurnOutcome::Completed,
            },
            ObservationEvent::ToolCallCreated {
                tool: ToolCallValue {
                    label: ToolLabel("tool".to_string()),
                    phase: ToolPhase::Executing,
                },
            },
        ];
        for (index, event) in events.into_iter().enumerate() {
            let sequence = u64::try_from(index).unwrap_or(0).saturating_add(1);
            apply_ok(
                &mut lifecycle,
                &EventRecord {
                    identity: identity.clone(),
                    source_sequence: sequence,
                    bridge_observed_ms: sequence,
                    event,
                },
            );
        }
        let projection = lifecycle.projection();
        assert_eq!(
            projection.turn_provenance,
            ProjectionProvenance::Authoritative
        );
        assert_eq!(
            projection.tool_provenance,
            ProjectionProvenance::Authoritative
        );
    }

    #[test]
    fn turn_ended_clears_turn_without_synthesizing_activity() {
        let identity = id("a", 1, "e");
        let mut events = ReferenceReducer::new();
        let mut acting = minimal_snapshot(identity.clone(), 0);
        acting.native_activity = FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Acting,
            },
        );
        acting.current_turn = FieldState::known(
            Provenance::Authoritative,
            Some(crate::domain::observation::CurrentTurn { elapsed_ms: 10 }),
        );
        events.apply_snapshot(&acting);
        apply_ok(
            &mut events,
            &EventRecord {
                identity: identity.clone(),
                source_sequence: 1,
                bridge_observed_ms: 1,
                event: ObservationEvent::TurnEnded {
                    outcome: crate::domain::observation::TurnOutcome::Completed,
                },
            },
        );

        // Ending a turn clears the turn and nothing else. Activity is
        // authoritative from the producer, so it must still read Acting until
        // the producer itself reports otherwise; inferring idle here would
        // report a state the source never claimed.
        let mut equivalent = minimal_snapshot(identity, 0);
        equivalent.cursor = 1;
        equivalent.current_turn = FieldState::known(Provenance::Authoritative, None);
        equivalent.native_activity = FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Acting,
            },
        );
        let mut snapshot = ReferenceReducer::new();
        snapshot.apply_snapshot(&equivalent);
        assert_eq!(
            events.observation().activity,
            snapshot.observation().activity
        );
        assert_eq!(events.observation().turn, snapshot.observation().turn);
        assert_eq!(
            events.observation().activity,
            FieldState::known(
                Provenance::Authoritative,
                NativeActivityValue {
                    state: NativeActivityState::Acting,
                },
            )
        );
    }

    #[test]
    fn terminal_lifecycle_rejections_are_atomic() {
        let identity = id("a", 1, "e");
        let mut tool = ReferenceReducer::new();
        tool.apply_snapshot(&minimal_snapshot(identity.clone(), 0));
        for (sequence, phase) in [(1, ToolPhase::Proposed), (2, ToolPhase::Succeeded)] {
            apply_ok(
                &mut tool,
                &EventRecord {
                    identity: identity.clone(),
                    source_sequence: sequence,
                    bridge_observed_ms: sequence,
                    event: if sequence == 1 {
                        ObservationEvent::ToolCallCreated {
                            tool: ToolCallValue {
                                label: ToolLabel("tool".to_string()),
                                phase,
                            },
                        }
                    } else {
                        ObservationEvent::ToolCallPhaseChanged {
                            tool: ToolCallValue {
                                label: ToolLabel("tool".to_string()),
                                phase,
                            },
                        }
                    },
                },
            );
        }
        let before = tool.projection();
        let regression = EventRecord {
            identity: identity.clone(),
            source_sequence: 3,
            bridge_observed_ms: 3,
            event: ObservationEvent::ToolCallPhaseChanged {
                tool: ToolCallValue {
                    label: ToolLabel("tool".to_string()),
                    phase: ToolPhase::Executing,
                },
            },
        };
        assert!(matches!(
            tool.apply_event(&regression),
            Err(ReducerError::IllegalTransition { .. })
        ));
        assert_eq!(tool.projection(), before);

        let ended = EventRecord {
            identity: identity.clone(),
            source_sequence: 3,
            bridge_observed_ms: 3,
            event: ObservationEvent::SessionEnded,
        };
        apply_ok(&mut tool, &ended);
        let ended_projection = tool.projection();
        assert!(matches!(
            tool.apply_event(&activity_event(identity, 4)),
            Err(ReducerError::IllegalTransition { .. })
        ));
        assert_eq!(tool.projection(), ended_projection);
    }

    #[test]
    fn production_reducer_preserves_preview_payloads() {
        let identity = id("a", 1, "e");
        let mut snapshot = minimal_snapshot(identity, 0);
        snapshot.todos = FieldState::known(
            Provenance::Authoritative,
            TodoList {
                revision: 1,
                items: vec![crate::domain::observation::TodoItem {
                    text: crate::domain::observation::BoundedText(
                        "Implement issue 522".to_string(),
                    ),
                    state: crate::domain::observation::TodoState::InProgress,
                }],
            },
        );
        snapshot.last_displayed_assistant_message = FieldState::known(
            Provenance::Authoritative,
            crate::domain::observation::DisplayedAssistantMessage {
                content: crate::domain::observation::BoundedText(
                    "JSP preview is wired".to_string(),
                ),
                committed_ms: 9,
            },
        );

        let mut reducer = ReferenceReducer::new();
        reducer.apply_snapshot(&snapshot);
        let observation = reducer.observation();

        let FieldState::Supported {
            availability: crate::domain::observation::Availability::Known(todos),
            ..
        } = &observation.todos
        else {
            panic!("todos must remain payload-preserving");
        };
        assert_eq!(todos.items[0].text.as_str(), "Implement issue 522");
        let FieldState::Supported {
            availability: crate::domain::observation::Availability::Known(message),
            ..
        } = &observation.last_message
        else {
            panic!("message must remain payload-preserving");
        };
        assert_eq!(message.content.as_str(), "JSP preview is wired");
    }
}
