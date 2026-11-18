use super::*;
use iced::widget::shader::Program as _;

fn device(bc: bool) -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let adapter = iced::futures::executor::block_on(
        instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    assert!(
        !bc || adapter
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
        "owned adapter must exercise BC path"
    );
    eprintln!("adapter={:?} bc={bc}", adapter.get_info());
    iced::futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: if bc {
            wgpu::Features::TEXTURE_COMPRESSION_BC
        } else {
            wgpu::Features::empty()
        },
        required_limits: wgpu::Limits {
            max_texture_array_layers: 32,
            ..wgpu::Limits::downlevel_defaults()
        },
        ..Default::default()
    }))
    .unwrap()
}

fn read_mip(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    layer: u32,
    mip: u32,
    extent: wgpu::Extent3d,
    origin: Option<wgpu::Origin3d>,
) -> Vec<u8> {
    let (row, rows, _) = texture_layout(texture.format(), extent);
    let aligned = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("owned atlas mip readback"),
        size: (aligned * rows) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            mip_level: mip,
            origin: origin.unwrap_or(wgpu::Origin3d {
                z: layer,
                ..Default::default()
            }),
            ..poster_layer(texture, 0)
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(aligned),
                rows_per_image: Some(rows),
            },
        },
        extent,
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    mapped
        .chunks(aligned as usize)
        .flat_map(|r| r[..row as usize].to_vec())
        .collect()
}

fn art() -> Arc<PosterBytes> {
    let mut pixels = vec![255; POSTER_BYTES];
    for y in 0..512usize {
        for x in 0..512usize {
            let i = (y * 512 + x) * 4;
            pixels[i] = x as u8;
            pixels[i + 1] = y as u8;
            pixels[i + 2] = if (x + y) % 2 == 0 { 0 } else { 255 };
        }
    }
    Arc::new(pixels.into())
}

#[test]
fn p51_mip_encoding_preserves_canonical_and_averages_linear_light() {
    let rgba = art();
    let original = rgba.to_vec();
    let packed = PosterEncoding::NearRgbaMips
        .encode(rgba.clone(), 1.0)
        .unwrap();
    assert_eq!(&rgba[..], &original);
    assert_eq!(&packed[..POSTER_BYTES], &original);
    assert_eq!(packed.len(), 1_397_760);
    assert_eq!(
        near_bytes(wgpu::TextureFormat::Rgba8UnormSrgb),
        packed.len()
    );
    assert_eq!(packed[POSTER_BYTES + 2], 188); // black/white average in linear light
    let bc = PosterEncoding::NearBc7.encode(rgba, 1.0).unwrap();
    assert_eq!(bc.len(), 349_440);
    assert_eq!(bc.len(), near_bytes(wgpu::TextureFormat::Bc7RgbaUnormSrgb));
    assert_eq!(near_extent(NEAR_MIP_COUNT - 1).width, 16);
}

fn mip_growth(bc: bool) {
    let (device, queue) = device(bc);
    let mut pipeline = GalleryPipeline::new(&device, &queue, wgpu::TextureFormat::Bgra8UnormSrgb);
    let poster = video_placeholder_poster(Path::new("p51-owned.mp4"), 255);
    let encoding = PosterEncoding::for_texture(PosterTier::Near, bc);
    let packed = encoding.encode(art(), 1.0).unwrap();
    *poster.pixels.lock().unwrap() = PosterPixels::Ready(encoding, packed.clone());
    let frame = primitive(vec![poster.clone()], 1);
    prepare(&frame, &mut pipeline, &device, &queue);
    assert_eq!(pipeline.near_slots.len(), 1);
    assert_eq!(pipeline.far_residents.len(), 1);
    pipeline.grow_atlas(&device, &queue, 32);
    assert!(pipeline.near_slots[0].0.same_resource(&poster));
    let mut offset = 0;
    for mip in 0..NEAR_MIP_COUNT {
        let size = texture_layout(pipeline.posters.format(), near_extent(mip)).2;
        assert_eq!(
            read_mip(
                &device,
                &queue,
                &pipeline.posters,
                0,
                mip,
                near_extent(mip),
                None
            ),
            packed[offset..offset + size],
            "mip={mip} bc={bc}"
        );
        offset += size;
    }
    eprintln!(
        "six mips copied/read back; bc={bc} slot_bytes={offset} allocation={}",
        offset * 32
    );
}

