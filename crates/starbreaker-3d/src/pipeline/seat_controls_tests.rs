//! Synthetic vehicle XML for seat roles and authored dashboard links.
use super::*;

#[test]
fn seat_role_is_from_flight_priority_and_dashboard_follows_usable_tags() {
    let xml = crate::test_xml::encode(
        r#"<Vehicle><Parts>
        <Part name="alpha"><ItemPort><Types><Type type="Seat"/></Types><ControllerDef><UserDef><PriorityGroups>
            <PriorityGroup itemType="FlightController"/>
        </PriorityGroups></UserDef><UsableDef><PriorityGroups>
            <PriorityGroup itemType="SeatDashboard" defaultPriority="no_control">
              <tags tag="dash_a"><Priority value="exclusive_control"/></tags>
            </PriorityGroup>
        </PriorityGroups></UsableDef></ControllerDef></ItemPort></Part>
        <Part name="beta"><ItemPort><Types><Type type="Seat"/></Types><ControllerDef><UserDef><PriorityGroups>
            <PriorityGroup itemType="FlightController" defaultPriority="observe_only"/>
        </PriorityGroups></UserDef></ControllerDef></ItemPort></Part>
        <Part name="gamma"><ItemPort><Types><Type type="SeatDashboard"/></Types>
            <ControllerDef controllableTags="dash_a"/></ItemPort></Part>
        <Part name="delta"><ItemPort><Types><Type type="SeatDashboard"/></Types>
            <ControllerDef controllableTags="dash_b"/></ItemPort></Part>
    </Parts></Vehicle>"#,
    );
    let xml = starbreaker_cryxml::from_bytes(&xml).unwrap();
    let mut ports = BTreeMap::new();
    collect(&xml, xml.root(), &mut ports);
    let alpha = route(&ports, "ALPHA");
    assert_eq!(alpha.can_drive, Some(true));
    assert_eq!(alpha.dashboard_port.as_deref(), Some("gamma"));
    let beta = route(&ports, "beta");
    assert_eq!(beta.can_drive, Some(false));
    assert!(beta.dashboard_port.is_none());
    assert_eq!(route(&ports, "absent").can_drive, None);
}
