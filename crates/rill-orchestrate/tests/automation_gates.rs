use rill_orchestrate::automation::{
    ActionDispatchError, ActionDispatcher, RillAction, RillEventKind, SplitDirection,
};

#[test]
fn t_aut_event_maturity_classifies_supported_surface() {
    assert_eq!(RillEventKind::WorkspaceOpened.maturity(), rill_orchestrate::automation::EventClass::V1);
    assert_eq!(RillEventKind::CommandCompleted.maturity(), rill_orchestrate::automation::EventClass::V1);
    assert_eq!(RillEventKind::AgentStarted.maturity(), rill_orchestrate::automation::EventClass::Future);
    assert_eq!(RillEventKind::AttentionCreated.maturity(), rill_orchestrate::automation::EventClass::Future);
}

#[test]
fn t_aut_action_validation_rejects_empty_values() {
    let invalid = RillAction::SetTabTitle {
        tab_id: "".into(),
        title: "   ".into(),
    };
    assert!(invalid.validate().is_err());

    let invalid = RillAction::ShowNotification {
        title: "alert".into(),
        body: "".into(),
    };
    assert!(invalid.validate().is_err());

    let valid = RillAction::SplitPane {
        pane_id: "pane-1".into(),
        direction: SplitDirection::Right,
        command: Some("cargo test".into()),
    };
    assert!(valid.validate().is_ok());
}

#[test]
fn t_aut_dispatcher_is_bounded_and_validates() {
    let mut dispatcher = ActionDispatcher::new(2);
    assert!(dispatcher
        .dispatch(RillAction::ShowNotification {
            title: "One".into(),
            body: "First".into(),
        })
        .is_ok());
    assert!(dispatcher
        .dispatch(RillAction::ShowNotification {
            title: "Two".into(),
            body: "Second".into(),
        })
        .is_ok());
    let err = dispatcher
        .dispatch(RillAction::ShowNotification {
            title: "Three".into(),
            body: "Third".into(),
        })
        .unwrap_err();
    assert!(matches!(err, ActionDispatchError::QueueFull { limit: 2 }));

    let invalid = RillAction::OpenWorkspace { path: "   ".into() };
    assert!(matches!(
        dispatcher.dispatch(invalid),
        Err(ActionDispatchError::Validation(_))
    ));
}
