//! The second screen (the AYN Thor's bottom panel): a Java `Presentation`
//! hands its `Surface` here, and this module draws on it with its own wgpu
//! instance so the main window and the game are not involved.
//!
//! The game publishes its minimap image with [`publish_map`]; the surface
//! draws it as a square in the middle of the panel. Until the first image
//! arrives it animates a colour, which also proves the surface works.
#![allow(unsafe_code)]

use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use jni::EnvUnowned;
use jni::objects::{JClass, JObject};
use ndk::native_window::NativeWindow;
use wgpu::rwh::{AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle};

enum Command {
    Window(NativeWindow),
    Destroy,
    /// The Items tab wants the window on the main renderer.
    HandOff,
}

/// The minimap image the game last published, copied for the render thread.
#[derive(Clone)]
struct Map {
    version: u64,
    width: u32,
    height: u32,
    rgba: Arc<Vec<u8>>,
}

static MAP: Mutex<Option<Map>> = Mutex::new(None);
static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

/// The player's horizontal facing direction in map coordinates (right, down).
static PLAYER: Mutex<(f32, f32)> = Mutex::new((0.0, 1.0));

/// The game calls this every frame so the marker turns with the player.
pub fn publish_player(dx: f32, dz: f32) {
    *PLAYER.lock().unwrap() = (dx, dz);
}

/// 0 draws the map, 1 hands the panel to the Java debug view.
static TAB: AtomicU32 = AtomicU32::new(0);
static DEBUG: Mutex<Option<(u64, String)>> = Mutex::new(None);
static NEXT_DEBUG: AtomicU64 = AtomicU64::new(1);

pub fn set_tab(tab: u32) {
    TAB.store(tab, Ordering::Relaxed);
    if tab == 2 {
        let _ = commands().lock().unwrap().send(Command::HandOff);
    }
}

/// A window the map thread handed to the main renderer for the Items tab.
static HANDOFF: Mutex<Option<NativeWindow>> = Mutex::new(None);

pub fn take_handoff() -> Option<NativeWindow> {
    HANDOFF.lock().unwrap().take()
}

/// Give the window back to the map thread (leaving the Items tab).
pub fn return_window(window: NativeWindow) {
    let _ = commands().lock().unwrap().send(Command::Window(window));
}

pub fn tab() -> u32 {
    TAB.load(Ordering::Relaxed)
}

/// The inventory snapshot and slot clicks from the bottom screen.
static INVENTORY: Mutex<Option<(u64, String)>> = Mutex::new(None);
static NEXT_INVENTORY: AtomicU64 = AtomicU64::new(1);
static INVENTORY_CLICKS: Mutex<Vec<(usize, i32)>> = Mutex::new(Vec::new());

pub fn publish_inventory(text: String) {
    let version = NEXT_INVENTORY.fetch_add(1, Ordering::Relaxed);
    *INVENTORY.lock().unwrap() = Some((version, text));
}

pub fn take_inventory_clicks() -> Vec<(usize, i32)> {
    std::mem::take(&mut *INVENTORY_CLICKS.lock().unwrap())
}

/// The last finger position on the bottom inventory, in surface pixels.
static CURSOR: Mutex<Option<(f32, f32)>> = Mutex::new(None);
static TOUCHED: AtomicBool = AtomicBool::new(false);
/// Whether a finger is still on the bottom inventory (down, not lifted).
static TOUCHING: AtomicBool = AtomicBool::new(false);

pub fn inventory_cursor() -> Option<(f32, f32)> {
    *CURSOR.lock().unwrap()
}

/// True once per batch of touch events that moved the bottom cursor.
pub fn take_inventory_touch() -> bool {
    TOUCHED.swap(false, Ordering::Relaxed)
}

/// Whether a finger is currently down on the bottom inventory.
pub fn inventory_touching() -> bool {
    TOUCHING.load(Ordering::Relaxed)
}

pub fn set_inventory_touching(touching: bool) {
    TOUCHING.store(touching, Ordering::Relaxed);
}

fn queue_inventory_click(slot: usize, button: i32) {
    INVENTORY_CLICKS.lock().unwrap().push((slot, button));
}

/// The game calls this with the debug screen's text; the second-screen thread
/// passes it to the Java view while the debug tab is shown.
pub fn publish_debug(text: String) {
    let version = NEXT_DEBUG.fetch_add(1, Ordering::Relaxed);
    *DEBUG.lock().unwrap() = Some((version, text));
}

/// The game calls this when its minimap image changed: `SIZE` x `SIZE` RGBA.
pub fn publish_map(width: u32, height: u32, rgba: &[u8]) {
    if width == 0 || height == 0 || rgba.len() < width as usize * height as usize * 4 {
        return;
    }
    let version = NEXT_VERSION.fetch_add(1, Ordering::Relaxed);
    *MAP.lock().unwrap() = Some(Map {
        version,
        width,
        height,
        rgba: Arc::new(rgba.to_vec()),
    });
}

