use super::*;
pub(super) fn movie_arrays(
    capture: &Value,
    options: &ProjectionOptions,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Projection> {
    audit_ok(capture, &[])?;
    let expected = if options.family == Family::ChildTimeline5 {
        Mode::Five
    } else {
        Mode::Seven
    };
    if options.mode != expected {
        return Err(invalid("Movie family key mode differs"));
    }
    let (defs, _) = definitions(expected)?;
    let five = expected == Mode::Five;
    let required = defs
        .iter()
        .map(|(lane, clip)| {
            format!(
                "{}{}",
                if five { lane } else { clip },
                if five { "_timeline" } else { "_dac" }
            )
        })
        .collect::<Vec<_>>();
    let mut initialized = Vec::new();
    for item in items(capture)? {
        checkpoint()?;
        let graph = Graph::new(&item["state"])?;
        let root = graph.root()?;
        let Some((bank_id, bank)) = graph.bank(&required)? else {
            continue;
        };
        let owner = if five { &bank } else { &root };
        let indices: &[&str] = if five {
            &["v", "y", "x", "w", "z"]
        } else {
            &["les", "leds", "dos", "onis", "ups", "rids", "ris"]
        };
        if indices
            .iter()
            .map(|k| number(owner, k))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .any(|v| *v != 0.0)
        {
            continue;
        }
        let mut selected = root
            .get("diffin")
            .map(|v| suffix(v))
            .transpose()?
            .unwrap_or_default();
        if let Some(suffix) = &options.source_suffix {
            if root.contains_key("diffin") && selected != *suffix {
                return Err(invalid("Explicit selection differs from native selector"));
            }
            selected = suffix.clone();
        }
        let mut lanes = Vec::new();
        for (name, clip) in defs {
            let key = format!(
                "{}{}",
                if five { name } else { clip },
                if five { "_timeline" } else { "_dac" }
            );
            let taps = numbers(&graph, get(&bank, &key)?)?;
            let flat = if five {
                vec![]
            } else {
                numbers(&graph, optional(&root, &format!("f{clip}_dac")))?
            };
            let pairs = holds(&flat, false)?;
            let field = source_field(&root, name, &selected, &options.source_prefix)?;
            if options.source_suffix.is_some() && field.is_none() {
                return Err(invalid("Selected chart requires all loaded lane fields"));
            }
            if let Some(field) = field
                && split_numbers(get(&root, &field)?)? != taps
            {
                return Err(invalid("Native taps differ from loaded source"));
            }
            let field = source_field(
                &root,
                &hold_stem(name),
                &selected,
                if five { &options.source_prefix } else { "" },
            )?;
            if let Some(field) = field {
                let source = split_numbers(get(&root, &field)?)?;
                if (five && !source.is_empty()) || (!five && source != flat) {
                    return Err(invalid(
                        "Unsupported child holds or native hold/source disagreement",
                    ));
                }
            }
            if taps
                .iter()
                .chain(&flat)
                .any(|v| v.fract() != 0.0 || !(0.0..=720000.0).contains(v))
            {
                return Err(invalid("Invalid movie-frame note"));
            }
            lanes.push(lane(name, taps, pairs, true));
        }
        let flat = if !five {
            let flat = numbers(&graph, get(&root, "speed_dac")?)?;
            if split_numbers(get(&root, "speed_change")?)? != flat {
                return Err(invalid("Speed source/native disagreement"));
            }
            flat
        } else if let Some(native) = bank.get("speed_timeline") {
            let flat = numbers(&graph, native)?;
            let field = if options.source_suffix.is_some() {
                source_field(&root, "speed", &selected, &options.source_prefix)?
            } else {
                Some("speed_data".into())
            };
            if let Some(field) = field
                && root.contains_key(&field)
                && split_numbers(get(&root, &field)?)? != flat
            {
                return Err(invalid("Speed source/native disagreement"));
            }
            flat
        } else {
            if let Some(field) = source_field(&root, "speed", &selected, &options.source_prefix)?
                && !split_numbers(get(&root, &field)?)?.is_empty()
            {
                return Err(Error::new(
                    "native-source-bank-missing",
                    "Source speeds have no native bank",
                ));
            }
            vec![]
        };
        for key in ["boost_data", "color_data", "acolor_data"] {
            if let Some(v) = root.get(key)
                && !text(v)?.trim_matches([',', '\r', '\n', ' ']).is_empty()
            {
                return Err(invalid("Unimplemented authored movie effect"));
            }
        }
        if options.source_suffix.is_some() {
            for stem in ["boost", "color", "acolor", "stop"] {
                if let Some(field) = source_field(&root, stem, &selected, &options.source_prefix)?
                    && !text(get(&root, &field)?)?
                        .trim_matches([',', '\r', '\n', ' '])
                        .is_empty()
                {
                    return Err(invalid("Unimplemented selected chart effect"));
                }
            }
        }
        let stop = if five {
            vec![]
        } else {
            numbers(&graph, optional(&bank, "stop_dac"))?
        };
        if let Some(v) = root.get("stop_data")
            && split_numbers(v)? != stop
        {
            return Err(invalid("Visual stop/source disagreement"));
        }
        let mut known = required.clone();
        known.extend(if five {
            vec!["speed_timeline".into()]
        } else {
            vec!["stop_dac".into(), "speed_dac".into()]
        });
        if bank
            .keys()
            .any(|k| k.ends_with(if five { "_timeline" } else { "_dac" }) && !known.contains(k))
        {
            return Err(invalid("Unrecognized extra native lane or effect"));
        }
        let codes = if optional(&root, "keyCtrl")["ref"].is_number() {
            json!(numbers(&graph, get(&root, "keyCtrl")?)?)
        } else {
            Value::Null
        };
        let mut chart = json!({"mode":options.mode,"lanes":lanes,"speed":events(&flat)?,"boost":[],"colors":[],"stops":stop.iter().enumerate().map(|(i,f)|json!({"nativeFrame":f,"paused":i%2==0})).collect::<Vec<_>>(),"metadata":metadata(&root,&["diffi","musicTitle","tuning","gatit"])?,"bankObject":bank_id,"nativeKeyCodes":codes});
        if let Some(s) = &options.source_suffix {
            chart["selectedSourceSuffix"] = json!(s);
        }
        initialized.push((iteration(item)?, chart));
    }
    if initialized.len() < 2 {
        return Err(invalid("Need repeated unconsumed native arrays"));
    }
    let (iteration, chart) = initialized.remove(0);
    for (_, later) in initialized {
        equal_fields(&chart, &later, &["lanes", "speed", "stops", "mode"])?;
    }
    Ok(Projection {
        prepared_iteration: iteration,
        chart,
    })
}
fn dac_speed(
    graph: &Graph<'_>,
    root: &Props<'_>,
    selected: &str,
) -> Result<(Vec<f64>, Vec<Value>)> {
    let value = get(root, "speed_dac")?;
    let raw = if undefined(value) {
        vec![]
    } else {
        graph.array(value)?
    };
    let field = source_field(root, "speed", selected, "")?;
    let strings = raw.iter().map(|v| text(v)).collect::<Result<Vec<_>>>()?;
    if let Some(field) = field {
        if strings != text(get(root, &field)?)?.split(',').collect::<Vec<_>>() {
            return Err(invalid("DAC speed split source disagreement"));
        }
    } else if !strings.is_empty() && strings != [""] {
        return Err(invalid("DAC speeds lack selected source"));
    }
    let mut cut = raw.len();
    while cut > 0
        && raw[cut - 1].is_object()
        && text(raw[cut - 1])?
            .lines()
            .all(|l| l.trim().is_empty() || l.trim_start().starts_with("//"))
    {
        cut -= 1;
    }
    if !cut.is_multiple_of(2) {
        return Err(invalid("Inactive speed tail begins inside a pair"));
    }
    Ok((
        raw[..cut]
            .iter()
            .map(|v| numeric(v))
            .collect::<Result<Vec<_>>>()?,
        raw[cut..].iter().map(|v| (*v).clone()).collect(),
    ))
}
pub(super) fn dac_five(
    capture: &Value,
    options: &ProjectionOptions,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Projection> {
    audit_ok(capture, &[])?;
    if options.mode != Mode::Five {
        return Err(invalid("DAC five mode differs"));
    }
    let fixed = options.family == Family::DacFixed5;
    let defs = [
        ("left", "le"),
        ("down", "do"),
        ("up", "up"),
        ("right", "ri"),
        ("space", "oni"),
    ];
    let required = defs
        .iter()
        .map(|(_, c)| format!("{c}_dac"))
        .collect::<Vec<_>>();
    let mut initialized = Vec::new();
    for item in items(capture)? {
        checkpoint()?;
        let graph = Graph::new(&item["state"])?;
        let root = graph.root()?;
        let Some((bank_id, bank)) = graph.bank(&required)? else {
            continue;
        };
        if fixed {
            if root.contains_key("keyLabel")
                || root.contains_key("keyCtrl")
                || ["LE", "DO", "UP", "RI", "SH"]
                    .iter()
                    .any(|k| !optional(&root, &format!("key_{k}")).is_boolean())
            {
                return Err(invalid("Need fixed five-key controls"));
            }
        } else if suffix(get(&root, "keyLabel")?)? != "5" || !root.contains_key("keyCtrl") {
            return Err(invalid("Need selected configurable five-key mode"));
        }
        let cursors = defs
            .iter()
            .map(|(_, c)| format!("{c}s"))
            .chain(
                defs.iter()
                    .filter(|(_, c)| root.contains_key(&format!("f{c}_dac")))
                    .map(|(_, c)| format!("f{c}s")),
            )
            .collect::<Vec<_>>();
        if cursors
            .iter()
            .map(|k| number(&root, k))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .any(|v| *v != 0.0)
            || ["cunt", "sp", "stopcunt"]
                .iter()
                .map(|k| number(&bank, k))
                .collect::<Result<Vec<_>>>()?
                .iter()
                .any(|v| *v != 0.0)
        {
            continue;
        }
        let selected = root
            .get("diffin")
            .map(|v| suffix(v))
            .transpose()?
            .unwrap_or_default();
        let mut lanes = Vec::new();
        for (name, clip) in defs {
            let taps = numbers(&graph, get(&bank, &format!("{clip}_dac"))?)?;
            let flat = numbers(&graph, optional(&root, &format!("f{clip}_dac")))?;
            for (stem, values) in [(name.to_string(), &taps), (hold_stem(name), &flat)] {
                let field = source_field(&root, &stem, &selected, "")?;
                let source = field
                    .map(|f| split_numbers(get(&root, &f)?))
                    .transpose()?
                    .unwrap_or_default();
                if source != *values {
                    return Err(invalid("DAC source/native lane disagreement"));
                }
            }
            if taps
                .iter()
                .chain(&flat)
                .any(|v| v.fract() != 0.0 || !(0.0..=720000.0).contains(v))
            {
                return Err(invalid("Invalid DAC frame"));
            }
            lanes.push(lane(name, taps, holds(&flat, true)?, true));
        }
        for clip in ["led", "rid"] {
            if !numbers(&graph, optional(&bank, &format!("{clip}_dac")))?.is_empty()
                || !numbers(&graph, optional(&root, &format!("f{clip}_dac")))?.is_empty()
            {
                return Err(invalid("Active diagonal DAC five lanes"));
            }
        }
        let mut known = required.clone();
        known.extend(["led_dac", "rid_dac", "stop_dac"].map(str::to_string));
        let mut known_root = defs
            .iter()
            .map(|(_, c)| format!("f{c}_dac"))
            .collect::<Vec<_>>();
        known_root.extend(["fled_dac", "frid_dac", "speed_dac"].map(str::to_string));
        if bank
            .keys()
            .any(|k| k.ends_with("_dac") && !known.contains(k))
            || root
                .keys()
                .any(|k| k.ends_with("_dac") && !known_root.contains(k))
        {
            return Err(invalid("Unknown DAC lane/effect bank"));
        }
        let (flat, inactive) = dac_speed(&graph, &root, &selected)?;
        if flat
            .as_chunks::<2>()
            .0
            .iter()
            .any(|p| p[0].fract() != 0.0 || !(150.0..=720000.0).contains(&p[0]) || p[1] <= 0.0)
            || flat
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>()
                .windows(2)
                .any(|p| p[0] > p[1])
        {
            return Err(invalid("Invalid DAC speed pairs"));
        }
        let stop = numbers(&graph, get(&bank, "stop_dac")?)?;
        let source = source_field(&root, "stop", &selected, "")?
            .map(|f| split_numbers(get(&root, &f)?))
            .transpose()?
            .unwrap_or_default();
        if stop != source
            || !stop.len().is_multiple_of(2)
            || stop.iter().any(|v| v.fract() != 0.0 || *v < 0.0)
            || stop.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(invalid("Invalid DAC visual stop flags"));
        }
        for stem in ["boost", "color", "acolor"] {
            if let Some(field) = source_field(&root, stem, &selected, "")?
                && !text(get(&root, &field)?)?
                    .trim_matches([',', '\r', '\n', ' '])
                    .is_empty()
            {
                return Err(invalid("Unsupported DAC effect"));
            }
        }
        let initial = number(&root, "set_spi")?;
        if initial <= 0.0
            || initial != 2.0 * number(&root, "speed")?
            || number(&root, "olds")? != initial
        {
            return Err(invalid("Unproved DAC initial pixel speed"));
        }
        let mut speed = Vec::new();
        let mut native_speed = Vec::new();
        for p in flat.as_chunks::<2>().0.iter() {
            let pixel = if fixed {
                initial * p[1]
            } else {
                initial + 2.0 * (p[1] - 1.0)
            };
            if pixel <= 0.0 || !pixel.is_finite() || !(pixel / initial).is_finite() {
                return Err(invalid("Nonpositive or overflowing DAC arrow speed"));
            }
            native_speed.push(json!({"nativeFrame":p[0],"multiplier":p[1],"applicationFrame":p[0]-150.0,"pixelSpeed":pixel}));
            speed.push(json!({"nativeFrame":p[0],"multiplier":pixel/initial}));
        }
        let codes = if root.contains_key("keyCtrl") {
            let all = numbers(&graph, get(&root, "keyCtrl")?)?;
            if all.len() != 7
                || all
                    .iter()
                    .any(|v| v.fract() != 0.0 || !(0.0..=65535.0).contains(v))
            {
                return Err(invalid("Invalid DAC key bank"));
            }
            json!([all[0], all[2], all[4], all[6], all[3]])
        } else {
            Value::Null
        };
        let mut chart = json!({"mode":"5","lanes":lanes,"speed":speed,"boost":[],"colors":[],"stops":stop.iter().enumerate().map(|(i,f)|json!({"nativeFrame":f,"paused":i%2==0})).collect::<Vec<_>>(),"initialPixelSpeed":initial,"bankObject":bank_id,"nativeKeyCodes":codes,"motionSpeedPolicy":"per-arrow-at-spawn","spawnLookaheadFrames":150,"nativeSpeedChanges":native_speed,"speedValuePolicy":if fixed{"multiplicative"}else{"additive-two-pixels"},"inactiveSpeedTail":inactive,"metadata":metadata(&root,&["diffi","gatit","musicTitle","tuning"])?});
        if fixed {
            let independent = number(&root, "oni_spi")?;
            if independent <= 0.0 || independent != initial {
                return Err(invalid("Unproved onigiri speed"));
            }
            for k in ["onisp_data", "onisp_change"] {
                if let Some(v) = root.get(k)
                    && !text(v)?.trim_matches([',', '\r', '\n', ' ']).is_empty()
                {
                    return Err(invalid("Independent onigiri speed changes unsupported"));
                }
            }
            chart["independentOnigiriPixelSpeed"] = json!(independent);
        }
        initialized.push((iteration(item)?, chart));
    }
    if initialized.len() < 2 {
        return Err(invalid("Need repeated unconsumed DAC banks"));
    }
    let (iteration, chart) = initialized.remove(0);
    for (_, later) in initialized {
        equal_fields(
            &chart,
            &later,
            &[
                "mode",
                "lanes",
                "speed",
                "stops",
                "initialPixelSpeed",
                "nativeKeyCodes",
                "metadata",
                "nativeSpeedChanges",
                "speedValuePolicy",
                "inactiveSpeedTail",
            ],
        )?;
    }
    Ok(Projection {
        prepared_iteration: iteration,
        chart,
    })
}
