//! The 2D pass's images on this machine's GPU: images decoded between
//! frames and evicted by the next draw (over the budget, not drawn) are
//! neither mipmapped nor destroyed while that draw's commands use them.
//! Skipped where there is no GPU adapter (CI).

use std::sync::{Arc, Mutex};

use ddi_desktop::gpu;
use ddi_render::texture_cache::TextureKey;
use ddi_render::{DrawList, Painter};

#[test]
fn images_evicted_in_the_draw_that_mipmaps_them_are_safe() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = errors.clone();
    device.on_uncaptured_error(Arc::new(move |e| seen.lock().unwrap().push(format!("{e}"))));
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut painter = Painter::new(&device, &queue, format);
    // A budget of about one 64×64 image (with its mipmaps).
    painter.set_budgets(64 * 64 * 4 * 2, 1 << 20);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let list = DrawList::default();
    let draw = |painter: &mut Painter| {
        let mut encoder = device.create_command_encoder(&Default::default());
        painter.draw(
            &device,
            &queue,
            &mut encoder,
            &view,
            [64, 64],
            1.0,
            Some(wgpu::Color::BLACK),
            &list,
            &mut |_, _| {},
        );
        queue.submit(Some(encoder.finish()));
        painter.submitted();
    };
    draw(&mut painter);
    // Three images come in between frames (asked for, no longer drawn).
    let rgba = vec![255u8; 64 * 64 * 4];
    for i in 0..3 {
        let id = painter.texture(TextureKey::drawn(format!("b{i}.png"), 64.0));
        painter.insert_rgba(&device, &queue, id, [64, 64], &rgba);
    }
    draw(&mut painter);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let stats = painter.stats().textures;
    assert!(stats.loaded <= 1, "{stats:?}");
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