static COMMANDS: OnceLock<Mutex<Sender<Command>>> = OnceLock::new();

fn commands() -> &'static Mutex<Sender<Command>> {
    COMMANDS.get_or_init(|| {
        let (sender, receiver) = channel();
        std::thread::Builder::new()
            .name("Second screen".into())
            .spawn(move || run(receiver))
            .expect("the second screen thread");
        Mutex::new(sender)
    })
}

fn set_surface(window: Option<NativeWindow>) {
    let command = match window {
        Some(window) => Command::Window(window),
        None => Command::Destroy,
    };
    let _ = commands().lock().unwrap().send(command);
}

/// `HorizonActivity.nativeSecondSurface`: the Presentation's Surface, or null
/// when it went away.
#[unsafe(no_mangle)]
extern "system" fn Java_com_lukagejak_horizon_HorizonActivity_nativeSecondSurface<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    surface: JObject<'caller>,
) {
    env.with_env(|env| -> Result<(), jni::errors::Error> {
        let window = if surface.is_null() {
            None
        } else {
            // SAFETY: the Surface is valid for the call; ANativeWindow_fromSurface
            // takes its own reference, which NativeWindow releases on drop.
            let ptr = unsafe {
                ndk_sys::ANativeWindow_fromSurface(
                    env.get_raw().cast(),
                    surface.as_raw().cast(),
                )
            };
            NonNull::new(ptr).map(|ptr| unsafe { NativeWindow::from_ptr(ptr) })
        };
        set_surface(window);
        Ok(())
    })
    .resolve::<jni::errors::LogErrorAndDefault>()
}

/// `HorizonActivity.nativeSecondTab`: 0 is the map, 1 the debug view.
#[unsafe(no_mangle)]
extern "system" fn Java_com_lukagejak_horizon_HorizonActivity_nativeSecondTab<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    tab: jni::sys::jint,
) {
    set_tab(tab.max(0) as u32);
}

/// `HorizonActivity.nativeInventoryClick`: a tap on an inventory slot.
#[unsafe(no_mangle)]
extern "system" fn Java_com_lukagejak_horizon_HorizonActivity_nativeInventoryClick<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    slot: jni::sys::jint,
    button: jni::sys::jint,
) {
    if slot >= 0 {
        queue_inventory_click(slot as usize, button);
    }
}

/// `HorizonActivity.nativeInventoryCursor`: where the finger is on the inventory.
#[unsafe(no_mangle)]
extern "system" fn Java_com_lukagejak_horizon_HorizonActivity_nativeInventoryCursor<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    x: jni::sys::jfloat,
    y: jni::sys::jfloat,
) {
    *CURSOR.lock().unwrap() = Some((x, y));
    TOUCHED.store(true, Ordering::Relaxed);
}

/// `HorizonActivity.nativeInventoryTouch`: a finger went down or came up.
#[unsafe(no_mangle)]
extern "system" fn Java_com_lukagejak_horizon_HorizonActivity_nativeInventoryTouch<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    touching: jni::sys::jboolean,
) {
    set_inventory_touching(touching);
}

fn run(commands: Receiver<Command>) {
    log::info!("Second screen: thread started");
    loop {
        match commands.recv() {
            Ok(Command::Window(window)) => {
                if TAB.load(Ordering::Relaxed) == 2 {
                    *HANDOFF.lock().unwrap() = Some(window);
                    continue;
                }
                log::info!(
                    "Second screen: surface {}x{}",
                    window.width(),
                    window.height()
                );
                if render(&window, &commands) {
                    *HANDOFF.lock().unwrap() = Some(window);
                }
                log::info!("Second screen: surface released");
            }
            // Destroy with no surface to drop; wait for the next one.
            Ok(Command::Destroy) | Ok(Command::HandOff) => {}
            Err(_) => return,
        }
    }
}

