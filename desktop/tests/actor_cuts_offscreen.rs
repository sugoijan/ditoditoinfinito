//! Crops and faded edges (`cropleft` … `fadebottom`, StepMania's
//! `Sprite::DrawPrimitives`) drawn by the 2D pass on this machine's GPU,
//! each direction and a tween: a white 200-unit square on black, read
//! back. Skipped where there is no GPU adapter (CI).

use ddi_actor::{Actor, Clocks, Drawn, Kind, Problems, Sprite, TRANSFORM_IDENTITY};
use ddi_desktop::gpu;
use ddi_render::{DrawList, Painter};

const SIZE: [u32; 2] = [256, 256];

struct Target {
    device: wgpu::Device,
    queue: wgpu::Queue,
    painter: Painter,
}

impl Target {
    fn new() -> Option<Target> {
        let instance = gpu::instance();
        let adapter = gpu::adapter(&instance, None)?;
        let (device, queue) = gpu::device(&adapter).ok()?;
        let painter = Painter::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        Some(Target {
            device,
            queue,
            painter,
        })
    }

    /// The square after `commands` (and `seconds` of its tweens): the
    /// grey level of each pixel.
    fn draw(&mut self, commands: &str, seconds: f32) -> Vec<u8> {
        let mut a = Actor::new(Kind::Sprite(Sprite::quad([200.0, 200.0])));
        let mut p = Problems::default();
        for c in ddi_actor::commands::parse(&format!("xy,128,128;{commands}")).unwrap() {
            a.call(&c, &mut p, 0).unwrap();
        }
        a.update(seconds, &Clocks::default(), &mut p);
        let mut drawn: Vec<Drawn> = Vec::new();
        a.draw(
            TRANSFORM_IDENTITY,
            [1.0; 4],
            [1.0, 1.0, 1.0, 0.0],
            &mut drawn,
        );
        let mut list = DrawList::new();
        ddi_render::actors2d::append(&drawn, &mut list, &mut |_| None);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: SIZE[0],
                height: SIZE[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.painter.draw(
            &self.device,
            &self.queue,
            &mut encoder,
            &view,
            SIZE,
            1.0,
            Some(wgpu::Color::BLACK),
            &list,
            &mut |_, _| {},
        );
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(SIZE[0] * 4 * SIZE[1]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE[0] * 4),
                    rows_per_image: Some(SIZE[1]),
                },
            },
            texture.size(),
        );
        self.queue.submit(Some(encoder.finish()));
        self.painter.submitted();
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let rgba = buffer
            .slice(..)
            .get_mapped_range()
            .expect("mapped")
            .to_vec();
        rgba.chunks(4).map(|p| p[0]).collect()
    }
}

fn at(px: &[u8], x: u32, y: u32) -> u8 {
    px[(y * SIZE[0] + x) as usize]
}

#[test]
fn crops_and_fades_draw_as_stepmania_draws_them() {
    let Some(mut t) = Target::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    // The square spans 28..228 each way.
    let plain = t.draw("", 0.0);
    assert!(at(&plain, 30, 128) > 250 && at(&plain, 226, 128) > 250);
    assert!(at(&plain, 20, 128) < 5);

    // Each crop cuts a quarter (50 units) off its side.
    let crops = [
        ("cropleft,0.25", (60, 128), (90, 128)),
        ("cropright,0.25", (196, 128), (166, 128)),
        ("croptop,0.25", (128, 60), (128, 90)),
        ("cropbottom,0.25", (128, 196), (128, 166)),
    ];
    for (c, gone, kept) in crops {
        let px = t.draw(c, 0.0);
        assert!(
            at(&px, gone.0, gone.1) < 5,
            "{c}: {}",
            at(&px, gone.0, gone.1)
        );
        assert!(
            at(&px, kept.0, kept.1) > 250,
            "{c}: {}",
            at(&px, kept.0, kept.1)
        );
    }

    // Each fade ramps from nothing at its edge to full a half in.
    let fades = [
        ("fadeleft,0.5", [(30, 128), (78, 128), (126, 128)]),
        ("faderight,0.5", [(226, 128), (178, 128), (130, 128)]),
        ("fadetop,0.5", [(128, 30), (128, 78), (128, 126)]),
        ("fadebottom,0.5", [(128, 226), (128, 178), (128, 130)]),
    ];
    for (f, [edge, mid, inner]) in fades {
        let px = t.draw(f, 0.0);
        let (e, m, i) = (
            at(&px, edge.0, edge.1),
            at(&px, mid.0, mid.1),
            at(&px, inner.0, inner.1),
        );
        assert!(e < 15, "{f}: edge {e}");
        assert!((100..160).contains(&m), "{f}: middle {m}");
        assert!(i > 240, "{f}: inside {i}");
        // The other side is untouched.
        let opposite = (256 - edge.0, 256 - edge.1);
        assert!(at(&px, opposite.0, opposite.1) > 250, "{f}: opposite");
    }

    // A tween: half a second into a second's `cropleft,0.5`, a quarter is
    // cut; at its end, half.
    let half = t.draw("linear,1;cropleft,0.5", 0.5);
    assert!(at(&half, 60, 128) < 5 && at(&half, 90, 128) > 250);
    let end = t.draw("linear,1;cropleft,0.5", 1.0);
    assert!(at(&end, 120, 128) < 5 && at(&end, 135, 128) > 250);
}
