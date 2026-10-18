use super::*;
use crate::assets::{self, AssetSource, Binding, Inputs, JSON_LIMIT};
pub const CAPTURE_LIMIT: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureBindings {
    pub snapshot: Binding,
    pub run: Binding,
    pub config: Binding,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectionRequest {
    pub capture: CaptureBindings,
    pub movie: Binding,
    pub options: ProjectionOptions,
    pub optional_color: Option<OptionalColorEvidence>,
}
/// Verify a successful saved capture before passive projection. This does not
/// launch a worker, decode music or establish a complete normalized work.
pub fn prepare_projection(
    request: &ProjectionRequest,
    source: &impl AssetSource,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    checkpoint()?;
    let mut inputs = Inputs::new(source);
    let (capture, retained) = checked_capture(
        &request.capture,
        &request.movie,
        request.optional_color.as_ref(),
        &mut inputs,
    )?;
    let projection = project_capture(&capture, &request.options, checkpoint)?;
    Ok(
        json!({"schema":"ddi-danoni-native-projection/1","producer":"native-family-projections/1","request":request,"preparedIteration":projection.prepared_iteration,"projection":projection.chart,"sourceInputs":inputs.used,"reviewedOptionalColorDiagnostics":retained,"scope":"Passive saved-state projection with successful run/config bindings; motion, audio demux/origin and worker supervision remain separate proofs"}),
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptionalColorEvidence {
    pub review: Binding,
    pub missing_responses: Vec<Binding>,
}
/// Exact historical missing-color exception. No response is synthesized and no
/// unrelated runtime failure is suppressed. The original audit remains evidence.
pub fn review_optional_colors(capture: &Value, review: &Value) -> Result<(Value, Vec<Value>)> {
    if review["schema"] != "ddi-native-optional-color-review/1"
        || review["sourceSwfSha256"] != capture["source"]["movieSha256"]
    {
        return Err(invalid("Optional color review belongs to another movie"));
    }
    let rows = array(&review["missingResources"])?;
    let source = capture["source"]["config"]["sourceUrl"]
        .as_str()
        .ok_or_else(|| invalid("Missing logical source URL"))?;
    // This reviewed exception is limited to a plain absolute movie URL. General
    // URL resolution belongs to acquisition; unusual inputs do not get guessed.
    if !(source.starts_with("http://") || source.starts_with("https://"))
        || source.contains(['?', '#'])
    {
        return Err(invalid("Unsupported optional-color source URL"));
    }
    let directory = source
        .rsplit_once('/')
        .map(|(d, _)| d)
        .ok_or_else(|| invalid("Missing source URL directory"))?;
    let expected = format!("{directory}/iro.txt");
    if rows.len() != 1
        || rows.iter().any(|r| {
            r["status"] != 404
                || r["url"] != expected
                || !r["retrievedAt"].as_str().is_some_and(|s| !s.is_empty())
                || !r["sha256"].as_str().is_some_and(assets::valid_hash)
        })
    {
        return Err(invalid("Need exact captured iro.txt HTTP-404 provenance"));
    }
    let entries = array(&capture["audit"]["entries"])?;
    let denied = entries
        .iter()
        .filter(|e| e["kind"] == "fetch-denied")
        .collect::<Vec<_>>();
    let errors = entries
        .iter()
        .filter(|e| e["kind"] == "runtime-error")
        .collect::<Vec<_>>();
    let message = "message=Asynchronous error occurred: Could not fetch: \"Invalid URL iro.txt: no exact GET match in immutable resource cache\" ";
    if denied.len() != 1
        || errors.len() != 1
        || denied[0]["detail"] != "iro.txt"
        || errors[0]["detail"] != message
        || denied[0]["iteration"] != errors[0]["iteration"]
    {
        return Err(invalid("Unknown missing-resource failure"));
    }
    let retained = vec![denied[0].clone(), errors[0].clone()];
    audit_ok(capture, &retained)?;
    let mut clean = capture.clone();
    clean["audit"]["entries"] = json!(
        entries
            .iter()
            .filter(|e| !retained.contains(e))
            .cloned()
            .collect::<Vec<_>>()
    );
    clean["audit"]["runtimeErrors"] = json!(0);
    Ok((clean, retained))
}

/// Common successful run/config/movie verification for passive consumers.
pub(crate) fn checked_capture(
    bindings: &CaptureBindings,
    movie: &Binding,
    optional_color: Option<&OptionalColorEvidence>,
    inputs: &mut Inputs<'_, impl AssetSource>,
) -> Result<(Value, Vec<Value>)> {
    let snapshot = inputs.checked(&bindings.snapshot, CAPTURE_LIMIT)?;
    let capture: Value = assets::json(&snapshot)?;
    let run: Value = assets::json(&inputs.checked(&bindings.run, JSON_LIMIT)?)?;
    let config: Value = assets::json(&inputs.checked(&bindings.config, JSON_LIMIT)?)?;
    inputs.checked(movie, crate::assets::MEDIA_LIMIT)?;
    if capture["schema"] != "ddi-ruffle-probe/experimental-1"
        || run["schema"] != "ddi-ruffle-run/experimental-1"
        || run["exitCode"] != 0
        || run.get("termination") != Some(&Value::Null)
        || run["inputsUnchanged"] != true
        || run["configSha256"] != bindings.config.sha256
        || config != capture["source"]["config"]
        || capture["source"]["movieSha256"] != movie.sha256
        || capture["ruffleRevision"] != "89a7049965f3090e706265670df309a5128233ac"
    {
        return Err(invalid(
            "Saved capture/run/config provenance differs or execution failed",
        ));
    }
    let result = if let Some(evidence) = optional_color {
        let review: Value = assets::json(&inputs.checked(&evidence.review, JSON_LIMIT)?)?;
        let rows = array(&review["missingResources"])?;
        if rows.len() != evidence.missing_responses.len() {
            return Err(invalid("Optional color response coverage differs"));
        }
        for row in rows {
            let matching = evidence
                .missing_responses
                .iter()
                .filter(|b| row["path"] == b.path && row["sha256"] == b.sha256)
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(invalid("Optional color response is not bound"));
            }
            inputs.checked(matching[0], crate::assets::EVIDENCE_LIMIT)?;
        }
        review_optional_colors(&capture, &review)?
    } else {
        (capture, vec![])
    };
    Ok(result)
}
