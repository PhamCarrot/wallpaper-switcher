use std::ffi::c_void;

use windows::core::BOOL;
use windows::Win32::Foundation::{DXGI_STATUS_OCCLUDED, HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDeviceAndSwapChain, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext,
    ID3D11PixelShader, ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView,
    ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_SHADER_RESOURCE,
    D3D11_BUFFER_DESC, D3D11_COMPARISON_NEVER, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_SAMPLER_DESC, D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA,
    D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_USAGE_IMMUTABLE,
    D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGISwapChain, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};

use crate::transition::{FrozenFrame, TransitionEffect, TRANSITION_CATALOG};

const VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/transition.vs.cso"));
const PIXEL_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/transition.ps.cso"));

#[repr(C)]
#[derive(Clone, Copy)]
struct TransitionConstants {
    progress: f32,
    target_aspect: f32,
    old_aspect: f32,
    new_aspect: f32,
    style: f32,
    padding: [f32; 3],
}

pub struct D3d11TransitionRenderer {
    width: u32,
    height: u32,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain,
    target: ID3D11RenderTargetView,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    constants: ID3D11Buffer,
    sampler: ID3D11SamplerState,
    _outgoing_texture: ID3D11Texture2D,
    outgoing_view: ID3D11ShaderResourceView,
    incoming: Option<InputTexture>,
}

struct InputTexture {
    identity: usize,
    texture: ID3D11Texture2D,
    view: ID3D11ShaderResourceView,
}

impl D3d11TransitionRenderer {
    pub fn new(hwnd: *mut c_void, outgoing: &FrozenFrame) -> Result<Self, String> {
        validate_frame(outgoing)?;

        let description = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: DXGI_MODE_DESC {
                Width: outgoing.width,
                Height: outgoing.height,
                RefreshRate: DXGI_RATIONAL {
                    Numerator: 0,
                    Denominator: 1,
                },
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ..Default::default()
            },
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            OutputWindow: HWND(hwnd),
            Windowed: BOOL(1),
            SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
            Flags: 0,
        };

        let mut swap_chain = None;
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDeviceAndSwapChain(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&description),
                Some(&mut swap_chain),
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map_err(|error| format!("Could not create the D3D11 transition device: {error}"))?;

        let swap_chain = swap_chain.ok_or("D3D11 did not return a transition swap chain")?;
        let device = device.ok_or("D3D11 did not return a transition device")?;
        let context = context.ok_or("D3D11 did not return a transition context")?;

        let back_buffer: ID3D11Texture2D = unsafe { swap_chain.GetBuffer(0) }
            .map_err(|error| format!("Could not access the transition back buffer: {error}"))?;
        let mut target = None;
        unsafe { device.CreateRenderTargetView(&back_buffer, None, Some(&mut target)) }
            .map_err(|error| format!("Could not create the transition render target: {error}"))?;
        let target = target.ok_or("D3D11 did not return a transition render target")?;

        let mut vertex_shader = None;
        unsafe { device.CreateVertexShader(VERTEX_SHADER, None, Some(&mut vertex_shader)) }
            .map_err(|error| format!("Could not create the transition vertex shader: {error}"))?;
        let vertex_shader = vertex_shader.ok_or("D3D11 did not return a vertex shader")?;

        let mut pixel_shader = None;
        unsafe { device.CreatePixelShader(PIXEL_SHADER, None, Some(&mut pixel_shader)) }
            .map_err(|error| format!("Could not create the transition pixel shader: {error}"))?;
        let pixel_shader = pixel_shader.ok_or("D3D11 did not return a pixel shader")?;

