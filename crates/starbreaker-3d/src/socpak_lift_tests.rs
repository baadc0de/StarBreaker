//! `.soc` lift wiring (WP-P5) from the C2 Hercules lift's own entities
//! (`top_deck_c2.soc`, LIVE 4.10.2), trimmed to the attributes the exporter
//! reads: interaction links, parent attachments, path-node overrides and
//! traversal trees, tile item ports; and the root item-port duplicate of an
//! entity that occupies a port.
use super::*;

const PLATFORM: &str = "42d29922-9f0a-b30b-33a9-05929f61d2bb";
const FLOOR_000: &str = "45276da2-b394-114c-bd1d-65441c12c7a0";
const FLOOR_001: &str = "4bed6723-a0f2-97a7-4123-48d75b9a98ab";
const FLOOR_003: &str = "4418aa71-e2b5-cff1-eb52-4cc6e9958890";
const HATCH: &str = "405af200-ec86-04b3-b5d2-4eb715112589";
const INNER_PANEL: &str = "4e867e5a-7436-4974-761c-883613ff8998";
const INNER_GATE: &str = "4b931226-afcb-d0de-0a46-b7760215b5b4";

/// The C2 lift's thirteen entities as authored (geometry-less ones keep their
/// class GUID so the pipeline can resolve them).
pub(crate) fn c2_lift_document() -> String {
    format!(
        r#"<Entities>
  <Entity Name="CRUS_Hercules_Lift_Platform-001" Pos="-7.5,-3,-7.875" EntityClass="CRUS_Hercules_Lift_Platform" EntityClassGUID="ea08b529-6dba-4893-baca-b2412482a31e" EntityCryGUID="{PLATFORM}">
    <EntityLinks>
      <Link TargetCryGUID="{FLOOR_001}" Name="NewLink" />
      <Link TargetCryGUID="{INNER_PANEL}" Name="NewLink" />
      <Link TargetCryGUID="{INNER_GATE}" Name="NewLink" />
    </EntityLinks>
    <InteractionLinkController>
      <Link TargetCryGUID="{HATCH}" NAME="NewInteractionLink" SourceSlotName="Arrived at 1" TargetSlotName="Close" Version="1" />
      <Link TargetCryGUID="{HATCH}" NAME="NewInteractionLink" SourceSlotName="Arrived at 2" TargetSlotName="Close" Version="1" />
    </InteractionLinkController>
    <TileAttachmentParams ItemPortName="Port5_CRUS_Hercules_Lift_Platform-001[Top_Deck_C2_SETUP]" ItemPortIndex="5" />
  </Entity>
  <Entity Name="Elevator_Floor-000" Pos="-7.5,-3,-0.5" EntityClass="Elevator_Floor" EntityClassGUID="64b85850-0ee2-4397-aabb-6cb3ce2a7f6a" EntityCryGUID="{FLOOR_000}">
    <EntityLinks><Link TargetCryGUID="4edb736f-b03a-ee7b-e220-1df68f498194" Name="NewLink" /></EntityLinks>
    <InteractionLinkController />
    <PropertiesDataCore __fullyInstanced="0" __version="2">
      <SCItemElevatorPathNode __type="SCItemElevatorPathNodeParams">
        <gatewayParams __fullyInstanced="1" __baseType="ElevatorPathNodeGatewayParams" __type="ElevatorPathNodeGatewayParams" interaction="" />
      </SCItemElevatorPathNode>
      <specialWeakPointers>
        <SCItemElevatorPathNode>
          <weakPointer PointerLocalURL="doorOpenInteraction" StructComponentName="InteractableComponent" StructLocalURL="Interactable.SharedInteractions[0]" />
        </SCItemElevatorPathNode>
      </specialWeakPointers>
    </PropertiesDataCore>
  </Entity>
  <Entity Name="CRUS_Hercules_Elevator_Door-001" Pos="-6.0824999999999996,-3,-7.875" EntityClass="CRUS_Hercules_Elevator_Door" EntityClassGUID="a13a3784-175e-4dc0-a4ff-a81e09ce5431" EntityCryGUID="4376aaa1-530b-5d53-88fb-a81eea7ca6b2" />
  <Entity Name="CRUS_Hercules_Elevator_Buttons_Exterior-001" Pos="-7.375,-3,-13.5" EntityClass="CRUS_Hercules_Elevator_Buttons_Exterior" EntityClassGUID="646e5ad1-fbe2-4345-9934-f3623c463680" EntityCryGUID="472581fe-5b39-a869-4a7c-7d22444152bd">
    <InteractionLinkController>
      <Link TargetCryGUID="{FLOOR_003}" NAME="NewInteractionLink" SourceSlotName="Descend" TargetSlotName="DescentRequest" Version="1" />
      <Link TargetCryGUID="{HATCH}" NAME="NewInteractionLink" SourceSlotName="Descend" TargetSlotName="Open" Version="1" />
    </InteractionLinkController>
    <TileAttachmentParams ItemPortName="Port7_CRUS_Hercules_Elevator_Buttons_Exterior-001[Top_Deck_C2_SETUP]" ItemPortIndex="7" />
  </Entity>
  <Entity Name="CRUS_Hercules_Elevator_Door_Top-001" Pos="-6.082465,-3,-0.5" EntityClass="CRUS_Hercules_Elevator_Door_Top" EntityClassGUID="0196e46a-0c96-4da4-b230-dc9921242d13" EntityCryGUID="4edb736f-b03a-ee7b-e220-1df68f498194" />
  <Entity Name="CRUS_Hercules_Elevator_ButtonsCall-000" Pos="-5.7187999999999999,-1.7813000000000001,-7.875" EntityClass="CRUS_Hercules_Elevator_ButtonsCall" EntityClassGUID="4a218af8-2210-4e50-b88e-50f854508e01" EntityCryGUID="4d52b73d-3b67-9a88-213d-f4c8017747a4">
    <InteractionLinkController>
      <Link TargetCryGUID="{FLOOR_001}" NAME="NewInteractionLink" SourceSlotName="Descend" TargetSlotName="DescentRequest" Version="1" />
    </InteractionLinkController>
  </Entity>
  <Entity Name="Elevator_Floor-001" Pos="-7.5,-3,-7.875" EntityClass="Elevator_Floor" EntityClassGUID="64b85850-0ee2-4397-aabb-6cb3ce2a7f6a" EntityCryGUID="{FLOOR_001}">
    <EntityLinks>
      <Link TargetCryGUID="4376aaa1-530b-5d53-88fb-a81eea7ca6b2" Name="NewLink" />
      <Link TargetCryGUID="{FLOOR_000}" Name="NewLink" />
    </EntityLinks>
    <InteractionLinkController />
    <PropertiesDataCore __fullyInstanced="0" __version="2">
      <SCItemElevatorPathNode __type="SCItemElevatorPathNodeParams">
        <gatewayParams __fullyInstanced="1" __baseType="ElevatorPathNodeGatewayParams" __type="ElevatorPathNodeGatewayParams" interaction="" />
      </SCItemElevatorPathNode>
      <EntityTraversingTreeComponent __type="SEntityTraversingTreeParams">
        <TraversingNodes __fullyInstanced="0">
          <SEntityTraversingNodeBase __index="0" __type="SEntityTraversingNodeBase">
            <entityTraversingType __fullyInstanced="1" __baseType="SEntityTraversingNodeTypeParams" __type="SEntityTraversingNodeTypeZoneHostVehicle" RootVehicle="0" />
            <nextNodes __fullyInstanced="1" __arraySize="1">
              <SEntityTraversingNodeBase __index="0" __type="SEntityTraversingNodeBase" name="">
                <id __fullyInstanced="1" __baseType="SEntityTraversingNodeUniqueId" __type="&lt;NullPtr&gt;" />
                <entityTraversingType __fullyInstanced="1" __baseType="SEntityTraversingNodeTypeParams" __type="SEntityTraversingNodeTypeItemPort" itemPortName="hardpoint_Elevator_Exterior_Door" />
                <executeNodes __fullyInstanced="1" __arraySize="1">
                  <SEntityTraversingExecuteNodeBase __index="0" __fullyInstanced="1" __baseType="SEntityTraversingExecuteNodeBase" __type="SInteractionLinkExecuteCreateInteractionLinkNode" linkedInteractionName="Close" localInteraction="InteractableComponent.Interactable.SharedInteractions[5]" />
                </executeNodes>
                <nextNodes __fullyInstanced="1" __arraySize="0" />
              </SEntityTraversingNodeBase>
            </nextNodes>
          </SEntityTraversingNodeBase>
        </TraversingNodes>
      </EntityTraversingTreeComponent>
    </PropertiesDataCore>
  </Entity>
  <Entity Name="Door_Ship_Exterior_CRUS_Starlifter_Elevator_Door" Pos="-7.3749989999999999,-3,-9.7499979999999997" EntityClass="Door_Ship_Exterior_CRUS_Starlifter_Elevator_Door" EntityClassGUID="a9140cda-2dd0-416d-b164-b636aa8207fb" EntityCryGUID="{HATCH}">
    <PropertiesDataCore __fullyInstanced="0" __version="2">
      <InteractableComponent __type="SEntityInteractableParams">
        <Interactable __type="SInteractableParams">
          <InteractionPoints __fullyInstanced="0">
            <SInteractionPointParams __index="0" __type="SInteractionPointParams">
              <Interactions __fullyInstanced="1" __arraySize="0" />
            </SInteractionPointParams>
          </InteractionPoints>
        </Interactable>
      </InteractableComponent>
    </PropertiesDataCore>
  </Entity>
  <Entity Name="CRUS_Hercules_Elevator_Buttons_Exterior-000" Pos="-7.375,-3,-9.7004380000000001" EntityClass="CRUS_Hercules_Elevator_Buttons_Exterior" EntityClassGUID="646e5ad1-fbe2-4345-9934-f3623c463680" EntityCryGUID="49bd738e-4318-f2b7-487a-8fee884c3a93">
    <InteractionLinkController>
      <Link TargetCryGUID="{FLOOR_003}" NAME="NewInteractionLink" SourceSlotName="Descend" TargetSlotName="DescentRequest" Version="1" />
      <Link TargetCryGUID="{HATCH}" NAME="NewInteractionLink" SourceSlotName="Descend" TargetSlotName="Open" Version="1" />
    </InteractionLinkController>
    <TileAttachmentParams ItemPortName="Port10_CRUS_Hercules_Elevator_Buttons_Exterior-000[Top_Deck_C2_SETUP]" ItemPortIndex="A" />
  </Entity>
  <Entity Name="CRUS_Hercules_Elevator_ButtonsCall-002" Pos="-5.75,-1.75,-0.5" EntityClass="CRUS_Hercules_Elevator_ButtonsCall" EntityClassGUID="4a218af8-2210-4e50-b88e-50f854508e01" EntityCryGUID="49b375f1-d5fb-1ee4-5f46-9a8fcffce2b2">
    <InteractionLinkController>
      <Link TargetCryGUID="{FLOOR_000}" NAME="NewInteractionLink" SourceSlotName="Descend" TargetSlotName="DescentRequest" Version="1" />
    </InteractionLinkController>
  </Entity>
  <Entity Name="Elevator_Floor-003" Pos="-7.5,-3,-14.65" EntityClass="Elevator_Floor" EntityClassGUID="64b85850-0ee2-4397-aabb-6cb3ce2a7f6a" EntityCryGUID="{FLOOR_003}">
    <EntityLinks><Link TargetCryGUID="{FLOOR_001}" Name="NewLink" /></EntityLinks>
    <InteractionLinkController>
      <Link TargetCryGUID="{FLOOR_001}" NAME="NewInteractionLink" SourceSlotName="DoorOpen" TargetSlotName="DoorOpen" Version="1" />
    </InteractionLinkController>
    <PropertiesDataCore __fullyInstanced="0" __version="2">
      <SCItemElevatorPathNode __type="SCItemElevatorPathNodeParams">
        <gatewayParams __fullyInstanced="1" __baseType="ElevatorPathNodeGatewayParams" __type="ElevatorPathNodeGatewayParams" interaction="" />
        <accessibilityParams __fullyInstanced="1" __baseType="AccessibilityBaseParams" __type="AccessibilityExteriorZoneParams" accessibleTimingInTransit="0.5" />
      </SCItemElevatorPathNode>
      <EntityTraversingTreeComponent __type="SEntityTraversingTreeParams">
        <TraversingNodes __fullyInstanced="0">
          <SEntityTraversingNodeBase __index="0" __type="SEntityTraversingNodeBase">
            <entityTraversingType __fullyInstanced="1" __baseType="SEntityTraversingNodeTypeParams" __type="SEntityTraversingNodeTypeEntityLink" entityTag="00000000-0000-0000-0000-000000000000" />
            <nextNodes __fullyInstanced="1" __arraySize="1">
              <SEntityTraversingNodeBase __index="0" __type="SEntityTraversingNodeBase" name="">
                <entityTraversingType __fullyInstanced="1" __baseType="SEntityTraversingNodeTypeParams" __type="SEntityTraversingNodeTypeZoneHostVehicle" RootVehicle="0" />
                <executeNodes __fullyInstanced="1" __arraySize="0" />
                <nextNodes __fullyInstanced="1" __arraySize="1">
                  <SEntityTraversingNodeBase __index="0" __type="SEntityTraversingNodeBase" name="">
                    <entityTraversingType __fullyInstanced="1" __baseType="SEntityTraversingNodeTypeParams" __type="SEntityTraversingNodeTypeItemPort" itemPortName="hardpoint_Elevator_Exterior_Door" />
                    <executeNodes __fullyInstanced="1" __arraySize="1">
                      <SEntityTraversingExecuteNodeBase __index="0" __fullyInstanced="1" __baseType="SEntityTraversingExecuteNodeBase" __type="SInteractionLinkExecuteCreateInteractionLinkNode" linkedInteractionName="OpenSelf" localInteraction="InteractableComponent.Interactable.SharedInteractions[5]" />
                    </executeNodes>
                    <nextNodes __fullyInstanced="1" __arraySize="0" />
                  </SEntityTraversingNodeBase>
                </nextNodes>
              </SEntityTraversingNodeBase>
            </nextNodes>
          </SEntityTraversingNodeBase>
        </TraversingNodes>
      </EntityTraversingTreeComponent>
    </PropertiesDataCore>
  </Entity>
  <Entity Name="CRUS_Hercules_Elevator_ButtonsInner-002" ParentEntityCryGUID="{PLATFORM}" AttachmentType="CharacterBone" AttachmentTarget="Lift_Platform" ParentOffsetPos="0,0,0" ParentOffsetRot="1,-4.3711381e-08,-1.9106853e-15,-4.3711392e-08" ParentOffsetScale="1" Pos="-7.5,-3,-7.875" EntityClass="CRUS_Hercules_Elevator_ButtonsInner" EntityClassGUID="ca2d6f90-5516-4b2f-9a5c-49a38d0ae00d" EntityCryGUID="{INNER_PANEL}">
    <InteractionLinkController>
      <Link TargetCryGUID="{PLATFORM}" NAME="NewInteractionLink" SourceSlotName="RequestSurface" TargetSlotName="From Elevator call 0" Version="1" />
      <Link TargetCryGUID="{PLATFORM}" NAME="NewInteractionLink" SourceSlotName="RequestFloor0" TargetSlotName="From Elevator call 1" Version="1" />
      <Link TargetCryGUID="{PLATFORM}" NAME="NewInteractionLink" SourceSlotName="RequestFloor1" TargetSlotName="From Elevator call 2" Version="1" />
      <Link TargetCryGUID="{HATCH}" NAME="NewInteractionLink" SourceSlotName="RequestSurface" TargetSlotName="Open" Version="1" />
    </InteractionLinkController>
  </Entity>
  <Entity Name="CRUS_Hercules_Elevator_Inner_Gate-001" ParentEntityCryGUID="{PLATFORM}" AttachmentType="CharacterBone" AttachmentTarget="Lift_Platform" ParentOffsetPos="0,0,0" ParentOffsetRot="1,-4.3711381e-08,-1.9106853e-15,-4.3711392e-08" ParentOffsetScale="1" Pos="-7.5,-3,-7.875" EntityClass="CRUS_Hercules_Elevator_Inner_Gate" EntityClassGUID="16bd601d-fc4b-42d6-99f0-081fd780726e" EntityCryGUID="{INNER_GATE}">
    <InteractionLinkController />
  </Entity>
</Entities>"#
    )
}

