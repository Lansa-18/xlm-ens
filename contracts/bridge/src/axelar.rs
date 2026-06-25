extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

pub fn build_forward_gmp_message(
    name: &[u8],
    destination_chain: &[u8],
    resolver: &[u8],
) -> String {
    let mut buf = String::new();
    buf.push_str("{\"type\":\"xlm-ns-resolution\",\"name\":\"");
    push_utf8(&mut buf, name);
    buf.push_str("\",\"destination_chain\":\"");
    push_utf8(&mut buf, destination_chain);
    buf.push_str("\",\"resolver\":\"");
    push_utf8(&mut buf, resolver);
    buf.push_str("\"}");
    buf
}

pub fn build_reverse_gmp_message(
    address: &[u8],
    primary_name: &[u8],
    destination_chain: &[u8],
    resolver: &[u8],
) -> String {
    let mut buf = String::new();
    buf.push_str("{\"type\":\"xlm-ns-reverse-resolution\",\"address\":\"");
    push_utf8(&mut buf, address);
    buf.push_str("\",\"primary_name\":\"");
    push_utf8(&mut buf, primary_name);
    buf.push_str("\",\"destination_chain\":\"");
    push_utf8(&mut buf, destination_chain);
    buf.push_str("\",\"resolver\":\"");
    push_utf8(&mut buf, resolver);
    buf.push_str("\"}");
    buf
}

fn push_utf8(buf: &mut String, bytes: &[u8]) {
    if let Ok(s) = core::str::from_utf8(bytes) {
        buf.push_str(s);
    }
}
