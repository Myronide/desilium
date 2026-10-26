//! The notification history and every state transition on it.

use std::collections::VecDeque;
use std::num::NonZeroU32;

use super::image::to_rgba;
use super::limits::{
    ACTION_KEY_MAX_CHARS, ACTION_LABEL_MAX_CHARS, ACTIONS_MAX_PAIRS, APP_NAME_MAX_CHARS,
    BODY_MAX_CHARS, HISTORY_CAPACITY, ICON_REF_MAX_CHARS, MAX_VISIBLE_TOASTS, clip, line_or_none,
};
use super::markup;
use super::policy::{self, RateLimit, RateLimiter};
use super::{
    Action, CloseReason, Event, Icon, Incoming, ModelError, Notification, NotificationId, Notified,
    Timestamp, Urgency,
};

/// Tunables for the model. `Default` gives the values from the design brief.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub history_capacity: usize,
    pub rate: RateLimit,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            history_capacity: HISTORY_CAPACITY,
            rate: RateLimit {
                burst: 8,
                refill_ms: 500,
            },
        }
    }
}

/// Notification history, toast visibility and do-not-disturb state.
#[derive(Debug)]
pub struct Model {
    config: Config,
    /// Oldest first; the newest entry is at the back.
    items: VecDeque<Notification>,
    next_raw_id: u32,
    dnd: bool,
    limiter: RateLimiter,
}

