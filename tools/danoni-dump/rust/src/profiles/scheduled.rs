//! Variable-travel proposals use the same pre-consumption graph as projection.
//! They do not add or alter fields in the exact native chart reference.
use super::*;
impl EngineProfile {
    pub fn motion_candidates(
        self,
        capture: &Value,
        chart: &Value,
        origin: &Value,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Vec<MotionSelection>> {
        checkpoint()?;
        if self != Self::RekuScheduledMenu {
            return super::motion_candidates(chart, origin, checkpoint);
        }
        let iteration = chart["preparedIteration"]
            .as_u64()
            .ok_or_else(|| invalid("Missing prepared schedule iteration"))?;
        let mut items = capture["captures"]
            .as_array()
            .ok_or_else(|| invalid("Missing schedule captures"))?
            .iter()
            .filter(|c| c["frame"].as_u64() == Some(iteration));
        let item = items
            .next()
            .ok_or_else(|| invalid("Prepared schedule snapshot missing"))?;
        if items.next().is_some() {
            return Err(invalid("Ambiguous prepared schedule snapshot"));
        }
        let graph = Graph::new(&item["state"])?;
        let root = graph.root()?;
        if extraction::numeric(prop(&root, "frame_num")?)? != 0.
            || chart["clockField"] != "frame_num"
            || chart["scheduleAgreement"] != true
        {
            return Err(invalid("Need proved, unconsumed variable-travel schedule"));
        }
        let travel = graph.props(prop(&root, "judgeY")?)?;
        let indices = chart["nativeLaneIndices"]
            .as_array()
            .ok_or_else(|| invalid("Missing native lane indices"))?;
        let lanes = chart["lanes"]
            .as_array()
            .filter(|a| a.len() <= 7 && a.len() == indices.len())
            .ok_or_else(|| invalid("Unproved schedule lane layout"))?;
        let mut values = vec![[None; 2]; lanes.len()];
        for (index, lane) in lanes.iter().enumerate() {
            for (slot, kind, head) in [
                (
                    0,
                    "taps",
                    lane["nativeFrames"].as_array().and_then(|a| a.first()),
                ),
                (
                    1,
                    "holds",
                    lane["nativeHoldFrames"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|p| p.as_array())
                        .and_then(|p| p.first()),
                ),
            ] {
                checkpoint()?;
                let Some(head) = head else { continue };
                let head = extraction::numeric(head)?;
                let rows = chart["nativeSchedules"][kind]
                    .as_array()
                    .filter(|a| a.len() <= 1000000)
                    .ok_or_else(|| invalid("Missing/bounded native schedule"))?;
                for row in rows {
                    checkpoint()?;
                    if row["lane"] != indices[index] {
                        continue;
                    }
                    let spawn = extraction::numeric(&row["frame"])?;
                    if spawn.fract() != 0. || spawn.abs() > 720000. {
                        return Err(invalid("Invalid scheduled spawn"));
                    }
                    let frames = extraction::numeric(prop(&travel, &format!("{spawn:.0}"))?)?;
                    if (spawn + frames - head).abs() <= 1e-9
                        && values[index][slot].replace(frames).is_some()
                    {
                        return Err(invalid("Ambiguous first-head travel"));
                    }
                }
                if values[index][slot].is_none() {
                    return Err(invalid("No exact scheduled travel for first head"));
                }
            }
        }
        super::propose_motion(
            chart,
            origin,
            &mut |lane, source, _| {
                values[lane][if source == MotionSource::Tap { 0 } else { 1 }]
                    .ok_or_else(|| invalid("Missing first-head travel"))
            },
            checkpoint,
        )
    }
    pub(super) fn check_render_support(self, capture: &Value) -> Result<()> {
        if self != Self::RekuScheduledMenu {
            return Ok(());
        }
        let Some(entries) = capture["audit"]["entries"].as_array() else {
            return Ok(());
        };
        let errors = entries
            .iter()
            .filter(|e| e["kind"] == "runtime-error")
            .collect::<Vec<_>>();
        if !errors.is_empty()
            && errors.iter().all(|e| {
                e["detail"] == "message=Render backend does not support BitmapData.draw "
                    && e["truncated"] == false
            })
        {
            return Err(Error::new(
                "native-profile-render-required",
                "This work needs bitmap drawing; this automatic profile has no bitmap renderer",
            ));
        }
        Ok(())
    }
    pub(super) fn confirm_start_settings(self, capture: &Value) -> Result<()> {
        if self != Self::RekuScheduledMenu {
            return Ok(());
        }
        for item in capture["captures"]
            .as_array()
            .ok_or_else(|| invalid("Missing pre-start settings"))?
        {
            let g = Graph::new(&item["state"])?;
            let root = g.root()?;
            if extraction::text(prop(&root, "ber")?)? != "Practice"
                || extraction::numeric(prop(&root, "startlife")?)? != 100.
                || ["d1", "d3", "d4", "d5", "d6"]
                    .iter()
                    .any(|k| prop(&root, k).and_then(extraction::numeric) != Ok(0.))
            {
                return Err(invalid(
                    "Scheduled profile requires observed Practice gauge before Start",
                ));
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn state(gauge: &str, life: f64, loss: f64) -> Value {
        let string = |s: &str| json!({"type":"string","utf16":s.encode_utf16().collect::<Vec<_>>(),"truncated":false});
        let mut props = vec![
            json!({"name":string("ber"),"value":string(gauge)}),
            json!({"name":string("startlife"),"value":life}),
        ];
        for key in ["d1", "d3", "d4", "d5", "d6"] {
            props.push(json!({"name":string(key),"value":loss}));
        }
        json!({"captures":[{"state":{"incomplete":false,"avm1Halted":false,"roots":[{"level":0,"value":{"ref":0},"movieFrame":3}],"objects":[{"id":0,"truncated":false,"properties":props}]}}]})
    }
    #[test]
    fn bitmap_requirement_is_reported_without_suppressing_any_runtime_error() {
        let entry = json!({"kind":"runtime-error","detail":"message=Render backend does not support BitmapData.draw ","truncated":false});
        let capture = json!({"audit":{"entries":[entry.clone()]}});
        assert_eq!(
            EngineProfile::RekuScheduledMenu
                .check_render_support(&capture)
                .unwrap_err()
                .code,
            "native-profile-render-required"
        );
        let mixed = json!({"avm1Halted":false,"audit":{"runtimeErrors":2,"truncated":false,"droppedEntries":0,"truncatedDetails":0,"entries":[entry,{"kind":"runtime-error","detail":"other runtime failure","truncated":false}]},"source":{"config":{"resources":[]}}});
        assert!(
            EngineProfile::RekuScheduledMenu
                .check_render_support(&mixed)
                .is_ok()
        ); // General audit rejects it, rather than hiding another error.
        assert!(crate::extraction::audit_ok(&mixed, &[]).is_err());
    }
    #[test]
    fn practice_is_an_observed_prestart_setting_not_an_assumed_click_effect() {
        let profile = EngineProfile::RekuScheduledMenu;
        assert!(
            profile
                .confirm_start_settings(&state("Practice", 100., 0.))
                .is_ok()
        );
        for s in [
            state("Default", 100., 0.),
            state("Practice", 15., 0.),
            state("Practice", 100., 1.),
        ] {
            assert!(profile.confirm_start_settings(&s).is_err());
        }
    }
}
