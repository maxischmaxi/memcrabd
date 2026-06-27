use std::ops::RangeInclusive;

pub const PORT_RANGE: RangeInclusive<usize> = 1..=65535;

pub fn port_in_range(s: &str) -> Result<u16, String> {
    let port: usize = s
        .parse()
        .map_err(|_| format!("`{s}` isn't a port number"))?;

    if PORT_RANGE.contains(&port) {
        Ok(port as u16)
    } else {
        Err(format!(
            "port not in range {} - {}",
            PORT_RANGE.start(),
            PORT_RANGE.end()
        ))
    }
}

pub fn item_size_valid(s: &str) -> Result<u64, String> {
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());

    let (number, suffix) = s.split_at(split);

    let value: u64 = number.parse().map_err(|_| "invalid number".to_string())?;

    let multiplier = match suffix.to_lowercase().as_str() {
        "" => 1,
        "k" => 1024,
        "m" => 1024 * 1024,
        "g" => 1024 * 1024 * 1024,
        "t" => 1024_u64.pow(4),
        _ => return Err(format!("unknown suffix '{suffix}'")),
    };

    value
        .checked_mul(multiplier)
        .ok_or_else(|| "value too large".to_string())
}