impl Model {
    /// Creates an empty model with do-not-disturb off.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            items: VecDeque::new(),
            next_raw_id: 1,
            dnd: false,
            limiter: RateLimiter::new(config.rate),
        }
    }

    pub fn do_not_disturb(&self) -> bool {
        self.dnd
    }

    /// Switches do-not-disturb. Turning it on hides visible non-critical toasts.
    pub fn set_do_not_disturb(&mut self, on: bool) -> Vec<Event> {
        if self.dnd == on {
            return Vec::new();
        }
        self.dnd = on;
        let mut events = vec![Event::DndChanged(on)];
        if on {
            events.extend(self.hide_non_critical_toasts());
        }
        events
    }

    /// Handles `Notify`: replaces, adds, throttles or stores silently, and reports the outcome.
    pub fn notify(&mut self, incoming: Incoming, now: Timestamp) -> Notified {
        let replaced = self.replace_target(&incoming);
        let app_key = app_key_of(&incoming);
        if !self.limiter.allow(&app_key, now.monotonic_ms) {
            let id = replaced.unwrap_or_else(|| self.allocate_id());
            return Notified {
                id,
                events: Vec::new(),
            };
        }
        let id = replaced.unwrap_or_else(|| self.allocate_id());
        let urgency = incoming.hints.urgency.unwrap_or(Urgency::Normal);
        let toast = urgency == Urgency::Critical || !self.dnd;
        let item = build_notification(id, incoming, now, toast);
        let mut events = Vec::new();
        if let Some(index) = self.position(id) {
            self.items.remove(index);
            events.push(Event::Updated { id, toast });
        } else {
            events.push(Event::Added { id, toast });
        }
        self.items.push_back(item);
        self.enforce_capacity(&mut events);
        events.extend(self.limit_visible_toasts());
        Notified { id, events }
    }

    /// Removes a notification with the given reason. Unknown ids are a no-op.
    pub fn close(&mut self, id: NotificationId, reason: CloseReason) -> Vec<Event> {
        match self.position(id) {
            Some(index) => {
                self.items.remove(index);
                vec![Event::Removed { id, reason }]
            }
            None => Vec::new(),
        }
    }

    /// Removes every notification, reporting each as dismissed.
    pub fn dismiss_all(&mut self) -> Vec<Event> {
        let ids: Vec<NotificationId> = self.items.iter().map(|item| item.id).collect();
        self.items.clear();
        ids.into_iter()
            .map(|id| Event::Removed {
                id,
                reason: CloseReason::Dismissed,
            })
            .collect()
    }

    /// Invokes an action. Non-resident notifications close afterwards; resident ones keep their history entry.
    pub fn invoke_action(
        &mut self,
        id: NotificationId,
        key: &str,
    ) -> Result<Vec<Event>, ModelError> {
        let index = self
            .position(id)
            .ok_or(ModelError::UnknownNotification(id.get()))?;
        let Some(item) = self.items.get(index) else {
            return Err(ModelError::UnknownNotification(id.get()));
        };
        if !item.actions.iter().any(|action| action.key == key) {
            return Err(ModelError::UnknownAction(id.get(), key.to_string()));
        }
        let resident = item.resident;
        let mut events = vec![Event::ActionInvoked {
            id,
            key: key.to_string(),
        }];
        if resident {
            events.extend(self.end_toast(id, CloseReason::Dismissed));
        } else {
            events.extend(self.close(id, CloseReason::Dismissed));
        }
        Ok(events)
    }

    /// Marks a notification read. Unknown ids are a no-op.
    pub fn mark_read(&mut self, id: NotificationId) -> Vec<Event> {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return Vec::new();
        };
        item.read = true;
        vec![Event::Updated {
            id,
            toast: item.toast,
        }]
    }

    /// Ends toasts whose deadline has passed. Call whenever the clock advances.
    pub fn tick(&mut self, now: Timestamp) -> Vec<Event> {
        let due: Vec<NotificationId> = self
            .items
            .iter()
            .filter(|item| item.toast && item.expires_at.is_some_and(|at| at <= now.monotonic_ms))
            .map(|item| item.id)
            .collect();
        due.into_iter()
            .filter_map(|id| self.end_toast(id, CloseReason::Expired))
            .collect()
    }

    /// The earliest monotonic deadline among visible toasts, if any.
    pub fn next_expiry(&self) -> Option<u64> {
        self.items
            .iter()
            .filter(|item| item.toast)
            .filter_map(|item| item.expires_at)
            .min()
    }

    pub fn get(&self, id: NotificationId) -> Option<&Notification> {
        self.items.iter().find(|item| item.id == id)
    }

    /// The whole history, newest first.
    pub fn history(&self) -> impl Iterator<Item = &Notification> {
        self.items.iter().rev()
    }

    /// The toast stack, newest first, at most [`MAX_VISIBLE_TOASTS`] entries.
    pub fn active_toasts(&self) -> Vec<&Notification> {
        self.items
            .iter()
            .rev()
            .filter(|item| item.toast)
            .take(MAX_VISIBLE_TOASTS)
            .collect()
    }

    fn position(&self, id: NotificationId) -> Option<usize> {
        self.items.iter().position(|item| item.id == id)
    }

    /// Finds the entry a `Notify` call replaces: by `replaces_id`, then by sync key.
    fn replace_target(&self, incoming: &Incoming) -> Option<NotificationId> {
        if let Some(raw) = NonZeroU32::new(incoming.replaces_id) {
            let id = NotificationId(raw);
            if self.position(id).is_some() {
                return Some(id);
            }
        }
        let key = incoming.hints.sync_key.as_deref()?;
        let app = app_key_of(incoming);
        self.items
            .iter()
            .find(|item| item.sync_key.as_deref() == Some(key) && item.app_key == app)
            .map(|item| item.id)
    }

    /// Hands out the next free id, wrapping past `u32::MAX` and skipping zero and ids in use.
    fn allocate_id(&mut self) -> NotificationId {
        loop {
            let raw = self.next_raw_id;
            self.next_raw_id = if raw == u32::MAX { 1 } else { raw + 1 };
            let id = NotificationId(NonZeroU32::new(raw).unwrap_or(NonZeroU32::MIN));
            if self.position(id).is_none() {
                return id;
            }
        }
    }

    /// Drops the oldest non-critical entry (or the oldest overall) while over capacity.
    fn enforce_capacity(&mut self, events: &mut Vec<Event>) {
        while self.items.len() > self.config.history_capacity {
            let candidates = self.items.len().saturating_sub(1);
            let index = self
                .items
                .iter()
                .take(candidates)
                .position(|item| item.urgency != Urgency::Critical)
                .unwrap_or(0);
            let Some(removed) = self.items.remove(index) else {
                break;
            };
            events.push(Event::Removed {
                id: removed.id,
                reason: CloseReason::Undefined,
            });
        }
    }

    /// Keeps at most [`MAX_VISIBLE_TOASTS`] toasts by demoting the oldest ones.
    fn limit_visible_toasts(&mut self) -> Vec<Event> {
        let visible = self.items.iter().filter(|item| item.toast).count();
        let mut excess = visible.saturating_sub(MAX_VISIBLE_TOASTS);
        let mut events = Vec::new();
        for item in self.items.iter_mut() {
            if excess == 0 {
                break;
            }
            if item.toast {
                item.toast = false;
                item.expires_at = None;
                events.push(Event::ToastHidden { id: item.id });
                excess -= 1;
            }
        }
        events
    }

    fn hide_non_critical_toasts(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for item in self.items.iter_mut() {
            if item.toast && item.urgency != Urgency::Critical {
                item.toast = false;
                item.expires_at = None;
                events.push(Event::ToastHidden { id: item.id });
            }
        }
        events
    }

    /// Ends a toast. Transient notifications leave the history with their toast.
    fn end_toast(&mut self, id: NotificationId, reason: CloseReason) -> Option<Event> {
        let index = self.position(id)?;
        let item = self.items.get_mut(index)?;
        if !item.toast {
            return None;
        }
        if item.transient {
            self.items.remove(index);
            return Some(Event::Removed { id, reason });
        }
        item.toast = false;
        item.expires_at = None;
        Some(Event::ToastClosed { id, reason })
    }
}

