use kakera_avg::{
    controller::{Intent, reduce, transition},
    loader::parse_story,
    model::{
        CompareOp, Condition, Effect, MAX_HISTORY, Route, Session, Story, TransitionRule,
        VariableValue as V, rules,
    },
    save::{SaveStore, SlotId, Snapshot},
};
use std::path::Path;

const STORY: &str = include_str!("../doc/v0.3-example/story.xml");
const FLOW: &str = include_str!("../doc/v0.3-example/flow.xml");
fn story() -> Story {
    parse_story(STORY, FLOW, Path::new("demo")).unwrap().story
}
fn advance(story: &Story, session: &Session) -> Session {
    reduce(story, session, Intent::Advance).unwrap()
}
fn choose(story: &Story, session: &Session, id: &str) -> Session {
    reduce(story, session, Intent::Choose(id.into())).unwrap()
}
fn walk(story: &Story, session: &Session) -> Session {
    advance(story, &choose(story, session, "accept"))
}
fn visible(story: &Story, session: &Session) -> Vec<String> {
    story
        .available_options(session)
        .unwrap()
        .iter()
        .map(|choice| choice.id.clone())
        .collect()
}
fn cmp(variable: &str, op: CompareOp, value: V) -> Condition {
    Condition::Compare {
        variable: variable.into(),
        op,
        value,
    }
}

#[test]
fn variables_unlock_options_and_all_three_demo_endings_are_reachable() {
    let story = story();
    let initial = story.start();
    assert_eq!(initial.variables["trust"], V::Int(0));
    assert_eq!(initial.variables["met"], V::Bool(false));
    let question = advance(&story, &initial);
    assert_eq!(visible(&story, &question), vec!["accept", "refuse"]);
    assert!(reduce(&story, &question, Intent::Choose("invite".into())).is_err());
    let end = advance(&story, &choose(&story, &question, "refuse"));
    assert_eq!(end.frame_id, "refuse");
    assert_eq!(end.variables["mood"], V::String("安静".into()));
    let once = walk(&story, &question);
    assert_eq!(once.variables["trust"], V::Int(1));
    assert_eq!(once.variables["met"], V::Bool(true));
    assert!(!visible(&story, &once).contains(&"invite".into()));
    let friendly = advance(&story, &choose(&story, &once, "refuse"));
    assert_eq!(friendly.frame_id, "friendly");
    let twice = walk(&story, &once);
    assert_eq!(visible(&story, &twice), vec!["accept", "refuse", "invite"]);
    let accepted = advance(&story, &choose(&story, &twice, "invite"));
    assert_eq!(accepted.frame_id, "accept");
    assert_eq!(accepted.variables["mood"], V::String("开心".into()));
    // Both cases now match: invited=true/trust>=2 and trust>=1. XML order selects the first.
    story.validate_session(&accepted).unwrap();
    let capped = walk(&story, &twice);
    assert_eq!(capped.variables["trust"], V::Int(3));
    assert_eq!(visible(&story, &capped), vec!["refuse", "invite"]);
    assert!(reduce(&story, &capped, Intent::Choose("accept".into())).is_err());
    assert_eq!(story.start(), initial);
}

