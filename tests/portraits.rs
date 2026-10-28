use kakera_avg::{
    content::{ContentSource, DirectorySource},
    controller::{Intent, transition_with},
    effects::{self, Player},
    interface::{
        Event,
        host::{self, Profile, State},
    },
    loader::load_game,
    model::{Slot, effects::*},
    save::Snapshot,
    settings::Settings,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

static STORY: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let source =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    kakera_avg::story_chain::compile(&source).unwrap().xml
});
struct Source {
    story: String,
    js: String,
}
impl ContentSource for Source {
    fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        match path.to_str().unwrap() {
            "story.xml" => Ok(self.story.as_bytes().to_vec()),
            "scripts/ui.js" => Ok(self.js.as_bytes().to_vec()),
            _ => {
                DirectorySource::new(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("games/demo"))?
                    .read(path)
            }
        }
    }
}
fn source() -> Source {
    Source {
        story: STORY.to_string(),
        js: include_str!("../games/demo/scripts/ui.js").into(),
    }
}
fn action(kind: PortraitActionKind) -> PortraitAnimation {
    PortraitAnimation {
        kind,
        duration: 1000,
        easing: Easing::Linear,
        ..Default::default()
    }
}
#[test]
fn actions_reach_explicit_poses_and_zero_duration_settles() {
    use PortraitActionKind::*;
    for (kind, pose) in [
        (FadeIn, PortraitPose::Normal),
        (EnterLeft, PortraitPose::Normal),
        (EnterRight, PortraitPose::Normal),
        (EnterUp, PortraitPose::Normal),
        (EnterDown, PortraitPose::Normal),
        (FadeOut, PortraitPose::Hidden),
        (ExitLeft, PortraitPose::Hidden),
        (ExitRight, PortraitPose::Hidden),
        (ExitUp, PortraitPose::Hidden),
        (ExitDown, PortraitPose::Hidden),
        (Shake, PortraitPose::Flipped),
        (Bounce, PortraitPose::Normal),
        (Turn, PortraitPose::Flipped),
        (Spin, PortraitPose::Normal),
        (FallLeft, PortraitPose::FallenLeft),
        (FallRight, PortraitPose::FallenRight),
    ] {
        let end = effects::portrait_motion(pose, action(kind), 1.0);
        // Hidden poses do not retain an offscreen offset after the clock is removed.
        if pose == PortraitPose::Hidden {
            assert_eq!(end.opacity, 0.0);
        } else {
            assert_eq!(end, effects::pose_motion(pose));
        }
        let zero = PortraitAnimation {
            duration: 0,
            ..action(kind)
        };
        assert_eq!(effects::portrait_motion(pose, zero, 0.0), end);
    }
    assert!(
        effects::portrait_motion(PortraitPose::Normal, action(EnterLeft), 0.0).offset[0] < -1280.0
    );
    assert!(
        effects::portrait_motion(PortraitPose::Normal, action(EnterRight), 0.0).offset[0] > 1280.0
    );
    assert!(effects::portrait_motion(PortraitPose::Normal, action(Spin), 0.25).scale_x < -0.99);
    assert_eq!(
        effects::portrait_motion(PortraitPose::Flipped, action(Turn), 0.0).scale_x,
        1.0
    );
    let fallen = effects::portrait_motion(PortraitPose::FallenRight, action(FallRight), 0.5);
    assert_eq!((fallen.rotation, fallen.offset[1]), (45.0, 32.0));
}
#[test]
fn strict_xml_rejects_invalid_actions_parameters_and_pose_pairs_with_location() {
    for attributes in [
        "pose=\"unknown\"",
        "enter=\"unknown\"",
        "enter=\"fall-left\"",
        "enter=\"exit-left\"",
        "pose=\"hidden\" enter=\"shake\"",
        "exit=\"spin\"",
        "duration=\"5001\"",
        "duration=\"1.5\"",
        "strength=\"NaN\"",
        "strength=\"129\"",
        "easing=\"unknown\"",
        "speed=\"2\"",
    ] {
        let mut src = source();
        src.story = STORY.replacen(
            "effect=\"portrait.enter\"",
            &format!("effect=\"portrait.enter\" {attributes}"),
            1,
        );
        let err = load_game(&src).err().unwrap();
        assert!(
            format!("{err:#}").contains("story.xml:"),
            "{attributes}: {err:#}"
        );
    }
}
#[test]
fn retirement_and_reentry_are_bounded_and_restore_has_no_clocks() {
    let (loaded, _) = load_game(&source()).unwrap();
    let old = &loaded.story.frames["actor.stage"];
    let empty = &loaded.story.frames["actor.empty"];
    let mut p = Player::default();
    p.enter_scene(Some(old.clone()), empty, Transition::default(), false);
    assert_eq!(p.departing.len(), 1);
    p.tick(0.5);
    let age = p.departing[0].age;
    p.pages_changed();
    p.tick(0.0);
    assert_eq!(p.departing[0].age, age);
    p.tick(2.0);
    assert!(p.departing.is_empty());
    let transition = Transition {
        kind: TransitionKind::Dissolve,
        duration: 400,
        easing: Easing::Linear,
    };
    p.enter_scene(Some(old.clone()), empty, transition, false);
    assert_eq!(p.departing[0].animation.duration, 400);
    p.tick(0.4);
    assert!(p.departing.is_empty());
    assert!(p.previous.is_none());
    let mut short_exit = old.clone();
    short_exit.visual.portraits[0].exit.duration = 100;
    let long_transition = Transition {
        duration: 1000,
        ..transition
    };
    p.enter_scene(Some(short_exit), empty, long_transition, false);
    p.tick(0.5);
    assert_eq!(
        p.portrait_motion(&old.visual.portraits[0], true).opacity,
        0.0
    );
    p.tick(0.5);
    assert!(p.departing.is_empty());
    let fall = &loaded.story.frames["actor.fall-left"];
    p.enter_scene(Some(fall.clone()), fall, Transition::default(), false);
    p.tick(0.4);
    assert!(!p.portrait_actions.is_empty());
    p.enter_scene(Some(fall.clone()), fall, Transition::default(), false);
    assert_eq!(p.portrait_actions[&Slot::Center].1, 0.0);
    p.enter_scene(Some(old.clone()), fall, transition, true);
    assert!(p.portrait_actions.is_empty());
    assert!(p.departing.is_empty());
    assert_eq!(
        p.portrait_motion(&fall.visual.portraits[0], false).rotation,
        -90.0
    );
}
#[test]
fn js_commands_validate_candidate_slot_and_pose_without_mutating_old_state() {
    let mut src = source();
    src.js.push_str("\non.ui('test-portrait',ctx=>{ctx.setUi('status','changed');ctx.portrait('center','fall-left');ctx.choose('fall-left');});");
    let (loaded, logic) = load_game(&src).unwrap();
    let def = loaded.interface;
    let state = State::new(&def);
    let commands = logic
        .ui_event(
            &Event {
                sound: None,
                route: None,
                name: "test-portrait".into(),
                item: Value::Null,
                value: Value::Null,
            },
            &loaded.story.start(),
            &state.variables,
            &json!({"game":{}}),
        )
        .unwrap();
    let plan = host::prepare(
        &def,
        &state,
        &Settings::default(),
        &Profile::new(&loaded.story.id, &def),
        commands,
        false,
    )
    .unwrap();
    assert!(effects::validate_portraits(&loaded.story.frames["opening"], &plan.portraits).is_err());
    assert!(
        effects::validate_portraits(&loaded.story.frames["actor.stage"], &plan.portraits).is_err()
    );
    assert!(
        effects::validate_portraits(&loaded.story.frames["actor.fall-left"], &plan.portraits)
            .is_ok()
    );
    assert_eq!(state.variables["status"].to_string(), "");
    for body in [
        "ctx.portrait('absent','shake')",
        "ctx.portrait('center','shake',5001)",
        "ctx.portrait('center','unknown')",
        "ctx.portrait('center','shake',100,129)",
        "ctx.portrait('center','fall-left')",
    ] {
        let mut src = source();
        src.js=src.js.replace("if (ctx.ui.fxTransition) ctx.transition(ctx.ui.fxTransition, Number(ctx.ui.fxDuration));",body);
        assert!(load_game(&src).is_err(), "{body}");
    }
}
#[test]
fn every_demo_branch_replays_and_restores_final_pose_without_animation_state() {
    let (loaded, logic) = load_game(&source()).unwrap();
    let story = &loaded.story;
    let question = transition_with(story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    let stage = transition_with(
        story,
        &question,
        Intent::Choose("portrait-demo".into()),
        &logic,
    )
    .unwrap()
    .session;
    for choice in &story.frames["actor.stage"].options {
        let session = transition_with(story, &stage, Intent::Choose(choice.id.clone()), &logic)
            .unwrap()
            .session;
        let snapshot = Snapshot::capture_with(story, &session, &logic).unwrap();
        let restored = snapshot.restore_with(story, &logic).unwrap();
        assert_eq!(restored, session);
        let json = serde_json::to_value(snapshot).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 10);
        assert!(json.get("portrait_actions").is_none());
        assert_eq!(restored.variables, question.variables);
    }
}
