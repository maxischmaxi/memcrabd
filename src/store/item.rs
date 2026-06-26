use tokio::time::Instant;

#[derive(Clone)]
pub struct Item {
    pub value: Vec<u8>,
    pub flags: u32,
    pub expires_at: Option<Instant>,
    pub cas: u64,
}

impl Item {
    pub fn new(value: Vec<u8>, flags: u32, expires_at: Option<Instant>, cas: u64) -> Self {
        Self {
            value,
            flags,
            expires_at,
            cas,
        }
    }
}

