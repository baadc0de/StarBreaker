//! Synthetic DataCore seat component records and interaction links.
use super::*;

fn object(type_name: &'static str, fields: Vec<(&'static str, Value<'static>)>) -> Value<'static> {
    Value::Object {
        type_name,
        fields,
        record_id: None,
    }
}

#[test]
fn entrance_preserves_authored_conditions_and_follows_shared_interaction() {
    let entrance = object(
        "SCSeatAccessEntranceInteractionParams",
        vec![
            ("name", Value::String("board")),
            (
                "seatConnection",
                object(
                    "SCSeatConnectionParams",
                    vec![(
                        "seatItemPort",
                        object(
                            "SCSeatItemPort",
                            vec![("itemPortName", Value::String("seat_here"))],
                        ),
                    )],
                ),
            ),
            (
                "conditions",
                object(
                    "SCSeatAccessInteractionConditionParams",
                    vec![
                        ("idle", Value::Enum("True")),
                        ("zeroG", Value::Enum("False")),
                    ],
                ),
            ),
            (
                "enterState",
                object(
                    "SCSeatAccessEntranceInteractionStateParams",
                    vec![
                        (
                            "interaction",
                            object(
                                "SSharedInteractionParams",
                                vec![("Name", Value::String("on_enter"))],
                            ),
                        ),
                        ("fragmentTags", Value::String("Right")),
                        ("skipState", Value::Enum("SkipDeploy")),
                        ("interiorEntrance", Value::Bool(true)),
                        (
                            "attachment",
                            object(
                                "SCSeatActorAttachmentParams",
                                vec![("boneName", Value::String("sit_helper"))],
                            ),
                        ),
                    ],
                ),
            ),
        ],
    );
    let port = BTreeMap::from([(
        vec!["owner".to_string(), "seat_here".to_string()],
        "FixtureSeat",
    )]);
    let decoded = entrance_json(&entrance, &["owner".into(), "access".into()], &port);
    assert_eq!(decoded["name"], "board");
    assert_eq!(decoded["conditions"]["idle"], "True");
    assert_eq!(decoded["conditions"]["zero_g"], "False");
    assert!(decoded["conditions"]["landed"].is_null());
    assert_eq!(decoded["seat_child"], "FixtureSeat");
    assert_eq!(decoded["seat_port_path"], json!(["owner", "seat_here"]));
    assert_eq!(decoded["enter"]["interaction"], "on_enter");
    assert_eq!(decoded["enter"]["skip_state"], "SkipDeploy");
    assert_eq!(decoded["enter"]["attachment"]["bone"], "sit_helper");
    assert!(decoded["exit"].is_null());
}
