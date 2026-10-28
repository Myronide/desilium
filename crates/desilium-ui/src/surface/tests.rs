//! Behaviour tests that need no display or GPU.

use super::*;
use crate::input::{DragPhase, Key, KeyEvent, PointerButton, UiEvent};

fn surface(html: &str, css: &str, w: u32, h: u32) -> UiSurface {
    UiSurface::new(html, &[css], SurfaceOptions::new(w, h)).unwrap()
}

fn draw(surface: &mut UiSurface) -> (Vec<u8>, crate::RenderStats) {
    let stride = surface.min_stride();
    let (_, height) = surface.buffer_size();
    let mut buffer = vec![0xEE; stride * height as usize];
    let stats = surface.render(&mut buffer, stride).unwrap();
    (buffer, stats)
}

fn pixel(buffer: &[u8], stride: usize, x: usize, y: usize) -> [u8; 4] {
    let at = y * stride + x * 4;
    [buffer[at], buffer[at + 1], buffer[at + 2], buffer[at + 3]]
}

const BOX_CSS: &str =
    "html,body{margin:0} .box{position:absolute;left:10px;top:10px;width:40px;height:20px}";

#[test]
fn empty_document_is_fully_transparent() {
    let mut s = surface("", "", 32, 16);
    let (buf, stats) = draw(&mut s);
    assert!(stats.rendered);
    assert!(buf.as_chunks::<4>().0.iter().all(|p| *p == [0, 0, 0, 0]));
}

#[test]
fn opaque_pixels_are_bgra() {
    let mut s = surface(
        "<div class=box style='background:rgb(10,20,200)'></div>",
        BOX_CSS,
        64,
        48,
    );
    let (buf, _) = draw(&mut s);
    assert_eq!(pixel(&buf, s.min_stride(), 20, 20), [200, 20, 10, 255]);
}

#[test]
fn translucent_pixels_are_premultiplied() {
    let mut s = surface(
        "<div class=box style='background:rgba(255,0,0,0.5)'></div>",
        BOX_CSS,
        64,
        48,
    );
    let (buf, _) = draw(&mut s);
    let [b, g, r, a] = pixel(&buf, s.min_stride(), 20, 20);
    assert!((i32::from(a) - 128).abs() <= 2, "alpha {a}");
    assert!(
        (i32::from(r) - 128).abs() <= 2,
        "red {r} must be premultiplied"
    );
    assert_eq!((b, g), (0, 0));
}

#[test]
fn stride_larger_than_a_row_is_respected() {
    let mut s = surface(
        "<div class=box style='background:#0f0'></div>",
        BOX_CSS,
        64,
        48,
    );
    let stride = s.min_stride() + 32;
    let mut buffer = vec![0xEE; stride * 48];
    s.render(&mut buffer, stride).unwrap();
    assert_eq!(pixel(&buffer, stride, 20, 20), [0, 255, 0, 255]);
    assert_eq!(
        &buffer[s.min_stride()..stride],
        &[0xEE; 32][..],
        "padding untouched"
    );
}

#[test]
fn small_buffers_are_rejected() {
    let mut s = surface("", "", 64, 48);
    let mut buffer = vec![0; 100];
    assert!(matches!(
        s.render(&mut buffer, 256),
        Err(crate::UiError::BufferTooSmall { .. })
    ));
    let mut buffer = vec![0; 64 * 48 * 4];
    assert!(s.render(&mut buffer, 10).is_err());
}

#[test]
fn renders_only_when_dirty() {
    let mut s = surface("<p id=t>a</p>", "", 64, 48);
    assert!(s.needs_render());
    assert!(draw(&mut s).1.rendered);
    assert!(!s.needs_render());
    let (_, stats) = draw(&mut s);
    assert!(!stats.rendered);
    s.set_text("#t", "b").unwrap();
    assert!(s.needs_render());
}

