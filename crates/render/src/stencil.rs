//! The stencil buffer, for StepMania's masks and for clips that are not
//! scissor rectangles. StepMania masks through the depth buffer: a
//! `MaskSource` actor draws depth only, nearer than the rest, and a
//! `MaskDest` actor tests depth, so it is drawn only where no mask is
//! (`ZTEST_WRITE_ON_PASS`) or only behind one (`ZTEST_WRITE_ON_FAIL`);
//! texels the alpha test drops (alpha 1/256 or less, on for every draw:
//! `RageDisplay::SetDefaultRenderStates`, `src/RageDisplay.cpp:275`;
//! `src/RageDisplay_OGL.cpp:2731`) leave no depth (`ddi_actor::Depth`).
//! 2D actors all lie at one depth, a mask's nudged nearer (`SetZBias(1)`
//! for `BlendMode_NoEffect`, `src/Actor.cpp:772-775`), so a bit of
//! stencil says the same.
//!
//! One byte: the high bit is the mask, the low seven count the clips
//! drawn into the stencil around what is drawn (a draw's reference is
//! that count, its level). Every draw in a pass with the stencil tests
//! its level, so a draw outside a clip's quad is cut.

/// The attachment's format: depth and stencil, which every backend
/// renders to (a stencil alone is not a WebGL2 texture).
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

/// The mask's bit.
const MASK: u32 = 0x80;
/// The clips' count.
const CLIPS: u32 = 0x7f;
/// The most clips counted.
pub const MAX_LEVEL: u32 = CLIPS - 1;

/// What a draw does with StepMania's mask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mask {
    #[default]
    None,
    /// Writes the mask where it is drawn (`zwrite` with
    /// `BlendMode_NoEffect`, nudged nearer than the rest).
    Write,
    /// Takes the mask away where it is drawn: an actor of another blend
    /// mode writing depth with no test writes its own depth, farther than
    /// a mask's, over it.
    Erase,
    /// Drawn only where no mask is (`ZTEST_WRITE_ON_PASS`, `MaskDest`).
    Outside,
    /// Drawn only where the mask is (`ZTEST_WRITE_ON_FAIL`).
    Inside,
    /// Clears the mask everywhere (`clearzbuffer`): drawn over the whole
    /// target, inside clips or not.
    Clear,
}

/// The stencil states a pipeline is made for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Tests the level alone.
    Level,
    /// Tests the level and the mask.
    Masked,
    /// Writes the mask (its bit from the reference) within the level.
    MaskWrite,
    /// Clears the mask, whatever the level.
    MaskClear,
    /// A clip's quad: one more level within it.
    ClipIn,
    /// The clip's quad again: one level less.
    ClipOut,
}

impl Mask {
    /// The stencil state and reference of a draw at clip `level`.
    pub fn stencil(self, level: u32) -> (Kind, u32) {
        let level = level.min(MAX_LEVEL);
        match self {
            Mask::None => (Kind::Level, level),
            Mask::Write => (Kind::MaskWrite, level | MASK),
            Mask::Erase => (Kind::MaskWrite, level),
            Mask::Clear => (Kind::MaskClear, 0),
            Mask::Outside => (Kind::Masked, level),
            Mask::Inside => (Kind::Masked, level | MASK),
        }
    }
}

/// The pipeline's depth and stencil state for `kind`.
pub fn state(kind: Kind) -> wgpu::DepthStencilState {
    use wgpu::{CompareFunction::Equal, StencilOperation as Op};
    let mut compare = Equal;
    let (read_mask, write_mask, pass_op) = match kind {
        Kind::Level => (CLIPS, 0, Op::Keep),
        Kind::Masked => (MASK | CLIPS, 0, Op::Keep),
        Kind::MaskWrite => (CLIPS, MASK, Op::Replace),
        Kind::MaskClear => {
            compare = wgpu::CompareFunction::Always;
            (0, MASK, Op::Replace)
        }
        Kind::ClipIn => (CLIPS, CLIPS, Op::IncrementClamp),
        Kind::ClipOut => (CLIPS, CLIPS, Op::DecrementClamp),
    };
    let face = wgpu::StencilFaceState {
        compare,
        fail_op: Op::Keep,
        depth_fail_op: Op::Keep,
        pass_op,
    };
    wgpu::DepthStencilState {
        format: FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: face,
            back: face,
            read_mask,
            write_mask,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

/// A stencil attachment kept at the target's size.
#[derive(Default)]
pub struct Attachment {
    texture: Option<(wgpu::Texture, wgpu::TextureView, [u32; 2])>,
}

impl Attachment {
    /// The view, made again when `size` changed.
    pub fn view(&mut self, device: &wgpu::Device, size: [u32; 2]) -> &wgpu::TextureView {
        let size = [size[0].max(1), size[1].max(1)];
        if self.texture.as_ref().is_none_or(|t| t.2 != size) {
            // The old one is dropped, not destroyed: an encoder not yet
            // submitted may still draw with it.
            let t = crate::gpu::texture_2d(
                device,
                "ddi-stencil",
                size,
                FORMAT,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
                &[],
            );
            let v = t.create_view(&wgpu::TextureViewDescriptor::default());
            self.texture = Some((t, v, size));
        }
        &self.texture.as_ref().expect("made above").1
    }

    /// The view last made.
    pub fn current(&self) -> Option<&wgpu::TextureView> {
        self.texture.as_ref().map(|t| &t.1)
    }

    /// The pass's attachment: cleared with `clear`, else kept.
    pub fn pass(
        view: &wgpu::TextureView,
        clear: bool,
    ) -> wgpu::RenderPassDepthStencilAttachment<'_> {
        wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: if clear {
                    wgpu::LoadOp::Clear(1.0)
                } else {
                    wgpu::LoadOp::Load
                },
                // Kept as the stencil is: GLES would invalidate the depth
                // and stencil texture's whole depth attachment.
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: Some(wgpu::Operations {
                load: if clear {
                    wgpu::LoadOp::Clear(0)
                } else {
                    wgpu::LoadOp::Load
                },
                store: wgpu::StoreOp::Store,
            }),
        }
    }

    /// Forgets the texture (a lost device).
    pub fn reset(&mut self) {
        self.texture = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_and_clips_share_the_byte() {
        assert_eq!(Mask::None.stencil(0), (Kind::Level, 0));
        assert_eq!(Mask::Write.stencil(2), (Kind::MaskWrite, 0x82));
        // An erase writes the mask's bit as 0, within the level; a clear
        // whatever the level.
        assert_eq!(Mask::Erase.stencil(2), (Kind::MaskWrite, 2));
        assert_eq!(Mask::Clear.stencil(2), (Kind::MaskClear, 0));
        let c = state(Kind::MaskClear);
        assert_eq!(c.stencil.front.compare, wgpu::CompareFunction::Always);
        assert_eq!(c.stencil.write_mask, 0x80);
        assert_eq!(Mask::Outside.stencil(1), (Kind::Masked, 1));
        assert_eq!(Mask::Inside.stencil(1), (Kind::Masked, 0x81));
        assert_eq!(Mask::None.stencil(500), (Kind::Level, MAX_LEVEL));
        let s = state(Kind::MaskWrite);
        assert_eq!((s.stencil.read_mask, s.stencil.write_mask), (0x7f, 0x80));
        assert_eq!(state(Kind::Masked).stencil.read_mask, 0xff);
        assert_eq!(state(Kind::Level).stencil.write_mask, 0);
    }
}
