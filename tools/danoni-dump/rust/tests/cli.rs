//! Exercise the production JSON boundary rather than only the library API.
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn invoke(request: &Value) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ddi-danoni-export"))
        .arg("compile-chart")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input
        .write_all(&serde_json::to_vec(request).unwrap())
        .unwrap();
    drop(input);
    child.wait_with_output().unwrap()
}
#[test]
fn cli_and_embedded_api_use_the_same_contract() {
    let request = json!({"schema":"ddi-danoni-compile-request/1", "chart":{
        "id":"synthetic", "mode":"5", "difficultyStars":4,
        "lanes":(0..5).map(|i| json!({"tapsSeconds":if i==0 {vec![1.0]} else {vec![]}, "holdsSeconds":[]})).collect::<Vec<_>>()
    }});
    let output = invoke(&request);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let got: Value = serde_json::from_slice(&output.stdout).unwrap();
    let input: ddi_danoni_export::CompileRequest = serde_json::from_value(request).unwrap();
    let expected =
        ddi_danoni_export::compile_chart(&input.chart, &input.metadata, &input.options).unwrap();
    assert_eq!(got, serde_json::to_value(expected).unwrap());
}
#[test]
fn rejected_requests_never_emit_a_partial_result() {
    let lanes = (0..5)
        .map(|_| json!({"tapsSeconds":[1.0],"holdsSeconds":[]}))
        .collect::<Vec<_>>();
    for (request, code) in [
        (
            json!({"schema":"unknown", "chart":{"id":"synthetic","mode":"5","lanes":lanes}}),
            "unsupported-schema",
        ),
        (
            json!({"schema":"ddi-danoni-compile-request/1", "chart":{"id":"synthetic","mode":"9i","lanes":lanes}}),
            "invalid-json",
        ),
        (
            json!({"schema":"ddi-danoni-compile-request/1", "chart":{"id":"synthetic","mode":"5","lanes":lanes}, "options":{"scrollFactor":1.2}}),
            "missing-native-motion",
        ),
    ] {
        let output = invoke(&request);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], code);
    }
}