fn meshes() -> Vec<InteriorMesh> {
    let bytes = crate::test_xml::encode(&c2_lift_document());
    extract_cryxml_entities(&starbreaker_cryxml::from_bytes(&bytes).unwrap()).0
}

fn by_id<'a>(meshes: &'a [InteriorMesh], id: &str) -> &'a InteriorMesh {
    meshes.iter().find(|m| m.entity.entity_id.as_deref() == Some(id)).unwrap_or_else(|| panic!("entity {id}"))
}

#[test]
fn every_lift_entity_is_kept_for_datacore_resolution() {
    // All carry a class GUID; the three path nodes and the two ground buttons have no inline geometry.
    let meshes = meshes();
    assert_eq!(meshes.len(), 13, "carriage, gate, floor panel, 3 stops, 2 landing doors, hatch, 2 call + 2 ground buttons");
    assert!(meshes.iter().all(|m| m.entity_class_guid.is_some() && m.entity.entity_id.is_some()));
}

#[test]
fn interaction_links_keep_slots_targets_and_authored_order() {
    let meshes = meshes();
    let panel = by_id(&meshes, INNER_PANEL);
    let slots: Vec<(&str, &str, &str)> = panel
        .entity
        .interaction_links
        .iter()
        .map(|l| (l.source_slot.as_str(), l.target_entity_id.as_str(), l.target_slot.as_str()))
        .collect();
    assert_eq!(
        slots,
        [
            ("RequestSurface", PLATFORM, "From Elevator call 0"),
            ("RequestFloor0", PLATFORM, "From Elevator call 1"),
            ("RequestFloor1", PLATFORM, "From Elevator call 2"),
            ("RequestSurface", HATCH, "Open"),
        ]
    );
    let platform = by_id(&meshes, PLATFORM);
    assert_eq!(platform.entity.interaction_links.len(), 2);
    assert_eq!(platform.entity.interaction_links[1].source_slot, "Arrived at 2");
    assert_eq!(platform.entity.interaction_links[1].target_slot, "Close");
    // 13 authored links; the brief's table lists 11 rows because both ground buttons carry the same two.
    let all: Vec<_> = meshes.iter().flat_map(|m| m.entity.interaction_links.iter()).collect();
    assert_eq!(all.len(), 13);
    let distinct: std::collections::BTreeSet<_> = meshes
        .iter()
        .flat_map(|m| {
            m.entity.interaction_links.iter().map(move |l| {
                (m.entity_class_guid.clone(), l.source_slot.clone(), l.target_entity_id.clone(), l.target_slot.clone())
            })
        })
        .collect();
    assert_eq!(distinct.len(), 11);
    assert!(by_id(&meshes, INNER_GATE).entity.interaction_links.is_empty(), "an empty controller has no links");
}

