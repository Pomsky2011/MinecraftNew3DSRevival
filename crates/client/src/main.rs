//! The 3DS app: rendering, input, sound and the interface (DESIGN.md §7, §8).
//!
//! For now, DESIGN.md §14 step 1: a textured cube in stereo 3D on the top screen, and on the
//! bottom one what the hardware gives an app (`probe`) and the frame time.

mod probe;
mod texture;

// Not used yet, but already linked so the release feature reaches them (DESIGN.md §5.6).
use n3dsr_protocol as _;
use n3dsr_server as _;

use std::io::Write as _;

use citro3d::macros::include_shader;
use citro3d::math::{AspectRatio, ClipPlanes, Matrix4, Projection, StereoDisplacement};
use citro3d::render::{ClearFlags, DepthFormat, Target};
use citro3d::{attrib, buffer, shader, texenv};
use ctru::linear::LinearAllocator;
use ctru::prelude::*;
use ctru::services::gfx::{RawFrameBuffer, Screen, TopScreen3D};

static SHADER: &[u8] = include_shader!("../shaders/cube.v.pica");
const SKY: u32 = 0x78_A7_FF_FF;
/// Radians the cube turns each frame.
const SPIN: f32 = 0.012;

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
}

fn main() {
    // The New 3DS's full clock (804 MHz instead of 268) and its L2 cache are off until asked for.
    // SAFETY: a plain system call.
    unsafe { ctru_sys::osSetSpeedupEnable(true) };

    let gfx = Gfx::new().expect("no graphics");
    let mut hid = Hid::new().expect("no input");
    let mut apt = Apt::new().expect("no applet services");
    let console = Console::new(gfx.bottom_screen.borrow_mut());

    probe::report(&mut apt);
    println!("\nSlide the 3D slider; START quits.");

    let mut gpu = citro3d::Instance::new().expect("citro3d didn't start");
    let top = TopScreen3D::from(&gfx.top_screen);
    let (mut left, right) = top.split_mut();
    let RawFrameBuffer { width, height, .. } = left.raw_framebuffer();
    let mut left = gpu
        .render_target(width, height, left, Some(DepthFormat::Depth24Stencil8))
        .expect("no left-eye target");
    let mut right = gpu
        .render_target(width, height, right, Some(DepthFormat::Depth24Stencil8))
        .expect("no right-eye target");

    let library = shader::Library::from_bytes(SHADER).expect("bad shader");
    let program = shader::Program::new(library.get(0).expect("no vertex shader")).unwrap();
    let transform = program.get_vertex_uniform("transform").unwrap();

    let mut attributes = attrib::Info::new();
    attributes
        .add_loader(attrib::Register::V0, attrib::Format::Float, 3)
        .unwrap();
    attributes
        .add_loader(attrib::Register::V1, attrib::Format::Float, 2)
        .unwrap();
    let (vertices, indices) = cube();
    let mut buffers = buffer::Info::new();
    buffers
        .add(
            buffer::Buffer::new_in_linear(vertices),
            attributes.permutation(),
        )
        .unwrap();

    let (w, h, texels) = texture::test_pattern();
    let mut pattern = citro3d::texture::Texture::new(citro3d::texture::TextureParameters::new_2d(
        w,
        h,
        citro3d::texture::ColorFormat::Rgba8,
    ))
    .expect("no texture");
    pattern.load_image(&texels, Default::default()).unwrap();
    // Nearest-neighbour both ways: Minecraft's pixels stay square.
    pattern.set_filter(
        citro3d::texture::Filter::Nearest,
        citro3d::texture::Filter::Nearest,
    );
    let textured = texenv::TexEnv::new()
        .src(texenv::Mode::BOTH, texenv::Source::Texture0, None, None)
        .func(texenv::Mode::BOTH, texenv::CombineFunc::Replace);

    let mut angle = 0.0f32;
    let mut frames = 0u32;
    let mut last = unsafe { ctru_sys::svcGetSystemTick() };
    while apt.main_loop() {
        hid.scan_input();
        if hid.keys_down().contains(KeyPad::START) {
            break;
        }

        angle += SPIN;
        let mut model = Matrix4::identity();
        model.rotate_x(angle * 0.7);
        model.rotate_y(angle);
        model.translate(0.0, 0.0, -2.5);
        let (left_eye, right_eye) = eyes();

        gpu.render_frame_with(|mut frame| {
            frame.bind_program(&program);
            frame.set_attr_info(&attributes);
            frame.set_texenvs(&[textured]);
            frame.bind_texture(citro3d::texture::Index::Texture0, &pattern);
            for (target, eye) in [(&mut left, &left_eye), (&mut right, &right_eye)] {
                target.clear(ClearFlags::ALL, SKY, 0);
                frame.select_render_target(target).unwrap();
                frame.bind_vertex_uniform(transform, eye * model);
                frame.draw_elements(buffer::Primitive::Triangles, &buffers, &indices);
            }
            frame
        });

        frames += 1;
        if frames == 60 {
            let now = unsafe { ctru_sys::svcGetSystemTick() };
            let ms = (now - last) as f64 * 1000.0 / f64::from(ctru_sys::SYSCLOCK_ARM11) / 60.0;
            // Row 28, column 1: under the probe's lines, rewritten in place.
            print!("\x1b[28;1HFrame: {ms:5.2} ms   ");
            // No newline, so nothing flushes it otherwise.
            let _ = std::io::stdout().flush();
            last = now;
            frames = 0;
        }
    }
    drop(console);
}

/// Each eye's projection, the eyes spread by the 3D slider (zero when it's down).
fn eyes() -> (Matrix4, Matrix4) {
    let spread = ctru::os::current_3d_slider_state() / 2.0;
    let (left, right) = StereoDisplacement::new(spread, 2.0);
    Projection::perspective(
        60.0_f32.to_radians(),
        AspectRatio::TopScreen,
        ClipPlanes {
            near: 0.05,
            far: 100.0,
        },
    )
    .stereo_matrices(left, right)
}

/// A unit cube's 24 vertices (4 per face, so each face has its own texture corners) and 36
/// indices, in linear memory, where the GPU can read them. Faces wind counter-clockwise seen from
/// outside, which is front-facing for citro3d's default culling.
fn cube() -> (Vec<Vertex, LinearAllocator>, Vec<u8, LinearAllocator>) {
    // Per face: the corner bottom-left seen from outside, then the directions right and up;
    // right × up points outward.
    const FACES: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([-0.5, -0.5, 0.5], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), // +z
        ([0.5, -0.5, -0.5], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), // -z
        ([0.5, -0.5, 0.5], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]), // +x
        ([-0.5, -0.5, -0.5], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]), // -x
        ([-0.5, 0.5, 0.5], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]), // +y
        ([-0.5, -0.5, -0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]), // -y
    ];
    let mut vertices = Vec::with_capacity_in(24, LinearAllocator);
    let mut indices = Vec::with_capacity_in(36, LinearAllocator);
    for (face, (origin, right, up)) in FACES.into_iter().enumerate() {
        let at = |r: f32, u: f32| std::array::from_fn(|i| origin[i] + right[i] * r + up[i] * u);
        for (r, u) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            vertices.push(Vertex {
                position: at(r, u),
                uv: [r, u],
            });
        }
        let first = face as u8 * 4;
        indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
    }
    (vertices, indices)
}