#[test]
fn text_update_reports_a_damage_rect() {
    let mut s = surface(
        "<p id=t style='margin:0;font-size:20px;color:#fff'>a</p>",
        "",
        120,
        60,
    );
    let (_, first) = draw(&mut s);
    assert_eq!(first.damage, Some(s.full_damage()));
    s.set_text("#t", "WWWW").unwrap();
    let (_, second) = draw(&mut s);
    let damage = second.damage.expect("text changed");
    assert!(damage.width > 0 && damage.height < 60);
    let (_, third) = {
        s.mark_dirty();
        draw(&mut s)
    };
    assert_eq!(third.damage, None, "identical frame has no damage");
}

#[test]
fn dom_updates_apply_and_report_misses() {
    let mut s = surface(
        "<div id=a class='x y' data-key=k></div><div class=x></div>",
        "",
        32,
        32,
    );
    s.set_class("#a", "y", false).unwrap();
    s.set_class("[data-key=k]", "z", true).unwrap();
    s.set_attr("#a", "title", "hi").unwrap();
    s.set_style_var("#a", "--v", "3px").unwrap();
    s.replace_children("#a", "<b id=in>x</b>").unwrap();
    s.set_text("#in", "text").unwrap();
    assert!(matches!(
        s.set_text("#missing", "x"),
        Err(crate::UiError::NoMatch(_))
    ));
    assert!(matches!(
        s.set_text("div p", "x"),
        Err(crate::UiError::InvalidSelector(_))
    ));
    assert!(matches!(
        s.set_style_var("#a", "color", "red"),
        Err(crate::UiError::InvalidSelector(_))
    ));
    let batch = Updates::new()
        .text("#in", "again")
        .class(".x", "on", true)
        .text("#nope", "x");
    assert!(s.apply(batch).is_err());
    let html = s.dump_html_for_tests();
    assert!(html.contains("again"), "{html}");
    assert!(
        html.contains("x z") || html.contains("x on z") || html.contains("z"),
        "{html}"
    );
}

#[test]
fn clicks_become_actions_only_on_the_same_element() {
    let css = "html,body{margin:0} button{position:absolute;top:0;width:50px;height:30px;border:0;padding:0}";
    let html = "<button id=a data-action=go data-arg=one style='left:0'>A</button>\
                <button data-action=other style='left:60px'>B</button><span id=plain style='position:absolute;top:40px'>plain</span>";
    let mut s = surface(html, css, 200, 100);
    draw(&mut s);
    s.pointer_down(10.0, 10.0, PointerButton::Primary);
    s.pointer_up(12.0, 12.0, PointerButton::Primary);
    let actions = s.take_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, "go");
    assert_eq!(actions[0].arg.as_deref(), Some("one"));
    assert_eq!(actions[0].element_id.as_deref(), Some("a"));
    assert_eq!((actions[0].rect.x, actions[0].rect.width), (0.0, 50.0));

    s.pointer_down(10.0, 10.0, PointerButton::Primary);
    s.pointer_up(70.0, 10.0, PointerButton::Primary);
    assert!(
        s.take_actions().is_empty(),
        "release on another element is no click"
    );

    s.pointer_down(100.0, 80.0, PointerButton::Primary);
    s.pointer_up(100.0, 80.0, PointerButton::Primary);
    assert!(s.take_actions().is_empty());

    s.pointer_down(10.0, 10.0, PointerButton::Secondary);
    s.pointer_up(10.0, 10.0, PointerButton::Secondary);
    assert!(
        s.take_actions().is_empty(),
        "only the primary button clicks"
    );
}

#[test]
fn disabled_elements_do_not_emit_actions() {
    let css = "html,body{margin:0} button{width:50px;height:30px;border:0}";
    let mut s = surface("<button data-action=go disabled>A</button>", css, 100, 50);
    draw(&mut s);
    s.pointer_down(5.0, 5.0, PointerButton::Primary);
    s.pointer_up(5.0, 5.0, PointerButton::Primary);
    assert!(s.take_actions().is_empty());
}

