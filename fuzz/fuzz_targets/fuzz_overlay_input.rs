#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_server::overlay::{
    ListItem, ListKind, ListOverlay, MenuOverlay, MenuItem, process_list_input, process_menu_input,
};

fuzz_target!(|data: &[u8]| {
    // Fuzz list overlay input processing with a realistic tree structure.
    let mut list = ListOverlay {
        items: vec![
            ListItem {
                display: "session-0: 2 windows".into(),
                command: vec!["switch-client".into(), "-t".into(), "s0".into()],
                indent: 0,
                collapsed: false,
                hidden_children: 0,
                deletable: true,
                delete_command: vec!["kill-session".into(), "-t".into(), "s0".into()],
            },
            ListItem {
                display: "0: bash*".into(),
                command: vec!["select-window".into(), "-t".into(), "s0:0".into()],
                indent: 1,
                collapsed: false,
                hidden_children: 0,
                deletable: true,
                delete_command: vec!["kill-window".into(), "-t".into(), "s0:0".into()],
            },
            ListItem {
                display: "1: vim".into(),
                command: vec!["select-window".into(), "-t".into(), "s0:1".into()],
                indent: 1,
                collapsed: false,
                hidden_children: 0,
                deletable: true,
                delete_command: vec!["kill-window".into(), "-t".into(), "s0:1".into()],
            },
            ListItem {
                display: "session-1: 1 windows".into(),
                command: vec!["switch-client".into(), "-t".into(), "s1".into()],
                indent: 0,
                collapsed: false,
                hidden_children: 0,
                deletable: false,
                delete_command: vec![],
            },
        ],
        selected: 0,
        scroll_offset: 0,
        filter: String::new(),
        filtering: false,
        title: "choose-tree".into(),
        kind: ListKind::Tree,
    };

    // Feed all fuzz bytes through the list input processor
    let mut offset = 0;
    while offset < data.len() {
        let (_, consumed) = process_list_input(&mut list, &data[offset..]);
        offset += consumed.max(1);
    }

    // Fuzz menu overlay input processing
    let mut menu = MenuOverlay {
        items: vec![
            MenuItem {
                name: "New Window".into(),
                key: Some('c'),
                command: vec!["new-window".into()],
            },
            MenuItem { name: String::new(), key: None, command: vec![] },
            MenuItem {
                name: "Kill Window".into(),
                key: Some('&'),
                command: vec!["kill-window".into()],
            },
        ],
        selected: 0,
        title: "Test".into(),
        x: 0,
        y: 0,
        width: 20,
    };

    let mut offset = 0;
    while offset < data.len() {
        let (_, consumed) = process_menu_input(&mut menu, &data[offset..]);
        offset += consumed.max(1);
    }
});
