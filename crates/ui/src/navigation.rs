//! Logical menu parents, shared by the hash router and native menu host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Start,
    Songs,
    Options,
    Calibration,
    Credits,
    Play,
}

impl Destination {
    pub fn fallback(self) -> Self {
        match self {
            Self::Play => Self::Songs,
            Self::Start | Self::Songs | Self::Options | Self::Calibration | Self::Credits => {
                Self::Start
            }
        }
    }
}

/// App-owned route ancestry. Each browser history entry carries its own
/// copy, so Back/Forward and reload recover the caller without globals.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouteTrail {
    pub routes: Vec<String>,
}

impl RouteTrail {
    pub fn at(route: String) -> Self {
        Self {
            routes: vec![route],
        }
    }
    pub fn parent(&self) -> Option<&str> {
        self.routes
            .len()
            .checked_sub(2)
            .map(|i| self.routes[i].as_str())
    }
    pub fn push(&self, route: String) -> Self {
        let mut next = self.clone();
        next.routes.push(route);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_callers_survive_serialization() {
        for origin in ["#/", "#/select"] {
            let options = RouteTrail::at(origin.into()).push("#/options".into());
            let calibration = options.push("#/calibrate".into());
            let restored: RouteTrail =
                serde_json::from_str(&serde_json::to_string(&calibration).unwrap()).unwrap();
            assert_eq!(restored.parent(), Some("#/options"));
            assert_eq!(options.parent(), Some(origin));
        }
        assert_eq!(Destination::Play.fallback(), Destination::Songs);
        assert_eq!(Destination::Options.fallback(), Destination::Start);
    }
}
