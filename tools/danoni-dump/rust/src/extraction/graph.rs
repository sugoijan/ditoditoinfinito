//! Stored AVM1 graph data only: no coercions, getters or executable lookup.
use super::*;
pub type Props<'a> = BTreeMap<String, &'a Value>;
pub fn text(value: &Value) -> Result<String> {
    if value["type"] != "string" || value["truncated"] != false {
        return Err(invalid("Need a complete stored UTF-16 string"));
    }
    let units = array(&value["utf16"])?;
    if units.len() > 131072 {
        return Err(invalid("Native string limit"));
    }
    let units = units
        .iter()
        .map(|v| {
            v.as_u64()
                .filter(|v| *v <= 65535)
                .map(|v| v as u16)
                .ok_or_else(|| invalid("Invalid UTF-16 unit"))
        })
        .collect::<Result<Vec<_>>>()?;
    String::from_utf16(&units)
        .map_err(|_| invalid("Unpaired UTF-16 surrogate; original units remain source evidence"))
}
pub fn numeric(value: &Value) -> Result<f64> {
    let number = if value.is_number() {
        value.as_f64()
    } else {
        text(value)?.trim().parse::<f64>().ok()
    };
    number
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("Need a finite stored number"))
}
pub fn undefined(value: &Value) -> bool {
    *value == json!({"type":"undefined"})
}
pub fn missing() -> &'static Value {
    static MISSING: std::sync::LazyLock<Value> =
        std::sync::LazyLock::new(|| json!({"type":"undefined"}));
    &MISSING
}
pub fn get<'a>(props: &Props<'a>, key: &str) -> Result<&'a Value> {
    props
        .get(key)
        .copied()
        .ok_or_else(|| invalid(format!("Missing native property {key}")))
}
pub fn optional<'a>(props: &Props<'a>, key: &str) -> &'a Value {
    props.get(key).copied().unwrap_or(missing())
}
pub fn number(props: &Props<'_>, key: &str) -> Result<f64> {
    numeric(get(props, key)?)
}
pub fn array(value: &Value) -> Result<&Vec<Value>> {
    value.as_array().ok_or_else(|| invalid("Need an array"))
}
#[derive(Clone)]
pub struct Graph<'a> {
    pub state: &'a Value,
    objects: &'a Vec<Value>,
}
impl<'a> Graph<'a> {
    pub fn new(state: &'a Value) -> Result<Self> {
        if state["incomplete"] != false || state["avm1Halted"] != false {
            return Err(invalid("Incomplete or halted native state"));
        }
        let objects = array(&state["objects"])?;
        if objects.len() > 32768 {
            return Err(invalid("Native object limit"));
        }
        Ok(Self { state, objects })
    }
    pub fn props(&self, reference: &Value) -> Result<Props<'a>> {
        let index = reference["ref"]
            .as_u64()
            .ok_or_else(|| invalid("Need an object reference"))?;
        let checked_index =
            usize::try_from(index).map_err(|_| invalid("Native reference index overflow"))?;
        let object = self
            .objects
            .get(checked_index)
            .ok_or_else(|| invalid("Dangling native reference"))?;
        if object["id"] != index || object["truncated"] != false {
            return Err(invalid("Invalid or truncated native object"));
        }
        let properties = array(&object["properties"])?;
        if properties.len() > 640000 {
            return Err(invalid("Native property limit"));
        }
        let mut result = Props::new();
        for property in properties {
            let name = text(&property["name"])?;
            let value = property
                .get("value")
                .ok_or_else(|| invalid("Missing property value"))?;
            if result.insert(name, value).is_some() {
                return Err(invalid("Duplicate native property"));
            }
        }
        Ok(result)
    }
    pub fn array(&self, reference: &Value) -> Result<Vec<&'a Value>> {
        let props = self.props(reference)?;
        let length = number(&props, "length")?;
        if !(0.0..=20000.0).contains(&length) || length.fract() != 0.0 {
            return Err(invalid("Invalid native array length"));
        }
        (0..length as usize)
            .map(|i| get(&props, &i.to_string()))
            .collect()
    }
    pub fn root(&self) -> Result<Props<'a>> {
        let roots = array(&self.state["roots"])?;
        if roots.len() != 1 || roots[0]["level"] != 0 {
            return Err(invalid("Unexpected native root levels"));
        }
        self.props(&roots[0]["value"])
    }
    pub fn root_frame(&self) -> Result<f64> {
        numeric(&self.state["roots"][0]["movieFrame"])
    }
    pub fn bank(&self, required: &[String]) -> Result<Option<(u64, Props<'a>)>> {
        let mut found = None;
        for object in self.objects {
            let id = object["id"]
                .as_u64()
                .ok_or_else(|| invalid("Missing object id"))?;
            let props = self.props(&json!({"ref":id}))?;
            if required.iter().all(|k| props.contains_key(k)) {
                if found.is_some() {
                    return Err(invalid("Ambiguous native data banks"));
                }
                found = Some((id, props));
            }
        }
        Ok(found)
    }
}
pub fn numbers(graph: &Graph<'_>, value: &Value) -> Result<Vec<f64>> {
    if undefined(value) {
        return Ok(vec![]);
    }
    let mut values = graph.array(value)?;
    if let Some(last) = values.last()
        && last["type"] == "string"
        && text(last)?.trim().is_empty()
    {
        values.pop();
    }
    values.into_iter().map(numeric).collect()
}
pub fn split_numbers(value: &Value) -> Result<Vec<f64>> {
    let value = text(value)?;
    let mut parts = value.split(',').collect::<Vec<_>>();
    while parts.last().is_some_and(|p| p.trim().is_empty()) {
        parts.pop();
    }
    parts
        .into_iter()
        .map(|s| {
            s.trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| invalid("Empty or invalid source frame token"))
        })
        .collect()
}
pub fn suffix(value: &Value) -> Result<String> {
    if value.is_object() {
        return text(value);
    }
    let v = numeric(value)?;
    if !(0.0..=999.0).contains(&v) || v.fract() != 0.0 {
        return Err(invalid("Invalid source selector"));
    }
    Ok(format!("{v:.0}"))
}
pub fn source_field(
    root: &Props<'_>,
    stem: &str,
    suffix: &str,
    prefix: &str,
) -> Result<Option<String>> {
    let a = format!("{prefix}{stem}{suffix}_data");
    let b = format!("{prefix}{stem}_data{suffix}");
    let first = root.contains_key(&a);
    let second = root.contains_key(&b);
    if first && second && a != b {
        return Err(invalid("Ambiguous selected source fields"));
    }
    Ok(if first {
        Some(a)
    } else if second {
        Some(b)
    } else {
        None
    })
}
pub fn hold_stem(name: &str) -> String {
    match name {
        "leftdia" => "frzLdia".into(),
        "rightdia" => "frzRdia".into(),
        "space" => "frzSpace".into(),
        _ => format!("frz{}{}", name[..1].to_uppercase(), &name[1..]),
    }
}
pub fn metadata(root: &Props<'_>, keys: &[&str]) -> Result<Value> {
    let mut result = serde_json::Map::new();
    for key in keys {
        if let Some(v) = root.get(*key)
            && v["type"] == "string"
        {
            result.insert((*key).into(), json!(text(v)?));
        }
    }
    Ok(Value::Object(result))
}