#[test]
fn comparisons_combinations_and_negative_add_are_typed_and_deterministic() {
    let story = story();
    let vars = story.initial_variables();
    for (op, rhs, result) in [
        (CompareOp::Eq, 0, true),
        (CompareOp::Ne, 0, false),
        (CompareOp::Lt, 1, true),
        (CompareOp::Le, 0, true),
        (CompareOp::Gt, -1, true),
        (CompareOp::Ge, 1, false),
    ] {
        assert_eq!(
            rules::evaluate(&cmp("trust", op, V::Int(rhs)), &vars).unwrap(),
            result
        );
    }
    let nested = Condition::All(vec![
        Condition::Any(vec![
            cmp("mood", CompareOp::Eq, V::String("平静".into())),
            cmp("trust", CompareOp::Gt, V::Int(9)),
        ]),
        Condition::Not(Box::new(cmp("met", CompareOp::Eq, V::Bool(true)))),
    ]);
    assert!(rules::evaluate(&nested, &vars).unwrap());
    assert!(rules::evaluate(&cmp("mood", CompareOp::Ne, V::String("开心".into())), &vars).unwrap());
    assert!(rules::evaluate(&cmp("met", CompareOp::Ge, V::Bool(true)), &vars).is_err());
    assert!(rules::evaluate(&cmp("trust", CompareOp::Eq, V::Bool(false)), &vars).is_err());
    assert!(rules::evaluate(&cmp("missing", CompareOp::Eq, V::Int(0)), &vars).is_err());
    let result = rules::apply_effects(
        &story,
        &vars,
        &[
            Effect::Set {
                variable: "trust".into(),
                value: V::Int(3),
            },
            Effect::Add {
                variable: "trust".into(),
                amount: -2,
            },
            Effect::Set {
                variable: "mood".into(),
                value: V::String(String::new()),
            },
        ],
    )
    .unwrap();
    assert_eq!(result["trust"], V::Int(1));
    assert_eq!(result["mood"], V::String(String::new()));
    assert_eq!(vars["trust"], V::Int(0));
}

#[test]
fn effects_and_location_roll_back_together_on_bounds_or_overflow() {
    let mut story = story();
    let question = advance(&story, &story.start());
    let Route::Choice(routes) = story.routes.get_mut("question").unwrap() else {
        panic!()
    };
    routes.get_mut("accept").unwrap().effects = vec![
        Effect::Set {
            variable: "mood".into(),
            value: V::String("不应提交".into()),
        },
        Effect::Add {
            variable: "trust".into(),
            amount: 4,
        },
    ];
    let baseline = question.clone();
    assert!(transition(&story, &question, Intent::Choose("accept".into())).is_err());
    assert_eq!(question, baseline);
    let mut variables = story.initial_variables();
    story.variables.get_mut("trust").unwrap().max = None;
    variables.insert("trust".into(), V::Int(i64::MAX));
    assert!(
        rules::apply_effects(
            &story,
            &variables,
            &[Effect::Add {
                variable: "trust".into(),
                amount: 1
            }]
        )
        .is_err()
    );
    assert_eq!(variables["trust"], V::Int(i64::MAX));
    for effect in [
        Effect::Set {
            variable: "met".into(),
            value: V::Int(1),
        },
        Effect::Add {
            variable: "met".into(),
            amount: 1,
        },
        Effect::Set {
            variable: "unknown".into(),
            value: V::Bool(true),
        },
    ] {
        assert!(rules::apply_effects(&story, &variables, &[effect]).is_err());
    }
}

#[test]
fn guards_are_evaluated_before_effects_and_empty_choices_remain_safe() {
    let mut story = story();
    let question = advance(&story, &story.start());
    let Route::Choice(routes) = story.routes.get_mut("question").unwrap() else {
        panic!()
    };
    for rule in routes.values_mut() {
        rule.when = Some(cmp("trust", CompareOp::Ge, V::Int(1)));
        rule.effects = vec![Effect::Set {
            variable: "trust".into(),
            value: V::Int(1),
        }];
    }
    assert!(visible(&story, &question).is_empty());
    assert!(reduce(&story, &question, Intent::Choose("accept".into())).is_err());
    assert_eq!(advance(&story, &question), question);
}

