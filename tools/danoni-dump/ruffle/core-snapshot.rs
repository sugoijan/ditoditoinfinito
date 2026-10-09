// Read stored AVM1 data without property lookup, getters, coercions or methods.
use crate::Player;
use crate::avm1::NativeObject;
use crate::avm1::{Object, Value};
use crate::display_object::{BoundsMode, DisplayObject, TDisplayObject};
use crate::prelude::TDisplayObjectContainer;
use serde_json::{Value as Json, json};
use std::collections::HashMap;

const MAX_OBJECTS: usize = 1024;
const MAX_PROPERTIES: usize = 20000;
const MAX_STRING_UNITS: usize = 4096;
const MAX_TOTAL_STRING_UNITS: usize = 262144;
const MAX_DISPLAYS: usize = 512;

struct Inspector<'gc> {
    ids: HashMap<usize, usize>,
    queue: Vec<Object<'gc>>,
    remaining: usize,
    string_remaining: usize,
    incomplete: bool,
}

impl<'gc> Inspector<'gc> {
    fn string(&mut self, s: crate::string::AvmString<'gc>) -> Json {
        let units: Vec<u16> = s
            .iter()
            .take(MAX_STRING_UNITS.min(self.string_remaining))
            .collect();
        self.string_remaining -= units.len();
        let truncated = s.len() > units.len();
        self.incomplete |= truncated;
        json!({"type":"string", "text":String::from_utf16_lossy(&units),
            "utf16":units, "truncated":truncated})
    }

    fn value(&mut self, value: Value<'gc>) -> Json {
        match value {
            Value::Undefined => json!({"type":"undefined"}),
            Value::Null => Json::Null,
            Value::Bool(v) => json!(v),
            Value::Number(v) if v.is_finite() && !(v == 0.0 && v.is_sign_negative()) => json!(v),
            Value::Number(v) => json!({"type":"number", "value":
                if v.is_nan() { "NaN" } else if v == 0.0 { "-0" }
                else if v.is_sign_negative() { "-Infinity" } else { "Infinity" }}),
            Value::String(v) => self.string(v),
            Value::Object(v) => {
                let ptr = v.as_ptr() as usize;
                if let Some(id) = self.ids.get(&ptr) {
                    return json!({"ref":id});
                }
                if self.queue.len() == MAX_OBJECTS {
                    self.incomplete = true;
                    return json!({"type":"uninspected", "reason":"object-limit"});
                }
                let id = self.queue.len();
                self.ids.insert(ptr, id);
                self.queue.push(v);
                json!({"ref":id})
            }
            Value::MovieClip(_) => {
                // Resolving a MovieClipReference can mutate VM state. Do not do it.
                json!({"type":"uninspected", "reason":"movie-clip-reference"})
            }
        }
    }
}