#[test]
fn hover_restyles_and_sets_cursor_and_tooltip() {
    let css = "html,body{margin:0} .b{position:absolute;left:0;top:0;width:50px;height:30px;background:#000;cursor:pointer} .b:hover{background:#fff}";
    let html = "<div class=b data-action=x data-tooltip='Hello'></div>";
    let mut s = surface(html, css, 100, 50);
    let (before, _) = draw(&mut s);
    assert_eq!(pixel(&before, s.min_stride(), 5, 5), [0, 0, 0, 255]);
    assert_eq!(s.cursor_hint(), crate::CursorHint::Default);
    s.pointer_move(5.0, 5.0);
    assert!(s.needs_render());
    let (after, _) = draw(&mut s);
    assert_eq!(pixel(&after, s.min_stride(), 5, 5), [255, 255, 255, 255]);
    assert_eq!(s.cursor_hint(), crate::CursorHint::Pointer);
    let target = s.hover_target().unwrap();
    assert_eq!(target.tooltip.as_deref(), Some("Hello"));
    assert_eq!(target.action.as_deref(), Some("x"));
    s.pointer_leave();
    let (left, _) = draw(&mut s);
    assert_eq!(pixel(&left, s.min_stride(), 5, 5), [0, 0, 0, 255]);
    assert!(s.hover_target().is_none());
}

#[test]
fn text_input_edits_and_reports_changes() {
    let css = "html,body{margin:0} input{position:absolute;left:0;top:0;width:150px;height:24px;font-size:14px}";
    let mut s = surface("<input id=q data-key=search data-submit=run>", css, 200, 60);
    draw(&mut s);
    s.pointer_down(20.0, 10.0, PointerButton::Primary);
    s.pointer_up(20.0, 10.0, PointerButton::Primary);
    for ch in ["h", "i", "!"] {
        s.key_down(&KeyEvent::character(ch));
        s.key_up(&KeyEvent::character(ch));
    }
    assert_eq!(s.input_value("#q").unwrap(), "hi!");
    s.key_down(&KeyEvent::named(Key::Backspace));
    assert_eq!(s.input_value("#q").unwrap(), "hi");
    s.key_down(&KeyEvent::named(Key::ArrowLeft));
    s.key_down(&KeyEvent::character("X"));
    assert_eq!(s.input_value("#q").unwrap(), "hXi");
    s.key_down(&KeyEvent::named(Key::Enter));
    let events = s.take_events();
    let changes: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            UiEvent::TextChanged { value, key, .. } => Some((value.as_str(), key.as_deref())),
            _ => None,
        })
        .collect();
    assert_eq!(changes.last(), Some(&("hXi", Some("search"))), "{events:?}");
    assert!(events.iter().any(
        |e| matches!(e, UiEvent::Submit { action, value } if action == "run" && value == "hXi")
    ));
    s.set_input_value("#q", "reset").unwrap();
    assert_eq!(s.input_value("#q").unwrap(), "reset");
}

#[test]
fn theme_changes_flow_into_the_document() {
    let css = "html,body{margin:0} .box{position:absolute;inset:0;background:var(--accent)}";
    let mut s = surface("<div class=box></div>", css, 16, 16);
    let (buf, _) = draw(&mut s);
    assert_eq!(pixel(&buf, s.min_stride(), 4, 4), [0xFF, 0x61, 0x7B, 255]);
    let mut theme = desilium_theme::Theme::dark();
    theme.palette.accent = desilium_theme::Color::rgb(0xFF8800);
    s.set_theme(&theme);
    assert!(s.needs_render());
    let (buf, _) = draw(&mut s);
    assert_eq!(pixel(&buf, s.min_stride(), 4, 4), [0x00, 0x88, 0xFF, 255]);
}

#[test]
fn resize_and_scale_change_the_buffer() {
    let mut s = surface(
        "<div style='width:10px;height:10px;background:#fff'></div>",
        "html,body{margin:0}",
        40,
        20,
    );
    assert_eq!(s.buffer_size(), (40, 20));
    s.set_scale(2.0).unwrap();
    assert_eq!(s.buffer_size(), (80, 40));
    assert_eq!(s.min_stride(), 320);
    s.resize(50, 30).unwrap();
    assert_eq!(s.buffer_size(), (100, 60));
    let (buf, stats) = draw(&mut s);
    assert_eq!(stats.damage, Some(s.full_damage()));
    assert_eq!(pixel(&buf, s.min_stride(), 15, 15), [255, 255, 255, 255]);
    assert_eq!(pixel(&buf, s.min_stride(), 25, 15), [0, 0, 0, 0]);
    assert!(s.resize(0, 10).is_err());
    assert!(s.set_scale(100.0).is_err());
}

