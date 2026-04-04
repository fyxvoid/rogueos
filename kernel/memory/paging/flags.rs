//! Page table entry flags. Type-safe construction; only valid x86-64 bits are set.

/// Single flag bit for a page table entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum PageFlag {
    Present = 1 << 0,
    Writable = 1 << 1,
    User = 1 << 2,
    NoExec = 1 << 63,
}

impl PageFlag {
    pub const fn mask(self) -> u64 {
        self as u64
    }
}

/// Builder for entry flags. Only bits for Present, Writable, User, NoExec are set.
#[derive(Clone, Copy, Debug, Default)]
pub struct EntryFlags(u64);

const VALID_MASK: u64 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 63);

impl EntryFlags {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn with(mut self, flag: PageFlag) -> Self {
        self.0 |= flag.mask();
        self
    }

    /// Raw value for encoding into a PTE. Clears any bits outside the valid mask.
    pub fn as_u64(self) -> u64 {
        self.0 & VALID_MASK
    }

    pub fn contains(self, flag: PageFlag) -> bool {
        (self.0 & flag.mask()) != 0
    }
}

/// Common flag combinations.
impl EntryFlags {
    pub fn kernel_rw() -> Self {
        Self::empty().with(PageFlag::Present).with(PageFlag::Writable)
    }

    pub fn kernel_rwx() -> Self {
        Self::empty().with(PageFlag::Present).with(PageFlag::Writable)
    }

    pub fn user_rw() -> Self {
        Self::empty()
            .with(PageFlag::Present)
            .with(PageFlag::Writable)
            .with(PageFlag::User)
    }

    pub fn user_rx() -> Self {
        Self::empty()
            .with(PageFlag::Present)
            .with(PageFlag::User)
    }

    pub fn user_rwx() -> Self {
        Self::empty()
            .with(PageFlag::Present)
            .with(PageFlag::Writable)
            .with(PageFlag::User)
    }

    pub fn user_ro() -> Self {
        Self::empty()
            .with(PageFlag::Present)
            .with(PageFlag::User)
            .with(PageFlag::NoExec)
    }
}

