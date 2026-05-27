pub fn build_gmp_message(name: &str, destination_chain: &str, resolver: &str) -> String {
    format!(
        "{{\"type\":\"xlm-ns-resolution\",\"name\":\"{}\",\"destination_chain\":\"{}\",\"resolver\":\"{}\"}}",
        name, destination_chain, resolver
    )
}
//oo