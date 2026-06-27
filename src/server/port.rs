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