fn app_key_of(incoming: &Incoming) -> String {
    match &incoming.hints.desktop_entry {
        Some(entry) => entry.clone(),
        None => clip(&incoming.app_name, APP_NAME_MAX_CHARS),
    }
}

/// Sanitises a client request into a stored notification.
fn build_notification(
    id: NotificationId,
    incoming: Incoming,
    now: Timestamp,
    toast: bool,
) -> Notification {
    let urgency = incoming.hints.urgency.unwrap_or(Urgency::Normal);
    let expires_at = policy::expiry_ms(urgency, incoming.expire_timeout)
        .filter(|_| toast)
        .map(|ms| now.monotonic_ms.saturating_add(ms));
    let icon = resolve_icon(&incoming);
    Notification {
        id,
        app_name: clip(&incoming.app_name, APP_NAME_MAX_CHARS),
        app_key: app_key_of(&incoming),
        summary: markup::summary_text(&incoming.summary),
        body: markup::parse(&incoming.body, BODY_MAX_CHARS),
        icon,
        actions: parse_actions(&incoming.actions),
        urgency,
        category: incoming.hints.category.clone(),
        transient: incoming.hints.transient,
        resident: incoming.hints.resident,
        progress: incoming.hints.progress,
        sync_key: incoming.hints.sync_key.clone(),
        received: now,
        expires_at,
        read: false,
        toast,
    }
}

/// Picks the best icon: inline pixels, then an image path, then the app icon reference.
fn resolve_icon(incoming: &Incoming) -> Icon {
    let decoded = incoming
        .hints
        .image_data
        .as_ref()
        .and_then(|raw| to_rgba(raw.clone()).ok());
    if let Some(rgba) = decoded {
        return Icon::Rgba(rgba);
    }
    if let Some(path) = incoming.hints.image_path.as_deref() {
        return Icon::Path(strip_file_scheme(path));
    }
    let Some(reference) = line_or_none(&incoming.app_icon, ICON_REF_MAX_CHARS) else {
        return Icon::None;
    };
    if reference.starts_with('/') || reference.starts_with("file://") {
        Icon::Path(strip_file_scheme(&reference))
    } else {
        Icon::Name(reference)
    }
}

fn strip_file_scheme(value: &str) -> String {
    let stripped = value.strip_prefix("file://").unwrap_or(value);
    clip(stripped, ICON_REF_MAX_CHARS)
}