#[test]
fn declarations_reject_invalid_ids_types_ranges_duplicates_and_limits() {
    for (from, to) in [
        ("schema=\"3\"", "schema=\"2\""),
        ("id=\"trust\" label", "id=\"9trust\" label"),
        ("id=\"trust\" label", "id=\"坏变量\" label"),
        ("<bool id=\"met\"", "<bool id=\"trust\""),
        ("initial=\"false\"", "initial=\"1\""),
        ("initial=\"0\"", "initial=\"1.5\""),
        ("min=\"0\" max=\"3\"", "min=\"4\" max=\"3\""),
        ("min=\"0\" max=\"3\"", "min=\"1\" max=\"3\""),
        ("<variables>", "<variables typo=\"yes\">"),
        ("<bool id=\"met\"", "<bool min=\"0\" id=\"met\""),
        ("</variables>", "</variables><variables/>"),
        ("<variables>", "<variables><script/>"),
    ] {
        assert!(STORY.contains(from));
        let error = parse_story(&STORY.replace(from, to), FLOW, Path::new("demo"))
            .err()
            .expect(from);
        assert!(error.to_string().starts_with("story.xml:"), "{error:#}");
    }
    for addition in [
        format!("<string id=\"huge\" initial=\"{}\"/>", "a".repeat(4097)),
        format!("<int id=\"{}\" initial=\"0\"/>", "a".repeat(65)),
        (0..129)
            .map(|id| format!("<bool id=\"v{id}\" initial=\"false\"/>"))
            .collect(),
    ] {
        assert!(
            parse_story(
                &STORY.replace("<variables>", &format!("<variables>{addition}")),
                FLOW,
                Path::new("demo")
            )
            .is_err()
        );
    }
}

#[test]
fn invalid_rules_report_file_position_including_dead_or_short_circuited_conditions() {
    for (from, to) in [
        ("schema=\"2\"", "schema=\"9\""),
        ("var=\"trust\" op=\"lt\"", "var=\"typo\" op=\"lt\""),
        ("op=\"lt\"", "op=\"less\""),
        ("op=\"lt\" value=\"3\"", "op=\"lt\" value=\"true\""),
        ("var=\"met\" op=\"eq\"", "var=\"met\" op=\"gt\""),
        (
            "<add var=\"trust\" value=\"1\" />",
            "<add var=\"met\" value=\"true\" />",
        ),
        (
            "<set var=\"met\" value=\"true\" />",
            "<set var=\"trust\" value=\"4\" />",
        ),
        (
            "<set var=\"met\" value=\"true\" />",
            "<set var=\"missing\" value=\"true\" />",
        ),
        (
            "<when><cmp var=\"trust\" op=\"lt\" value=\"3\" /></when>",
            "<when><all/></when>",
        ),
        (
            "<when><cmp var=\"trust\" op=\"lt\" value=\"3\" /></when>",
            "<when><not/></when>",
        ),
        (
            "<next from=\"opening\" to=\"question\" />",
            "<next from=\"opening\" to=\"question\"><when><cmp var=\"met\" op=\"eq\" value=\"true\"/></when></next>",
        ),
        (
            "<case to=\"friendly\">",
            "<case to=\"friendly\" extra=\"1\">",
        ),
        ("<otherwise to=\"refuse\">", "<otherwise to=\"missing\">"),
        (
            "<when><cmp var=\"trust\" op=\"ge\" value=\"1\" /></when>",
            "",
        ),
        ("<otherwise to=\"refuse\">", "<case to=\"refuse\">"),
        (
            "<on option=\"refuse\" to=\"decision\" />",
            "<on option=\"refuse\" to=\"decision\"><when><any><cmp var=\"trust\" op=\"ge\" value=\"0\"/><cmp var=\"unknown\" op=\"eq\" value=\"0\"/></any></when></on>",
        ),
    ] {
        assert!(FLOW.contains(from), "{from}");
        let error = parse_story(STORY, &FLOW.replace(from, to), Path::new("demo"))
            .err()
            .expect(from);
        assert!(format!("{error:#}").contains("flow.xml"), "{error:#}");
    }
}

