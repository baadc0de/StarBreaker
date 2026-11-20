//! Synthetic authored seat fragment queries. Visual clip names are test data.
use super::*;

#[test]
fn seat_action_can_resolve_main_and_look_layers_independently() {
    let fragments = vec![
        json!({"fragment": "SeatState", "tags": ["AI"], "frag_tags": ["trans_fixture","Enter","Right"], "guid": "ai"}),
        json!({"fragment": "SeatState", "tags": ["OtherSeat"], "frag_tags": ["trans_fixture","Enter","Right"], "guid": "other"}),
        json!({"fragment": "SeatState", "tags": [], "frag_tags": ["trans_fixture","Enter","Left"], "guid": "left"}),
        json!({"fragment": "SeatState", "tags": [], "frag_tags": ["trans_fixture","Enter","Right"], "guid": "right"}),
        json!({"fragment": "SeatState", "tags": [LOOK_SCOPE], "frag_tags": ["trans_fixture","Enter","Right"], "guid": "look"}),
    ];
    let tags = vec!["FixtureSeat".into()];
    let wanted = vec!["trans_fixture".into(), "Enter".into(), "Right".into()];
    assert_eq!(
        select(&fragments, &tags, &wanted, false).unwrap()["guid"],
        "right"
    );
    let look_tags = vec!["FixtureSeat".into(), LOOK_SCOPE.into()];
    assert_eq!(
        select(&fragments, &look_tags, &wanted, true).unwrap()["guid"],
        "look"
    );
}
