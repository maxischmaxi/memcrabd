use bytes::BytesMut;

pub fn find_crlf(buf: &BytesMut) -> Option<usize> {
    buf.windows(2).position(|window| window == b"\r\n")
}
