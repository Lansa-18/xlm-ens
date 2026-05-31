pub fn build_forward_gmp_message(
    name: &impl core::fmt::Display,
    destination_chain: &impl core::fmt::Display,
    resolver: &impl core::fmt::Display,
) -> String {
    format!(
        "{{\"type\":\"xlm-ns-resolution\",\"name\":\"{}\",\"destination_chain\":\"{}\",\"resolver\":\"{}\"}}",
        name, destination_chain, resolver
    )
}

pub fn build_reverse_gmp_message(
    address: &impl core::fmt::Display,
    primary_name: &impl core::fmt::Display,
    destination_chain: &impl core::fmt::Display,
    resolver: &impl core::fmt::Display,
) -> String {
    format!(
        "{{\"type\":\"xlm-ns-reverse-resolution\",\"address\":\"{}\",\"primary_name\":\"{}\",\"destination_chain\":\"{}\",\"resolver\":\"{}\"}}",
        address, primary_name, destination_chain, resolver
    )
}
