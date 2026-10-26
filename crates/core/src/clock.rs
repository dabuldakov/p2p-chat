//! Логические часы Лампорта — задают причинно-следственный порядок
//! без центрального сервера.

/// Логическое время. Правила:
/// * локальное событие: `tick()`
/// * приём сообщения с временем `remote`: `observe(remote)`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Lamport(pub u64);

impl Lamport {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    /// Локальное событие.
    pub fn tick(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }

    /// Событие получения: max(local, remote) + 1.
    pub fn observe(&mut self, remote: u64) -> u64 {
        self.0 = self.0.max(remote) + 1;
        self.0
    }

    pub fn advance_to(&mut self, value: u64) {
        if value > self.0 {
            self.0 = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic() {
        let mut c = Lamport::new();
        assert_eq!(c.tick(), 1);
        assert_eq!(c.tick(), 2);
        assert_eq!(c.observe(10), 11);
        assert_eq!(c.observe(3), 12);
    }
}