        let constant_description = D3D11_BUFFER_DESC {
            ByteWidth: std::mem::size_of::<TransitionConstants>() as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let mut constants = None;
        unsafe { device.CreateBuffer(&constant_description, None, Some(&mut constants)) }
            .map_err(|error| format!("Could not create transition constants: {error}"))?;
        let constants = constants.ok_or("D3D11 did not return a transition constant buffer")?;

        let sampler_description = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MinLOD: 0.0,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let mut sampler = None;
        unsafe { device.CreateSamplerState(&sampler_description, Some(&mut sampler)) }
            .map_err(|error| format!("Could not create the transition sampler: {error}"))?;
        let sampler = sampler.ok_or("D3D11 did not return a transition sampler")?;

        let (outgoing_texture, outgoing_view) = create_input_texture(&device, outgoing)?;

        Ok(Self {
            width: outgoing.width,
            height: outgoing.height,
            device,
            context,
            swap_chain,
            target,
            vertex_shader,
            pixel_shader,
            constants,
            sampler,
            _outgoing_texture: outgoing_texture,
            outgoing_view,
            incoming: None,
        })
    }

    pub fn render(
        &mut self,
        incoming: &FrozenFrame,
        effect: TransitionEffect,
        progress: f32,
    ) -> Result<(), String> {
        validate_frame(incoming)?;
        if incoming.width != self.width || incoming.height != self.height {
            return Err("The incoming frame does not match the D3D11 transition surface".into());
        }
        let identity = incoming.rgba.as_ptr() as usize;
        if self.incoming.as_ref().map(|texture| texture.identity) != Some(identity) {
            let (texture, view) = create_input_texture(&self.device, incoming)?;
            self.incoming = Some(InputTexture {
                identity,
                texture,
                view,
            });
        }
        let incoming = self
            .incoming
            .as_ref()
            .expect("incoming texture initialized");
        let constants = TransitionConstants {
            progress: progress.clamp(0.0, 1.0),
            target_aspect: self.width as f32 / self.height.max(1) as f32,
            old_aspect: self.width as f32 / self.height.max(1) as f32,
            new_aspect: incoming.texture_description_aspect(),
            style: shader_style(effect)? as f32,
            padding: [0.0; 3],
        };
        let views = [
            Some(self.outgoing_view.clone()),
            Some(incoming.view.clone()),
        ];
        let targets = [Some(self.target.clone())];
        let constant_buffers = [Some(self.constants.clone())];
        let samplers = [Some(self.sampler.clone())];
        let viewport = D3D11_VIEWPORT {
            Width: self.width as f32,
            Height: self.height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
            ..Default::default()
        };

        unsafe {
            self.context.UpdateSubresource(
                &self.constants,
                0,
                None,
                (&constants as *const TransitionConstants).cast(),
                0,
                0,
            );
            self.context.OMSetRenderTargets(Some(&targets), None);
            self.context.RSSetViewports(Some(&[viewport]));
            self.context
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.VSSetShader(&self.vertex_shader, None);
            self.context.PSSetShader(&self.pixel_shader, None);
            self.context
                .VSSetConstantBuffers(0, Some(&constant_buffers));
            self.context
                .PSSetConstantBuffers(0, Some(&constant_buffers));
            self.context.PSSetShaderResources(0, Some(&views));
            self.context.PSSetSamplers(0, Some(&samplers));
            self.context.Draw(3, 0);
        }
        Ok(())
    }

    pub fn present(&self) -> Result<(), String> {
        let result = unsafe { self.swap_chain.Present(0, DXGI_PRESENT(0)) };
        if result == DXGI_STATUS_OCCLUDED {
            return Err("DXGI reported the desktop transition surface was occluded".into());
        }
        result
            .ok()
            .map_err(|error| format!("Could not present the D3D11 transition frame: {error}"))
    }
}

impl InputTexture {
    fn texture_description_aspect(&self) -> f32 {
        let mut description = D3D11_TEXTURE2D_DESC::default();
        unsafe { self.texture.GetDesc(&mut description) };
        description.Width as f32 / description.Height.max(1) as f32
    }
}

