//! Physical controls for the exact reviewed 2004 collection. These propose
//! captures only: the pinned extractor proves the selected source, clock,
//! movement and PCM again before anything can reach the ordinary library.
use super::*;

const SELECTIONS: [PinnedSelection; 6] = [
    PinnedSelection::Tougenkyo,
    PinnedSelection::TougenkyoHard,
    PinnedSelection::Run,
    PinnedSelection::Kitakaze,
    PinnedSelection::Korokke,
    PinnedSelection::Emy,
];
struct SelectionPlan {
    events: Vec<Value>,
    frames: Vec<u64>,
    motion: MotionSelection,
}
fn plan(selection: PinnedSelection) -> SelectionPlan {
    use PinnedSelection::*;
    let mut events = vec![json!({"frame":70,"type":"click","x":160,"y":157})];
    let (steps, start, first_motion) = match selection {
        Tougenkyo => (0, 100, 170),
        TougenkyoHard => (0, 180, 250),
        Run => (1, 250, 320),
        Kitakaze => (2, 160, 530),
        Korokke => (3, 250, 320),
        Emy => (4, 250, 320),
    };
    for step in 0..steps {
        events.push(json!({"frame":80+40*step,"type":"key","code":"ArrowRight"}));
    }
    if selection == TougenkyoHard {
        // The original FMV button toggles the hidden chart; no global writes.
        events.extend([
            json!({"frame":100,"type":"key","code":"Space"}),
            json!({"frame":120,"type":"click","x":165,"y":169}),
            json!({"frame":140,"type":"key","code":"Space"}),
        ]);
    }
    events.push(json!({"frame":start,"type":"enter"}));
    let iterations = (0..4).map(|i| first_motion + 20 * i).collect::<Vec<_>>();
    let mut frames = vec![start - 10, start, start + 1, start + 2];
    frames.extend(&iterations);
    frames.push(1200);
    let (note, receptor, lane) = match selection {
        Korokke => ("spaceMC2", "space_step", 4),
        Emy => ("downMC3", "down_step", 1),
        _ => ("leftMC2", "left_step", 0),
    };
    SelectionPlan {
        events,
        frames,
        motion: MotionSelection {
            source: MotionSource::Tap,
            note: note.into(),
            receptor: receptor.into(),
            lane,
            iterations,
            target_y: None,
            geometry_reference: Some("left_step".into()),
        },
    }
}
impl<F: FnMut(&ProfileProgress) -> Result<()>> Run<'_, F> {
    pub(super) fn kouhaku(
        &mut self,
        planned: &mut Vec<ChartPlan>,
        observed_menu: &mut Option<Value>,
        excluded: &mut Vec<Value>,
    ) -> Result<Value> {
        if self
            .request
            .selectors
            .as_ref()
            .is_some_and(|indices| indices.iter().any(|i| *i >= SELECTIONS.len()))
        {
            return Err(invalid(
                "Requested selector is outside the reviewed Kouhaku collection",
            ));
        }
        let mut charts = vec![];
        let mut observations = vec![];
        for (index, selection) in SELECTIONS.into_iter().enumerate() {
            if self
                .request
                .selectors
                .as_ref()
                .is_some_and(|indices| !indices.contains(&index))
            {
                excluded.push(
                    json!({"selector":index,"selection":selection,"reason":"unselected-selector"}),
                );
                continue;
            }
            self.state.selector = Some(index);
            self.state.stage = "initialization";
            let plan = plan(selection);
            let id = format!("chart-{index}");
            let (capture, binding) = self.capture(&id, &plan.events, &plan.frames, true)?;
            let options = ProjectionOptions {
                family: Family::Kouhaku2004,
                mode: Mode::Five,
                source_prefix: String::new(),
                source_suffix: None,
                pinned_selection: Some(selection),
                review_missing_decorative_sprites: false,
            };
            let projection = project_capture(&capture, &options, &mut || self.check())?;
            observations.push(json!({"selector":index,"selection":selection,"capture":binding,"reference":projection.chart["pinnedReference"]}));
            *observed_menu = Some(
                json!({"selections":observations,"scope":"Exact reviewed collection, including the FMV-selected hidden chart; each selection revalidated from its original menu and initialized source"}),
            );
            let label = Some(
                if selection == PinnedSelection::TougenkyoHard {
                    "Hard"
                } else {
                    "Normal"
                }
                .into(),
            );
            charts.push(NativeChartRequest {
                id: id.clone(),
                label: label.clone(),
                options: options.clone(),
                projection: binding.clone(),
                motion: binding.clone(),
                motion_selection: plan.motion.clone(),
                audio: binding.clone(),
                native_pcm: self.pcm_binding(&binding)?,
                stream_start_index: 0,
                optional_color: self.prepared.optional.clone(),
            });
            let observation = Observation {
                capture_frames: plan.frames,
                render: self.request.renderer.is_some(),
                mix_audio: true,
                avm_execution_ms: None,
            };
            planned.push(ChartPlan {
                id,
                label,
                options,
                config: binding.config,
                projection: observation.clone(),
                motion: observation.clone(),
                motion_selection: plan.motion,
                audio: observation,
                stream_start_index: 0,
                optional_color: self.prepared.optional.clone(),
            });
        }
        self.normalize(charts, planned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_controls_include_hidden_chart_and_bound_all_observations() {
        for selection in SELECTIONS {
            let p = plan(selection);
            let raw = config(&json!({"movie":"work.swf","sourceUrl":"https://example.test/work.swf","resources":[]}), &p.events, &p.frames, true, EngineProfile::IsoKouhaku2004, None).unwrap();
            let config: Value = serde_json::from_slice(&raw).unwrap();
            assert_eq!(config["inspectGlobals"], true);
            assert_eq!(config["render"], false);
            let start = p.events.last().unwrap()["frame"].as_u64().unwrap();
            for f in [start - 10, start, start + 1, start + 2, 1200] {
                assert!(p.frames.contains(&f));
            }
            assert!(p.frames.windows(2).all(|f| f[0] < f[1]));
            assert!(p.motion.iterations.iter().all(|f| p.frames.contains(f)));
            let hidden = p
                .events
                .iter()
                .any(|e| e["type"] == "click" && e["frame"] == 120);
            assert_eq!(hidden, selection == PinnedSelection::TougenkyoHard);
        }
    }
}