#[test]
#[ignore = "Owned GPU readback; explicit invocation only"]
fn p51_rgba_each_mip_survives_growth() {
    mip_growth(false);
}
#[test]
#[ignore = "Owned GPU readback with required BC support; explicit invocation only"]
fn p51_bc7_each_mip_survives_growth() {
    mip_growth(true);
}

fn primitive(posters: Vec<Poster>, near: usize) -> GalleryPrimitive {
    let cards = posters
        .iter()
        .enumerate()
        .map(|(i, p)| SliceCard {
            index: i,
            center: [320.0, 240.0],
            size: [320.0, 180.0],
            corner_radius: 0.0,
            skew_distance: 0.0,
            selected: i < near,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            opacity: 1.0,
            layer: Some(p.layer),
            shape: 0,
            poster_aspect_ratio: p.aspect_ratio,
            triangle_direction: 0,
        })
        .collect();
    GalleryPrimitive {
        prewarm: None,
        poster_feedback: None,
        snapshot: Arc::new(GallerySnapshot {
            cards,
            posters: Arc::new(posters),
            ordinary: None,
            colors: GalleryColors::default(),
            depth: None,
            wall_clip: None,
            scene: DonorSceneState {
                stage: Default::default(),
                camera: 0.0,
                camera_target: 0.0,
                camera_velocity: 0.0,
                detail_progress: 0.0,
                external_detail: false,
                flip_options: Default::default(),
                detail_source: None,
                detail_card: None,
                viewport: [640.0, 480.0],
                time: 0.0,
                visibility: 0.0,
            },
        }),
    }
}
fn prepare(
    frame: &GalleryPrimitive,
    pipeline: &mut GalleryPipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    frame.prepare(
        pipeline,
        device,
        queue,
        &Rectangle::with_size(Size::new(640.0, 480.0)),
        &Viewport::with_physical_size(iced::Size::new(640, 480), 1.0),
    );
}
#[test]
fn p51_packed_staging_reclaims_retained_snapshots_under_the_existing_budget() {
    let decoder = PosterDecoder::with_producers(24, 2);
    assert_eq!(decoder.upload_staging_bytes, 6 * POSTER_BYTES);
    let posters: Vec<_> = (0..32)
        .map(|i| video_placeholder_poster(Path::new(&format!("retained-{i}.mp4")), i))
        .collect();
    for poster in &posters {
        decoder.begin_work();
        decoder.restore(
            poster
                .request_restore(PosterEncoding::NearRgbaMips)
                .unwrap(),
            &mut ReadBatch::default(),
            || true,
        );
        let staged: usize = posters
            .iter()
            .map(|p| match &*p.pixels.lock().unwrap() {
                PosterPixels::Ready(PosterEncoding::NearRgbaMips, b) => b.len(),
                _ => 0,
            })
            .sum();
        assert!(staged <= decoder.upload_staging_bytes);
    }
    assert!(
        posters
            .iter()
            .filter(|p| p.has_encoded_pixels(PosterEncoding::NearRgbaMips))
            .count()
            < 32
    );
    assert!(posters[0].needs_restore(PosterEncoding::NearRgbaMips));
}
#[test]
fn p52_hysteresis_resets_with_renewed_demand_and_avoids_oscillation() {
    let now = std::time::Instant::now();
    let mut idle = None;
    assert!(!shrink_due(32, 8, &mut idle, now));
    assert!(!shrink_due(
        32,
        8,
        &mut idle,
        now + std::time::Duration::from_millis(1999)
    ));
    assert!(!shrink_due(32, 24, &mut idle, now + ATLAS_SHRINK_DELAY));
    assert!(idle.is_none());
    assert!(!shrink_due(32, 8, &mut idle, now + ATLAS_SHRINK_DELAY));
    assert!(shrink_due(32, 8, &mut idle, now + ATLAS_SHRINK_DELAY * 2));
    for step in 0..20 {
        assert!(!shrink_due(
            8,
            8,
            &mut idle,
            now + ATLAS_SHRINK_DELAY * step
        ));
    }
    assert_eq!(far_height(256), 2048);
    assert_eq!(far_height(1), 88);
}
fn shrink_and_regrow(bc: bool) {
    let (device, queue) = device(bc);
    let mut pipeline = GalleryPipeline::new(&device, &queue, wgpu::TextureFormat::Bgra8UnormSrgb);
    let poster = video_placeholder_poster(Path::new("survivor-pin.mp4"), 255);
    let encoding = PosterEncoding::for_texture(PosterTier::Near, bc);
    let packed = encoding.encode(art(), 1.0).unwrap();
    *poster.pixels.lock().unwrap() = PosterPixels::Ready(encoding, packed.clone());
    // The GPU capability selects far encoding; this source-bound fixture follows it.
    *poster.far_pixels.lock().unwrap() = PosterPixels::Ready(
        PosterEncoding::for_texture(PosterTier::Far, bc),
        PosterEncoding::for_texture(PosterTier::Far, bc)
            .encode(art(), 1.0)
            .unwrap(),
    );
    let frame = primitive(vec![poster.clone()], 1);
    prepare(&frame, &mut pipeline, &device, &queue);
    pipeline.grow_atlas(&device, &queue, 32);
    pipeline.resize_far(&device, &queue, 2048);
    let far = read_mip(
        &device,
        &queue,
        &pipeline.far,
        0,
        0,
        FAR_EXTENT,
        Some(far_tile_origin(0)),
    );
    // Force sparse high physical slots, as retained transition pins can leave.
    let mut encoder = device.create_command_encoder(&Default::default());
    for mip in 0..NEAR_MIP_COUNT {
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                mip_level: mip,
                ..poster_layer(&pipeline.posters, 0)
            },
            wgpu::TexelCopyTextureInfo {
                mip_level: mip,
                ..poster_layer(&pipeline.posters, 31)
            },
            near_extent(mip),
        );
    }
    let (row, rows, _) = texture_layout(pipeline.far.format(), FAR_EXTENT);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            origin: far_tile_origin(255),
            ..poster_layer(&pipeline.far, 0)
        },
        &far,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(row),
            rows_per_image: Some(rows),
        },
        FAR_EXTENT,
    );
    queue.submit([encoder.finish()]);
    pipeline.near_slots[0].1 = 31;
    pipeline.far_residents[0].1 = 255;
    let before = near_bytes(pipeline.posters.format()) * 32
        + texture_layout(pipeline.far.format(), pipeline.far.size()).2;
    let now = std::time::Instant::now();
    pipeline.retune_atlases(&device, &queue, 1, 1, now, None);
    assert_eq!(pipeline.posters.depth_or_array_layers(), 32);
    assert_eq!(pipeline.far.height(), 2048);
    pipeline.retune_atlases(&device, &queue, 1, 1, now + ATLAS_SHRINK_DELAY, None);
    assert_eq!(pipeline.posters.depth_or_array_layers(), 1);
    assert_eq!(pipeline.far.height(), 88);
    assert_eq!(pipeline.near_slots[0].1, 0);
    assert_eq!(pipeline.far_residents[0].1, 0);
    assert!(pipeline.near_slots[0].0.same_resource(&poster));
    assert!(pipeline.far_residents[0].0.same_resource(&poster));
    let after = near_bytes(pipeline.posters.format())
        + texture_layout(pipeline.far.format(), pipeline.far.size()).2;
    assert!(after < before / 8);
    for growing in [false, true] {
        if growing {
            pipeline.grow_atlas(&device, &queue, 32);
            pipeline.resize_far(&device, &queue, 2048);
        }
        let mut offset = 0;
        for mip in 0..NEAR_MIP_COUNT {
            let size = texture_layout(pipeline.posters.format(), near_extent(mip)).2;
            assert_eq!(
                read_mip(
                    &device,
                    &queue,
                    &pipeline.posters,
                    0,
                    mip,
                    near_extent(mip),
                    None
                ),
                packed[offset..offset + size]
            );
            offset += size;
        }
        assert_eq!(
            read_mip(
                &device,
                &queue,
                &pipeline.far,
                0,
                0,
                FAR_EXTENT,
                Some(far_tile_origin(0))
            ),
            far
        );
    }
    eprintln!("bc={bc} requested_texture_bytes before={before} after={after} released={}; sparse survivor/pin compacted and regrown",before-after);
}
#[test]
#[ignore = "Owned GPU shrink/readback; explicit invocation only"]
fn p52_rgba_shrink_preserves_sparse_survivors_pins_and_regrows() {
    shrink_and_regrow(false);
}
#[test]
#[ignore = "Owned GPU shrink/readback requiring BC; explicit invocation only"]
fn p52_bc_shrink_preserves_sparse_survivors_pins_and_regrows() {
    shrink_and_regrow(true);
}

