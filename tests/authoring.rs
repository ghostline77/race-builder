use ghostline_race_builder::{compile, roads::RoadGraph, route};
use serde_json::{Value, json};

fn example() -> (Value, Value) {
    (
        serde_json::from_str(include_str!("../examples/race.json")).unwrap(),
        route::import(
            serde_json::from_str(include_str!("../examples/game-scout-export.json")).unwrap(),
        )
        .unwrap(),
    )
}

#[test]
fn authored_speed_changes_survive_race_compilation() {
    let (config, mut route) = example();
    route["speed_mps"] = json!(30.);
    let point = route["marks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "spline_point" && m["order"] == 1)
        .unwrap()["position"]
        .clone();
    route["marks"].as_array_mut().unwrap().push(json!({"id":"speed_straight","role":"speed_change","order":0,"name":"Straight","position":point,"yaw":0,"options":{"speed_mps":50.},"evidence":{"source":"map"}}));
    let survey = route::survey(&route).unwrap();
    let expected = survey["world_spec"]["race_splines"][0]["speed_sections"].clone();
    assert_eq!(expected[0]["speed_mps"], 30.);
    assert_eq!(expected[1]["speed_mps"], 50.);
    let (_, _, world) = compile::documents(&config, &route).unwrap();
    assert_eq!(world["race_splines"][0]["speed_sections"], expected);
}

#[test]
fn scout_exports_and_saved_records_share_native_bindings() {
    let export: Value =
        serde_json::from_str(include_str!("../examples/game-scout-export.json")).unwrap();
    let imported = route::import(export.clone()).unwrap();
    let survey = route::survey(&imported).unwrap();
    for binding in export["bindings"].as_array().unwrap() {
        let found = survey["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["mark_id"] == binding["mark_id"])
            .unwrap();
        assert_eq!(found["ref"], binding["ref"]);
        if binding.get("trigger_ref").is_some() {
            assert_eq!(found["trigger_ref"], binding["trigger_ref"]);
        }
    }
    let record = route::import(export["race"].clone()).unwrap();
    for (a, b) in imported["marks"]
        .as_array()
        .unwrap()
        .iter()
        .zip(record["marks"].as_array().unwrap())
    {
        for field in ["id", "role", "order", "position", "yaw", "options"] {
            assert_eq!(a[field], b[field]);
        }
    }
    assert_eq!(route::import(imported.clone()).unwrap(), imported);
}

#[test]
fn incomplete_capture_is_editable_but_cannot_build() {
    let route=route::import(json!({"format":"ghostline-race-route","schema_version":1,"id":"captured_route","prefix":"races","title":"Captured route","looped":false,"width_default":5,"speed_mps":25,"marks":{}})).unwrap();
    assert_eq!(route["marks"], json!([]));
    assert!(
        !route::validate(&route, false).unwrap()["complete"]
            .as_bool()
            .unwrap()
    );
    assert!(route::validate(&route, true).is_err());
}

#[test]
fn rejects_duplicate_mark_orders_and_incompatible_grids() {
    let (mut config, mut route) = example();
    config["entrants"][0]["grid"] = json!(100);
    assert!(compile::validate(&config, &route).is_err());
    let mut duplicate = route["marks"][0].clone();
    duplicate["id"] = json!("other_mark");
    route["marks"].as_array_mut().unwrap().push(duplicate);
    assert!(route::validate(&route, false).is_err());
}

#[test]
fn road_routes_follow_connected_geometry_and_preserve_height() {
    let graph = RoadGraph::new(
        json!({"format":"ghostline-road-network","schema_version":1,"roads":[
            {"id":"lower_a","points":[[0,0,0],[20,0,0]]},
            {"id":"lower_b","points":[[20,0,0],[20,20,0]]},
            {"id":"overpass","points":[[0,0,10],[20,0,10],[20,20,10]]}
        ]}),
    )
    .unwrap();
    let start = json!({"x":5,"y":0,"z":0});
    let end = json!({"x":20,"y":15,"z":0});
    let result = graph.path(&start, &end, Some(0.)).unwrap();
    let points = result["points"].as_array().unwrap();
    assert!(points.iter().any(|p| p["x"] == 20. && p["y"] == 0.));
    assert!(points.iter().all(|p| p["z"] == 0.));
    assert_eq!(graph.snap(&start, Some(10.)).unwrap()["position"]["z"], 10.);
    assert!(graph.path(&start, &end, Some(10.)).is_err());
}

#[test]
fn builds_audited_documents_transactionally_and_rejects_existing_output() {
    let (config, route) = example();
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("race");
    let report = compile::generate(&config, &route, &output, None, None).unwrap();
    assert_eq!(report["resource_count"], 16);
    assert_eq!(report["native"], false);
    let saved = compile::read_json(&output.join("race.json")).unwrap();
    assert_eq!(saved["route"], "route.json");
    assert!(compile::generate(&config, &route, &output, None, None).is_err());
    let mut invalid = config;
    invalid["entrants"] = json!([]);
    let rejected = temporary.path().join("rejected");
    assert!(compile::generate(&invalid, &route, &rejected, None, None).is_err());
    assert!(!rejected.exists());
}

#[test]
fn generates_full_grid_forced_vehicle_diagnostics_and_vanilla_checkpoints() {
    let (mut config, mut route) = example();
    config["checkpoint_mode"] = json!("vanilla");
    config["vehicle_diagnostics"] = json!(true);
    config["player_vehicle"] = json!({"record":"Vehicle.v_standard2_archer_quartz"});
    route["looped"] = json!(true);
    let grid = route["marks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "start_grid")
        .unwrap()
        .clone();
    for i in 1..12 {
        let mut mark = grid.clone();
        mark["id"] = json!(format!("grid_{i}"));
        mark["order"] = json!(i);
        mark["native_ref"] = json!(format!("#grid_{i}"));
        mark["position"]["x"] = json!(-6. * i as f64);
        route["marks"].as_array_mut().unwrap().push(mark);
        let mut entrant = config["entrants"][0].clone();
        entrant["id"] = json!(format!("opponent_{i}"));
        entrant["grid"] = json!(i);
        config["entrants"].as_array_mut().unwrap().push(entrant);
    }
    let directory = tempfile::tempdir().unwrap();
    let report =
        compile::generate(&config, &route, &directory.path().join("grid"), None, None).unwrap();
    assert_eq!(report["resource_count"], 12);
    assert_eq!(report["bindings"]["entrants"].as_array().unwrap().len(), 12);
    assert!(report["bindings"]["supplied_player_vehicle"].is_string());
}

#[test]
fn native_bended_road_extraction_checks_transforms_and_rejects_bad_frames() {
    let mut document = json!({"Data":{"RootChunk":{"nodes":[{"Data":{"$type":"worldBendedMeshNode","isBendedRoad":true,"mesh":{"DepotPath":{"$value":"base\\worlds\\roads\\road_meshes\\bended\\road.mesh"}},"deformedBox":{"Min":{"X":-1,"Y":-1,"Z":-1},"Max":{"X":1,"Y":21,"Z":1}},"deformationData":[{"W":{"X":0,"Y":0,"Z":0,"W":1},"Y":{"X":0,"Y":1,"Z":0}},{"W":{"X":0,"Y":20,"Z":0,"W":1},"Y":{"X":0,"Y":1,"Z":0}}]}}],"nodeData":{"Data":[{"NodeIndex":0,"Position":{"X":100,"Y":200,"Z":12},"Scale":{"X":1,"Y":2,"Z":1},"Orientation":{"i":0,"j":0,"k":std::f64::consts::FRAC_1_SQRT_2,"r":std::f64::consts::FRAC_1_SQRT_2}}]}}}});
    let result =
        ghostline_race_builder::roads::sector_roads(&document, "test.streamingsector").unwrap();
    assert_eq!(result["roads"].as_array().unwrap().len(), 1);
    let end = &result["roads"][0]["points"][1];
    assert!((end["x"].as_f64().unwrap() - 60.).abs() < 1e-8);
    assert_eq!(end["z"], 12.);
    document["Data"]["RootChunk"]["nodes"][0]["Data"]["deformationData"][0]["W"]["W"] = json!(0);
    let rejected =
        ghostline_race_builder::roads::sector_roads(&document, "test.streamingsector").unwrap();
    assert!(rejected["roads"].as_array().unwrap().is_empty());
    assert_eq!(rejected["failures"].as_array().unwrap().len(), 1);
}

#[test]
fn native_lane_connections_split_at_transition_stations_without_gluing_crossings() {
    let mut network = json!({"format":"ghostline-road-network","schema_version":1,"topology":"native-traffic","roads":[{"id":"through","points":[[0,0,0],[100,0,0]]},{"id":"turn","points":[[40,10,0],[40,60,0]]},{"id":"bridge","points":[[0,0,10],[100,0,10]]}],"connections":[{"from":"through","to":"turn","from_m":40,"to_m":0}]});
    let graph = RoadGraph::new(network.clone()).unwrap();
    let result = graph
        .path(
            &json!({"x":35,"y":0,"z":0}),
            &json!({"x":40,"y":50,"z":0}),
            Some(0.),
        )
        .unwrap();
    let points = result["points"].as_array().unwrap();
    assert!(points.iter().any(|p| p["x"] == 40. && p["y"] == 0.));
    assert!(points.iter().all(|p| p["z"] == 0.));
    assert!(
        graph
            .path(
                &json!({"x":35,"y":0,"z":10}),
                &json!({"x":40,"y":50,"z":0}),
                Some(0.)
            )
            .is_err()
    );
    network["connections"] = json!([]);
    let disconnected = RoadGraph::new(network).unwrap();
    assert!(
        disconnected
            .path(
                &json!({"x":35,"y":0,"z":0}),
                &json!({"x":40,"y":50,"z":0}),
                Some(0.)
            )
            .is_err()
    );
}

#[test]
fn road_click_uses_reachable_ground_lane_below_overpass() {
    let graph=RoadGraph::new(json!({"format":"ghostline-road-network","schema_version":1,"topology":"native-traffic","roads":[{"id":"bridge","points":[[19.5,0,20],[19.5,20,20]]},{"id":"approach","points":[[0,0,0],[20,0,0]]},{"id":"turn","points":[[20,0,0],[20,20,0]]}],"connections":[{"from":"approach","to":"turn","from_m":20,"to_m":0}]})).unwrap();
    let result = graph
        .path(
            &json!({"x":5,"y":0,"z":0}),
            &json!({"x":19.5,"y":15,"z":0}),
            None,
        )
        .unwrap();
    let points = result["points"].as_array().unwrap();
    assert!(points.iter().all(|p| p["z"] == 0.));
    assert_eq!(points.last().unwrap()["y"], 15.);
}

#[test]
fn continuation_preserves_actual_start_level_for_coincident_lanes() {
    let graph=RoadGraph::new(json!({"format":"ghostline-road-network","schema_version":1,"topology":"native-traffic","roads":[{"id":"upper","points":[[0,0,1],[20,0,1]]},{"id":"lower","points":[[0,0,0],[20,0,0]]},{"id":"turn","points":[[20,0,0],[20,20,0]]}],"connections":[{"from":"lower","to":"turn","from_m":20,"to_m":0}]})).unwrap();
    let result = graph
        .path(
            &json!({"x":5,"y":0,"z":0}),
            &json!({"x":20,"y":15,"z":0}),
            Some(0.),
        )
        .unwrap();
    assert_eq!(result["points"][0]["z"], 0.);
}

#[test]
fn continuation_can_use_reachable_coincident_native_lane() {
    let graph=RoadGraph::new(json!({"format":"ghostline-road-network","schema_version":1,"topology":"native-traffic","roads":[{"id":"unconnected_duplicate","points":[[0,0,0],[20,0,0]]},{"id":"approach","points":[[0,0,0],[20,0,0]]},{"id":"turn","points":[[20,0,0],[20,20,0]]}],"connections":[{"from":"approach","to":"turn","from_m":20,"to_m":0}]})).unwrap();
    let result = graph
        .path(
            &json!({"x":5,"y":0,"z":0}),
            &json!({"x":20,"y":15,"z":0}),
            None,
        )
        .unwrap();
    assert_eq!(
        result["points"].as_array().unwrap().last().unwrap()["y"],
        15.
    );
}

#[test]
fn road_corridor_allows_adjacent_lane_without_connecting_an_overpass() {
    let graph = RoadGraph::new(json!({"format":"ghostline-road-network","schema_version":1,"topology":"native-traffic","roads":[
        {"id":"isolated","corridor":"street","points":[[0,0,0],[30,0,0]]},
        {"id":"connected","corridor":"street","points":[[0,5,0],[30,5,0]]},
        {"id":"turn","points":[[30,5,0],[30,30,0]]},
        {"id":"bridge","corridor":"other","points":[[0,0,1],[30,0,1]]}
    ],"connections":[{"from":"connected","to":"turn","from_m":30,"to_m":0}]})).unwrap();
    let result = graph
        .path(
            &json!({"x":10,"y":0,"z":0}),
            &json!({"x":30,"y":25,"z":0}),
            None,
        )
        .unwrap();
    assert_eq!(result["points"][0], json!({"x":10.0,"y":0.0,"z":0.0}));
    assert!(
        result["points"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["z"] == 0.)
    );
    assert_eq!(
        result["points"].as_array().unwrap().last().unwrap()["y"],
        25.
    );
}

#[test]
fn native_pavements_cannot_connect_race_roads() {
    let graph = RoadGraph::new(json!({"format":"ghostline-road-network","schema_version":1,"topology":"native-traffic","roads":[
        {"id":"west","drivable":true,"points":[[0,0,0],[20,0,0]]},
        {"id":"walkway","drivable":false,"points":[[20,0,0],[200,0,0]]},
        {"id":"east","drivable":true,"points":[[200,0,0],[220,0,0]]}
    ],"connections":[{"from":"west","to":"walkway","from_m":20,"to_m":0},{"from":"walkway","to":"east","from_m":180,"to_m":0}]})).unwrap();
    assert!(
        graph
            .path(
                &json!({"x":10,"y":0,"z":0}),
                &json!({"x":210,"y":0,"z":0}),
                None
            )
            .is_err()
    );
}

#[test]
fn native_turns_cannot_be_entered_backwards_at_a_shared_junction() {
    let graph = RoadGraph::new(json!({"format":"ghostline-road-network","schema_version":1,"topology":"native-traffic","roads":[
        {"id":"outgoing_turn","points":[[10,0,0],[8,0,0],[8,-10,0]]},
        {"id":"proper_return","points":[[8,-10,0],[8,-12,0],[16,-12,0],[16,0,0],[14,2,0]]},
        {"id":"other_turn","points":[[10,0,0],[12,0,0],[14,2,0],[14,10,0]]}
    ],"connections":[{"from":"outgoing_turn","to":"proper_return","from_m":12,"to_m":0},{"from":"proper_return","to":"other_turn","from_m":24.82842712474619,"to_m":4.82842712474619}]})).unwrap();
    let result = graph
        .path(
            &json!({"x":8,"y":-8,"z":0}),
            &json!({"x":14,"y":8,"z":0}),
            None,
        )
        .unwrap();
    let points = result["points"].as_array().unwrap();
    assert!(points.iter().any(|p| p["y"] == -12.));
    assert!(!points.iter().any(|p| p["x"] == 10. && p["y"] == 0.));
}

#[test]
fn native_gps_only_road_turns_are_kept_for_race_splines() {
    let graph = RoadGraph::new(json!({"format":"ghostline-road-network","schema_version":1,"topology":"native-traffic","roads":[
        {"id":"approach","lane_flags":17,"drivable":true,"points":[[0,0,0],[10,0,0]]},
        {"id":"turn","lane_flags":689,"drivable":false,"points":[[10,0,0],[12,0,0],[14,2,0],[15,5,0]]},
        {"id":"exit","lane_flags":17,"drivable":true,"points":[[15,5,0],[15,200,0]]}
    ],"connections":[{"from":"approach","to":"turn","from_m":10,"to_m":0},{"from":"turn","to":"exit","from_m":7.99070478491457,"to_m":0}]})).unwrap();
    let result = graph
        .path(
            &json!({"x":5,"y":0,"z":0}),
            &json!({"x":15,"y":180,"z":0}),
            None,
        )
        .unwrap();
    let points = result["points"].as_array().unwrap();
    assert!(points.iter().any(|p| p["x"] == 14. && p["y"] == 2.));
    assert_eq!(graph.network["roads"][1]["drivable"], true);
}
