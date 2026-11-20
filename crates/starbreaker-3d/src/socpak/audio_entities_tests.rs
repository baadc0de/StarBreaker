//! Placed audio on a synthetic `.soc` chunk: a triggered spot with two extra positions, an untriggered spot, an area
//! with its shape and governed spots, and an entity of an unrelated class.

use super::*;

const SOC: &str = r#"<Entities>
  <Entity Name="Spot_A" Pos="1,2,3" EntityClass="Synthetic" EntityClassGUID="AAAA0000-0000-0000-0000-000000000001" EntityCryGUID="10000000-0000-0000-0000-000000000001">
    <EntityLinks>
      <Link TargetCryGUID="10000000-0000-0000-0000-000000000002" Name="NewLink" />
      <Link TargetCryGUID="10000000-0000-0000-0000-000000000003" Name="NewLink" />
    </EntityLinks>
    <PropertiesDataCore>
      <AudioTriggerSpot __type="AudioTriggerSpotParams" volume="5" pitchRandomRange="144" attenuationScalingFactor="1.5">
        <audioTriggerPlayTriggerName __type="GlobalResourceAudio" audioTrigger="Play_Synthetic_Hum" />
      </AudioTriggerSpot>
    </PropertiesDataCore>
    <AudioPropagationComponent staticApcRoom="20000000-0000-0000-0000-00000000000A" />
  </Entity>
  <Entity Name="Spot_B" Pos="4,5,6" EntityCryGUID="10000000-0000-0000-0000-000000000002">
    <PropertiesDataCore>
      <AudioTriggerSpot __type="AudioTriggerSpotParams" volume="1" />
    </PropertiesDataCore>
  </Entity>
  <Entity Name="Spot_C" Pos="7,8,9" EntityCryGUID="10000000-0000-0000-0000-000000000003">
    <PropertiesDataCore />
  </Entity>
  <Entity Name="Area" Pos="0,0,0" EntityCryGUID="30000000-0000-0000-0000-000000000001">
    <EntityLinks><Link TargetCryGUID="10000000-0000-0000-0000-000000000001" Name="NewLink" /></EntityLinks>
    <PropertiesDataCore>
      <AudioAreaAmbience __type="AudioAreaAmbienceComponentParams" volume="2" atsVolumeOffset="-6">
        <audioPlayTrigger __type="GlobalResourceAudio" audioTrigger="Play_Synthetic_Bed" />
      </AudioAreaAmbience>
    </PropertiesDataCore>
  </Entity>
  <Entity Name="AS_Area" Pos="10,0,0" EntityCryGUID="40000000-0000-0000-0000-000000000001">
    <AreaComponent Id="0">
      <Entities><Entity EntityCryGUID="30000000-0000-0000-0000-000000000001" /></Entities>
      <Segment index="0" transform_ls="Q : 1,0,0,0 | T : 0,0,0" Type="Shape" Height="4.5">
        <Points><Point Pos="0,0,0" /><Point Pos="5,0,0" /><Point Pos="5,5,0" /></Points>
      </Segment>
    </AreaComponent>
  </Entity>
  <Entity Name="Unrelated" Pos="0,0,0" EntityCryGUID="50000000-0000-0000-0000-000000000001">
    <PropertiesDataCore><Other __type="SomethingElseParams" volume="9" /></PropertiesDataCore>
  </Entity>
</Entities>"#;

#[test]
fn spots_areas_extra_positions_and_shapes() {
    let bytes = crate::test_xml::encode(SOC);
    let xml = starbreaker_cryxml::from_bytes(&bytes).unwrap();
    let (spots, areas, untriggered) = extract_audio_entities(&xml);
    assert_eq!(spots.len(), 1);
    assert_eq!(untriggered, 1);
    let spot = &spots[0];
    assert_eq!(spot.play_trigger.as_deref(), Some("Play_Synthetic_Hum"));
    assert_eq!(spot.position, [1.0, 2.0, 3.0]);
    assert_eq!(spot.params["volume"], "5");
    assert_eq!(spot.params["attenuationScalingFactor"], "1.5");
    assert_eq!(spot.extra_positions, vec![[4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
    assert_eq!(spot.room.as_deref(), Some("20000000-0000-0000-0000-00000000000a"));
    assert_eq!(areas.len(), 1);
    let area = &areas[0];
    assert_eq!(area.play_trigger.as_deref(), Some("Play_Synthetic_Bed"));
    assert_eq!(area.links, vec!["10000000-0000-0000-0000-000000000001".to_owned()]);
    assert_eq!(area.shapes.len(), 1);
    assert_eq!(area.shapes[0].transform[3][0], 10.0, "the shape entity's position");
    let moved = spot.clone().transformed(glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, 1.0)).to_cols_array_2d());
    assert_eq!(moved.position, [1.0, 2.0, 4.0]);
    assert_eq!(moved.extra_positions[0], [4.0, 5.0, 7.0]);
}