fn create_input_texture(
    device: &ID3D11Device,
    frame: &FrozenFrame,
) -> Result<(ID3D11Texture2D, ID3D11ShaderResourceView), String> {
    validate_frame(frame)?;
    let description = D3D11_TEXTURE2D_DESC {
        Width: frame.width,
        Height: frame.height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_IMMUTABLE,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };
    let initial = D3D11_SUBRESOURCE_DATA {
        pSysMem: frame.rgba.as_ptr().cast(),
        SysMemPitch: frame.width.saturating_mul(4),
        SysMemSlicePitch: frame.rgba.len().min(u32::MAX as usize) as u32,
    };
    let mut texture = None;
    unsafe { device.CreateTexture2D(&description, Some(&initial), Some(&mut texture)) }
        .map_err(|error| format!("Could not upload a transition frame to D3D11: {error}"))?;
    let texture = texture.ok_or("D3D11 did not return a transition texture")?;
    let mut view = None;
    unsafe { device.CreateShaderResourceView(&texture, None, Some(&mut view)) }
        .map_err(|error| format!("Could not create a transition texture view: {error}"))?;
    let view = view.ok_or("D3D11 did not return a transition texture view")?;
    Ok((texture, view))
}

fn validate_frame(frame: &FrozenFrame) -> Result<(), String> {
    let expected = frame.width as usize * frame.height as usize * 4;
    if frame.width == 0 || frame.height == 0 || frame.rgba.len() != expected {
        return Err("The transition frame has invalid dimensions".into());
    }
    Ok(())
}