#[test]
fn gate_and_panel_ride_the_carriage_node() {
    let meshes = meshes();
    for id in [INNER_GATE, INNER_PANEL] {
        let parent = by_id(&meshes, id).entity.parent.clone().expect("attached");
        assert_eq!(parent.parent_entity_id, PLATFORM);
        assert_eq!(parent.attachment_type.as_deref(), Some("CharacterBone"));
        assert_eq!(parent.attachment_target.as_deref(), Some("Lift_Platform"));
        assert_eq!(parent.offset_position, [0.0; 3]);
        assert_eq!(parent.offset_scale, [1.0; 3], "a uniform scale is one value");
        assert!((parent.offset_rotation[0] - 1.0).abs() < 1e-9);
    }
    assert!(by_id(&meshes, PLATFORM).entity.parent.is_none());
}

#[test]
fn path_node_overrides_and_traversal_links_are_read() {
    let meshes = meshes();
    let ground = &by_id(&meshes, FLOOR_003).entity.overrides;
    assert_eq!(
        ground.accessibility,
        Some(crate::types::Accessibility {
            kind: "AccessibilityExteriorZoneParams".into(),
            accessible_timing_in_transit: Some(0.5)
        })
    );
    assert_eq!(ground.gateway_interaction, None, "an empty interaction is not an override");
    assert_eq!(ground.traversal_links.len(), 1);
    let link = &ground.traversal_links[0];
    assert_eq!(link.via, ["EntityLink", "ZoneHostVehicle", "ItemPort"]);
    assert_eq!(link.item_port_name.as_deref(), Some("hardpoint_Elevator_Exterior_Door"));
    assert_eq!(link.linked_interaction_name, "OpenSelf");
    assert_eq!(link.local_interaction_path, "InteractableComponent.Interactable.SharedInteractions[5]");
    let hold = &by_id(&meshes, FLOOR_001).entity.overrides;
    assert_eq!(hold.accessibility, None);
    assert_eq!(hold.traversal_links[0].linked_interaction_name, "Close");
    assert!(by_id(&meshes, FLOOR_000).entity.overrides.traversal_links.is_empty());
}

