//! Player settings: the data and its rules are shared with every shell
//! (`ddi_ui::state::settings`, P12); they are kept in `localStorage` under
//! the same key as ever through the store `crate::web::kv` installs.

pub(crate) use ddi_ui::state::settings::*;