#[test]
fn inline_icons_render_without_a_sprite() {
    let icon = crate::icon_svg("bell", 24, "");
    let css = "html,body{margin:0} svg{color:#fff}";
    let mut s = surface(&icon, css, 24, 24);
    let (buf, _) = draw(&mut s);
    let lit = buf.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
    assert!(lit > 20, "icon drew {lit} pixels");
}

impl UiSurface {
    fn dump_html_for_tests(&self) -> String {
        let mut out = String::new();
        let mut stack = vec![self.doc.root_node().id];
        while let Some(id) = stack.pop() {
            let Some(node) = self.doc.get_node(id) else {
                continue;
            };
            if let Some(attrs) = node.attrs() {
                out.push_str(&format!(
                    "<{:?}",
                    node.element_data().map(|e| e.name.local.to_string())
                ));
                for a in attrs {
                    out.push_str(&format!(" {}={}", a.name.local, a.value));
                }
                out.push('>');
            } else {
                out.push_str(&node.text_content());
            }
            stack.extend(node.children.iter().rev());
        }
        out
    }
}

fn input_surface() -> UiSurface {
    let css = "html,body{margin:0} input{position:absolute;left:0;top:0;width:150px;height:24px;font-size:14px}";
    let mut s = surface("<input id=q value='hello world'>", css, 200, 60);
    draw(&mut s);
    s.focus("#q").unwrap();
    s
}

fn press(s: &mut UiSurface, event: KeyEvent) {
    s.key_down(&event);
    s.key_up(&event);
}

fn with_ctrl(text: &str) -> KeyEvent {
    let mut event = KeyEvent::character(text);
    event.modifiers.ctrl = true;
    event.text = None;
    event
}

#[test]
fn input_supports_selection_and_deletion() {
    let mut s = input_surface();
    press(&mut s, KeyEvent::named(Key::End));
    for _ in 0..5 {
        let mut event = KeyEvent::named(Key::ArrowLeft);
        event.modifiers.shift = true;
        press(&mut s, event);
    }
    press(&mut s, KeyEvent::named(Key::Backspace));
    assert_eq!(
        s.input_value("#q").unwrap(),
        "hello ",
        "shift+arrows select, backspace deletes the selection"
    );
    press(&mut s, with_ctrl("a"));
    press(&mut s, KeyEvent::character("x"));
    assert_eq!(s.input_value("#q").unwrap(), "x", "ctrl+a selects all");
}

#[test]
fn input_clipboard_round_trips() {
    let mut s = input_surface();
    press(&mut s, with_ctrl("a"));
    press(&mut s, with_ctrl("c"));
    assert_eq!(s.clipboard_text(), "hello world");
    press(&mut s, KeyEvent::named(Key::End));
    press(&mut s, with_ctrl("v"));
    assert_eq!(s.input_value("#q").unwrap(), "hello worldhello world");
}

#[test]
fn ime_commit_inserts_text_and_preedit_does_not_change_the_value() {
    let mut s = input_surface();
    press(&mut s, KeyEvent::named(Key::End));
    s.ime_preedit("ni", Some((2, 2)));
    assert_eq!(s.input_value("#q").unwrap(), "hello world");
    s.ime_commit("\u{4f60}\u{597d}");
    assert_eq!(s.input_value("#q").unwrap(), "hello world\u{4f60}\u{597d}");
}

#[test]
fn losing_focus_hides_the_caret_and_regaining_restores_it() {
    let mut s = input_surface();
    assert!(s.wants_text_input());
    s.focus_changed(false);
    press(&mut s, KeyEvent::character("z"));
    assert_eq!(
        s.input_value("#q").unwrap(),
        "hello world",
        "unfocused surface ignores typing"
    );
    s.focus_changed(true);
    press(&mut s, KeyEvent::named(Key::End));
    press(&mut s, KeyEvent::character("z"));
    assert_eq!(s.input_value("#q").unwrap(), "hello worldz");
}

