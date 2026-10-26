/// Opaque identifier for a managed window; only `WindowManager` mints these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(u64);

impl WindowId {
    pub(crate) const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_returns_raw_value() {
        assert_eq!(WindowId::new(7).get(), 7);
    }
}
