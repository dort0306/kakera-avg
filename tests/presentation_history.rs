mod common;
use kakera_avg::{
    controller::{Intent, transition_with},
    presentation::{GatedIntent, Typewriter, gate},
};
use std::time::Duration;
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

#[test]
fn reveal_choose_history_and_invalid_input_share_the_current_script_boundary() {
    let (loaded, logic) = common::game();
    let story = &loaded.story;
    let mut session = story.start();
    let mut writer = Typewriter::new(&story.frames["opening"].text, false);
    assert!(matches!(
        gate(Intent::Advance, writer.complete()),
        GatedIntent::Reveal
    ));
    writer.finish();
    assert_eq!(session.history.len(), 1);
    session = transition_with(story, &session, Intent::Advance, &logic)
        .unwrap()
        .session;
    assert!(matches!(
        gate(Intent::Choose("accept".into()), false),
        GatedIntent::Ignore
    ));
    assert!(
        !transition_with(story, &session, Intent::Advance, &logic)
            .unwrap()
            .entered
    );
    session = transition_with(story, &session, Intent::Choose("accept".into()), &logic)
        .unwrap()
        .session;
    assert_eq!(session.history.len(), 3);
    assert_eq!(session.history[1].choice_id.as_deref(), Some("accept"));
    assert!(
        !transition_with(story, &session, Intent::Advance, &logic)
            .unwrap()
            .entered
    );
    session.history[1].choice_id = Some("refuse".into());
    assert!(story.validate_session_with(&session, &logic).is_err());
}