#[test]
fn condition_depth_node_count_and_effect_count_are_bounded() {
    let base = "<cmp var=\"trust\" op=\"eq\" value=\"0\"/>";
    for expression in [
        format!("{}{}{}", "<not>".repeat(17), base, "</not>".repeat(17)),
        format!("<all>{}</all>", base.repeat(129)),
    ] {
        let flow = FLOW.replace(
            "<on option=\"refuse\" to=\"decision\" />",
            &format!("<on option=\"refuse\" to=\"decision\"><when>{expression}</when></on>"),
        );
        assert!(parse_story(STORY, &flow, Path::new("demo")).is_err());
    }
    let flow = FLOW.replace(
        "<on option=\"refuse\" to=\"decision\" />",
        &format!(
            "<on option=\"refuse\" to=\"decision\"><effects>{}</effects></on>",
            "<add var=\"trust\" value=\"0\"/>".repeat(129)
        ),
    );
    assert!(parse_story(STORY, &flow, Path::new("demo")).is_err());
}

#[test]
fn branch_requires_ordered_cases_and_one_last_fallback() {
    let branch = "<branch from=\"decision\">{}</branch>";
    let start = FLOW.find("<branch from=\"decision\">").unwrap();
    let end = FLOW[start..].find("</branch>").unwrap() + start + "</branch>".len();
    for invalid in [
        "<otherwise to=\"refuse\"/>",
        "<otherwise to=\"refuse\"/><case to=\"accept\"><when><cmp var=\"met\" op=\"eq\" value=\"true\"/></when></case>",
        "<case to=\"accept\"><when><cmp var=\"met\" op=\"eq\" value=\"true\"/></when></case>",
        "<case to=\"accept\"><when><cmp var=\"met\" op=\"eq\" value=\"true\"/></when></case><otherwise to=\"refuse\"/><otherwise to=\"friendly\"/>",
        "<case to=\"accept\"><when><cmp var=\"met\" op=\"eq\" value=\"true\"/></when></case><otherwise to=\"refuse\"><when><cmp var=\"met\" op=\"eq\" value=\"false\"/></when></otherwise>",
    ] {
        let flow = format!(
            "{}{}{}",
            &FLOW[..start],
            branch.replace("{}", invalid),
            &FLOW[end..]
        );
        assert!(parse_story(STORY, &flow, Path::new("demo")).is_err());
    }
}

#[test]
fn checkpoint_advances_when_capping_history_and_survives_serialization() {
    let mut story = story();
    story.variables.get_mut("trust").unwrap().max = None;
    story.routes.insert(
        "opening".into(),
        Route::Next(TransitionRule {
            to: "opening".into(),
            when: None,
            effects: vec![Effect::Add {
                variable: "trust".into(),
                amount: 1,
            }],
        }),
    );
    let mut session = story.start();
    for _ in 0..MAX_HISTORY + 3 {
        let result = transition(&story, &session, Intent::Advance).unwrap();
        assert!(result.entered);
        session = result.session;
    }
    assert_eq!(session.history.len(), MAX_HISTORY);
    assert_eq!(session.variables["trust"], V::Int((MAX_HISTORY + 3) as i64));
    assert_eq!(session.history_base_variables["trust"], V::Int(4));
    story.validate_session(&session).unwrap();
    let snapshot = Snapshot::capture(&story, &session).unwrap();
    let restored: Snapshot =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(restored.restore(&story).unwrap(), session);
}