#[test]
fn instance_interaction_point_overrides_are_read_by_index() {
    // The belly hatch clears its class point 0 (an empty `Interactions` array); nothing else overrides points.
    let meshes = meshes();
    let hatch = &by_id(&meshes, HATCH).entity.overrides;
    assert_eq!(hatch.cleared_interaction_points, [0]);
    assert!(hatch.replaced_interaction_points.is_empty());
    assert!(by_id(&meshes, INNER_PANEL).entity.overrides.cleared_interaction_points.is_empty());
}

#[test]
fn tile_item_ports_and_names_are_read() {
    let meshes = meshes();
    let platform = by_id(&meshes, PLATFORM);
    assert_eq!(platform.entity.tile_port_name.as_deref(), Some("Port5_CRUS_Hercules_Lift_Platform-001[Top_Deck_C2_SETUP]"));
    assert_eq!(platform.entity.entity_name.as_deref(), Some("CRUS_Hercules_Lift_Platform-001"));
    assert_eq!(by_id(&meshes, FLOOR_001).entity.tile_port_name, None);
}

#[test]
fn door_speed_override_is_read_by_component_type() {
    // hangar_medfrnt_001: a LoadingPlatform instance authors `SCItemDoorParams DoorAnimationSpeed="1.2"`.
    let bytes = crate::test_xml::encode(
        r#"<Entities><Entity Name="pad" EntityClassGUID="11110000-0000-0000-0000-000000000001" EntityCryGUID="aaaa0000-0000-0000-0000-000000000001">
             <PropertiesDataCore><SCItemDoor __type="SCItemDoorParams" DoorAnimationSpeed="1.2" /></PropertiesDataCore>
           </Entity></Entities>"#,
    );
    let meshes = extract_cryxml_entities(&starbreaker_cryxml::from_bytes(&bytes).unwrap()).0;
    assert_eq!(meshes[0].entity.overrides.door_animation_speed, Some(1.2));
    assert_eq!(meshes[0].entity.overrides.use_auto_close_delay, None);
}