/// Experimental graph snapshot. IDs are local to this snapshot, never VM addresses.
pub fn capture(player: &mut Player, inspect_globals: bool) -> Json {
    player.mutate_with_update_context(|context| {
        let mut inspect = Inspector {
            ids: HashMap::new(),
            queue: Vec::new(),
            remaining: MAX_PROPERTIES,
            string_remaining: MAX_TOTAL_STRING_UNITS,
            incomplete: false,
        };
        let mut roots = Vec::new();
        let mut displays = context.stage.iter_render_list();
        for display in displays.by_ref().take(64) {
            if let Some(object) = display.object1() {
                roots.push(json!({"level":display.depth(),
                    "movieFrame":display.as_movie_clip().map(|clip|clip.current_frame()),
                    "value":inspect.value(Value::Object(object))}));
            }
        }
        inspect.incomplete |= displays.next().is_some();
        // Native display traversal can inspect child MovieClip storage without
        // resolving script MovieClipReference values or invoking user getters.
        let mut display_queue: Vec<_> = context
            .stage
            .iter_render_list()
            .take(64)
            .map(|display| (None, display))
            .collect();
        let mut display_tree = Vec::new();
        let mut display_cursor = 0;
        while display_cursor < display_queue.len() {
            let (parent, display) = display_queue[display_cursor];
            let kind = match display {
                DisplayObject::MovieClip(_) => "movie-clip",
                DisplayObject::Avm1Button(_) => "avm1-button",
                DisplayObject::EditText(_) => "edit-text",
                DisplayObject::Text(_) => "text",
                _ => "display",
            };
            let bounds = display.world_bounds(BoundsMode::Engine);
            let registration = display.local_to_global(swf::Point::default());
            display_tree.push(json!({"id":display_cursor,"parent":parent,
                "kind":kind,"name":display.name().map(|name| inspect.string(name)),
                "depth":display.depth(),"visible":display.visible(),
                "movieFrame":display.as_movie_clip().map(|clip|clip.current_frame()),
                "stageRegistrationPixels":[registration.x.to_pixels(), registration.y.to_pixels()],
                "stageBoundsPixels":[bounds.x_min.to_pixels(),bounds.y_min.to_pixels(),
                    bounds.x_max.to_pixels(),bounds.y_max.to_pixels()],
                "object":display.object1().map(|object|inspect.value(Value::Object(object)))}));
            if let Some(container) = display.as_container() {
                for child in container.iter_render_list() {
                    if display_queue.len() == MAX_DISPLAYS {
                        inspect.incomplete = true;
                        break;
                    }
                    display_queue.push((Some(display_cursor), child));
                }
            }
            display_cursor += 1;
        }
        // Reading native scope storage does not resolve any ActionScript name.
        // Keep the two AVM1 case-sensitivity environments explicitly identified.
        let global_scopes: Vec<_> = if inspect_globals {
            [6, 7].into_iter().map(|version| {
                let object = context.avm1.global_scope(version).locals_cell();
                let (properties, truncated) = object.ddi_properties(inspect.remaining);
                inspect.remaining -= properties.len();
                inspect.incomplete |= truncated;
                let properties: Vec<_> = properties.into_iter()
                    .filter(|(_, _, _, enumerable)| *enumerable)
                    .map(|(name, value, accessor, _)| {
                        let value = if accessor {
                            json!({"type":"uninspected","reason":"accessor"})
                        } else if matches!(value, Value::Object(object)
                            if matches!(object.native(), NativeObject::Function(_))) {
                            json!({"type":"uninspected","reason":"global-function"})
                        } else { inspect.value(value) };
                        json!({"name":inspect.string(name),"value":value,"accessor":accessor})
                    }).collect();
                json!({"caseSensitive":version >= 7,"properties":properties,
                    "excluded":["non-enumerable-properties","function-values"]})
            }).collect()
        } else { Vec::new() };
        let mut objects = Vec::new();
        let mut cursor = 0;
        while cursor < inspect.queue.len() {
            let object = inspect.queue[cursor];
            let kind = match object.native() {
                NativeObject::None => "object",
                NativeObject::Array(_) => "array",
                NativeObject::MovieClip(_) => "movie-clip",
                NativeObject::Function(_) => "function",
                _ => "native-unsupported",
            };
            let (properties, truncated) = object.ddi_properties(inspect.remaining);
            inspect.remaining -= properties.len();
            inspect.incomplete |= truncated;
            let mut output = Vec::new();
            for (name, value, virtual_property, enumerable) in properties {
                // Exclude prototype traversal explicitly, not by enumerable flags.
                let value = if name.to_utf8_lossy() == "__proto__" {
                    json!({"type":"uninspected", "reason":"prototype"})
                } else if virtual_property {
                    json!({"type":"uninspected", "reason":"accessor"})
                } else {
                    inspect.value(value)
                };
                output.push(json!({"name":inspect.string(name), "value":value,
                    "enumerable":enumerable, "accessor":virtual_property}));
            }
            objects.push(
                json!({"id":cursor, "kind":kind, "properties":output, "truncated":truncated}),
            );
            cursor += 1;
        }
        json!({"schema":"ddi-avm1-snapshot/experimental-1", "roots":roots,
            "objects":objects,"displayTree":display_tree,"globalScopes":global_scopes, "incomplete":inspect.incomplete,
            "avm1Halted":context.avm1.ddi_halted(),
            "limits":{"roots":64,"objects":MAX_OBJECTS,"properties":MAX_PROPERTIES,
                "stringUnits":MAX_STRING_UNITS,"totalStringUnits":MAX_TOTAL_STRING_UNITS,
                "displays":MAX_DISPLAYS},
            "excluded":["prototypes","accessor-values","movie-clip-references","native-internals"]})
    })
}

pub fn halted(player: &mut Player) -> bool {
    player.mutate_with_update_context(|context| context.avm1.ddi_halted())
}
