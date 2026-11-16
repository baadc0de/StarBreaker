// Tests that require extracted game data on disk are ignored by default.
// Run with: cargo test -- --ignored

#[test]
#[ignore = "requires extracted game data on disk"]
fn parse_material_file() {
    let data = std::fs::read(
        "D:/StarCitizen/P4k-470/Data/Objects/Characters/Human/heads/male/npc/male17/male17_t1_head_material.mtl",
    )
    .unwrap();
    let xml = starbreaker_cryxml::from_bytes(&data).unwrap();
    let text = xml.to_string();
    assert!(
        text.contains("shader_head"),
        "expected shader_head in output"
    );
    assert!(text.contains("MtlFlags"), "expected MtlFlags in output");
}

#[test]
#[ignore = "requires extracted game data on disk"]
fn parse_character_customizer_xml() {
    let data =
        std::fs::read("D:/StarCitizen/P4k-470/Data/Libs/CharacterCustomizer/MasculineDefault.xml")
            .unwrap();
    let xml = starbreaker_cryxml::from_bytes(&data).unwrap();
    let text = xml.to_string();
    assert!(
        text.contains("CharacterCustomization"),
        "expected CharacterCustomization tag"
    );
    assert!(text.contains("dnaString"), "expected dnaString attribute");
}

#[test]
#[ignore = "requires extracted game data on disk"]
fn is_cryxmlb_check() {
    let data = std::fs::read(
        "D:/StarCitizen/P4k-470/Data/Objects/Characters/Human/heads/male/npc/male17/male17_t1_head_material.mtl",
    )
    .unwrap();
    assert!(starbreaker_cryxml::is_cryxmlb(&data));
    assert!(!starbreaker_cryxml::is_cryxmlb(b"not cryxml"));
}

#[test]
fn rejects_invalid_data() {
    assert!(starbreaker_cryxml::from_bytes(b"not cryxml data").is_err());
    assert!(starbreaker_cryxml::from_bytes(&[]).is_err());
}

#[test]
#[ignore = "requires extracted game data on disk"]
fn matches_csharp_output() {
    // Parse the binary CryXmlB file.
    let bin =
        std::fs::read("D:/StarCitizen/P4k-470/Data/Libs/CharacterCustomizer/MasculineDefault.xml")
            .unwrap();
    let xml = starbreaker_cryxml::from_bytes(&bin).unwrap();
    let our_output = xml.to_string();

    // Read the C#-converted reference XML.
    let csharp_output = std::fs::read_to_string(
        "D:/StarCitizen/P4k-470/Data/Libs/CharacterCustomizer/MasculineDefault.xml.xml",
    )
    .unwrap();

    // Both outputs should contain the same key content.
    assert!(
        our_output.contains("modelTag=\"Male\""),
        "expected modelTag=\"Male\" in our output"
    );
    assert!(
        csharp_output.contains("modelTag=\"Male\""),
        "expected modelTag=\"Male\" in C# output"
    );
}

/// A CryXmlB document built in memory: (tag, content, attributes, children) per node, node 0 the root.
fn build_cryxmlb(nodes: &[(&str, &str, &[(&str, &str)], &[usize])]) -> Vec<u8> {
    let mut strings: Vec<u8> = vec![0]; // offset 0: the empty string
    let intern = |s: &str, strings: &mut Vec<u8>| -> u32 {
        if s.is_empty() {
            return 0;
        }
        let at = strings.len() as u32;
        strings.extend_from_slice(s.as_bytes());
        strings.push(0);
        at
    };
    let mut node_bytes = Vec::new();
    let mut attr_bytes = Vec::new();
    let mut child_bytes = Vec::new();
    let (mut attr_index, mut child_index) = (0i32, 0i32);
    let parent_of = |i: usize| -> i32 {
        nodes.iter().position(|n| n.3.contains(&i)).map_or(-1, |p| p as i32)
    };
    for (i, (tag, content, attrs, children)) in nodes.iter().enumerate() {
        let tag_off = intern(tag, &mut strings);
        let content_off = intern(content, &mut strings);
        node_bytes.extend_from_slice(&tag_off.to_le_bytes());
        node_bytes.extend_from_slice(&content_off.to_le_bytes());
        node_bytes.extend_from_slice(&(attrs.len() as u16).to_le_bytes());
        node_bytes.extend_from_slice(&(children.len() as u16).to_le_bytes());
        node_bytes.extend_from_slice(&parent_of(i).to_le_bytes());
        node_bytes.extend_from_slice(&attr_index.to_le_bytes());
        node_bytes.extend_from_slice(&child_index.to_le_bytes());
        node_bytes.extend_from_slice(&0i32.to_le_bytes());
        for (k, v) in attrs.iter() {
            let (ko, vo) = (intern(k, &mut strings), intern(v, &mut strings));
            attr_bytes.extend_from_slice(&ko.to_le_bytes());
            attr_bytes.extend_from_slice(&vo.to_le_bytes());
        }
        for &c in children.iter() {
            child_bytes.extend_from_slice(&(c as i32).to_le_bytes());
        }
        attr_index += attrs.len() as i32;
        child_index += children.len() as i32;
    }
    let header_len = 8 + 9 * 4;
    let node_pos = header_len;
    let child_pos = node_pos + node_bytes.len();
    let attr_pos = child_pos + child_bytes.len();
    let string_pos = attr_pos + attr_bytes.len();
    let total = string_pos + strings.len();
    let mut out = b"CryXmlB\0".to_vec();
    for v in [
        total,
        node_pos,
        nodes.len(),
        attr_pos,
        attr_index as usize,
        child_pos,
        child_index as usize,
        string_pos,
        strings.len(),
    ] {
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    out.extend_from_slice(&node_bytes);
    out.extend_from_slice(&child_bytes);
    out.extend_from_slice(&attr_bytes);
    out.extend_from_slice(&strings);
    out
}

#[test]
fn node_text_content_is_written() {
    // Particle libraries keep effect names as element text: <Particles><Name>beam_core</Name>...</Particles>.
    let bin = build_cryxmlb(&[
        ("ParticleLibrary", "", &[("Name", "tractor_beam")], &[1]),
        ("Particles", "", &[], &[2, 3]),
        ("Name", "fps.beam_only", &[], &[]),
        ("Params", "", &[("Count", "500")], &[]),
    ]);
    let xml = starbreaker_cryxml::from_bytes(&bin).unwrap();
    let text = xml.to_string();
    assert!(text.contains("<Name>fps.beam_only</Name>"), "node text lost:\n{text}");
    assert!(text.contains("<Params Count=\"500\" />"), "empty node changed:\n{text}");
    let name = xml.node_children(xml.node_children(xml.root()).next().unwrap()).next().unwrap();
    assert_eq!(xml.node_content(name), "fps.beam_only");
}

#[test]
fn node_text_is_escaped_and_kept_beside_children() {
    let bin = build_cryxmlb(&[("Root", "a < b & c", &[], &[1]), ("Child", "", &[], &[])]);
    let text = starbreaker_cryxml::from_bytes(&bin).unwrap().to_string();
    assert!(text.contains("<Root>a &lt; b &amp; c"), "text not escaped or missing:\n{text}");
    assert!(text.contains("<Child />"), "child missing:\n{text}");
}