/// Pairs up the flattened action list, dropping incomplete or empty pairs.
fn parse_actions(flat: &[String]) -> Vec<Action> {
    flat.as_chunks::<2>()
        .0
        .iter()
        .take(ACTIONS_MAX_PAIRS)
        .filter_map(|pair| {
            let [raw_key, raw_label] = pair;
            let key = line_or_none(raw_key, ACTION_KEY_MAX_CHARS)?;
            let label =
                line_or_none(raw_label, ACTION_LABEL_MAX_CHARS).unwrap_or_else(|| key.clone());
            Some(Action { key, label })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::limits::SUMMARY_MAX_CHARS;
    use crate::model::policy::RateLimit;
    use crate::model::{HintValue, Hints, Urgency};

    fn ts(ms: u64) -> Timestamp {
        Timestamp {
            monotonic_ms: ms,
            unix_ms: 1_700_000_000_000 + ms,
        }
    }

    fn request(summary: &str) -> Incoming {
        Incoming {
            app_name: "Mail".to_string(),
            replaces_id: 0,
            app_icon: String::new(),
            summary: summary.to_string(),
            body: String::new(),
            actions: Vec::new(),
            hints: Hints::default(),
            expire_timeout: -1,
        }
    }

    fn with_urgency(mut incoming: Incoming, urgency: Urgency) -> Incoming {
        incoming.hints.urgency = Some(urgency);
        incoming
    }

    fn model() -> Model {
        Model::new(Config {
            history_capacity: HISTORY_CAPACITY,
            rate: RateLimit {
                burst: 1_000,
                refill_ms: 1,
            },
        })
    }

    fn id(raw: u32) -> NotificationId {
        NotificationId::new(raw).expect("nonzero")
    }

    #[test]
    fn first_notification_gets_id_one_and_a_toast() {
        let mut m = model();
        let out = m.notify(request("hi"), ts(0));
        assert_eq!(out.id, id(1));
        assert_eq!(
            out.events,
            vec![Event::Added {
                id: id(1),
                toast: true
            }]
        );
        assert_eq!(m.active_toasts().len(), 1);
    }

    #[test]
    fn replaces_id_updates_in_place_and_keeps_id() {
        let mut m = model();
        let first = m.notify(request("one"), ts(0)).id;
        let mut again = request("two");
        again.replaces_id = first.get();
        let out = m.notify(again, ts(10));
        assert_eq!(out.id, first);
        assert_eq!(
            out.events,
            vec![Event::Updated {
                id: first,
                toast: true
            }]
        );
        assert_eq!(m.get(first).map(|n| n.summary.as_str()), Some("two"));
        assert_eq!(m.history().count(), 1);
    }

    #[test]
    fn replaces_unknown_id_creates_new_entry() {
        let mut m = model();
        let mut req = request("x");
        req.replaces_id = 77;
        let out = m.notify(req, ts(0));
        assert_ne!(out.id.get(), 77);
        assert!(matches!(out.events[0], Event::Added { .. }));
    }

    #[test]
    fn sync_key_replaces_same_app_only() {
        let mut m = model();
        let mut first = request("45%");
        first.hints.sync_key = Some("download".into());
        first.hints.progress = Some(45);
        let original = m.notify(first, ts(0)).id;

        let mut other_app = request("other");
        other_app.app_name = "Other".into();
        other_app.hints.sync_key = Some("download".into());
        assert_ne!(m.notify(other_app, ts(1)).id, original);

        let mut second = request("90%");
        second.hints.sync_key = Some("download".into());
        assert_eq!(m.notify(second, ts(2)).id, original);
    }

    #[test]
    fn toast_expires_but_stays_in_history() {
        let mut m = model();
        let mut req = request("bye");
        req.expire_timeout = 200;
        let nid = m.notify(req, ts(0)).id;
        assert!(m.tick(ts(199)).is_empty());
        assert_eq!(
            m.tick(ts(200)),
            vec![Event::ToastClosed {
                id: nid,
                reason: CloseReason::Expired
            }]
        );
        assert!(m.active_toasts().is_empty());
        assert!(m.get(nid).is_some_and(|n| !n.toast));
    }

    #[test]
    fn zero_timeout_never_expires() {
        let mut m = model();
        let mut req = request("sticky");
        req.expire_timeout = 0;
        m.notify(req, ts(0));
        assert_eq!(m.next_expiry(), None);
        assert!(m.tick(ts(u64::MAX / 2)).is_empty());
    }

    #[test]
    fn critical_never_expires() {
        let mut m = model();
        let req = with_urgency(request("fire"), Urgency::Critical);
        m.notify(req, ts(0));
        assert_eq!(m.next_expiry(), None);
    }

    #[test]
    fn default_expiry_depends_on_urgency() {
        let mut m = model();
        m.notify(with_urgency(request("low"), Urgency::Low), ts(0));
        assert_eq!(m.next_expiry(), Some(5_000));
        let mut m = model();
        m.notify(request("normal"), ts(0));
        assert_eq!(m.next_expiry(), Some(8_000));
    }

    #[test]
    fn next_expiry_is_the_earliest_toast_deadline() {
        let mut m = model();
        let mut late = request("late");
        late.expire_timeout = 9_000;
        m.notify(late, ts(0));
        let mut early = request("early");
        early.expire_timeout = 1_000;
        m.notify(early, ts(0));
        assert_eq!(m.next_expiry(), Some(1_000));
    }

    #[test]
    fn dnd_sends_non_critical_to_history_without_toast() {
        let mut m = model();
        assert_eq!(m.set_do_not_disturb(true), vec![Event::DndChanged(true)]);
        let out = m.notify(request("quiet"), ts(0));
        assert_eq!(
            out.events,
            vec![Event::Added {
                id: out.id,
                toast: false
            }]
        );
        assert!(m.active_toasts().is_empty());
        assert_eq!(m.history().count(), 1);
    }

    #[test]
    fn dnd_lets_critical_through() {
        let mut m = model();
        m.set_do_not_disturb(true);
        m.notify(with_urgency(request("fire"), Urgency::Critical), ts(0));
        assert_eq!(m.active_toasts().len(), 1);
    }

    #[test]
    fn enabling_dnd_hides_visible_non_critical_toasts() {
        let mut m = model();
        let normal = m.notify(request("a"), ts(0)).id;
        let critical = m
            .notify(with_urgency(request("b"), Urgency::Critical), ts(0))
            .id;
        let events = m.set_do_not_disturb(true);
        assert_eq!(
            events,
            vec![Event::DndChanged(true), Event::ToastHidden { id: normal }]
        );
        assert_eq!(
            m.active_toasts().iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![critical]
        );
    }

    #[test]
    fn setting_dnd_to_same_value_is_a_no_op() {
        let mut m = model();
        assert!(m.set_do_not_disturb(false).is_empty());
        m.set_do_not_disturb(true);
        assert!(m.set_do_not_disturb(true).is_empty());
        assert!(m.do_not_disturb());
    }

    #[test]
    fn toast_stack_is_capped_and_oldest_are_demoted() {
        let mut m = model();
        let ids: Vec<NotificationId> = (0..4)
            .map(|n| m.notify(request(&format!("n{n}")), ts(n)).id)
            .collect();
        let stack: Vec<NotificationId> = m.active_toasts().iter().map(|n| n.id).collect();
        assert_eq!(stack, vec![ids[3], ids[2], ids[1]]);
        assert!(m.get(ids[0]).is_some_and(|n| !n.toast));
        let out = m.notify(request("n4"), ts(4));
        assert!(out.events.contains(&Event::ToastHidden { id: ids[1] }));
    }

    #[test]
    fn dismiss_removes_entry_with_reason() {
        let mut m = model();
        let nid = m.notify(request("x"), ts(0)).id;
        assert_eq!(
            m.close(nid, CloseReason::Dismissed),
            vec![Event::Removed {
                id: nid,
                reason: CloseReason::Dismissed
            }]
        );
        assert!(m.get(nid).is_none());
        assert!(m.close(nid, CloseReason::Closed).is_empty());
    }

    #[test]
    fn dismiss_all_empties_history() {
        let mut m = model();
        m.notify(request("a"), ts(0));
        m.notify(request("b"), ts(0));
        assert_eq!(m.dismiss_all().len(), 2);
        assert_eq!(m.history().count(), 0);
    }

    #[test]
    fn invoke_action_closes_non_resident_notification() {
        let mut m = model();
        let mut req = request("reply?");
        req.actions = vec!["reply".into(), "Reply".into()];
        let nid = m.notify(req, ts(0)).id;
        let events = m.invoke_action(nid, "reply").expect("known action");
        assert_eq!(
            events,
            vec![
                Event::ActionInvoked {
                    id: nid,
                    key: "reply".into()
                },
                Event::Removed {
                    id: nid,
                    reason: CloseReason::Dismissed
                },
            ]
        );
    }

    #[test]
    fn invoke_action_keeps_resident_entry_but_ends_its_toast() {
        let mut m = model();
        let mut req = request("keep");
        req.actions = vec!["open".into(), "Open".into()];
        req.hints.resident = true;
        let nid = m.notify(req, ts(0)).id;
        let events = m.invoke_action(nid, "open").expect("known action");
        assert_eq!(events.len(), 2);
        assert!(matches!(events[1], Event::ToastClosed { .. }));
        assert!(m.get(nid).is_some());
    }

    #[test]
    fn invoke_action_errors_for_unknown_id_or_key() {
        let mut m = model();
        let mut req = request("x");
        req.actions = vec!["a".into(), "A".into()];
        let nid = m.notify(req, ts(0)).id;
        assert_eq!(
            m.invoke_action(id(999), "a"),
            Err(ModelError::UnknownNotification(999))
        );
        assert_eq!(
            m.invoke_action(nid, "b"),
            Err(ModelError::UnknownAction(nid.get(), "b".into()))
        );
    }

    #[test]
    fn transient_toast_leaves_history_when_it_ends() {
        let mut m = model();
        let mut req = request("tmp");
        req.hints.transient = true;
        req.expire_timeout = 100;
        let nid = m.notify(req, ts(0)).id;
        assert_eq!(
            m.tick(ts(100)),
            vec![Event::Removed {
                id: nid,
                reason: CloseReason::Expired
            }]
        );
        assert!(m.get(nid).is_none());
    }

    #[test]
    fn mark_read_flags_entry_and_reports_update() {
        let mut m = model();
        let nid = m.notify(request("x"), ts(0)).id;
        assert_eq!(
            m.mark_read(nid),
            vec![Event::Updated {
                id: nid,
                toast: true
            }]
        );
        assert!(m.get(nid).is_some_and(|n| n.read));
        assert!(m.mark_read(id(500)).is_empty());
    }

    #[test]
    fn history_is_capped_and_drops_oldest_non_critical_first() {
        let mut m = Model::new(Config {
            history_capacity: 3,
            rate: RateLimit {
                burst: 1_000,
                refill_ms: 1,
            },
        });
        let critical = m
            .notify(with_urgency(request("keep"), Urgency::Critical), ts(0))
            .id;
        let first_normal = m.notify(request("a"), ts(1)).id;
        m.notify(request("b"), ts(2));
        let out = m.notify(request("c"), ts(3));
        assert!(out.events.contains(&Event::Removed {
            id: first_normal,
            reason: CloseReason::Undefined
        }));
        assert!(m.get(critical).is_some());
        assert_eq!(m.history().count(), 3);
    }

    #[test]
    fn history_never_evicts_the_entry_just_added_when_all_others_are_critical() {
        let mut m = Model::new(Config {
            history_capacity: 2,
            rate: RateLimit {
                burst: 1_000,
                refill_ms: 1,
            },
        });
        let a = m
            .notify(with_urgency(request("a"), Urgency::Critical), ts(0))
            .id;
        let b = m
            .notify(with_urgency(request("b"), Urgency::Critical), ts(0))
            .id;
        let c = m.notify(request("c"), ts(0)).id;
        assert!(m.get(c).is_some());
        assert!(m.get(a).is_some() || m.get(b).is_some());
    }

    #[test]
    fn rate_limited_notification_gets_an_id_but_no_entry() {
        let mut m = Model::new(Config {
            history_capacity: 10,
            rate: RateLimit {
                burst: 1,
                refill_ms: 10_000,
            },
        });
        m.notify(request("first"), ts(0));
        let out = m.notify(request("spam"), ts(1));
        assert!(out.events.is_empty());
        assert!(m.get(out.id).is_none());
        assert_eq!(m.history().count(), 1);
    }

    #[test]
    fn rate_limit_is_per_application() {
        let mut m = Model::new(Config {
            history_capacity: 10,
            rate: RateLimit {
                burst: 1,
                refill_ms: 10_000,
            },
        });
        m.notify(request("a"), ts(0));
        let mut other = request("b");
        other.app_name = "Chat".into();
        assert_eq!(m.notify(other, ts(0)).events.len(), 1);
    }

    #[test]
    fn ids_wrap_past_u32_max_to_one_never_zero() {
        let mut m = model();
        m.next_raw_id = u32::MAX;
        assert_eq!(m.notify(request("max"), ts(0)).id.get(), u32::MAX);
        assert_eq!(m.notify(request("wrap"), ts(0)).id.get(), 1);
    }

    #[test]
    fn allocation_skips_ids_still_in_use() {
        let mut m = model();
        let first = m.notify(request("used"), ts(0)).id;
        assert_eq!(first.get(), 1);
        m.next_raw_id = 1;
        assert_eq!(m.notify(request("skip"), ts(0)).id.get(), 2);
    }

    #[test]
    fn body_markup_and_summary_are_sanitised_on_store() {
        let mut m = model();
        let mut req = request("<b>Hi</b>\u{7}");
        req.body = "<a href=\"javascript:x\">click</a> <img src=y>ok".into();
        let nid = m.notify(req, ts(0)).id;
        let stored = m.get(nid).expect("stored");
        assert_eq!(stored.summary, "Hi");
        assert_eq!(markup::plain(&stored.body), "click ok");
        assert!(stored.body.iter().all(|s| s.link.is_none()));
    }

    #[test]
    fn oversized_fields_are_capped() {
        let mut m = model();
        let mut req = request(&"s".repeat(SUMMARY_MAX_CHARS * 2));
        req.body = "b".repeat(BODY_MAX_CHARS * 2);
        req.app_name = "a".repeat(APP_NAME_MAX_CHARS * 2);
        let nid = m.notify(req, ts(0)).id;
        let stored = m.get(nid).expect("stored");
        assert_eq!(stored.summary.chars().count(), SUMMARY_MAX_CHARS);
        assert_eq!(markup::plain(&stored.body).chars().count(), BODY_MAX_CHARS);
        assert_eq!(stored.app_name.chars().count(), APP_NAME_MAX_CHARS);
    }

    #[test]
    fn actions_are_paired_capped_and_empty_keys_dropped() {
        let flat: Vec<String> = (0..40).map(|n| format!("s{n}")).collect();
        let actions = parse_actions(&flat);
        assert_eq!(actions.len(), ACTIONS_MAX_PAIRS);
        assert_eq!(
            actions[1],
            Action {
                key: "s2".into(),
                label: "s3".into()
            }
        );
        let odd = vec!["k".to_string(), "Label".to_string(), "dangling".to_string()];
        assert_eq!(parse_actions(&odd).len(), 1);
        let blank = vec![" ".to_string(), "x".to_string()];
        assert!(parse_actions(&blank).is_empty());
    }

    #[test]
    fn missing_action_label_falls_back_to_key() {
        let flat = vec!["open".to_string(), String::new()];
        assert_eq!(
            parse_actions(&flat),
            vec![Action {
                key: "open".into(),
                label: "open".into()
            }]
        );
    }

    #[test]
    fn icon_prefers_pixels_then_path_then_app_icon() {
        let mut req = request("x");
        req.app_icon = "mail-unread".into();
        assert_eq!(resolve_icon(&req), Icon::Name("mail-unread".into()));

        req.app_icon = "file:///usr/share/a.png".into();
        assert_eq!(resolve_icon(&req), Icon::Path("/usr/share/a.png".into()));

        req.hints.image_path = Some("/tmp/b.png".into());
        assert_eq!(resolve_icon(&req), Icon::Path("/tmp/b.png".into()));

        req.hints.image_data = Some(crate::model::RawImage {
            width: 1,
            height: 1,
            rowstride: 4,
            has_alpha: true,
            bits_per_sample: 8,
            channels: 4,
            data: vec![1, 2, 3, 4],
        });
        assert!(matches!(resolve_icon(&req), Icon::Rgba(_)));
    }

    #[test]
    fn invalid_image_data_falls_back_to_other_icon_sources() {
        let mut req = request("x");
        req.hints.image_data = Some(crate::model::RawImage {
            width: 0,
            height: 0,
            rowstride: 0,
            has_alpha: true,
            bits_per_sample: 8,
            channels: 4,
            data: Vec::new(),
        });
        assert_eq!(resolve_icon(&req), Icon::None);
    }

    #[test]
    fn desktop_entry_becomes_the_app_key() {
        let mut req = request("x");
        req.hints.desktop_entry = Some("org.example.Mail".into());
        assert_eq!(app_key_of(&req), "org.example.Mail");
        assert_eq!(app_key_of(&request("x")), "Mail");
    }

    #[test]
    fn hint_values_convert_into_history_fields() {
        let mut m = model();
        let mut req = request("progress");
        req.hints = super::super::hints::parse(vec![
            ("value".to_string(), HintValue::Int(30)),
            ("category".to_string(), HintValue::Text("transfer".into())),
        ]);
        let nid = m.notify(req, ts(0)).id;
        let stored = m.get(nid).expect("stored");
        assert_eq!(stored.progress, Some(30));
        assert_eq!(stored.category.as_deref(), Some("transfer"));
    }

    #[test]
    fn actions_on_unknown_notification_do_not_change_state() {
        let mut m = model();
        m.notify(request("x"), ts(0));
        assert!(m.invoke_action(id(42), "a").is_err());
        assert_eq!(m.history().count(), 1);
    }
}
