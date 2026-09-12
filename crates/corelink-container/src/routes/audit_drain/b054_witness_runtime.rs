pub(crate) fn b054_load_witness_client() -> Result<Option<Arc<WitnessClient>>, String> {
    let values = [
        std::env::var("AUDIT_WITNESS_URL").ok(),
        std::env::var("AUDIT_WITNESS_APPEND_TOKEN").ok(),
        std::env::var("AUDIT_WITNESS_ID").ok(),
        std::env::var("AUDIT_WITNESS_PUBLIC_KEYS_JSON").ok(),
    ];
    let populated = values
        .iter()
        .filter(|value| value.as_ref().is_some_and(|v| !v.trim().is_empty()))
        .count();
    if populated == 0 {
        return Ok(None);
    }
    if populated != values.len() {
        return Err("partial audit witness configuration".to_owned());
    }
    let [origin, token, witness_id, keys] = values;
    let origin = origin.ok_or("missing AUDIT_WITNESS_URL")?;
    let token = token.ok_or("missing AUDIT_WITNESS_APPEND_TOKEN")?;
    let witness_id = witness_id.ok_or("missing AUDIT_WITNESS_ID")?;
    let keys = keys.ok_or("missing AUDIT_WITNESS_PUBLIC_KEYS_JSON")?;
    WitnessClient::new(
        origin.trim(),
        token,
        witness_id.trim().to_owned(),
        keys.trim(),
    )
    .map(Arc::new)
    .map(Some)
}

fn load_witness_client() -> Result<Option<Arc<WitnessClient>>, String> {
    b054_load_witness_client()
}

include!("b054_witness_tests.rs");
