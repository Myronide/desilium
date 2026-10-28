//! Sets the mouse settings on KWin's input devices over D-Bus.
//!
//! KWin lists its devices under `/org/kde/KWin/InputDevice/<sysName>` and makes their settings
//! writable properties. The object and property names are from memory and unverified (see
//! `docs/kwin-integration.md`); a device or KWin version without one of the properties is left
//! alone for that property.

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedValue;

use crate::error::KwinError;
use crate::plan::{
    PROP_ACCELERATION, PROP_FLAT_PROFILE, PROP_LEFT_HANDED, PROP_NATURAL_SCROLL, PROP_POINTER,
    PROP_SCROLL_FACTOR, PROP_TOUCHPAD, PointerPlan,
};

const SERVICE: &str = "org.kde.KWin";
const MANAGER_PATH: &str = "/org/kde/KWin/InputDevice";
const MANAGER_INTERFACE: &str = "org.kde.KWin.InputDeviceManager";
const DEVICE_INTERFACE: &str = "org.kde.KWin.InputDevice";
/// Doubles closer than this are the same value (KWin keeps them as `qreal`).
const SAME: f64 = 1e-6;

/// A property value of an input device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Property {
    Bool(bool),
    Double(f64),
}

impl Property {
    fn same(self, other: Self) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Double(a), Self::Double(b)) => (a - b).abs() < SAME,
            _ => false,
        }
    }
}

/// The input devices of a compositor. A seam so that tests need no bus.
pub trait InputDevices {
    /// System names of all devices, such as `event5`.
    fn sys_names(&self) -> Result<Vec<String>, KwinError>;
    /// The current value of `property`; an error when the device does not have it.
    fn get(&self, sys_name: &str, property: &str) -> Result<Property, KwinError>;
    /// Sets `property`.
    fn set(&self, sys_name: &str, property: &str, value: Property) -> Result<(), KwinError>;
}

/// What one pass over the devices did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PointerOutcome {
    /// Properties written.
    pub written: usize,
    /// Properties that could not be written.
    pub failed: usize,
}

/// A device name is one element of an object path: letters, digits and underscores.
fn is_path_element(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The properties a mouse should have under `plan`, in write order.
fn wanted(plan: &PointerPlan) -> [(&'static str, Property); 5] {
    [
        (PROP_ACCELERATION, Property::Double(plan.acceleration())),
        (PROP_FLAT_PROFILE, Property::Bool(plan.flat)),
        (PROP_NATURAL_SCROLL, Property::Bool(plan.natural_scroll)),
        (PROP_LEFT_HANDED, Property::Bool(plan.left_handed)),
        (PROP_SCROLL_FACTOR, Property::Double(plan.scroll_factor())),
    ]
}

/// The devices that are mice: pointing devices that are not touchpads.
pub fn mice(bus: &dyn InputDevices, names: &[String]) -> Vec<String> {
    let flag =
        |name: &str, property: &str| matches!(bus.get(name, property), Ok(Property::Bool(true)));
    names
        .iter()
        .filter(|name| is_path_element(name))
        .filter(|name| flag(name, PROP_POINTER) && !flag(name, PROP_TOUCHPAD))
        .cloned()
        .collect()
}

/// Writes `plan` to every mouse, and only the properties that differ.
pub fn apply_pointer(
    bus: &dyn InputDevices,
    names: &[String],
    plan: &PointerPlan,
) -> PointerOutcome {
    let mut outcome = PointerOutcome::default();
    for name in mice(bus, names) {
        for (property, value) in wanted(plan) {
            let current = match bus.get(&name, property) {
                Ok(current) => current,
                Err(error) => {
                    tracing::debug!(device = %name, property, %error, "device has no such property");
                    continue;
                }
            };
            if current.same(value) {
                continue;
            }
            match bus.set(&name, property, value) {
                Ok(()) => outcome.written += 1,
                Err(error) => {
                    tracing::warn!(device = %name, property, %error, "mouse setting not written");
                    outcome.failed += 1;
                }
            }
        }
    }
    outcome
}

/// The real thing: KWin on the session bus.
pub struct BusDevices<'a>(pub &'a Connection);

impl BusDevices<'_> {
    fn device(&self, sys_name: &str) -> Result<Proxy<'static>, KwinError> {
        if !is_path_element(sys_name) {
            return Err(KwinError::BadDeviceName(sys_name.to_owned()));
        }
        let path = format!("{MANAGER_PATH}/{sys_name}");
        Ok(Proxy::new(self.0, SERVICE, path, DEVICE_INTERFACE)?)
    }
}

