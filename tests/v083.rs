use anyhow::Result;
use kakera_avg::{
    app::PlayerApp,
    content::{ContentSource, DirectorySource},
    debug::Server,
    package::{self, Archive, StagedProject},
    runtime::{Bundle, LoadOptions},
    save::SaveStore,
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};

const TOKEN: &str = "test_debug_token_01234567890123456789";
fn options() -> LoadOptions {
    LoadOptions {
        core: None,
        no_live2d: true,
    }
}
fn game(root: &Path) -> Result<()> {
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
        root.join("font.otf"),
    )?;
    image::RgbaImage::new(4, 4).save(root.join("bg.png"))?;
    std::fs::write(
        root.join("story.xml"),
        r#"<story schema="8" id="v083" content-version="8" entry="opening"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/><image id="bg" src="bg.png"/></assets><characters/><visuals><visual id="room" background="bg"/></visuals><variables><int id="count" initial="0" min="0" max="2"/></variables><frames><frame id="opening" visual="room"><text>开场。</text></frame><frame id="question" name="question.alias" visual="room"><text>选择。</text><options><option id="again">循环</option><option id="end">结束</option><option id="bad">坏目标</option></options></frame><frame id="ending"><text>结束。</text></frame></frames><next end="true"/></story>"#,
    )?;
    std::fs::write(
        root.join("ui.xml"),
        r#"<ui schema="3" initial="hud" title-bind="ui.title"><variables><string id="title" initial="初始"/></variables><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/></screen></ui>"#,
    )?;
    std::fs::write(
        root.join("story.js"),
        r#"on.advance('opening',()=> 'question.alias');on.choice('question.alias','again',ctx=>{ctx.add('count',1n);return 'question.alias';},ctx=>ctx.vars.count<2n);on.choice('question.alias','end',()=> 'ending',ctx=>ctx.vars.count>=1n);on.choice('question.alias','bad',()=> 'missing');on.end('ending');on.ui('game.enter',ctx=>ctx.setUi('title',ctx.game.frameId));"#,
    )?;
    Ok(())
}
fn exchange(address: SocketAddr, request: Value) -> Result<Value> {
    let mut socket = TcpStream::connect(address)?;
    socket.set_read_timeout(Some(Duration::from_secs(8)))?;
    writeln!(socket, "{request}")?;
    let mut line = String::new();
    BufReader::new(socket).read_line(&mut line)?;
    Ok(serde_json::from_str(&line)?)
}
fn request(id: u64, command: Value, revision: Option<u64>) -> Value {
    json!({"version":1,"id":id,"token":TOKEN,"expected_revision":revision,"command":command})
}
fn call(app: &mut PlayerApp, address: SocketAddr, request: Value) -> Result<Value> {
    let task = std::thread::spawn(move || exchange(address, request));
    let until = Instant::now() + Duration::from_secs(8);
    while !task.is_finished() {
        app.poll_debug();
        assert!(Instant::now() < until, "debug response timeout");
        std::thread::sleep(Duration::from_millis(2));
    }
    task.join().unwrap()
}
fn control(
    app: &mut PlayerApp,
    address: SocketAddr,
    id: &mut u64,
    state: &mut Value,
    command: Value,
) -> Result<Value> {
    *id += 1;
    let reply = call(
        app,
        address,
        request(*id, command, state["state_revision"].as_u64()),
    )?;
    if reply["ok"] == true {
        *state = reply["result"].clone();
    }
    Ok(reply)
}
#[test]
fn dependency_selection_excludes_private_large_files_and_cli_packs_both_formats() -> Result<()> {
    let root = tempfile::tempdir()?;
    let output = tempfile::tempdir()?;
    game(root.path())?;
    std::fs::write(root.path().join("secret.env"), "PRIVATE")?;
    std::fs::create_dir(root.path().join(".editor"))?;
    std::fs::write(root.path().join(".editor/index.json"), "private")?;
    std::fs::File::create(root.path().join("unused.gguf"))?.set_len(600 * 1024 * 1024)?;
    std::fs::create_dir_all(root.path().join("assets/licenses"))?;
    std::fs::write(
        root.path().join("assets/licenses/font.txt"),
        "font permission",
    )?;
    std::fs::write(root.path().join("notes.txt"), "EXPLICIT")?;
    std::fs::write(
        root.path().join("kakera.package.json"),
        r#"{"schema":1,"include":["notes.txt"]}"#,
    )?;
    let selected = package::selection::Selection::discover(root.path(), &options())?;
    assert!(!selected.paths.contains(Path::new("unused.gguf")));
    for format in [1, 2] {
        let path = output.path().join(format!("release-{format}.kpak"));
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_kakera-pack"))
            .arg(root.path())
            .arg(&path)
            .args([
                "--format",
                &format.to_string(),
                "--level",
                "1",
                "--no-live2d",
            ])
            .output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let archive = Archive::open(&path)?;
        archive.verify_all()?;
        Bundle::load(&archive, &options())?;
        for name in [
            "secret.env",
            "unused.gguf",
            ".editor/index.json",
            "kakera.package.json",
        ] {
            assert!(archive.read(Path::new(name)).is_err(), "leaked {name}");
        }
        assert_eq!(archive.read(Path::new("notes.txt"))?, b"EXPLICIT");
        assert_eq!(
            archive.read(Path::new("assets/licenses/font.txt"))?,
            b"font permission"
        );
    }
    Ok(())
}
#[test]
fn malformed_manifests_and_source_changes_preserve_old_outputs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let output = tempfile::tempdir()?;
    game(root.path())?;
    let manifest = root.path().join("kakera.package.json");
    for name in ["settings.json", "read.json", "backup.tmp"] {
        std::fs::write(root.path().join(name), "private")?;
    }
    std::fs::create_dir(root.path().join("saves"))?;
    std::fs::write(root.path().join("saves/private.txt"), "private")?;
    for body in [
        r#"{"schema":2,"include":[]}"#,
        r#"{"schema":1,"include":[],"unknown":true}"#,
        r#"{"schema":1,"include":["../secret"]}"#,
        r#"{"schema":1,"include":["missing"]}"#,
        r#"{"schema":1,"include":["bg.png","BG.PNG"]}"#,
        r#"{"schema":1,"include":["."]}"#,
        r#"{"schema":1,"include":["settings.json"]}"#,
        r#"{"schema":1,"include":["read.json"]}"#,
        r#"{"schema":1,"include":["backup.tmp"]}"#,
        r#"{"schema":1,"include":["saves/private.txt"]}"#,
    ] {
        std::fs::write(&manifest, body)?;
        assert!(package::selection::Selection::discover(root.path(), &options()).is_err());
    }
    std::fs::write(&manifest, r#"{"schema":1,"include":[]}"#)?;
    let selection = package::selection::Selection::discover(root.path(), &options())?;
    let mut stage =
        StagedProject::capture_selected(root.path(), selection.paths.iter().cloned().collect())?;
    selection.validate_candidate(&stage, &options())?;
    stage.for_publication()?;
    let path = output.path().join("old.kpak");
    std::fs::write(&path, b"OLD")?;
    std::fs::write(&manifest, r#"{"schema":1,"include":["bg.png"]}"#)?;
    assert!(
        package::write_streaming(&stage, &path, 1, true, |_| selection.ensure_unchanged()).is_err()
    );
    assert_eq!(std::fs::read(path)?, b"OLD");
    assert!(stage.ensure_unchanged().is_err());
    Ok(())
}
#[test]
fn debug_authentication_framing_and_request_replay_are_bounded() -> Result<()> {
    assert!(Server::bind("0.0.0.0:0".parse()?, TOKEN.into()).is_err());
    assert!(Server::bind("127.0.0.1:0".parse()?, "weak".into()).is_err());
    let server = Server::bind("127.0.0.1:0".parse()?, TOKEN.into())?;
    let mut bad = request(100, json!({"type":"status"}), None);
    bad["token"] = json!("wrong");
    assert_eq!(exchange(server.address, bad)?["error"], "unauthorized");
    let mut bad = request(100, json!({"type":"status"}), None);
    bad["version"] = json!(99);
    assert_eq!(exchange(server.address, bad)?["ok"], false);
    let bad = request(100, json!({"type":"status","unknown":true}), None);
    assert_eq!(exchange(server.address, bad)?["error"], "invalid request");
    let address = server.address;
    let req = request(1, json!({"type":"pause"}), Some(0));
    let copy = req.clone();
    let worker = std::thread::spawn(move || exchange(address, req));
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(job) = server.try_next() {
            job.respond(Ok(json!({"once":true})));
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(2));
    }
    let first = worker.join().unwrap()?;
    assert_eq!(exchange(address, copy)?, first);
    assert!(server.try_next().is_none());
    assert_eq!(
        exchange(address, request(1, json!({"type":"resume"}), Some(0)))?["ok"],
        false
    );
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(&vec![b'x'; kakera_avg::debug::MAX_REQUEST + 1])?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    assert!(line.contains("budget"));
    Ok(())
}
#[test]
fn remote_breakpoints_pause_before_commit_and_self_loops_remain_atomic() -> Result<()> {
    let root = tempfile::tempdir()?;
    let saves = tempfile::tempdir()?;
    game(root.path())?;
    let bundle = Bundle::load(&DirectorySource::new(root.path())?, &options())?;
    let ctx = eframe::egui::Context::default();
    let mut app = PlayerApp::new_context(
        &ctx,
        bundle.loaded,
        bundle.assets,
        SaveStore::at(saves.path().to_owned()),
        true,
        bundle.logic,
        kakera_avg::render::Renderer::cpu(),
    )?;
    let server = Server::bind("127.0.0.1:0".parse()?, TOKEN.into())?;
    let address = server.address;
    app.attach_debug(server);
    let mut id = 1;
    let mut state = call(
        &mut app,
        address,
        request(id, json!({"type":"status"}), None),
    )?["result"]
        .clone();
    assert_eq!(state["paused"], true);
    assert_eq!(state["frame_id"], "opening");
    assert!(
        control(
            &mut app,
            address,
            &mut id,
            &mut state,
            json!({"type":"breakpoints","frames":["question.alias"]})
        )?["ok"]
            == true
    );
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"skip_transition"}),
    )?;
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"reveal"}),
    )?;
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"advance"}),
    )?;
    assert_eq!(state["frame_id"], "opening");
    assert_eq!(state["pending_frame"], "question");
    assert_eq!(state["window_title"], "opening");
    assert_eq!(state["history_length"], 1);
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"advance"}),
    )?;
    assert_eq!(state["frame_id"], "question");
    assert_eq!(state["window_title"], "question.alias");
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"skip_transition"}),
    )?;
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"reveal"}),
    )?;
    let before = state.clone();
    assert_eq!(
        control(
            &mut app,
            address,
            &mut id,
            &mut state,
            json!({"type":"choose","option":"end"})
        )?["ok"],
        false
    );
    assert_eq!(state, before);
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"choose","option":"again"}),
    )?;
    assert_eq!(state["variables"]["count"]["value"], "0");
    assert_eq!(state["pending_variables"]["count"]["value"], "1");
    assert_eq!(state["history_length"], 2);
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"advance"}),
    )?;
    assert_eq!(state["variables"]["count"]["value"], "1");
    assert_eq!(state["history_length"], 3);
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"skip_transition"}),
    )?;
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"reveal"}),
    )?;
    assert_eq!(
        control(
            &mut app,
            address,
            &mut id,
            &mut state,
            json!({"type":"choose","option":"bad"})
        )?["ok"],
        false
    );
    id += 1;
    let observed = call(
        &mut app,
        address,
        request(id, json!({"type":"status"}), None),
    )?["result"]
        .clone();
    assert_eq!(observed["variables"], state["variables"]);
    assert_eq!(observed["history_length"], 3);
    state = observed;
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"breakpoints","frames":["ending"]}),
    )?;
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"choose","option":"end"}),
    )?;
    assert_eq!(state["pending_frame"], "ending");
    app.rebuild_cpu_presentation()?;
    id += 1;
    state = call(
        &mut app,
        address,
        request(id, json!({"type":"status"}), None),
    )?["result"]
        .clone();
    assert_eq!(state["pending_frame"], Value::Null);
    assert_eq!(state["frame_id"], "question");
    assert_eq!(state["history_length"], 3);
    id += 1;
    assert_eq!(
        call(
            &mut app,
            address,
            request(id, json!({"type":"resume"}), Some(0))
        )?["ok"],
        false
    );
    assert_eq!(std::fs::read_dir(saves.path())?.count(), 0);
    Ok(())
}