#[test]
fn bundled_fonts_are_validated() {
    let mut options = SurfaceOptions::new(10, 10);
    options.fonts = vec![b"not a font".to_vec()];
    assert!(matches!(
        UiSurface::new("", &[], options),
        Err(crate::UiError::BadFont)
    ));
}

#[test]
fn element_rect_reports_the_laid_out_box() {
    let mut s = surface("<div class=box id=b></div>", BOX_CSS, 64, 48);
    let rect = s.element_rect("#b").unwrap();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (10.0, 10.0, 40.0, 20.0)
    );
    s.set_attr("#b", "style", "width:30px").unwrap();
    assert_eq!(s.element_rect("#b").unwrap().width, 30.0);
    assert!(s.element_rect("#missing").is_err());
}

#[test]
fn middle_click_fires_only_the_middle_action() {
    let mut s = surface(
        "<div class=box data-action=open data-middle-action=open-new data-arg=x></div>",
        BOX_CSS,
        64,
        48,
    );
    s.pointer_down(20.0, 20.0, PointerButton::Middle);
    s.pointer_up(20.0, 20.0, PointerButton::Middle);
    let actions = s.take_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, "open-new");
    assert_eq!(actions[0].arg.as_deref(), Some("x"));
    s.pointer_down(20.0, 20.0, PointerButton::Primary);
    s.pointer_up(20.0, 20.0, PointerButton::Primary);
    assert_eq!(s.take_actions()[0].action, "open");
}

fn drags(ui: &mut UiSurface) -> Vec<crate::UiDrag> {
    ui.take_events()
        .into_iter()
        .filter_map(|event| match event {
            UiEvent::Drag(drag) => Some(drag),
            _ => None,
        })
        .collect()
}

#[test]
fn a_drag_goes_on_when_the_pressed_element_is_replaced_and_no_event_needs_a_layout_first() {
    let mut ui = surface(
        "<div id=root><div class=box data-drag=icon data-arg=7></div></div>",
        BOX_CSS,
        64,
        48,
    );
    ui.pointer_move(20.0, 20.0);
    ui.pointer_down(20.0, 20.0, PointerButton::Primary);
    ui.pointer_move(30.0, 25.0);
    // The host redraws between two events, as the shell does after every move of a drag; no
    // render happens before the next event.
    ui.replace_children("#root", "<div class=box data-drag=icon data-arg=7></div>")
        .unwrap();
    ui.pointer_move(40.0, 30.0);
    ui.replace_children("#root", "<div class=box data-drag=icon data-arg=7></div>")
        .unwrap();
    ui.pointer_up(40.0, 30.0, PointerButton::Primary);
    ui.replace_children("#root", "").unwrap();
    ui.wheel(40.0, 30.0, crate::WheelDelta::Pixels { x: 0.0, y: 5.0 });
    let events = drags(&mut ui);
    assert!(
        events
            .iter()
            .all(|drag| drag.name == "icon" && drag.arg.as_deref() == Some("7")),
        "{events:?}"
    );
    assert_eq!(
        events.first().map(|drag| drag.phase),
        Some(DragPhase::Start)
    );
    assert_eq!(events.last().map(|drag| drag.phase), Some(DragPhase::End));
}

#[test]
fn a_press_that_hardly_moves_is_a_click_and_no_drag() {
    let mut ui = surface(
        "<div class=box data-action=open data-drag=icon data-arg=7></div>",
        BOX_CSS,
        64,
        48,
    );
    ui.pointer_move(20.0, 20.0);
    ui.pointer_down(20.0, 20.0, PointerButton::Primary);
    ui.pointer_move(22.0, 21.0);
    ui.pointer_up(22.0, 21.0, PointerButton::Primary);
    let events = ui.take_events();
    assert!(
        matches!(events.as_slice(), [UiEvent::Action(action)] if action.action == "open"),
        "{events:?}"
    );
}

