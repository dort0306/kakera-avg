mod common;
use common::{SCRIPT, STORY};
use kakera_avg::{
    controller::{Intent, transition_with},
    model::Slot,
};
#[test]
fn current_script_branches_restore_full_frames_and_end_explicitly() {
    let (loaded, logic) = common::game();
    let story = &loaded.story;
    let question = transition_with(story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    assert_eq!(
        story.frames["question"].visual.portraits[0].slot,
        Slot::Center
    );
    assert!(transition_with(story, &question, Intent::Choose("missing".into()), &logic).is_err());
    assert!(
        !transition_with(story, &question, Intent::Advance, &logic)
            .unwrap()
            .entered
    );
    for id in ["accept", "refuse"] {
        let end = transition_with(story, &question, Intent::Choose(id.into()), &logic)
            .unwrap()
            .session;
        assert_eq!(end.frame_id, id);
        assert!(
            !transition_with(story, &end, Intent::Advance, &logic)
                .unwrap()
                .entered
        );
        story.validate_session_with(&end, &logic).unwrap();
    }
    assert!(story.frames["refuse"].visual.portraits.is_empty());
}
#[test]
fn omitted_visual_and_speaker_do_not_inherit_previous_frames() {
    let xml = STORY.replace(
        "id=\"accept\" visual=\"room.lin\" speaker=\"lin\"",
        "id=\"accept\"",
    );
    let (loaded, _) = common::load(&xml, SCRIPT).unwrap();
    let frame = &loaded.story.frames["accept"];
    assert!(
        frame.visual.background.is_none()
            && frame.visual.portraits.is_empty()
            && frame.speaker.is_none()
    );
}
#[test]
fn strict_current_xml_rejects_bad_fields_references_duplicates_and_missing_groups() {
    for (from, to) in [
        ("schema=\"8\"", "schema=\"9\""),
        ("content-version=\"8\"", "content-version=\"0\""),
        ("<characters>", "<characters typo=\"yes\">"),
        ("<visuals>", "<visuals><oops/>"),
        ("<story schema", "<story xmlns=\"urn:typo\" schema"),
        ("id=\"kakera-demo\"", "id=\"../escape\""),
        ("id=\"portrait.lin\"", "id=\"bg.room\""),
        ("id=\"refuse\" visual", "id=\"accept\" visual"),
        ("visual=\"room.lin\"", "visual=\"typo\""),
        ("speaker=\"lin\"", "speaker=\"typo\""),
        ("background=\"bg.room\"", "background=\"dialogue\""),
        ("character=\"lin\"", "character=\"typo\""),
        ("slot=\"center\"", "slot=\"middle\""),
        ("<text>那就走吧。</text>", "<text><b>那就走吧。</b></text>"),
        ("<options>", "<options><option id=\"accept\">重复</option>"),
        ("src=\"assets/bg/room.png\"", "src=\"../room.png\""),
        ("<interface src=\"ui.xml\"/>", ""),
        ("<next end=\"true\"/>", ""),
    ] {
        assert!(STORY.contains(from));
        let error = common::load(&STORY.replace(from, to), SCRIPT)
            .err()
            .expect(from);
        assert!(
            format!("{error:#}").contains("story.xml:"),
            "{from}: {error:#}"
        );
    }
}
#[test]
fn xml_text_preserves_spaces_entities_and_normalizes_newlines() {
    let xml = STORY.replace(
        "<text>那就走吧。</text>",
        "<text>  第一行 &amp; 第二行\r\n末行  </text>",
    );
    assert_eq!(
        common::load(&xml, SCRIPT).unwrap().0.story.frames["accept"].text,
        "  第一行 & 第二行\n末行  "
    );
    assert!(format!("{:#}", common::load("<story>", SCRIPT).err().unwrap()).contains("story.xml"));
}
#[test]
fn every_obsolete_story_schema_is_rejected_without_falling_back_to_flow() {
    for schema in 1..8 {
        let source = common::source(
            &STORY.replace("schema=\"8\"", &format!("schema=\"{schema}\"")),
            SCRIPT,
        );
        assert!(
            format!(
                "{:#}",
                kakera_avg::loader::load_game(&source).err().unwrap()
            )
            .contains("仅支持 story schema 8")
        );
    }
}