#[test]
fn paused_debug_freezes_presentation_and_blocks_native_story_input() -> Result<()> {
    use eframe::egui::{Event, Key, Modifiers, Pos2, RawInput, Rect, vec2};
    let root = tempfile::tempdir()?;
    let saves = tempfile::tempdir()?;
    game(root.path())?;
    let bundle = Bundle::load(&DirectorySource::new(root.path())?, &options())?;
    let ctx = eframe::egui::Context::default();
    let mut app = PlayerApp::new_context(
        &ctx,
        bundle.loaded,
        bundle.assets,
        SaveStore::at(saves.path().to_owned()),
        true,
        bundle.logic,
        kakera_avg::render::Renderer::cpu(),
    )?;
    let server = Server::bind("127.0.0.1:0".parse()?, TOKEN.into())?;
    let address = server.address;
    app.attach_debug(server);
    for _ in 0..5 {
        let _ = ctx.run(
            RawInput {
                focused: true,
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0))),
                events: vec![Event::Key {
                    key: Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
        std::thread::sleep(Duration::from_millis(15));
    }
    let mut state = call(
        &mut app,
        address,
        request(1, json!({"type":"status"}), None),
    )?["result"]
        .clone();
    assert_eq!(state["frame_id"], "opening");
    assert_eq!(state["text_complete"], false);
    assert_eq!(state["transition_age"], 0.0);
    let mut id = 1;
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"resume"}),
    )?;
    std::thread::sleep(Duration::from_millis(60));
    let _ = ctx.run(
        RawInput {
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
    id += 1;
    state = call(
        &mut app,
        address,
        request(id, json!({"type":"status"}), None),
    )?["result"]
        .clone();
    assert!(state["transition_age"].as_f64().unwrap() > 0.0);
    control(
        &mut app,
        address,
        &mut id,
        &mut state,
        json!({"type":"pause"}),
    )?;
    let age = state["transition_age"].clone();
    std::thread::sleep(Duration::from_millis(60));
    let _ = ctx.run(
        RawInput {
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
    id += 1;
    state = call(
        &mut app,
        address,
        request(id, json!({"type":"status"}), None),
    )?["result"]
        .clone();
    assert_eq!(state["transition_age"], age);
    Ok(())
}

#[test]
fn debug_cli_requires_explicit_project_and_isolated_data_and_rejects_check() -> Result<()> {
    let binary = env!("CARGO_BIN_EXE_kakera-avg");
    for args in [
        vec!["--check", "--debug-listen", "127.0.0.1:19200"],
        vec![
            "--check",
            "--project",
            "games/demo",
            "--debug-token-file",
            "missing",
        ],
        vec![
            "--check",
            "--package",
            "missing.kpak",
            "--debug-listen",
            "127.0.0.1:19200",
            "--debug-token-file",
            "missing",
            "--save-dir",
            "target/v083-cli",
        ],
    ] {
        let output = std::process::Command::new(binary).args(args).output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("调试要求"));
    }
    let help = std::process::Command::new(binary).arg("--help").output()?;
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--debug-listen"));
    Ok(())
}