impl InputDevices for BusDevices<'_> {
    fn sys_names(&self) -> Result<Vec<String>, KwinError> {
        let proxy = Proxy::new(self.0, SERVICE, MANAGER_PATH, MANAGER_INTERFACE)?;
        Ok(proxy.get_property("devicesSysNames")?)
    }

    fn get(&self, sys_name: &str, property: &str) -> Result<Property, KwinError> {
        let value: OwnedValue = self.device(sys_name)?.get_property(property)?;
        if let Ok(flag) = bool::try_from(&value) {
            return Ok(Property::Bool(flag));
        }
        f64::try_from(&value)
            .map(Property::Double)
            .map_err(|_| KwinError::BadProperty(property.to_owned()))
    }

    fn set(&self, sys_name: &str, property: &str, value: Property) -> Result<(), KwinError> {
        let proxy = self.device(sys_name)?;
        match value {
            Property::Bool(flag) => proxy.set_property(property, flag)?,
            Property::Double(number) => proxy.set_property(property, number)?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    /// Devices kept in memory; `missing` are properties a device does not have.
    #[derive(Default)]
    struct Fake {
        devices: RefCell<HashMap<String, HashMap<&'static str, Property>>>,
        order: Vec<String>,
        writes: RefCell<Vec<(String, String)>>,
    }

    impl Fake {
        fn add(&mut self, name: &str, pointer: bool, touchpad: bool) {
            let mut properties = HashMap::new();
            properties.insert(PROP_POINTER, Property::Bool(pointer));
            properties.insert(PROP_TOUCHPAD, Property::Bool(touchpad));
            properties.insert(PROP_ACCELERATION, Property::Double(0.0));
            properties.insert(PROP_FLAT_PROFILE, Property::Bool(false));
            properties.insert(PROP_NATURAL_SCROLL, Property::Bool(false));
            properties.insert(PROP_LEFT_HANDED, Property::Bool(false));
            properties.insert(PROP_SCROLL_FACTOR, Property::Double(1.0));
            self.devices
                .borrow_mut()
                .insert(name.to_owned(), properties);
            self.order.push(name.to_owned());
        }
    }

    impl InputDevices for Fake {
        fn sys_names(&self) -> Result<Vec<String>, KwinError> {
            Ok(self.order.clone())
        }

        fn get(&self, name: &str, property: &str) -> Result<Property, KwinError> {
            self.devices
                .borrow()
                .get(name)
                .and_then(|properties| properties.get(property).copied())
                .ok_or_else(|| KwinError::BadProperty(property.to_owned()))
        }

        fn set(&self, name: &str, property: &str, value: Property) -> Result<(), KwinError> {
            let mut devices = self.devices.borrow_mut();
            let slot = devices
                .get_mut(name)
                .and_then(|properties| properties.get_mut(property))
                .ok_or_else(|| KwinError::BadProperty(property.to_owned()))?;
            *slot = value;
            self.writes
                .borrow_mut()
                .push((name.to_owned(), property.to_owned()));
            Ok(())
        }
    }

    fn fake() -> Fake {
        let mut fake = Fake::default();
        fake.add("event3", false, false); // keyboard
        fake.add("event5", true, false); // mouse
        fake.add("event6", true, true); // touchpad
        fake.add("event7", true, false); // second mouse
        fake
    }

    fn plan() -> PointerPlan {
        PointerPlan {
            speed: 50,
            flat: true,
            natural_scroll: true,
            left_handed: false,
            scroll_percent: 200,
        }
    }

    #[test]
    fn only_pointing_devices_that_are_not_touchpads_are_mice() {
        let bus = fake();
        let names = bus.sys_names().unwrap();
        assert_eq!(mice(&bus, &names), ["event5", "event7"]);
    }

    #[test]
    fn names_that_are_not_path_elements_are_never_used() {
        let bus = fake();
        let names = ["event5".to_owned(), "../x".to_owned(), String::new()];
        assert_eq!(mice(&bus, &names), ["event5"]);
    }

    #[test]
    fn every_mouse_gets_the_plan_and_the_others_are_left_alone() {
        let bus = fake();
        let names = bus.sys_names().unwrap();
        let outcome = apply_pointer(&bus, &names, &plan());
        // Four properties differ from the device defaults (left-handed does not), on two mice.
        assert_eq!((outcome.written, outcome.failed), (8, 0));
        assert_eq!(
            bus.get("event5", PROP_ACCELERATION).unwrap(),
            Property::Double(0.5)
        );
        assert_eq!(
            bus.get("event7", PROP_SCROLL_FACTOR).unwrap(),
            Property::Double(2.0)
        );
        assert_eq!(
            bus.get("event6", PROP_ACCELERATION).unwrap(),
            Property::Double(0.0),
            "the touchpad is untouched"
        );
        assert!(
            bus.writes
                .borrow()
                .iter()
                .all(|(name, _)| name != "event6" && name != "event3")
        );
    }

    #[test]
    fn a_second_pass_writes_nothing() {
        let bus = fake();
        let names = bus.sys_names().unwrap();
        apply_pointer(&bus, &names, &plan());
        let again = apply_pointer(&bus, &names, &plan());
        assert_eq!((again.written, again.failed), (0, 0));
    }

    #[test]
    fn a_property_the_device_lacks_is_skipped_not_failed() {
        let bus = fake();
        bus.devices
            .borrow_mut()
            .get_mut("event5")
            .unwrap()
            .remove(PROP_SCROLL_FACTOR);
        let names = bus.sys_names().unwrap();
        let outcome = apply_pointer(&bus, &names, &plan());
        assert_eq!(outcome.failed, 0);
        assert_eq!(outcome.written, 7);
    }

    #[test]
    fn doubles_that_differ_by_rounding_only_are_the_same() {
        assert!(Property::Double(0.3).same(Property::Double(0.3 + 1e-9)));
        assert!(!Property::Double(0.3).same(Property::Double(0.31)));
        assert!(!Property::Double(1.0).same(Property::Bool(true)));
    }
}