/// Root container `ItemPort`s describe the entities attached to them; an entity that names its port in
/// `TileAttachmentParams` is that port's occupant and, once placed, is not placed a second time from the port. The C2's
/// port 5 put a second, static carriage one metre below the hold (at the port's `interactionOffset`). A port whose
/// occupant was not placed keeps its placement.
#[test]
fn a_port_occupied_by_a_placed_soc_entity_is_not_placed_again() {
    let entities = meshes();
    let xml = r#"<ObjectContainer>
        <ItemPort interactionOffset="-7.5,-3,-8.875" name="Port5_CRUS_Hercules_Lift_Platform-001[Top_Deck_C2_SETUP]" rotation="1,0,0,0" />
        <ItemPort interactionOffset="1,2,3" name="Port6_Synthetic_Unclaimed-001[Top_Deck_C2_SETUP]" rotation="1,0,0,0" />
      </ObjectContainer>"#;
    let identity = glam::Mat4::IDENTITY.to_cols_array_2d();
    let ports = extract_item_port_meshes_from_text_xml(xml, identity, None);
    assert_eq!(ports.len(), 2);
    let placed: std::collections::HashSet<String> =
        entities.iter().filter_map(|m| m.entity.tile_port_name.clone()).collect();
    let kept: Vec<_> = ports.iter().filter(|p| !crate::pipeline::occupied_port(p, &placed)).collect();
    let names: Vec<_> = kept.iter().filter_map(|m| m.entity_class_name.as_deref()).collect();
    assert_eq!(names, ["Synthetic_Unclaimed"]);
    // The occupant was not placed (no geometry, an unresolved class): the port keeps its own placement.
    assert!(ports.iter().all(|p| !crate::pipeline::occupied_port(p, &Default::default())));
    // An entity is never mistaken for a port.
    assert!(entities.iter().all(|e| !crate::pipeline::occupied_port(e, &placed)));
}

