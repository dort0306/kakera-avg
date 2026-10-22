use kakera_avg::{
    controller::{Intent, reduce, transition},
    loader::parse_story,
    model::{MAX_HISTORY, Route},
    presentation::{GatedIntent, Typewriter, gate},
};
use std::{path::Path, time::Duration};

fn story() -> kakera_avg::model::Story {
    parse_story(
        include_str!("../doc/rust-mvc-example/story.xml"),
        include_str!("../doc/rust-mvc-example/flow.xml"),
        Path::new("demo"),
    )
    .unwrap()
    .story
}

#[test]
fn typewriter_uses_graphemes_and_preserves_unicode_sequences() {
    let mut writer = Typewriter::new("中e\u{301}👨‍👩‍👧‍👦文", false);
    assert_eq!(writer.visible_text(), "");
    for expected in ["中", "中e\u{301}", "中e\u{301}👨‍👩‍👧‍👦", "中e\u{301}👨‍👩‍👧‍👦文"]
    {
        writer.tick(Duration::from_millis(100), 10.0);
        assert_eq!(writer.visible_text(), expected);
    }
    assert!(writer.complete());
}

#[test]
fn typewriter_timing_is_frame_rate_independent_and_zero_speed_is_instant() {
    let text = "一二三四五六七八九十";
    let mut a = Typewriter::new(text, false);
    let mut b = Typewriter::new(text, false);
    a.tick(Duration::from_secs(1), 4.0);
    for _ in 0..20 {
        b.tick(Duration::from_millis(50), 4.0);
    }
    assert_eq!(a.visible_text(), "一二三四");
    assert_eq!(a.visible_text(), b.visible_text());
    // Paused time is not submitted by App; resumption advances only its new interval.
    b.tick(Duration::ZERO, 4.0);
    assert_eq!(b.visible_text(), "一二三四");
    b.tick(Duration::from_millis(250), 4.0);
    assert_eq!(b.visible_text(), "一二三四五");
    b.tick(Duration::ZERO, 0.0);
    assert!(b.complete());
    assert!(Typewriter::new(text, true).complete());
}

#[test]
fn first_advance_reveals_second_advance_transitions_and_choices_are_gated() {
    let story = story();
    let mut session = story.start();
    let mut writer = Typewriter::new(&story.frames[&session.frame_id].text, false);
    match gate(Intent::Advance, writer.complete()) {
        GatedIntent::Reveal => writer.finish(),
        _ => panic!("must reveal first"),
    }
    assert_eq!(session, story.start());
    assert_eq!(session.history.len(), 1);
    if let GatedIntent::Dispatch(intent) = gate(Intent::Advance, writer.complete()) {
        session = reduce(&story, &session, intent).unwrap();
    } else {
        panic!("must advance after reveal");
    }
    assert_eq!(session.frame_id, "question");
    assert_eq!(session.history.len(), 2);
    assert!(matches!(
        gate(Intent::Choose("accept".into()), false),
        GatedIntent::Ignore
    ));
}

#[test]
fn history_records_only_real_transitions_and_selected_branch() {
    let story = story();
    let question = transition(&story, &story.start(), Intent::Advance).unwrap();
    assert!(question.entered);
    let ignored = transition(&story, &question.session, Intent::Advance).unwrap();
    assert!(!ignored.entered);
    assert_eq!(ignored.session, question.session);
    assert!(transition(&story, &question.session, Intent::Choose("missing".into())).is_err());
    let end = reduce(&story, &question.session, Intent::Choose("accept".into())).unwrap();
    assert_eq!(end.history.len(), 3);
    assert_eq!(end.history[1].choice_id.as_deref(), Some("accept"));
    assert!(end.history[2].choice_id.is_none());
    assert!(!transition(&story, &end, Intent::Advance).unwrap().entered);
    story.validate_session(&end).unwrap();
    assert_eq!(story.start().history.len(), 1);
}

#[test]
fn self_loops_keep_entry_event_even_when_capped_history_is_identical() {
    let mut story = story();
    story
        .routes
        .insert("opening".into(), Route::Next("opening".into()));
    let mut session = story.start();
    for _ in 0..MAX_HISTORY + 2 {
        let result = transition(&story, &session, Intent::Advance).unwrap();
        assert!(result.entered);
        session = result.session;
    }
    assert_eq!(session.history.len(), MAX_HISTORY);
    let result = transition(&story, &session, Intent::Advance).unwrap();
    assert!(result.entered);
    assert_eq!(result.session, session);
    story.validate_session(&session).unwrap();
}

#[test]
fn forged_choice_and_target_in_saved_history_are_rejected() {
    let story = story();
    let question = reduce(&story, &story.start(), Intent::Advance).unwrap();
    let mut end = reduce(&story, &question, Intent::Choose("accept".into())).unwrap();
    end.history[1].choice_id = Some("refuse".into());
    assert!(story.validate_session(&end).is_err());
    end.history[1].choice_id = Some("invented".into());
    assert!(story.validate_session(&end).is_err());
}

#[test]
fn pause_and_focus_gaps_are_not_added_to_the_presentation_clock() {
    use kakera_avg::presentation::PresentationClock;
    use std::time::Instant;
    let now = Instant::now();
    let mut clock = PresentationClock::new(now);
    assert_eq!(clock.elapsed(now, false), Duration::ZERO);
    assert_eq!(
        clock.elapsed(now + Duration::from_millis(100), false),
        Duration::from_millis(100)
    );
    clock.set_paused(true);
    assert_eq!(
        clock.elapsed(now + Duration::from_secs(10), true),
        Duration::ZERO
    );
    assert_eq!(
        clock.elapsed(now + Duration::from_secs(20), false),
        Duration::ZERO
    );
    assert_eq!(
        clock.elapsed(now + Duration::from_millis(20_100), false),
        Duration::from_millis(100)
    );
}

#[test]
fn hidden_suffix_keeps_final_line_breaks_and_glyph_positions() {
    use eframe::egui::{self, FontId};
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let text = "A long sentence with several words to wrap across rows.";
            let layout = |prefix: &str| {
                ui.fonts_mut(|fonts| {
                    fonts.layout_job(kakera_avg::view::dialogue_job(
                        text,
                        prefix,
                        FontId::proportional(22.0),
                        150.0,
                    ))
                })
            };
            let full = layout(text);
            assert!(full.rows.len() > 1);
            for prefix in ["", "A", "A long", "A long sentence with"] {
                let partial = layout(prefix);
                assert_eq!(partial.size(), full.size());
                for (a, b) in partial.rows.iter().zip(&full.rows) {
                    assert_eq!(a.glyphs.len(), b.glyphs.len());
                    for (a, b) in a.glyphs.iter().zip(&b.glyphs) {
                        assert_eq!(a.pos, b.pos);
                    }
                }
            }
        });
    });
}