fn shader_style(effect: TransitionEffect) -> Result<usize, String> {
    TRANSITION_CATALOG
        .iter()
        .position(|candidate| *candidate == effect)
        .ok_or_else(|| format!("{effect:?} is not a concrete transition effect"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_styles_follow_the_public_thirty_effect_catalog() {
        for (index, effect) in TRANSITION_CATALOG.iter().copied().enumerate() {
            assert_eq!(shader_style(effect).unwrap(), index);
        }
        assert!(shader_style(TransitionEffect::Off).is_err());
        assert!(shader_style(TransitionEffect::Random).is_err());
    }

    #[test]
    fn transition_constants_remain_shader_compatible() {
        assert_eq!(std::mem::size_of::<TransitionConstants>(), 32);
    }

    /// Offscreen replay of the production shader bytecode on the WARP
    /// software device, so every catalog effect is checked without a desktop.
    struct Replay {
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        vertex_shader: ID3D11VertexShader,
        pixel_shader: ID3D11PixelShader,
        constants: ID3D11Buffer,
        sampler: ID3D11SamplerState,
    }

    impl Replay {
        fn new() -> Self {
            use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
            use windows::Win32::Graphics::Direct3D11::D3D11CreateDevice;
            let mut device = None;
            let mut context = None;
            unsafe {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&[D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
            }
            .unwrap();
            let (device, context) = (device.unwrap(), context.unwrap());
            let mut vertex_shader = None;
            let mut pixel_shader = None;
            let mut constants = None;
            let mut sampler = None;
            unsafe {
                device
                    .CreateVertexShader(VERTEX_SHADER, None, Some(&mut vertex_shader))
                    .unwrap();
                device
                    .CreatePixelShader(PIXEL_SHADER, None, Some(&mut pixel_shader))
                    .unwrap();
                device
                    .CreateBuffer(
                        &D3D11_BUFFER_DESC {
                            ByteWidth: std::mem::size_of::<TransitionConstants>() as u32,
                            Usage: D3D11_USAGE_DEFAULT,
                            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                            ..Default::default()
                        },
                        None,
                        Some(&mut constants),
                    )
                    .unwrap();
                device
                    .CreateSamplerState(
                        &D3D11_SAMPLER_DESC {
                            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                            ComparisonFunc: D3D11_COMPARISON_NEVER,
                            MaxLOD: f32::MAX,
                            ..Default::default()
                        },
                        Some(&mut sampler),
                    )
                    .unwrap();
            }
            Self {
                device,
                context,
                vertex_shader: vertex_shader.unwrap(),
                pixel_shader: pixel_shader.unwrap(),
                constants: constants.unwrap(),
                sampler: sampler.unwrap(),
            }
        }

        fn render(
            &self,
            outgoing: &FrozenFrame,
            incoming: &FrozenFrame,
            effect: TransitionEffect,
            progress: f32,
        ) -> Vec<u8> {
            use windows::Win32::Graphics::Direct3D11::{
                D3D11_BIND_RENDER_TARGET, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE,
                D3D11_MAP_READ, D3D11_USAGE_STAGING,
            };
            let (width, height) = (outgoing.width, outgoing.height);
            let (_old_texture, old_view) = create_input_texture(&self.device, outgoing).unwrap();
            let (_new_texture, new_view) = create_input_texture(&self.device, incoming).unwrap();
            let mut description = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..Default::default()
            };
            let aspect = width as f32 / height as f32;
            // Same constants as D3d11TransitionRenderer::render.
            let constants = TransitionConstants {
                progress: progress.clamp(0.0, 1.0),
                target_aspect: aspect,
                old_aspect: aspect,
                new_aspect: aspect,
                style: shader_style(effect).unwrap() as f32,
                padding: [0.0; 3],
            };
            unsafe {
                let mut target = None;
                self.device
                    .CreateTexture2D(&description, None, Some(&mut target))
                    .unwrap();
                let target = target.unwrap();
                let mut view = None;
                self.device
                    .CreateRenderTargetView(&target, None, Some(&mut view))
                    .unwrap();
                description.Usage = D3D11_USAGE_STAGING;
                description.BindFlags = 0;
                description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                let mut staging = None;
                self.device
                    .CreateTexture2D(&description, None, Some(&mut staging))
                    .unwrap();
                let staging = staging.unwrap();
                self.context.UpdateSubresource(
                    &self.constants,
                    0,
                    None,
                    (&constants as *const TransitionConstants).cast(),
                    0,
                    0,
                );
                self.context.OMSetRenderTargets(Some(&[view]), None);
                self.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    Width: width as f32,
                    Height: height as f32,
                    MaxDepth: 1.0,
                    ..Default::default()
                }]));
                self.context
                    .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                self.context.VSSetShader(&self.vertex_shader, None);
                self.context.PSSetShader(&self.pixel_shader, None);
                self.context
                    .PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
                self.context
                    .PSSetShaderResources(0, Some(&[Some(old_view), Some(new_view)]));
                self.context
                    .PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
                self.context.Draw(3, 0);
                self.context.CopyResource(&staging, &target);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                self.context
                    .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                    .unwrap();
                let mut rgba = Vec::with_capacity((width * height * 4) as usize);
                for y in 0..height as usize {
                    rgba.extend_from_slice(std::slice::from_raw_parts(
                        (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                        width as usize * 4,
                    ));
                }
                self.context.Unmap(&staging, 0);
                rgba
            }
        }
    }

    // Asymmetric patterns. Only the incoming frame carries blue, so the blue
    // channel reads the shader's blend mask even where an effect resamples it.
    fn patterned(width: u32, height: u32, incoming: bool) -> FrozenFrame {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let u = (x as f32 + 0.5) / width as f32;
                let v = (y as f32 + 0.5) / height as f32;
                rgba.extend_from_slice(&if incoming {
                    let cell = ((u * 16.0) as u32 + (v * 9.0) as u32) % 2;
                    [
                        [64, 224][cell as usize],
                        ((1.0 - u) * 255.0) as u8,
                        255,
                        255,
                    ]
                } else {
                    [(u * 255.0) as u8, (v * 255.0) as u8, 0, 255]
                });
            }
        }
        FrozenFrame {
            width,
            height,
            rgba,
        }
    }

    #[test]
    fn every_catalog_effect_completes_moves_and_keeps_its_landmarks() {
        let replay = Replay::new();
        let steps = [0.0, 0.001, 0.1, 0.25, 0.5, 0.75, 0.9, 0.999, 1.0];
        for (width, height) in [(160u32, 90u32), (210, 90)] {
            let (outgoing, incoming) = (
                patterned(width, height, false),
                patterned(width, height, true),
            );
            let mask_at = |rgba: &[u8], x: u32, y: u32| {
                f32::from(rgba[((y * width + x) * 4 + 2) as usize]) / 255.0
            };
            let mean_where = |rgba: &[u8], keep: &dyn Fn(f32, f32) -> bool| {
                let (mut sum, mut count) = (0.0, 0.0);
                for y in 0..height {
                    for x in 0..width {
                        let u = (x as f32 + 0.5) / width as f32;
                        let v = (y as f32 + 0.5) / height as f32;
                        if keep(u, v) {
                            sum += mask_at(rgba, x, y);
                            count += 1.0;
                        }
                    }
                }
                sum / count
            };
            let fraction = |rgba: &[u8], test: &dyn Fn(f32) -> bool| {
                rgba.chunks_exact(4)
                    .filter(|pixel| test(f32::from(pixel[2]) / 255.0))
                    .count() as f32
                    / (width * height) as f32
            };
            for effect in TRANSITION_CATALOG {
                let frames: Vec<_> = steps
                    .iter()
                    .map(|progress| replay.render(&outgoing, &incoming, effect, *progress))
                    .collect();
                let name = format!("{effect:?} at {width}x{height}");
                assert_eq!(frames[0], outgoing.rgba, "{name} start frame");
                assert_eq!(frames[8], incoming.rgba, "{name} final frame");
                assert!(
                    fraction(&frames[1], &|mask| mask > 0.5) <= 0.005,
                    "{name} jumps at the first frame"
                );
                assert!(
                    fraction(&frames[7], &|mask| mask < 0.98) <= 0.005,
                    "{name} snaps at the final frame"
                );
                let means: Vec<f32> = frames.iter().map(|f| mean_where(f, &|_, _| true)).collect();
                assert!(means[3] >= 0.02, "{name} barely moves by 25%: {means:?}");
                assert!(
                    means.windows(2).all(|pair| pair[1] >= pair[0] - 0.02),
                    "{name} reverses: {means:?}"
                );
                let residual = frames[7]
                    .iter()
                    .zip(&incoming.rgba)
                    .map(|(a, b)| f32::from(a.abs_diff(*b)))
                    .sum::<f32>()
                    / frames[7].len() as f32;
                assert!(
                    residual <= 2.0,
                    "{name} distortion remains at 99.9%: {residual}"
                );

                let quarter = &frames[3];
                let half = &frames[4];
                let side = |first: &dyn Fn(f32, f32) -> bool, second: &dyn Fn(f32, f32) -> bool| {
                    mean_where(quarter, first) - mean_where(quarter, second)
                };
                let left = |u: f32, _: f32| u < 0.5;
                let right = |u: f32, _: f32| u >= 0.5;
                let top = |_: f32, v: f32| v < 0.5;
                let bottom = |_: f32, v: f32| v >= 0.5;
                let lead = match effect {
                    TransitionEffect::WipeLeft | TransitionEffect::Wave => side(&left, &right),
                    TransitionEffect::WipeRight => side(&right, &left),
                    TransitionEffect::WipeUp | TransitionEffect::Checker => side(&top, &bottom),
                    TransitionEffect::WipeDown => side(&bottom, &top),
                    TransitionEffect::Iris
                    | TransitionEffect::Ring
                    | TransitionEffect::Diamond
                    | TransitionEffect::Circle
                    | TransitionEffect::Kaleidoscope
                    | TransitionEffect::ChromaticRipple => {
                        mean_where(half, &|u, v| (u - 0.5).abs() < 0.1 && (v - 0.5).abs() < 0.1)
                            - mean_where(half, &|u, v| u < 0.1 || u > 0.9 || v < 0.1 || v > 0.9)
                    }
                    _ => continue,
                };
                assert!(lead > 0.2, "{name} landmark lead {lead}");
            }
        }
    }
}