static FEEDBACK: Mutex<Vec<PosterFeedback>> = Mutex::new(Vec::new());
fn record(feedback: PosterFeedback) {
    FEEDBACK.lock().unwrap().push(feedback);
}
#[test]
#[ignore = "Owned GPU pre-entry settlement; explicit invocation only"]
fn p53_held_entry_requires_terminal_decode_and_gpu_completion_before_receipt() {
    let (device, queue) = device(false);
    let mut pipeline = GalleryPipeline::new(&device, &queue, wgpu::TextureFormat::Bgra8UnormSrgb);
    let poster = video_placeholder_poster(Path::new("held-entry.mp4"), 255);
    let mut frame = primitive(vec![poster.clone()], 1);
    frame.prewarm = Some((test_key(71), false));
    frame.poster_feedback = Some(record);
    FEEDBACK.lock().unwrap().clear();
    prepare(&frame, &mut pipeline, &device, &queue);
    queue.submit([]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(!FEEDBACK
        .lock()
        .unwrap()
        .iter()
        .any(|f| matches!(f, PosterFeedback::Prewarmed { .. })));
    frame.prewarm = Some((test_key(71), true));
    prepare(&frame, &mut pipeline, &device, &queue);
    assert!(pipeline.near_slots.is_empty()); // decode delivered canonical, packed near is still held
    assert!(!FEEDBACK
        .lock()
        .unwrap()
        .iter()
        .any(|f| matches!(f, PosterFeedback::Prewarmed { .. })));
    poster
        .request_restore(PosterEncoding::NearRgbaMips)
        .unwrap()
        .restore(|| true);
    prepare(&frame, &mut pipeline, &device, &queue);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let receipts: Vec<_> = FEEDBACK
        .lock()
        .unwrap()
        .iter()
        .filter_map(|f| match f {
            PosterFeedback::Prewarmed { key, posters } => Some((key.generation, posters.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].0, 71);
    assert!(Arc::ptr_eq(&receipts[0].1, &frame.snapshot.posters));
    assert_eq!(pipeline.near_slots.len(), 1);
    assert_eq!(pipeline.far_residents.len(), 1);
    assert_eq!(frame.snapshot.scene.visibility, 0.0);
    let encoding = PosterEncoding::NearRgbaMips;
    let expected = encoding
        .encode(
            Arc::new({
                let mut b = vec![0; POSTER_BYTES];
                fill_video_placeholder(&poster.source, &mut b);
                b.into()
            }),
            poster.aspect_ratio,
        )
        .unwrap();
    let mut offset = 0;
    for mip in 0..NEAR_MIP_COUNT {
        let size = texture_layout(pipeline.posters.format(), near_extent(mip)).2;
        assert_eq!(
            read_mip(
                &device,
                &queue,
                &pipeline.posters,
                0,
                mip,
                near_extent(mip),
                None
            ),
            expected[offset..offset + size]
        );
        offset += size;
    }
    prepare(&frame, &mut pipeline, &device, &queue);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(
        FEEDBACK
            .lock()
            .unwrap()
            .iter()
            .filter(|f| matches!(f, PosterFeedback::Prewarmed { .. }))
            .count(),
        1
    );
}

fn test_key(generation: u64) -> AtlasPrewarmKey {
    AtlasPrewarmKey {
        generation,
        motion_generation: 0,
        selected: Some(0),
        viewport: [640.0f32.to_bits(), 480.0f32.to_bits()],
    }
}

#[test]
fn p51_packed_batch_keeps_first_priority_when_staging_is_full() {
    let decoder = PosterDecoder::with_producers(4, 1);
    let posters: Vec<_> = (0..8)
        .map(|i| video_placeholder_poster(Path::new(&format!("priority-{i}.mp4")), i))
        .collect();
    decoder.begin_work();
    let requests = posters
        .iter()
        .map(|p| p.request_restore(PosterEncoding::NearRgbaMips).unwrap())
        .collect();
    decoder.restore_batch(requests, &mut ReadBatch::default(), || true);
    assert!(posters[0].has_encoded_pixels(PosterEncoding::NearRgbaMips));
    assert!(posters
        .iter()
        .skip(1)
        .all(|p| p.needs_restore(PosterEncoding::NearRgbaMips)));
    assert_eq!(decoder.upload_staging_bytes, 2 * POSTER_BYTES);
}
#[test]
fn p53_hold_preserves_modifier_release_bookkeeping() {
    let frame = primitive(Vec::new(), 0);
    let program = GalleryProgram {
        snapshot: frame.snapshot,
        prewarm: Some((test_key(1), false)),
        poster_feedback: None,
    };
    let mut state = iced::keyboard::Modifiers::CTRL;
    let action = program.update(
        &mut state,
        &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::empty(),
        )),
        Rectangle::default(),
        mouse::Cursor::Unavailable,
    );
    assert!(action.is_none());
    assert!(state.is_empty());
}

#[test]
#[ignore = "Owned ordinary-to-Depth GPU lifecycle; explicit invocation only"]
fn p52_depth_entry_retires_far_tiles_before_zero_demand_shrink() {
    let (device, queue) = device(false);
    let mut pipeline = GalleryPipeline::new(&device, &queue, wgpu::TextureFormat::Bgra8UnormSrgb);
    let posters: Vec<_> = (0..32)
        .map(|i| video_placeholder_poster(Path::new(&format!("depth-retain-{i}.mp4")), i))
        .collect();
    let mut frame = primitive(posters, 0);
    prepare(&frame, &mut pipeline, &device, &queue);
    assert_eq!(pipeline.far_residents.len(), 32);
    assert!(pipeline.far.height() > 88);
    let empty = depth_frame(
        &[],
        1,
        None,
        None,
        0.0,
        [640.0, 480.0],
        [320.0, 640.0],
        crate::donor_layout::DepthParams::default(),
        1.0,
    );
    Arc::make_mut(&mut frame.snapshot).depth = Some(DepthSnapshot::from_frame(empty));
    prepare(&frame, &mut pipeline, &device, &queue);
    assert!(pipeline.far_residents.is_empty());
    pipeline.retune_atlases(
        &device,
        &queue,
        32,
        0,
        std::time::Instant::now() + ATLAS_SHRINK_DELAY,
        None,
    );
    assert_eq!(pipeline.far.height(), 88);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

fn render_art(bc: bool, near: bool) -> Vec<u8> {
    let (device, queue) = device(bc);
    let mut pipeline = GalleryPipeline::new(&device, &queue, wgpu::TextureFormat::Bgra8UnormSrgb);
    let mut poster = video_placeholder_poster(Path::new("gradient-art.mp4"), 0);
    poster.aspect_ratio = 1.0;
    let rgba = art();
    let near_encoding = PosterEncoding::for_texture(PosterTier::Near, bc);
    let far_encoding = PosterEncoding::for_texture(PosterTier::Far, bc);
    *poster.pixels.lock().unwrap() = PosterPixels::Ready(
        near_encoding,
        near_encoding.encode(rgba.clone(), 1.0).unwrap(),
    );
    *poster.far_pixels.lock().unwrap() =
        PosterPixels::Ready(far_encoding, far_encoding.encode(rgba, 1.0).unwrap());
    let mut frame = primitive(vec![poster], usize::from(near));
    Arc::make_mut(&mut frame.snapshot).scene.visibility = 1.0;
    prepare(&frame, &mut pipeline, &device, &queue);
    let extent = wgpu::Extent3d {
        width: 640,
        height: 480,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("poster art quality readback"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    frame.render(
        &pipeline,
        &mut encoder,
        &view,
        &Rectangle {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        },
    );
    queue.submit([encoder.finish()]);
    let pixels = read_mip(&device, &queue, &target, 0, 0, extent, None);
    let mut samples = Vec::new();
    for y in 170..310 {
        for x in 180..460 {
            let i = (y * 640 + x) * 4;
            assert_eq!(pixels[i + 3], 255, "poster art must actually render");
            samples.extend_from_slice(&pixels[i..i + 3]);
        }
    }
    assert!(
        samples.iter().copied().max().unwrap() - samples.iter().copied().min().unwrap() > 100,
        "gradient oracle must contain art"
    );
    samples
}
#[test]
#[ignore = "Owned production shader BC1/BC7 art comparison; explicit invocation only"]
fn p51_bc_gradient_art_has_bounded_error_against_rgba_render() {
    for near in [false, true] {
        let rgba = render_art(false, near);
        let bc = render_art(true, near);
        let errors: Vec<_> = rgba
            .iter()
            .zip(&bc)
            .map(|(a, b)| a.abs_diff(*b) as usize)
            .collect();
        let mean = errors.iter().sum::<usize>() as f64 / errors.len() as f64;
        let peak = *errors.iter().max().unwrap();
        assert!(mean <= 8.0, "near={near} mean error={mean}");
        assert!(peak <= 48, "near={near} peak error={peak}");
        eprintln!("production shader art comparison near={near} samples={} mean_absolute_srgb_error={mean:.4} peak={peak}",errors.len());
    }
}
