// Included inside avm1/object/script_object.rs by prepare.py. No AVM calls.
impl<'gc> Object<'gc> {
    pub(crate) fn ddi_properties(
        self,
        limit: usize,
    ) -> (Vec<(AvmString<'gc>, Value<'gc>, bool, bool)>, bool) {
        let data = self.0.borrow();
        let mut properties = data.properties.iter();
        let result = properties
            .by_ref()
            .take(limit)
            .map(|(name, p)| (name, p.data(), p.is_virtual(), p.is_enumerable()))
            .collect();
        (result, properties.next().is_some())
    }
}