#[test]
fn save_restore_preserves_variables_guards_and_rejects_forged_history_or_values() {
    let story = story();
    let question = advance(&story, &story.start());
    let twice = walk(&story, &walk(&story, &question));
    // Preserve the historical example, but never silently migrate its schema.
    assert!(
        serde_json::from_str::<Snapshot>(include_str!("../doc/v0.3-save.example.json")).is_err()
    );
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    let slot = SlotId::new(1).unwrap();
    store.save(slot, &story, &twice).unwrap();
    let restored = SaveStore::at(directory.path().into())
        .load(slot, &story)
        .unwrap();
    assert_eq!(restored, twice);
    assert!(visible(&story, &restored).contains(&"invite".into()));
    assert_eq!(
        advance(&story, &choose(&story, &restored, "invite")).frame_id,
        "accept"
    );
    let snapshot = Snapshot::capture(&story, &twice).unwrap();
    for (key, value) in [
        ("trust", V::Int(0)),
        ("trust", V::Bool(true)),
        ("trust", V::Int(4)),
        ("unknown", V::Bool(false)),
    ] {
        let mut bad = snapshot.clone();
        bad.variables.insert(key.into(), value);
        assert!(bad.restore(&story).is_err());
    }
    let mut missing = snapshot.clone();
    missing.variables.remove("met");
    assert!(missing.restore(&story).is_err());
    let mut bad_base = snapshot.clone();
    bad_base
        .history_base_variables
        .insert("trust".into(), V::Int(1));
    assert!(bad_base.restore(&story).is_err());
    let mut bad_history = snapshot.clone();
    bad_history.history[1].choice_id = Some("invite".into());
    assert!(bad_history.restore(&story).is_err());
    let mut wrong_version = snapshot.clone();
    wrong_version.content_version = 1;
    assert!(wrong_version.restore(&story).is_err());
    let mut missing_fields = serde_json::to_value(&snapshot).unwrap();
    missing_fields.as_object_mut().unwrap().remove("variables");
    assert!(serde_json::from_value::<Snapshot>(missing_fields).is_err());
    assert_eq!(store.load(slot, &story).unwrap(), twice);
}

#[test]
fn old_save_is_rejected_without_modifying_files_in_v06() {
    let legacy_story = parse_story(
        include_str!("../doc/rust-mvc-example/story.xml"),
        include_str!("../doc/rust-mvc-example/flow.xml"),
        Path::new("demo"),
    )
    .unwrap()
    .story;
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    let slot = SlotId::new(1).unwrap();
    let old = include_bytes!("../doc/v0.2-save.example.json");
    std::fs::write(store.slot_path(slot), old).unwrap();
    assert!(store.load(slot, &legacy_story).is_err());
    assert_eq!(std::fs::read(store.slot_path(slot)).unwrap(), old);
    let session = legacy_story.start();
    store.save(slot, &legacy_story, &session).unwrap();
    let current: Snapshot =
        serde_json::from_slice(&std::fs::read(store.slot_path(slot)).unwrap()).unwrap();
    assert_eq!(current.schema_version, 6);
}

#[test]
fn original_story_schemas_still_load_and_script_elements_remain_rejected() {
    let old = include_str!("../doc/rust-mvc-example/story.xml");
    let flow = include_str!("../doc/rust-mvc-example/flow.xml");
    for schema in ["1", "2", "3"] {
        let loaded = parse_story(
            &old.replace("schema=\"1\"", &format!("schema=\"{schema}\"")),
            flow,
            Path::new("demo"),
        )
        .unwrap();
        assert!(loaded.story.start().variables.is_empty());
    }
    assert!(
        parse_story(
            &STORY.replace(
                "<variables>",
                "<variables><script language=\"javascript\"/>"
            ),
            FLOW,
            Path::new("demo")
        )
        .is_err()
    );
}

#[test]
fn xml_next_effects_and_nested_guards_execute_through_the_controller() {
    let flow = FLOW.replace("<next from=\"opening\" to=\"question\" />", "<next from=\"opening\" to=\"question\"><effects><set var=\"mood\" value=\"已出门\"/></effects></next>")
        .replace("<on option=\"refuse\" to=\"decision\" />", "<on option=\"refuse\" to=\"decision\"><when><all><not><cmp var=\"met\" op=\"eq\" value=\"true\"/></not><any><cmp var=\"mood\" op=\"eq\" value=\"已出门\"/><cmp var=\"trust\" op=\"gt\" value=\"100\"/></any></all></when></on>");
    let story = parse_story(STORY, &flow, Path::new("demo")).unwrap().story;
    let question = advance(&story, &story.start());
    assert_eq!(question.variables["mood"], V::String("已出门".into()));
    assert!(visible(&story, &question).contains(&"refuse".into()));
    let once = walk(&story, &question);
    assert!(!visible(&story, &once).contains(&"refuse".into()));
    assert!(reduce(&story, &once, Intent::Choose("refuse".into())).is_err());
    story.validate_session(&once).unwrap();
}