fn render(window: &NativeWindow, commands: &Receiver<Command>) -> bool {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::GL;
    let instance = wgpu::Instance::new(descriptor);

    let handle = RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(window.ptr().cast()));
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_window_handle: handle,
        raw_display_handle: Some(RawDisplayHandle::Android(AndroidDisplayHandle::new())),
    };
    // SAFETY: `window` outlives the surface, which is dropped before it.
    let surface = match unsafe { instance.create_surface_unsafe(target) } {
        Ok(surface) => surface,
        Err(e) => {
            log::warn!("Second screen: no surface: {e}");
            return false;
        }
    };
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(e) => {
            log::warn!("Second screen: no adapter: {e}");
            return false;
        }
    };
    let (device, queue) = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Second screen"),
        required_limits: adapter.limits(),
        ..Default::default()
    })) {
        Ok(device) => device,
        Err(e) => {
            log::warn!("Second screen: no device: {e}");
            return false;
        }
    };
    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(|format| !format.is_srgb())
        .unwrap_or(caps.formats[0]);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: u32::try_from(window.width()).unwrap_or(0).max(1),
        height: u32::try_from(window.height()).unwrap_or(0).max(1),
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        color_space: wgpu::SurfaceColorSpace::default(),
    };
    surface.configure(&device, &config);
    log::info!(
        "Second screen: {}x{} {:?} on {}",
        config.width,
        config.height,
        format,
        adapter.get_info().name
    );

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Second screen map sampler"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Second screen map bindings"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Second screen map"),
        source: wgpu::ShaderSource::Wgsl(include_str!("second_screen.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Second screen map"),
        bind_group_layouts: &[Some(&layout)],
        ..Default::default()
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Second screen map"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let mut map_texture: Option<wgpu::Texture> = None;
    let mut map_bind: Option<wgpu::BindGroup> = None;
    let mut map_size = (0u32, 0u32);
    let mut map_version = 0u64;
    let mut last_debug = 0u64;
    let mut last_inventory = 0u64;

    let player_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Second screen player"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let player_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Second screen player"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let player_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Second screen player"),
        layout: &player_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: player_buffer.as_entire_binding(),
        }],
    });
    let marker_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Second screen marker"),
        source: wgpu::ShaderSource::Wgsl(include_str!("marker.wgsl").into()),
    });
    let marker_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Second screen marker"),
        bind_group_layouts: &[Some(&player_layout)],
        ..Default::default()
    });
    let marker_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Second screen marker"),
        layout: Some(&marker_layout),
        vertex: wgpu::VertexState {
            module: &marker_module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &marker_module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let start = Instant::now();
    loop {
        match commands.try_recv() {
            Ok(Command::HandOff) => return true,
            Ok(Command::Destroy) | Ok(Command::Window(_)) => return false,
            Err(TryRecvError::Disconnected) => return false,
            Err(TryRecvError::Empty) => {}
        }
        let tab = TAB.load(Ordering::Relaxed);
        if tab != 0 {
            if tab == 1 {
                if let Some((version, text)) = DEBUG.lock().unwrap().clone() {
                    if version != last_debug {
                        super::jni::platform("second_debug", &text);
                        last_debug = version;
                    }
                }
            } else if tab == 2
                && let Some((version, text)) = INVENTORY.lock().unwrap().clone()
                && version != last_inventory
            {
                super::jni::platform("second_inventory", &text);
                last_inventory = version;
            }
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        let (dx, dz) = *PLAYER.lock().unwrap();
        queue.write_buffer(
            &player_buffer,
            0,
            bytemuck::cast_slice(&[dx, dz, 0.0f32, 0.0]),
        );
        if let Some(map) = MAP.lock().unwrap().clone() {
            if map.version != map_version {
                if map_size != (map.width, map.height) || map_texture.is_none() {
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("Second screen map"),
                        size: wgpu::Extent3d {
                            width: map.width.max(1),
                            height: map.height.max(1),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    let map_view = texture.create_view(&Default::default());
                    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Second screen map"),
                        layout: &layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&map_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&sampler),
                            },
                        ],
                    });
                    map_texture = Some(texture);
                    map_bind = Some(bind);
                    map_size = (map.width, map.height);
                }
                if let Some(texture) = &map_texture {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                            aspect: wgpu::TextureAspect::All,
                        },
                        map.rgba.as_slice(),
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(map.width * 4),
                            rows_per_image: Some(map.height),
                        },
                        wgpu::Extent3d {
                            width: map.width,
                            height: map.height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                map_version = map.version;
            }
        }
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&device, &config);
                continue;
            }
            _ => return false,
        };
        let view = frame.texture.create_view(&Default::default());
        let t = start.elapsed().as_secs_f64();
        let color = if map_bind.is_some() {
            wgpu::Color {
                r: 0.04,
                g: 0.04,
                b: 0.06,
                a: 1.0,
            }
        } else {
            wgpu::Color {
                r: 0.5 + 0.5 * (t * 2.0).sin(),
                g: 0.5 + 0.5 * (t * 2.0 + 2.094).sin(),
                b: 0.5 + 0.5 * (t * 2.0 + 4.188).sin(),
                a: 1.0,
            }
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Second screen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if let Some(bind) = &map_bind {
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, bind, &[]);
                let side = config.width.min(config.height) as f32;
                let x = config.width as f32 - side;
                let y = (config.height as f32 - side) / 2.0;
                pass.set_viewport(x, y, side, side, 0.0, 1.0);
                pass.draw(0..6, 0..1);
            }
            if map_bind.is_some() {
                pass.set_pipeline(&marker_pipeline);
                pass.set_bind_group(0, &player_bind, &[]);
                pass.draw(0..6, 0..1);
            }
        }
        queue.submit([encoder.finish()]);
        queue.present(frame);
        std::thread::sleep(Duration::from_millis(16));
    }
    false
}
