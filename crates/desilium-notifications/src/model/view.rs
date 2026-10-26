//! Read-side helpers for the shell: grouping the history by application.

use super::Notification;

/// All notifications from one application, newest first.
#[derive(Debug, Clone, PartialEq)]
pub struct AppGroup<'a> {
    pub app_key: &'a str,
    pub app_name: &'a str,
    pub unread: usize,
    pub items: Vec<&'a Notification>,
}

/// Groups newest-first notifications by application.
///
/// Groups appear in the order their newest member appears, so the most
/// recently active application is first.
pub fn group_by_app<'a, I>(newest_first: I) -> Vec<AppGroup<'a>>
where
    I: IntoIterator<Item = &'a Notification>,
{
    let mut groups: Vec<AppGroup<'a>> = Vec::new();
    for item in newest_first {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.app_key == item.app_key)
        {
            push_into(group, item);
        } else {
            groups.push(AppGroup {
                app_key: &item.app_key,
                app_name: &item.app_name,
                unread: usize::from(!item.read),
                items: vec![item],
            });
        }
    }
    groups
}

fn push_into<'a>(group: &mut AppGroup<'a>, item: &'a Notification) {
    if !item.read {
        group.unread += 1;
    }
    group.items.push(item);
}