/// A light attached to another entity (`ParentEntityCryGUID`) records it, so a consumer moves it with that entity (a
/// lift carriage); a free light records none.
#[test]
fn attached_lights_name_the_entity_they_ride() {
    let text = format!(
        r#"<Entities>
             <Entity EntityClass="LightGroupObjectAttach" Name="riding" ParentEntityCryGUID="{PLATFORM}" AttachmentType="CharacterBone" AttachmentTarget="Lift_Platform" ParentOffsetPos="0,0,0" ParentOffsetRot="1,0,0,0" ParentOffsetScale="1" Pos="-7.5,-3,-7.875">
               <EntityComponentLightGroup><BakedInLights><Light><EntityComponentLight lightType="Omni"><defaultState intensity="1"/><sizeParams lightRadius="5"/></EntityComponentLight><RelativeXForm rotation="1,0,0,0" translation="0,0,2"/></Light></BakedInLights></EntityComponentLightGroup>
             </Entity>
             <Entity EntityClass="Light" Name="free" Pos="1,2,3"><PropertiesDataCore><EntityComponentLight lightType="Omni"><defaultState intensity="1"/><sizeParams lightRadius="5"/></EntityComponentLight></PropertiesDataCore></Entity>
           </Entities>"#
    );
    let bytes = crate::test_xml::encode(&text);
    let lights = extract_cryxml_entities(&starbreaker_cryxml::from_bytes(&bytes).unwrap()).1;
    assert_eq!(lights.len(), 2);
    assert_eq!(lights[0].parent_entity_id.as_deref(), Some(PLATFORM));
    assert_eq!(lights[1].parent_entity_id, None);
    assert!(serde_json::to_value(&lights[1]).unwrap().get("parent_entity_id").is_none(), "omitted when free");
}