#[test]
fn moving_past_the_threshold_drags_and_swallows_the_click() {
    let mut ui = surface(
        "<div class=box data-action=open data-drag=icon data-arg=7></div>",
        BOX_CSS,
        64,
        48,
    );
    ui.pointer_move(20.0, 20.0);
    ui.pointer_down(20.0, 20.0, PointerButton::Primary);
    ui.pointer_move(23.0, 20.0);
    assert!(drags(&mut ui).is_empty(), "still inside the threshold");
    ui.pointer_move(40.0, 30.0);
    ui.pointer_move(45.0, 35.0);
    ui.pointer_up(45.0, 35.0, PointerButton::Primary);
    let all = ui.take_events();
    let phases: Vec<_> = all
        .iter()
        .filter_map(|event| match event {
            UiEvent::Drag(drag) => Some(drag.phase),
            _ => None,
        })
        .collect();
    assert_eq!(
        phases,
        [DragPhase::Start, DragPhase::Move, DragPhase::End],
        "{all:?}"
    );
    assert!(
        !all.iter().any(|event| matches!(event, UiEvent::Action(_))),
        "a drag is not a click"
    );
    let UiEvent::Drag(start) = &all[0] else {
        panic!("first event is the start");
    };
    assert_eq!(
        (start.name.as_str(), start.arg.as_deref()),
        ("icon", Some("7"))
    );
    assert_eq!(start.origin, (20.0, 20.0));
    assert_eq!(start.pos, (40.0, 30.0));
}

#[test]
fn only_elements_with_data_drag_start_drags_and_the_next_press_starts_clean() {
    let mut ui = surface("<div class=box data-action=open></div>", BOX_CSS, 64, 48);
    ui.pointer_move(20.0, 20.0);
    ui.pointer_down(20.0, 20.0, PointerButton::Primary);
    ui.pointer_move(50.0, 40.0);
    ui.pointer_up(50.0, 40.0, PointerButton::Primary);
    assert!(drags(&mut ui).is_empty());

    let mut ui = surface("<div class=box data-drag=icon></div>", BOX_CSS, 64, 48);
    ui.pointer_move(20.0, 20.0);
    ui.pointer_down(20.0, 20.0, PointerButton::Primary);
    ui.pointer_move(50.0, 40.0);
    ui.pointer_up(50.0, 40.0, PointerButton::Primary);
    let phases: Vec<_> = drags(&mut ui).iter().map(|drag| drag.phase).collect();
    assert_eq!(
        phases,
        [DragPhase::Start, DragPhase::End],
        "one jump past the threshold starts the drag, the release ends it"
    );
    ui.pointer_move(30.0, 20.0);
    assert!(
        drags(&mut ui).is_empty(),
        "moving after the drop is not a drag"
    );
}

#[test]
fn leaving_the_surface_ends_a_drag() {
    let mut ui = surface("<div class=box data-drag=icon></div>", BOX_CSS, 64, 48);
    ui.pointer_move(20.0, 20.0);
    ui.pointer_down(20.0, 20.0, PointerButton::Primary);
    ui.pointer_move(40.0, 20.0);
    ui.pointer_leave();
    let phases: Vec<_> = drags(&mut ui).iter().map(|drag| drag.phase).collect();
    assert_eq!(phases, [DragPhase::Start, DragPhase::End]);
}

#[test]
fn middle_click_without_the_attribute_does_nothing() {
    let mut s = surface("<div class=box data-action=open></div>", BOX_CSS, 64, 48);
    s.pointer_down(20.0, 20.0, PointerButton::Middle);
    s.pointer_up(20.0, 20.0, PointerButton::Middle);
    assert!(s.take_actions().is_empty());
}

#[test]
fn secondary_click_fires_only_an_explicit_context_action() {
    let mut ui = surface(
        "<div class=box data-action=open data-secondary-action=context data-arg=file></div>",
        BOX_CSS,
        64,
        48,
    );
    ui.pointer_down(20.0, 20.0, PointerButton::Secondary);
    ui.pointer_up(20.0, 20.0, PointerButton::Secondary);
    let actions = ui.take_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, "context");
    assert_eq!(actions[0].arg.as_deref(), Some("file"));
    ui.pointer_down(20.0, 20.0, PointerButton::Primary);
    ui.pointer_up(20.0, 20.0, PointerButton::Primary);
    assert_eq!(ui.take_actions()[0].action, "open");
}
