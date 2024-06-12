use nostr_types::{Event, EventKind, PreEvent, Signer, Tag, Unixtime};
use std::collections::HashMap;
use std::ops::Sub;
use std::time::Duration;

const GROUP_A: [(&str, u64, EventKind, &[&[&str]]); 11] = [
    ("limit_test_first", 40, EventKind::TextNote, &[]),
    ("limit_test_third", 50, EventKind::TextNote, &[]),
    ("limit_test_second", 45, EventKind::TextNote, &[]),
    ("limit_test_fourth", 55, EventKind::TextNote, &[]),
    ("metadata_older", 60, EventKind::Metadata, &[]),
    ("metadata_newer", 0, EventKind::Metadata, &[]),
    ("contactlist_newer", 10, EventKind::ContactList, &[]),
    ("contactlist_older", 70, EventKind::ContactList, &[]),
    ("ephemeral", 10, EventKind::Ephemeral(21212), &[]),
    ("older_param_replaceable", 120, EventKind::FollowSets, &[&["d","1"]]),
    ("newer_param_replaceable", 60, EventKind::FollowSets, &[&["d","1"]]),
];

pub fn build_event_group_a(user: &dyn Signer) -> HashMap<&'static str, Event> {
    let mut map: HashMap<&'static str, Event> = HashMap::new();

    for (s, m, k, t) in GROUP_A.iter() {
        let mut tags: Vec<Tag> = Vec::new();
        for tin in t.iter() {
            tags.push(
                Tag::from_strings(
                    tin.iter().map(|s| (*s).to_owned()).collect()
                )
            );
        }

        let pre_event = PreEvent {
            pubkey: user.public_key(),
            created_at: Unixtime::now().unwrap().sub(Duration::new(m * 60, 0)),
            kind: *k,
            tags,
            content: "This is a test.".to_owned(),
        };
        let event = user.sign_event(pre_event).unwrap();
        map.insert(s, event);
    }

    map
}
