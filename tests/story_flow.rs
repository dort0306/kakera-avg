use kakera_avg::{
    controller::{Intent, reduce},
    loader::parse_story,
    model::{Session, Slot},
};
use std::path::Path;

const STORY: &str = include_str!("../doc/rust-mvc-example/story.xml");
const FLOW: &str = include_str!("../doc/rust-mvc-example/flow.xml");

#[test]
fn both_branches_restore_complete_frames_and_end_explicitly() {
    let story = parse_story(STORY, FLOW, Path::new("demo")).unwrap().story;
    let question = reduce(&story, &story.start(), Intent::Advance).unwrap();
    assert_eq!(question.frame_id, "question");
    assert_eq!(
        story.frame(&question),
        story.frame(&Session {
            frame_id: "question".into()
        })
    );
    assert_eq!(
        story.frame(&question).unwrap().visual.portraits[0].slot,
        Slot::Center
    );
    assert_eq!(
        reduce(&story, &question, Intent::Advance).unwrap(),
        question
    );
    assert!(reduce(&story, &question, Intent::Choose("typo".into())).is_err());
    assert_eq!(question.frame_id, "question");
    assert!(reduce(&story, &story.start(), Intent::Choose("accept".into())).is_err());
    for branch in ["accept", "refuse"] {
        let end = reduce(&story, &question, Intent::Choose(branch.into())).unwrap();
        assert_eq!(end.frame_id, branch);
        assert_eq!(reduce(&story, &end, Intent::Advance).unwrap(), end);
        assert!(reduce(&story, &end, Intent::Choose(branch.into())).is_err());
    }
    let refuse = &story.frames["refuse"];
    assert!(refuse.speaker.is_none());
    assert!(refuse.visual.portraits.is_empty());
}

#[test]
fn omitted_visual_means_empty_not_inherited() {
    let xml = STORY.replace(
        "id=\"accept\" visual=\"room.lin\" speaker=\"lin\"",
        "id=\"accept\"",
    );
    let story = parse_story(&xml, FLOW, Path::new("demo")).unwrap().story;
    let question = reduce(&story, &story.start(), Intent::Advance).unwrap();
    let end = reduce(&story, &question, Intent::Choose("accept".into())).unwrap();
    let frame = story.frame(&end).unwrap();
    assert!(frame.visual.background.is_none());
    assert!(frame.visual.portraits.is_empty());
    assert!(frame.speaker.is_none());
}

#[test]
fn bad_content_has_filename_and_position() {
    let cases = [
        ("schema=\"1\"", "schema=\"2\""),
        ("content-version=\"1\"", "content-version=\"0\""),
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
        (
            "<portrait character=\"lin\" image=\"portrait.lin\" slot=\"center\" />",
            "<portrait character=\"lin\" image=\"portrait.lin\" slot=\"center\" /><portrait character=\"lin\" image=\"portrait.lin\" slot=\"center\" />",
        ),
        (
            "<text>要一起去看看吗？</text>",
            "<text><b>要一起去看看吗？</b></text>",
        ),
        ("<text>那就走吧。</text>", "<text> </text>"),
        ("<options>", "<options><option id=\"accept\">重复</option>"),
        ("src=\"assets/bg/room.png\"", "src=\"../room.png\""),
        (
            "<font id=\"dialogue\" src=\"assets/fonts/chinese.ttf\" />",
            "",
        ),
    ];
    for (from, to) in cases {
        assert!(STORY.contains(from), "fixture mismatch: {from}");
        let error = parse_story(&STORY.replace(from, to), FLOW, Path::new("demo"))
            .err()
            .expect(from)
            .to_string();
        assert!(error.starts_with("story.xml:"), "{from}: {error}");
        assert!(
            error.split(':').nth(1).unwrap().parse::<usize>().is_ok(),
            "{error}"
        );
    }
}

#[test]
fn bad_routes_are_rejected_before_playback() {
    let cases = [
        ("schema=\"1\"", "schema=\"9\""),
        ("story=\"kakera-demo\"", "story=\"another\""),
        ("entry=\"opening\"", "entry=\"missing\""),
        ("to=\"question\"", "to=\"missing\""),
        ("from=\"opening\"", "from=\"missing\""),
        ("option=\"accept\"", "option=\"typo\""),
        (
            "<end frame=\"accept\" />",
            "<end frame=\"accept\" /><end frame=\"accept\" />",
        ),
        ("<end frame=\"refuse\" />", ""),
        ("<on option=\"refuse\" to=\"refuse\" />", ""),
        (
            "<on option=\"refuse\" to=\"refuse\" />",
            "<on option=\"accept\" to=\"refuse\" />",
        ),
        (
            "<end frame=\"accept\" />",
            "<choice from=\"accept\"><on option=\"x\" to=\"opening\"/></choice>",
        ),
        (
            "<choice from=\"question\">",
            "<choice from=\"question\" unknown=\"true\">",
        ),
    ];
    for (from, to) in cases {
        assert!(FLOW.contains(from));
        let error = parse_story(STORY, &FLOW.replace(from, to), Path::new("demo"))
            .err()
            .expect(from)
            .to_string();
        assert!(error.contains(".xml:"), "{from}: {error}");
    }
    let no_choice = FLOW.replace("<choice from=\"question\">\n    <on option=\"accept\" to=\"accept\" />\n    <on option=\"refuse\" to=\"refuse\" />\n  </choice>", "<next from=\"question\" to=\"accept\"/>");
    assert!(parse_story(STORY, &no_choice, Path::new("demo")).is_err());
}

#[test]
fn text_preserves_spaces_entities_and_normalizes_xml_newlines() {
    let xml = STORY.replace(
        "<text>那就走吧。</text>",
        "<text>  第一行 &amp; 第二行\r\n末行  </text>",
    );
    let story = parse_story(&xml, FLOW, Path::new("demo")).unwrap().story;
    assert_eq!(story.frames["accept"].text, "  第一行 & 第二行\n末行  ");
}

#[test]
fn cycles_are_allowed_and_unreachable_frames_warn() {
    let flow = FLOW.replace(
        "from=\"opening\" to=\"question\"",
        "from=\"opening\" to=\"opening\"",
    );
    let loaded = parse_story(STORY, &flow, Path::new("demo")).unwrap();
    assert_eq!(loaded.warnings.len(), 3);
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.contains("question"))
    );
}

#[test]
fn malformed_xml_reports_source() {
    let error = parse_story("<story>", FLOW, Path::new("demo"))
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("story.xml"));
}
