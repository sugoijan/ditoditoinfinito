//! The route of songs shared by link, `#/s/<id>` (`crate::share`,
//! `docs/plans/shared-songs.md`): the share read from the Worker, then
//! song select on it in its shared mode, or the page saying the link is
//! gone. Nothing shows while it loads: the boot shell, on a fresh page,
//! or the previous screen's last frame.

use crate::component::{Component, Context, Mounted, mount};
use crate::components::page_routes::{self, PagePage, Which};
use crate::components::select_page::{self, SelectPage};
use crate::share::{self, ShareError};

#[derive(Clone, PartialEq)]
pub(crate) struct Props {
    pub(crate) id: String,
}

pub(crate) enum Msg {
    Loaded(Result<(), ShareError>),
}

pub(crate) struct SharedPage {
    select: Option<Mounted<SelectPage>>,
    gone: Option<Mounted<PagePage>>,
}

impl Component for SharedPage {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let id = ctx.props().id.clone();
        let link = ctx.link().clone();
        wasm_bindgen_futures::spawn_local(async move {
            let loaded = share::load(&id).await.map(|_| ());
            link.send_message(Msg::Loaded(loaded));
        });
        SharedPage {
            select: None,
            gone: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Msg) -> bool {
        let Msg::Loaded(loaded) = msg;
        // Beside this component's (empty) element, as the app mounts it.
        let parent = ctx
            .root()
            .parent_element()
            .unwrap_or_else(|| ctx.root().clone().into());
        match loaded {
            Ok(()) => {
                self.select = Some(mount::<SelectPage>(
                    &parent,
                    "main",
                    select_page::Props { shared: true },
                ));
            }
            Err(e) => {
                self.gone = Some(mount::<PagePage>(
                    &parent,
                    "main",
                    page_routes::Props {
                        which: Which::SharedGone(e.message()),
                    },
                ));
            }
        }
        false
    }

    fn view(&self, ctx: &Context<Self>) {
        ctx.root().set_hidden(true);
    }
}
